//! Tests of the radial menu: pointer input to parameter writes, and what an open page draws.

use super::*;

const SCREEN: [f32; 2] = [1000.0, 800.0];

fn control(name: &str, kind: ControlKind, parameter: Option<EntryIndex>, subs: &[EntryIndex]) -> Control {
    Control {
        name: name.into(),
        icon: None,
        kind,
        parameter,
        value: 1.0,
        sub_parameters: subs.iter().map(|&p| Some(p)).collect(),
    }
}

/// Root: a toggle (entry 0), a button (1) and a submenu (2) holding a two-axis (3; 4, 5), a four-axis (no
/// parameter; 6 to 9) and a radial puppet (10; 11).
fn menu() -> Menu {
    let sub = Menu {
        controls: vec![
            control("Move", ControlKind::TwoAxisPuppet, Some(3), &[4, 5]),
            control("Face", ControlKind::FourAxisPuppet, None, &[6, 7, 8, 9]),
            control("Dial", ControlKind::RadialPuppet, Some(10), &[11]),
        ],
    };
    Menu {
        controls: vec![
            control("Hat", ControlKind::Toggle, Some(0), &[]),
            control("Wave", ControlKind::Button, Some(1), &[]),
            control("More", ControlKind::SubMenu(sub), Some(2), &[]),
        ],
    }
}

/// A press on sector `i` of `n`.
fn press_on(n: usize, i: usize) -> Pointer {
    let ring = Ring::new(SCREEN);
    Pointer {
        pos: ring.at((ring.inner + ring.outer) / 2.0, Ring::middle(n, i)),
        press: true,
        ..Default::default()
    }
}

/// The pointer `offset` outer radii from the centre (x right, y up), no buttons.
fn hover_at(offset: [f32; 2]) -> Pointer {
    let ring = Ring::new(SCREEN);
    Pointer {
        pos: [
            ring.center[0] + offset[0] * ring.outer,
            ring.center[1] - offset[1] * ring.outer,
        ],
        ..Default::default()
    }
}

fn assert_near(got: &[(EntryIndex, f32)], want: &[(EntryIndex, f32)], what: &str) {
    let near = got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| g.0 == w.0 && (g.1 - w.1).abs() < 1e-5);
    assert!(near, "{what}: {got:?}, want {want:?}");
}

fn step(r: &mut Radial, m: &Menu, params: &[f32], p: Pointer) -> Writes {
    let mut w = Writes::new();
    r.update(m, params, &p, SCREEN, &mut w);
    w
}

#[test]
fn a_toggle_writes_its_value_then_zero() {
    let (m, mut r) = (menu(), Radial::default());
    assert!(
        step(&mut r, &m, &[0.0; 12], press_on(3, 0)).is_empty(),
        "closed menus ignore the pointer"
    );
    r.open();
    assert_eq!(step(&mut r, &m, &[0.0; 12], press_on(3, 0)), vec![(0, 1.0)]);
    let mut on = [0.0; 12];
    on[0] = 1.0;
    assert_eq!(step(&mut r, &m, &on, press_on(3, 0)), vec![(0, 0.0)]);
}

#[test]
fn a_button_holds_its_value_until_release() {
    let (m, mut r) = (menu(), Radial::default());
    r.open();
    assert_eq!(step(&mut r, &m, &[0.0; 12], press_on(3, 1)), vec![(1, 1.0)]);
    assert_eq!(r.hover(), Some(Hover::Control(1)));
    assert!(step(&mut r, &m, &[0.0; 12], hover_at([0.0, 0.5])).is_empty());
    let release = Pointer {
        release: true,
        ..hover_at([0.0, 0.5])
    };
    assert_eq!(step(&mut r, &m, &[0.0; 12], release), vec![(1, 0.0)]);
}

#[test]
fn puppets_map_the_pointer_and_back_leaves_them_then_the_page() {
    let (m, mut r) = (menu(), Radial::default());
    let p = [0.0; 12];
    r.open();
    assert_eq!(
        step(&mut r, &m, &p, press_on(3, 2)),
        vec![(2, 1.0)],
        "the submenu holds its parameter"
    );
    assert_eq!(step(&mut r, &m, &p, press_on(3, 0)), vec![(3, 1.0)]);
    let w = step(&mut r, &m, &p, hover_at([0.5, 1.0]));
    let (x, y) = (0.5 / 1.25f32.sqrt(), 1.0 / 1.25f32.sqrt());
    assert_near(&w, &[(4, x), (5, y)], "clamped to the unit disc");
    let back = Pointer {
        back: true,
        ..hover_at([0.0, 0.0])
    };
    assert_eq!(step(&mut r, &m, &p, back), vec![(3, 0.0), (4, 0.0), (5, 0.0)]);

    assert!(
        step(&mut r, &m, &p, press_on(3, 1)).is_empty(),
        "the four-axis puppet has no parameter"
    );
    let w = step(&mut r, &m, &p, hover_at([-0.5, 0.25]));
    assert_near(&w, &[(6, 0.25), (7, 0.0), (8, 0.0), (9, 0.5)], "up, right, down, left");
    step(&mut r, &m, &p, back);

    step(&mut r, &m, &p, press_on(3, 2));
    assert_near(
        &step(&mut r, &m, &p, hover_at([0.3, 0.0])),
        &[(11, 0.25)],
        "a quarter turn clockwise",
    );
    assert_eq!(step(&mut r, &m, &p, back), vec![(10, 0.0)], "the dial keeps its value");

    let centre = Pointer {
        press: true,
        ..hover_at([0.0, 0.0])
    };
    assert_eq!(
        step(&mut r, &m, &p, centre),
        vec![(2, 0.0)],
        "the centre goes back to the root"
    );
    assert!(r.is_open());
    assert!(step(&mut r, &m, &p, centre).is_empty());
    assert!(!r.is_open(), "the root's centre closes");
}

#[test]
fn closing_leaves_every_page_and_puppet() {
    let (m, mut r) = (menu(), Radial::default());
    let p = [0.0; 12];
    r.open();
    step(&mut r, &m, &p, press_on(3, 2));
    step(&mut r, &m, &p, press_on(3, 0));
    let mut w = Writes::new();
    r.toggle(&m, &mut w);
    assert_eq!(w, vec![(3, 0.0), (4, 0.0), (5, 0.0), (2, 0.0)]);
    assert!(!r.is_open());
    r.toggle(&m, &mut w);
    assert!(r.is_open());
    assert_eq!(
        step(&mut r, &m, &p, press_on(3, 0)),
        vec![(0, 1.0)],
        "reopens at the root"
    );
}

#[test]
fn the_page_draws_a_sector_per_control_and_icons() {
    let mut m = menu();
    m.controls[1].icon = Some(4);
    let mut r = Radial::default();
    let mut text = Text::without_fonts();
    let mut list = DrawList::default();
    r.draw(&m, &[0.0; 12], &mut text, &mut list, SCREEN);
    assert!(list.vertices.is_empty(), "closed");
    r.open();
    r.draw(&m, &[0.0; 12], &mut text, &mut list, SCREEN);
    let icons: Vec<_> = list.vertices.iter().filter(|v| v.texture == Texture::Icon(4)).collect();
    assert_eq!(icons.len(), 6);
    let ring = Ring::new(SCREEN);
    assert!(
        icons.iter().all(|v| v.pos[1] > ring.center[1]),
        "the second of three sectors is at the bottom"
    );
}
