//! The desktop expression menu (ADR 0117): the player avatar's radial menu, driven by the mouse or a stick (#158),
//! writing the player island's parameters and drawn into the [`Overlay`] that the extract step hands to the
//! renderer. The client appends its own locomotion page (ADR 0121): floor poses and flight, set on the [`Player`].

use std::sync::Arc;
use std::thread::JoinHandle;

use bevy_ecs::prelude::*;
use civbb_anim::defaults::POSE;
use civbb_format::expressions::{Control, ControlKind, Menu};
use civbb_format::state::EntryIndex;
use civbb_ui::draw::DrawList;
use civbb_ui::radial::{Pointer, Radial, Ring, Writes};
use civbb_ui::text::{Atlas, Text};

use crate::frame::Shows;
use crate::island::Island;
use crate::player::{FloorPose, Player, WorldRules};

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
    /// The avatar's menu with the locomotion page, and what it was built for: package id, floor poses, flight.
    menu: Option<Menu>,
    built: Option<(u64, bool, bool)>,
    /// The island's parameters followed by the locomotion page's two entries.
    params: Vec<f32>,
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
    mut player: ResMut<Player>,
    rules: Res<WorldRules>,
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
    // The locomotion page's entries come after the island's own.
    let first = island.params().len() as EntryIndex;
    let poses = island.model().controller().and_then(|c| c.state.find(POSE)).is_some();
    let built = Some((shows.0.id, poses, rules.flight));
    if m.built != built {
        let own = (shows.0.avatar.avatar()).and_then(|r| r.expressions.as_ref());
        (m.menu, m.built) = (with_locomotion(own.map(|x| &x.menu), first, poses, rules.flight), built);
    }
    let Some(page) = &m.menu else {
        m.radial = Radial::default();
        return;
    };
    fill(&mut m.params, &island, &player);
    m.writes.clear();
    if toggle {
        m.radial.toggle(page, &mut m.writes);
    }
    m.radial.update(page, &m.params, &pointer, m.screen, &mut m.writes);
    for &(p, v) in &m.writes {
        match p.checked_sub(first) {
            None => island.set_param(p, v),
            Some(0) => {
                player.floor_pose = match v as u8 {
                    1 => Some(FloorPose::Sitting),
                    2 => Some(FloorPose::Lying),
                    _ => None,
                }
            }
            Some(_) => {
                player.flying = v != 0.0;
            }
        }
    }
    let text = m.text.get_or_insert_with(Text::without_fonts);
    text.begin();
    fill(&mut m.params, &island, &player);
    m.radial.draw(page, &m.params, text, &mut overlay.list, m.screen);
    overlay.atlas = Some(Arc::clone(text.atlas()));
    overlay.icons = Some(e.to_bits());
}

/// The parameters the menu reads: the island's, then the locomotion page's floor pose and flight.
fn fill(params: &mut Vec<f32>, island: &Island, player: &Player) {
    params.clear();
    params.extend_from_slice(island.params());
    params.extend([FloorPose::value(player.floor_pose), f32::from(u8::from(player.flying))]);
}

/// The avatar's root menu (`own`) with a locomotion page appended where it has anything to offer: stand, sit and lie
/// on entry `first` (values 0, 1, 2 as [`FloorPose::value`]) when the avatar has [`POSE`], fly on `first + 1` where
/// the world allows it (ADR 0121). `None` without either menu.
fn with_locomotion(own: Option<&Menu>, first: EntryIndex, poses: bool, flight: bool) -> Option<Menu> {
    let control = |name: &str, kind, parameter, value| Control {
        name: name.into(),
        icon: None,
        kind,
        parameter,
        value,
        sub_parameters: Vec::new(),
    };
    let mut page = Vec::new();
    if poses {
        page.push(control("Stand", ControlKind::Button, Some(first), 0.0));
        page.push(control("Sit", ControlKind::Toggle, Some(first), 1.0));
        page.push(control("Lie down", ControlKind::Toggle, Some(first), 2.0));
    }
    if flight {
        page.push(control("Fly", ControlKind::Toggle, Some(first + 1), 1.0));
    }
    let mut menu = own.cloned();
    if !page.is_empty() {
        let page = ControlKind::SubMenu(Menu { controls: page });
        menu.get_or_insert_default()
            .controls
            .push(control("Locomotion", page, None, 0.0));
    }
    menu
}
