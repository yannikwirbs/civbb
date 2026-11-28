//! The character controller over the test world (ADR 0110, #144): stairs and the 30° ramp climb to the platform on
//! the ground all the way, the 60° ramp does not, walls slide, edges drop and stairs lead back down; a prone body
//! crawls under a low slab and gets up only past it (ADR 0118).

use civbb_client::controller::{Body, HEIGHT, RADIUS, WorldMesh};
use civbb_core::math::Pose;
use civbb_corpus::world::{FRONT, PLATFORM, STEP};
use civbb_format::math::Vec3;

const DT: f32 = 1.0 / 60.0;

fn world() -> WorldMesh {
    WorldMesh::from_document(&civbb_corpus::world(), Pose::IDENTITY)
}

fn body(pos: Vec3) -> Body {
    Body {
        pos,
        vertical: 0.0,
        grounded: true,
        height: HEIGHT,
        scale: 1.0,
    }
}

/// Steps `b` for `seconds` walking at `walk`; returns every position and grounded state on the way.
fn walk(w: &WorldMesh, b: &mut Body, walk: Vec3, seconds: f32) -> Vec<(Vec3, bool)> {
    (0..(seconds / DT).round() as usize)
        .map(|_| {
            b.step(w, walk, false, DT);
            (b.pos, b.grounded)
        })
        .collect()
}

#[test]
fn stairs_climb_to_the_platform_on_the_ground() {
    let w = world();
    let mut b = body([-3.0, 0.0, 3.0]);
    let path = walk(&w, &mut b, [0.0, 0.0, 2.0], 2.5);
    assert!(path.iter().all(|&(_, g)| g), "left the ground on the stairs");
    // Never higher than the step the feet are over (within a capsule radius of its edge), never sinking.
    for (k, &(p, _)) in path.iter().enumerate().skip(1) {
        assert!(p[1] >= path[k - 1].0[1] - 1e-4, "{p:?} sank");
        assert!(p[1] <= PLATFORM + 1e-4, "{p:?}");
    }
    let [x, y, z] = b.pos;
    assert!(
        (x + 3.0).abs() < 1e-3 && (y - PLATFORM).abs() < 1e-3 && (z - 8.0).abs() < 0.05,
        "{:?}",
        b.pos
    );
    // Halfway up: on the second or third step.
    let mid = path
        .iter()
        .find(|(p, _)| p[2] >= FRONT - 0.75)
        .expect("reached the stairs")
        .0;
    assert!(mid[1] >= 2.0 * STEP - 1e-3 && mid[1] <= 4.0 * STEP + 1e-3, "{mid:?}");
}

#[test]
fn the_walkable_ramp_climbs_at_full_speed() {
    let w = world();
    let mut b = body([1.0, 0.0, 3.0]);
    let path = walk(&w, &mut b, [0.0, 0.0, 2.0], 2.5);
    assert!(path.iter().all(|&(_, g)| g), "left the ground on the ramp");
    let [x, y, z] = b.pos;
    assert!(
        (x - 1.0).abs() < 1e-3 && (y - PLATFORM).abs() < 1e-3 && (z - 8.0).abs() < 0.05,
        "{:?}",
        b.pos
    );
    // On the slope the feet's sphere touches it a little uphill: height within the capsule's rounding of the
    // surface below.
    let start = FRONT - PLATFORM / 30f32.to_radians().tan();
    let mid = path
        .iter()
        .find(|(p, _)| p[2] >= start + 0.8)
        .expect("reached the ramp")
        .0;
    let surface = (mid[2] - start) * 30f32.to_radians().tan();
    let rise = RADIUS * (1.0 / 30f32.to_radians().cos() - 1.0);
    assert!(
        mid[1] >= surface - 1e-3 && mid[1] <= surface + rise + 1e-3,
        "{mid:?} over {surface}"
    );
}

#[test]
fn the_steep_ramp_is_not_climbed() {
    let w = world();
    let mut b = body([3.0, 0.0, 3.0]);
    walk(&w, &mut b, [0.0, 0.0, 2.0], 3.0);
    assert!(b.pos[1] < STEP + 0.05 && b.pos[2] < FRONT, "{b:?}");
}

#[test]
fn walls_slide() {
    let w = world();
    let mut b = body([5.0, 0.0, 1.0]);
    walk(&w, &mut b, [1.5, 0.0, 1.5], 3.0);
    let [x, y, z] = b.pos;
    assert!(x <= 6.0 - RADIUS + 1e-3 && x > 6.0 - RADIUS - 0.01, "{:?}", b.pos);
    // Along the wall at the speed's component along it.
    assert!(y.abs() < 1e-3 && (z - 5.5).abs() < 0.05 && b.grounded, "{:?}", b.pos);
}

#[test]
fn walking_off_the_platform_falls_and_lands() {
    let w = world();
    let mut b = body([0.0, PLATFORM, 9.0]);
    let path = walk(&w, &mut b, [0.0, 0.0, 2.0], 1.5);
    assert!(path.iter().any(|&(_, g)| !g), "never fell");
    assert!(b.grounded && b.pos[1].abs() < 1e-3 && b.pos[2] > 10.0, "{b:?}");
}

#[test]
fn stairs_lead_down_on_the_ground() {
    let w = world();
    let mut b = body([-3.0, PLATFORM, 7.0]);
    let path = walk(&w, &mut b, [0.0, 0.0, -2.0], 2.5);
    assert!(path.iter().all(|&(_, g)| g), "left the ground on the way down");
    assert!(b.pos[1].abs() < 1e-3 && (b.pos[2] - 2.0).abs() < 0.05, "{b:?}");
}

#[test]
fn prone_crawls_under_a_low_overhang_and_stands_up_only_past_it() {
    use civbb_client::player::{Input, Player, Posture};
    // The plane and a slab over x -1..1, z 1..3: its underside 0.8 m up, its front face up to 1.2 m.
    let mut w = WorldMesh::plane();
    let (x, low, high) = (1.0, 0.8, 1.2);
    w.triangles.extend([
        [[-x, low, 1.0], [x, low, 3.0], [-x, low, 3.0]],
        [[-x, low, 1.0], [x, low, 1.0], [x, low, 3.0]],
        [[-x, low, 1.0], [-x, high, 1.0], [x, high, 1.0]],
        [[-x, low, 1.0], [x, high, 1.0], [x, low, 1.0]],
    ]);
    let forward = Input {
        forward: 1.0,
        ..Default::default()
    };
    let run = |p: &mut Player, input: &Input, seconds: f32| {
        for _ in 0..(seconds / DT).round() as usize {
            p.step(input, DT, &w);
        }
    };
    // Standing and crouching, the slab's front stops the capsule (crouching, at its lower edge, 5 cm below the top
    // sphere's centre).
    for (posture, seconds) in [(Posture::Standing, 1.0), (Posture::Crouching, 2.0)] {
        let mut p = Player {
            posture,
            ..Default::default()
        };
        run(&mut p, &forward, seconds);
        let stop = 1.0 - RADIUS;
        assert!(
            p.pos[2] > stop - 1e-3 && p.pos[2] < stop + 0.01,
            "{posture:?}: {:?}",
            p.pos
        );
    }
    // Prone it passes under, on the ground.
    let mut p = Player {
        posture: Posture::Prone,
        ..Default::default()
    };
    run(&mut p, &forward, 4.0);
    assert!(
        p.grounded && p.pos[1].abs() < 1e-4 && (p.pos[2] - 2.0).abs() < 0.05,
        "{:?}",
        p.pos
    );
    // Beneath it neither standing nor crouching fits.
    for input in [
        Input {
            prone: true,
            ..Default::default()
        },
        Input {
            crouch: true,
            ..Default::default()
        },
    ] {
        p.step(&input, DT, &w);
        assert_eq!(p.posture, Posture::Prone, "{input:?}");
    }
    // Out from under it, it stands.
    run(&mut p, &forward, 3.0);
    assert!(p.pos[2] > 3.0 + RADIUS, "{:?}", p.pos);
    p.step(
        &Input {
            prone: true,
            ..Default::default()
        },
        DT,
        &w,
    );
    assert_eq!(p.posture, Posture::Standing);
}
