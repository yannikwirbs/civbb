//! The draw list widgets emit: a screen-space triangle list the renderer's overlay pass draws (ADR 0117).

use std::f32::consts::TAU;

/// What a vertex samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Texture {
    /// Plain colour.
    None,
    /// The glyph atlas ([`crate::text::Atlas`]).
    Atlas,
    /// A texture of the avatar whose menu is shown, by its index in the document.
    Icon(u32),
}

/// One vertex: window pixels (origin top left, y down), texture coordinates, linear colour with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Position in window pixels.
    pub pos: [f32; 2],
    /// Texture coordinates; ignored for [`Texture::None`].
    pub uv: [f32; 2],
    /// Multiplies the texel.
    pub color: [f32; 4],
    /// What to sample.
    pub texture: Texture,
}

/// Triangles in drawing order, three vertices each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    /// Three per triangle.
    pub vertices: Vec<Vertex>,
}

impl DrawList {
    /// Removes every triangle, keeping the allocation.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// An axis-aligned rectangle from `min` to `max` with texture coordinates `uv_min` to `uv_max`.
    pub fn rect(
        &mut self,
        min: [f32; 2],
        max: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
        texture: Texture,
    ) {
        let v = |x: usize, y: usize| Vertex {
            pos: [[min[0], max[0]][x], [min[1], max[1]][y]],
            uv: [[uv_min[0], uv_max[0]][x], [uv_min[1], uv_max[1]][y]],
            color,
            texture,
        };
        self.vertices
            .extend([v(0, 0), v(1, 0), v(1, 1), v(0, 0), v(1, 1), v(0, 1)]);
    }

    /// A ring sector around `center` between radii `inner` and `outer`, from angle `from` to `to` (radians clockwise
    /// from the top on screen), in plain colour. `inner` 0 makes a pie slice.
    pub fn sector(&mut self, center: [f32; 2], inner: f32, outer: f32, from: f32, to: f32, color: [f32; 4]) {
        let steps = (((to - from).abs() / TAU) * 64.0).ceil().max(1.0) as u32;
        let at = |r: f32, a: f32| Vertex {
            pos: [center[0] + r * a.sin(), center[1] - r * a.cos()],
            uv: [0.0; 2],
            color,
            texture: Texture::None,
        };
        for i in 0..steps {
            let (a, b) = (
                from + (to - from) * i as f32 / steps as f32,
                from + (to - from) * (i + 1) as f32 / steps as f32,
            );
            let (ia, ib, oa, ob) = (at(inner, a), at(inner, b), at(outer, a), at(outer, b));
            self.vertices.extend([ia, oa, ob]);
            if inner > 0.0 {
                self.vertices.extend([ia, ob, ib]);
            }
        }
    }

    /// A filled circle.
    pub fn disc(&mut self, center: [f32; 2], radius: f32, color: [f32; 4]) {
        self.sector(center, 0.0, radius, 0.0, TAU, color);
    }

    /// A line from `a` to `b`, `width` pixels wide, in plain colour.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4]) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let v = |p: [f32; 2], s: f32| Vertex {
            pos: [p[0] + nx * s, p[1] + ny * s],
            uv: [0.0; 2],
            color,
            texture: Texture::None,
        };
        let (a0, a1, b0, b1) = (v(a, -1.0), v(a, 1.0), v(b, -1.0), v(b, 1.0));
        self.vertices.extend([a0, b0, b1, a0, b1, a1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sector_starts_at_the_top_and_runs_clockwise() {
        let mut list = DrawList::default();
        list.sector([100.0, 100.0], 10.0, 50.0, 0.0, TAU / 4.0, [1.0; 4]);
        assert_eq!(list.vertices.len() % 6, 0);
        let first = list.vertices[1].pos;
        assert!(
            (first[0] - 100.0).abs() < 1e-4 && (first[1] - 50.0).abs() < 1e-4,
            "{first:?}"
        );
        let last = list.vertices[list.vertices.len() - 4].pos;
        assert!(
            (last[0] - 150.0).abs() < 1e-3 && (last[1] - 100.0).abs() < 1e-3,
            "{last:?}"
        );
    }
}
