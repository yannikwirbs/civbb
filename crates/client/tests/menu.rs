//! The expression menu on the frame schedule (ADR 0117): simulated mouse input opens the lean corpus avatar's menu,
//! enters its page of toggles and flips one, which the avatar's animator then applies; the extract step hands the
//! drawn menu to the renderer. A stick does the same (#158).

use civbb_client::extract::Snapshot;
use civbb_client::frame::Runtime;
use civbb_client::menu::ExpressionMenu;
use civbb_client::package::Package;
use civbb_client::player::Player;
use civbb_core::math::Pose;
use civbb_format::expressions::ControlKind;
use civbb_ui::draw::Texture;
use civbb_ui::radial::{Pointer, Ring};

const DT: f32 = 1.0 / 60.0;
const SCREEN: [f32; 2] = [1280.0, 720.0];

/// A press on sector `i` of `n`.
fn press_on(n: usize, i: usize) -> Pointer {
    let ring = Ring::new(SCREEN);
    Pointer {
        pos: ring.at((ring.inner + ring.outer) / 2.0, Ring::middle(n, i)),
        press: true,
        ..Default::default()
    }
}

/// A stick pushed 0.8 of the way towards sector `i` of `n`.
fn stick_to(n: usize, i: usize) -> [f32; 2] {
    let a = Ring::middle(n, i);
    [0.8 * a.sin(), 0.8 * a.cos()]
}

/// A frame with the stick at `stick`, pressing or not.
fn point(runtime: &mut Runtime, stick: [f32; 2], press: bool) {
    let mut m = runtime.world.resource_mut::<ExpressionMenu>();
    // The mouse sits elsewhere: the stick decides.
    m.pointer = Pointer {
        pos: [0.0; 2],
        press,
        ..Default::default()
    };
    m.stick = Some(stick);
    runtime.step(DT);
}

#[test]
fn a_stick_enters_a_page_and_flips_a_toggle() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let root = &package.avatar.avatar().unwrap().expressions.as_ref().unwrap().menu;
    let ControlKind::SubMenu(page) = &root.controls[0].kind else {
        panic!("the root holds pages of toggles")
    };
    let (pages, toggles) = (root.controls.len(), page.controls.len());
    let entry = page.controls[0].parameter.unwrap() as usize;
    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.add_menu();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    runtime.world.resource_mut::<ExpressionMenu>().screen = SCREEN;
    runtime.step(DT);
    let before = runtime.islands()[0].params()[entry];
    runtime.world.resource_mut::<ExpressionMenu>().toggle = true;
    // Centred, a press hovers the centre disc: back, which on the root page closes the menu.
    point(&mut runtime, [0.0; 2], false);
    point(&mut runtime, [0.1, 0.0], true);
    assert!(!runtime.world.resource::<ExpressionMenu>().radial.is_open());
    runtime.world.resource_mut::<ExpressionMenu>().toggle = true;
    point(&mut runtime, stick_to(pages, 0), true);
    point(&mut runtime, stick_to(toggles, 0), false);
    assert_eq!(
        runtime.islands()[0].params()[entry],
        before,
        "pointing alone flips nothing"
    );
    point(&mut runtime, stick_to(toggles, 0), true);
    assert_eq!(runtime.islands()[0].params()[entry], 1.0 - before);
    assert!(runtime.world.resource::<ExpressionMenu>().radial.is_open());
}

#[test]
fn the_mouse_opens_the_menu_and_flips_a_toggle() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let avatar = &package.avatar;
    let root = &avatar
        .avatar()
        .unwrap()
        .expressions
        .as_ref()
        .expect("lean has a menu")
        .menu;
    let ControlKind::SubMenu(page) = &root.controls[0].kind else {
        panic!("the root holds pages of toggles")
    };
    let (toggle, pages, toggles) = (&page.controls[0], root.controls.len(), page.controls.len());
    assert_eq!(toggle.kind, ControlKind::Toggle);
    let entry = toggle.parameter.unwrap() as usize;
    let node = (avatar.hierarchy.nodes.iter())
        .position(|n| n.name == avatar.state.entries[entry].name)
        .expect("each toggle is named after its node");

    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.add_menu();
    runtime.extract_to_snapshot();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    runtime.world.resource_mut::<ExpressionMenu>().screen = SCREEN;
    runtime.step(DT);
    let before = runtime.islands()[0].active()[node];
    assert!(
        runtime.world.resource::<Snapshot>().overlay.list.vertices.is_empty(),
        "closed"
    );

    runtime.world.resource_mut::<ExpressionMenu>().toggle = true;
    runtime.step(DT);
    assert!(runtime.world.resource::<ExpressionMenu>().radial.is_open());
    runtime.world.resource_mut::<ExpressionMenu>().pointer = press_on(pages, 0);
    runtime.step(DT);
    runtime.world.resource_mut::<ExpressionMenu>().pointer = press_on(toggles, 0);
    for _ in 0..5 {
        runtime.step(DT);
    }
    let island = &runtime.islands()[0];
    assert_eq!(island.params()[entry], if before { 0.0 } else { 1.0 });
    assert_eq!(island.active()[node], !before, "the animator applied the toggle");
    let overlay = &runtime.world.resource::<Snapshot>().overlay;
    assert!(
        overlay.list.vertices.iter().any(|v| v.texture == Texture::None),
        "the ring is drawn"
    );
    assert_eq!(overlay.icons, Some(e.to_bits()));

    runtime.world.resource_mut::<ExpressionMenu>().toggle = true;
    runtime.step(DT);
    assert!(!runtime.world.resource::<ExpressionMenu>().radial.is_open());
    assert!(runtime.world.resource::<Snapshot>().overlay.list.vertices.is_empty());
    assert_eq!(runtime.islands()[0].active()[node], !before, "toggles outlast the menu");
}
