//! The desktop expression menu (ADR 0117): the player avatar's radial menu, driven by the mouse or a stick (#158),
//! writing the player island's parameters and drawn into the [`Overlay`] that the extract step hands to the
//! renderer.

use std::sync::Arc;
use std::thread::JoinHandle;

use bevy_ecs::prelude::*;
use civbb_ui::draw::DrawList;
use civbb_ui::radial::{Pointer, Radial, Ring, Writes};
use civbb_ui::text::{Atlas, Text};

use crate::frame::Shows;
use crate::island::Island;
use crate::player::Player;

/// The menu's state and this frame's input.
#[derive(Resource, Default)]
pub struct ExpressionMenu {
    /// Open pages, held controls.
    pub radial: Radial,
    /// The mouse this frame; the viewer sets it, the system clears its presses after reading them.
    pub pointer: Pointer,
    /// A stick pointing instead of the mouse (a pad's right stick, later a controller's thumbstick): its
    /// deflection, x right and y up ([`Ring::stick`]); `None` points with [`ExpressionMenu::pointer`]'s position.
    pub stick: Option<[f32; 2]>,
    /// The window's size in pixels.
    pub screen: [f32; 2],
    /// Open or close at the next frame (Tab, a pad's Start).
    pub toggle: bool,
    /// Labels; without fonts until [`ExpressionMenu::load_fonts`] is done.
    text: Option<Text>,
    fonts: Option<JoinHandle<Text>>,
    writes: Writes,
}

impl ExpressionMenu {
    /// Scans the system's fonts on a thread of its own (over a second, ADR 0116); labels show once it is done.
    pub fn load_fonts(&mut self) {
        self.fonts = Some(std::thread::spawn(Text::new));
    }
}

/// What the overlay pass draws, copied into the snapshot by the extract step.
#[derive(Resource, Clone, Debug, Default)]
pub struct Overlay {
    /// Triangles in window pixels.
    pub list: DrawList,
    /// The glyph atlas the labels sample.
    pub atlas: Option<Arc<Atlas>>,
    /// The island (by key) whose package's textures [`civbb_ui::draw::Texture::Icon`] indexes.
    pub icons: Option<u64>,
}

/// The menu system of [`civbb_core::schedule::Step::PosesIn`]: opens, closes and drives the player avatar's menu,
/// applies its writes to the player's island before it steps, and draws it into the [`Overlay`].
pub(crate) fn menu_system(
    mut menu: ResMut<ExpressionMenu>,
    player: Res<Player>,
    mut q: Query<(&Shows, &mut Island)>,
    mut overlay: ResMut<Overlay>,
) {
    let m = &mut *menu;
    if m.fonts.as_ref().is_some_and(JoinHandle::is_finished) {
        m.text = m.fonts.take().and_then(|f| f.join().ok());
    }
    overlay.list.clear();
    let mut pointer = m.pointer;
    m.pointer = Pointer {
        pos: pointer.pos,
        ..Default::default()
    };
    if let Some(s) = m.stick {
        pointer.pos = Ring::new(m.screen).stick(s);
    }
    let toggle = std::mem::take(&mut m.toggle);
    let Some((e, (shows, mut island))) = player.island.and_then(|e| Some((e, q.get_mut(e).ok()?))) else {
        return;
    };
    let Some(page) = (shows.0.avatar.avatar())
        .and_then(|r| r.expressions.as_ref())
        .map(|x| &x.menu)
    else {
        m.radial = Radial::default();
        return;
    };
    m.writes.clear();
    if toggle {
        m.radial.toggle(page, &mut m.writes);
    }
    m.radial
        .update(page, island.params(), &pointer, m.screen, &mut m.writes);
    for &(p, v) in &m.writes {
        island.set_param(p, v);
    }
    let text = m.text.get_or_insert_with(Text::without_fonts);
    text.begin();
    m.radial.draw(page, island.params(), text, &mut overlay.list, m.screen);
    overlay.atlas = Some(Arc::clone(text.atlas()));
    overlay.icons = Some(e.to_bits());
}
