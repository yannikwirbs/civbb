//! The radial expression menu (design doc §7, ADR 0117): pages of controls around a ring, driven by a pointer,
//! writing the avatar's expression parameters.

use std::f32::consts::TAU;

use civbb_format::expressions::{Control, ControlKind, Menu};
use civbb_format::state::EntryIndex;

use crate::draw::{DrawList, Texture};
use crate::text::Text;

/// The pointer this frame, in window pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    /// Position, origin top left.
    pub pos: [f32; 2],
    /// The primary button went down this frame.
    pub press: bool,
    /// The primary button went up this frame.
    pub release: bool,
    /// Back was asked for (secondary button or Escape).
    pub back: bool,
}

/// Where the ring sits in a window of `screen` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    /// Centre in pixels.
    pub center: [f32; 2],
    /// Outer radius in pixels.
    pub outer: f32,
    /// The centre disc's radius.
    pub inner: f32,
}

impl Ring {
    /// Centred, the outer radius 0.3 of the smaller side, the centre disc 0.3 of that.
    pub fn new(screen: [f32; 2]) -> Self {
        let outer = 0.3 * screen[0].min(screen[1]);
        Self {
            center: [screen[0] / 2.0, screen[1] / 2.0],
            outer,
            inner: 0.3 * outer,
        }
    }

    /// The angle of sector `i` of `n`'s middle, radians clockwise from the top.
    pub fn middle(n: usize, i: usize) -> f32 {
        (i as f32 + 0.5) * TAU / n as f32
    }

    /// A point `r` pixels from the centre at angle `a`.
    pub fn at(&self, r: f32, a: f32) -> [f32; 2] {
        [self.center[0] + r * a.sin(), self.center[1] - r * a.cos()]
    }
}

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    /// The centre disc: back, or close on the root page.
    Centre,
    /// A control of the open page.
    Control(usize),
}

/// The menu's state: which pages are open, what is held.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Radial {
    open: bool,
    /// The submenu controls opened from the root, by index on their page.
    path: Vec<usize>,
    hover: Option<Hover>,
    /// A button of the open page held down.
    held: Option<usize>,
    /// A puppet of the open page taking the pointer.
    puppet: Option<usize>,
    pointer: [f32; 2],
}

/// Parameter writes the menu asks for, in order.
pub type Writes = Vec<(EntryIndex, f32)>;

const BACKGROUND: [f32; 4] = [0.01, 0.012, 0.016, 0.8];
const SECTOR: [f32; 4] = [0.03, 0.035, 0.05, 0.85];
const HOVERED: [f32; 4] = [0.1, 0.13, 0.2, 0.9];
const ON: [f32; 4] = [0.03, 0.2, 0.3, 0.9];
const ACCENT: [f32; 4] = [0.1, 0.5, 0.8, 1.0];
const LABEL: [f32; 4] = [0.9, 0.9, 0.9, 1.0];
/// Radians left empty between sectors.
const GAP: f32 = 0.012;

impl Radial {
    /// Whether the menu shows.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// What the pointer was over at the last update.
    pub fn hover(&self) -> Option<Hover> {
        self.hover
    }

    /// Opens the root page.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Closes the menu, leaving every open page, held button and puppet (their parameters go to 0).
    pub fn close(&mut self, menu: &Menu, writes: &mut Writes) {
        while self.open {
            self.back(menu, writes);
        }
    }

    /// Opens a closed menu or closes an open one.
    pub fn toggle(&mut self, menu: &Menu, writes: &mut Writes) {
        if self.open {
            self.close(menu, writes);
        } else {
            self.open();
        }
    }

    /// Handles a frame's pointer over a window of `screen` pixels. `params` holds the current value of every
    /// entry the menu names; the writes go to `writes`.
    pub fn update(&mut self, menu: &Menu, params: &[f32], pointer: &Pointer, screen: [f32; 2], writes: &mut Writes) {
        if !self.open {
            return;
        }
        if page(menu, &self.path).is_none() {
            // The menu changed under us: start over at the root.
            (self.path, self.held, self.puppet) = (Vec::new(), None, None);
        }
        self.pointer = pointer.pos;
        let ring = Ring::new(screen);
        let (dx, dy) = (pointer.pos[0] - ring.center[0], pointer.pos[1] - ring.center[1]);
        let controls = &page(menu, &self.path).expect("checked above").controls;
        if let Some(c) = self.puppet.and_then(|i| controls.get(i)) {
            if pointer.press || pointer.back {
                self.back(menu, writes);
            } else {
                puppet(c, [dx / ring.outer, -dy / ring.outer], writes);
            }
            return;
        }
        self.hover = if (dx * dx + dy * dy).sqrt() < ring.inner || controls.is_empty() {
            Some(Hover::Centre)
        } else {
            let a = dx.atan2(-dy).rem_euclid(TAU);
            Some(Hover::Control(
                ((a / TAU * controls.len() as f32) as usize).min(controls.len() - 1),
            ))
        };
        if let Some(i) = self.held.filter(|_| pointer.release) {
            self.held = None;
            write(controls[i].parameter, 0.0, writes);
        }
        if pointer.back {
            return self.back(menu, writes);
        }
        if !pointer.press {
            return;
        }
        let Some(Hover::Control(i)) = self.hover else {
            return self.back(menu, writes);
        };
        let c = &controls[i];
        match &c.kind {
            ControlKind::Button => {
                write(c.parameter, c.value, writes);
                self.held = Some(i);
            }
            ControlKind::Toggle => {
                let on = c.parameter.is_some_and(|p| params.get(p as usize) == Some(&c.value));
                write(c.parameter, if on { 0.0 } else { c.value }, writes);
            }
            ControlKind::SubMenu(_) => {
                write(c.parameter, c.value, writes);
                self.path.push(i);
                self.hover = None;
            }
            ControlKind::TwoAxisPuppet | ControlKind::FourAxisPuppet | ControlKind::RadialPuppet => {
                write(c.parameter, c.value, writes);
                self.puppet = Some(i);
            }
        }
    }

    /// One step back: out of a puppet, then out of a submenu, then closed.
    fn back(&mut self, menu: &Menu, writes: &mut Writes) {
        let Some(controls) = page(menu, &self.path).map(|p| &p.controls) else {
            (self.open, self.path, self.held, self.puppet) = (false, Vec::new(), None, None);
            return;
        };
        if let Some(c) = self.held.take().and_then(|i| controls.get(i)) {
            write(c.parameter, 0.0, writes);
        }
        if let Some(c) = self.puppet.take().and_then(|i| controls.get(i)) {
            write(c.parameter, 0.0, writes);
            if !matches!(c.kind, ControlKind::RadialPuppet) {
                for p in &c.sub_parameters {
                    write(*p, 0.0, writes);
                }
            }
            return;
        }
        self.hover = None;
        match self.path.pop() {
            Some(i) => {
                let parent = page(menu, &self.path).expect("a prefix of a valid path");
                write(parent.controls[i].parameter, 0.0, writes);
            }
            None => self.open = false,
        }
    }

    /// Draws the open page (or the active puppet) for a window of `screen` pixels.
    pub fn draw(&self, menu: &Menu, params: &[f32], text: &mut Text, list: &mut DrawList, screen: [f32; 2]) {
        let Some(page) = page(menu, &self.path).filter(|_| self.open) else {
            return;
        };
        let ring = Ring::new(screen);
        let size = (ring.outer * 0.07).max(12.0);
        let width = (ring.outer - ring.inner) * 1.1;
        let mid = (ring.outer + ring.inner) / 2.0;
        let value = |p: Option<EntryIndex>| p.and_then(|p| params.get(p as usize).copied()).unwrap_or(0.0);
        if let Some(c) = self.puppet.and_then(|i| page.controls.get(i)) {
            list.disc(ring.center, ring.outer, BACKGROUND);
            match c.kind {
                ControlKind::RadialPuppet => {
                    let v = value(c.sub_parameters.first().copied().flatten()).clamp(0.0, 1.0);
                    list.sector(ring.center, ring.inner, ring.outer, 0.0, v * TAU, ACCENT);
                }
                _ => {
                    let (c0, r) = (ring.center, ring.outer);
                    list.line([c0[0] - r, c0[1]], [c0[0] + r, c0[1]], 2.0, SECTOR);
                    list.line([c0[0], c0[1] - r], [c0[0], c0[1] + r], 2.0, SECTOR);
                    let (dx, dy) = (self.pointer[0] - c0[0], self.pointer[1] - c0[1]);
                    let k = (r / (dx * dx + dy * dy).sqrt().max(f32::EPSILON)).min(1.0);
                    list.disc([c0[0] + dx * k, c0[1] + dy * k], size * 0.6, ACCENT);
                }
            }
            text.label(
                list,
                &c.name,
                [ring.center[0], ring.center[1] - ring.outer - size],
                width * 2.0,
                size,
                LABEL,
            );
            return;
        }
        list.disc(ring.center, ring.outer + 4.0, BACKGROUND);
        let n = page.controls.len();
        for (i, c) in page.controls.iter().enumerate() {
            let step = TAU / n as f32;
            let on = matches!(c.kind, ControlKind::Toggle | ControlKind::Button)
                && c.parameter.is_some()
                && c.value != 0.0
                && value(c.parameter) == c.value;
            let color = match (self.hover == Some(Hover::Control(i)), on) {
                (true, _) => HOVERED,
                (false, true) => ON,
                (false, false) => SECTOR,
            };
            let gap = GAP.min(step / 4.0);
            list.sector(
                ring.center,
                ring.inner,
                ring.outer,
                i as f32 * step + gap,
                (i + 1) as f32 * step - gap,
                color,
            );
            let a = Ring::middle(n, i);
            let at = ring.at(mid, a);
            match c.icon {
                Some(icon) => {
                    let s = (ring.outer - ring.inner) * 0.45;
                    let top = at[1] - s * 0.75;
                    let (min, max) = ([at[0] - s / 2.0, top], [at[0] + s / 2.0, top + s]);
                    list.rect(min, max, [0.0; 2], [1.0; 2], [1.0; 4], Texture::Icon(icon));
                    text.label(list, &c.name, [at[0], top + s + size * 0.8], width, size, LABEL);
                }
                None => text.label(list, &c.name, at, width, size, LABEL),
            }
        }
        let centre = if self.hover == Some(Hover::Centre) {
            HOVERED
        } else {
            SECTOR
        };
        list.disc(ring.center, ring.inner - 4.0, centre);
        let back = if self.path.is_empty() { "Close" } else { "Back" };
        text.label(list, back, ring.center, ring.inner * 1.8, size, LABEL);
    }
}

/// The page at `path` (submenu indices from the root), if every step is a submenu.
fn page<'m>(menu: &'m Menu, path: &[usize]) -> Option<&'m Menu> {
    path.iter().try_fold(menu, |m, &i| match &m.controls.get(i)?.kind {
        ControlKind::SubMenu(sub) => Some(sub),
        _ => None,
    })
}

fn write(p: Option<EntryIndex>, v: f32, writes: &mut Writes) {
    if let Some(p) = p {
        writes.push((p, v));
    }
}

/// A puppet's axis values from the pointer's offset in outer radii (x right, y up), clamped to the unit disc.
fn puppet(c: &Control, offset: [f32; 2], writes: &mut Writes) {
    let len = (offset[0] * offset[0] + offset[1] * offset[1]).sqrt();
    let [x, y] = offset.map(|o| if len > 1.0 { o / len } else { o });
    let axes: &[f32] = match c.kind {
        ControlKind::TwoAxisPuppet => &[x, y],
        ControlKind::FourAxisPuppet => &[y.max(0.0), x.max(0.0), (-y).max(0.0), (-x).max(0.0)],
        ControlKind::RadialPuppet => &[x.atan2(y).rem_euclid(TAU) / TAU],
        _ => &[],
    };
    for (p, v) in c.sub_parameters.iter().zip(axes) {
        write(*p, *v, writes);
    }
}

#[cfg(test)]
mod tests;
