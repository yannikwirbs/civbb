//! Labels: shaped and rasterised by cosmic-text from the system's fonts into one glyph atlas (ADR 0116).

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{Align, Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, SwashCache, SwashContent};

use crate::draw::{DrawList, Texture};

/// Side of the square atlas in pixels.
pub const ATLAS_SIZE: u32 = 1024;

/// The glyph atlas: RGBA8, linear; coverage glyphs are white with coverage in alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct Atlas {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major texels, four bytes each.
    pub rgba: Vec<u8>,
    /// Bumped on every change; the renderer replaces its copy when it differs.
    pub version: u64,
}

/// A glyph's place in the atlas and its offset from the pen position.
#[derive(Clone, Copy, Debug)]
struct Glyph {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
}

/// Fonts, the rasteriser's cache and the atlas glyphs are packed into on first use.
pub struct Text {
    fonts: FontSystem,
    raster: SwashCache,
    buffer: Buffer,
    atlas: Arc<Atlas>,
    /// `None` for glyphs without pixels (spaces).
    glyphs: HashMap<CacheKey, Option<Glyph>>,
    /// The open shelf's left edge, top and height.
    shelf: (u32, u32, u32),
    /// A glyph did not fit: the atlas is cleared at the next [`Text::begin`].
    full: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

impl Text {
    /// Scans the system's fonts (once per process is enough) and starts an empty atlas.
    pub fn new() -> Self {
        Self::with_fonts(FontSystem::new())
    }

    /// With the given fonts, for tests.
    pub fn with_fonts(fonts: FontSystem) -> Self {
        Self {
            fonts,
            raster: SwashCache::new(),
            buffer: Buffer::new_empty(Metrics::new(16.0, 20.0)),
            atlas: Arc::new(Atlas {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                rgba: vec![0; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
                version: 0,
            }),
            glyphs: HashMap::new(),
            shelf: (0, 0, 0),
            full: false,
        }
    }

    /// The atlas the drawn labels sample.
    pub fn atlas(&self) -> &Arc<Atlas> {
        &self.atlas
    }

    /// Starts a frame's labels: clears the atlas if a glyph did not fit last frame.
    pub fn begin(&mut self) {
        if !self.full {
            return;
        }
        self.full = false;
        self.glyphs.clear();
        self.shelf = (0, 0, 0);
        let atlas = Arc::make_mut(&mut self.atlas);
        atlas.rgba.fill(0);
        atlas.version += 1;
    }

    /// Draws `text` centred on `center`, wrapped to `width` pixels, `size` pixels high, in `color`.
    pub fn label(&mut self, list: &mut DrawList, text: &str, center: [f32; 2], width: f32, size: f32, color: [f32; 4]) {
        self.buffer
            .set_metrics_and_size(Metrics::new(size, size * 1.25), Some(width), None);
        self.buffer
            .set_text(text, &Attrs::new(), Shaping::Advanced, Some(Align::Center));
        self.buffer.shape_until_scroll(&mut self.fonts, false);
        let height = (self.buffer.layout_runs())
            .last()
            .map_or(0.0, |r| r.line_top + r.line_height);
        let (left, top) = (center[0] - width / 2.0, (center[1] - height / 2.0).round());
        let mut placed = Vec::new();
        for run in self.buffer.layout_runs() {
            for g in run.glyphs {
                let p = g.physical((left, top + run.line_y), 1.0);
                placed.push((p.cache_key, p.x, p.y));
            }
        }
        let size = [self.atlas.width as f32, self.atlas.height as f32];
        for (key, x, y) in placed {
            let Some(g) = self.glyph(key) else { continue };
            let min = [(x + g.left) as f32, (y - g.top) as f32];
            let max = [min[0] + g.width as f32, min[1] + g.height as f32];
            let uv_min = [g.x as f32 / size[0], g.y as f32 / size[1]];
            let uv_max = [(g.x + g.width) as f32 / size[0], (g.y + g.height) as f32 / size[1]];
            list.rect(min, max, uv_min, uv_max, color, Texture::Atlas);
        }
    }

    /// The glyph's place in the atlas, rasterised and packed on first use; `None` without pixels or room.
    fn glyph(&mut self, key: CacheKey) -> Option<Glyph> {
        if let Some(g) = self.glyphs.get(&key) {
            return *g;
        }
        let image = self.raster.get_image(&mut self.fonts, key).as_ref();
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert(key, None);
            return None;
        };
        let (width, height) = (image.placement.width, image.placement.height);
        let atlas = Arc::make_mut(&mut self.atlas);
        // Shelves with one pixel of padding around each glyph.
        let (mut x, mut y, mut h) = self.shelf;
        if x + width + 1 > atlas.width {
            (x, y, h) = (0, y + h, 0);
        }
        if y + height + 1 > atlas.height || width + 1 > atlas.width {
            self.full = true;
            return None;
        }
        for row in 0..height {
            for col in 0..width {
                let i = (row * width + col) as usize;
                let texel = match image.content {
                    SwashContent::Mask => [255, 255, 255, image.data[i]],
                    SwashContent::Color => {
                        let c = &image.data[i * 4..i * 4 + 4];
                        [c[0], c[1], c[2], c[3]]
                    }
                    SwashContent::SubpixelMask => [255, 255, 255, image.data[i * 4 + 1]],
                };
                let at = (((y + row) * atlas.width + x + col) * 4) as usize;
                atlas.rgba[at..at + 4].copy_from_slice(&texel);
            }
        }
        atlas.version += 1;
        self.shelf = (x + width + 1, y, h.max(height + 1));
        let g = Glyph {
            x,
            y,
            width,
            height,
            left: image.placement.left,
            top: image.placement.top,
        };
        self.glyphs.insert(key, Some(g));
        Some(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With whatever fonts the machine has: a label's glyphs land in the atlas once, centred on the point asked for.
    /// Machines without fonts draw nothing (ADR 0116).
    #[test]
    fn a_label_is_packed_once_and_centred() {
        let start = std::time::Instant::now();
        let mut text = Text::new();
        eprintln!("font scan: {:?}", start.elapsed());
        let mut list = DrawList::default();
        text.begin();
        text.label(&mut list, "Hat", [200.0, 100.0], 120.0, 16.0, [1.0; 4]);
        if list.vertices.is_empty() {
            return eprintln!("skipped: no fonts");
        }
        assert_eq!(list.vertices.len(), 18, "three glyphs");
        let version = text.atlas().version;
        let (lo, hi) = list
            .vertices
            .iter()
            .fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), v| {
                (
                    [lo[0].min(v.pos[0]), lo[1].min(v.pos[1])],
                    [hi[0].max(v.pos[0]), hi[1].max(v.pos[1])],
                )
            });
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        assert!((mid[0] - 200.0).abs() < 3.0 && (mid[1] - 100.0).abs() < 6.0, "{mid:?}");
        list.clear();
        text.label(&mut list, "Hat", [200.0, 100.0], 120.0, 16.0, [1.0; 4]);
        assert_eq!(text.atlas().version, version, "drawn again from the atlas");
        assert!(text.atlas().rgba.chunks(4).any(|t| t[3] > 0));
    }
}
