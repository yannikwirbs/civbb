//! The VR player on the frame schedule (ADR 0125), driven by simulated headset input: the left stick walks where the
//! head looks (or the left hand points), the right stick snap-turns 30° per push about the head, the head's room-scale
//! walk carries the capsule until a wall stops it while the view stays on the capsule, the head's height over the
//! calibrated one sets the posture, the platform values say VR, and a seat holds the play space.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_6, TAU};

use civbb_client::controller::{RADIUS, WorldMesh};
use civbb_client::extract::Camera;
use civbb_client::frame::Runtime;
use civbb_client::gamepad::{SNAP, Turning};
use civbb_client::package::Package;
use civbb_client::player::{Input, Player, Posture, WALK};
use civbb_client::station::Station;
use civbb_client::tracking::Trackers;
use civbb_client::vr::{HeadsetPoses, Steer, Vr};
use civbb_core::math::{Pose, length, rotate, sub};
use civbb_format::state::{Entry, Platform};
use civbb_xr::{Poses, xr};

const DT: f32 = 1.0 / 60.0;
/// The user's eyes over the floor of the play space.
const EYES: f32 = 1.6;

fn close(a: [f32; 3], b: [f32; 3], within: f32) -> bool {
    length(sub(a, b)) < within
}

/// A play-space pose (floor at y 0, forward -Z) at `pos`, turned by `yaw` about +Y (positive to the left).
fn pose(pos: [f32; 3], yaw: f32) -> xr::Posef {
    let (s, c) = (yaw / 2.0).sin_cos();
    xr::Posef {
        orientation: xr::Quaternionf {
            x: 0.0,
            y: s,
            z: 0.0,
            w: c,
        },
        position: xr::Vector3f {
            x: pos[0],
            y: pos[1],
            z: pos[2],
        },
    }
}

/// A head at the play space's centre looking ahead, hands at its sides.
fn standing() -> Poses {
    Poses {
        head: Some(pose([0.0, EYES, 0.0], 0.0)),
        grips: [Some(pose([-0.7, 1.4, 0.0], 0.0)), Some(pose([0.7, 1.4, 0.0], 0.0))],
        ..Poses::default()
    }
}

/// The lean avatar as a VR player in `world`, with `Upright` declared on it: the runtime and the parameter's index.
fn vr_player(world: WorldMesh, vr: Vr) -> (Runtime, usize) {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let upright = avatar.state.entries.len();
    let entries = [Platform::Upright, Platform::VrMode, Platform::TrackingType];
    avatar
        .state
        .entries
        .extend(entries.map(|p| Entry::new(p.name(), p.kind(), 0.0)));
    let package = Package::new(avatar, Vec::new());
    let mut runtime = Runtime::default();
    runtime.world.insert_resource(world);
    runtime.add_player();
    runtime.add_headset(vr);
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    (runtime, upright)
}

/// One frame: the keys move nothing, as the viewer writes them each frame, and the headset is at `poses`.
fn frame(runtime: &mut Runtime, poses: Poses) {
    let mut input = runtime.world.resource_mut::<Input>();
    (input.forward, input.right) = (0.0, 0.0);
    runtime.world.insert_resource(HeadsetPoses(poses));
    runtime.step(DT);
}

/// Where the headset's eyes are in the world, as the render thread places them.
fn eye(runtime: &Runtime, poses: &Poses) -> [f32; 3] {
    let stage = runtime
        .world
        .resource::<Camera>()
        .stage
        .expect("the VR player places the stage");
    let p = poses.head.expect("tracked").position;
    stage.point([p.x, p.y, p.z])
}

/// The player's position after a second of the left stick fully forward.
fn walk(runtime: &mut Runtime, poses: Poses) -> [f32; 3] {
    let start = runtime.world.resource::<Player>().pos;
    for _ in 0..60 {
        frame(
            runtime,
            Poses {
                sticks: [[0.0, 1.0], [0.0; 2]],
                ..poses
            },
        );
    }
    sub(runtime.world.resource::<Player>().pos, start)
}

#[test]
fn the_left_stick_walks_where_the_head_looks_or_the_hand_points() {
    let (mut runtime, _) = vr_player(WorldMesh::plane(), Vr::default());
    // Ahead: the avatar's forward, +Z.
    let went = walk(&mut runtime, standing());
    assert!(close(went, [0.0, 0.0, WALK], 1e-3), "{went:?}");
    // The head turned 30° left, short of turning the body: 30° left of the body, towards +X.
    let mut poses = standing();
    poses.head = Some(pose([0.0, EYES, 0.0], FRAC_PI_6));
    let went = walk(&mut runtime, poses);
    let (s, c) = FRAC_PI_6.sin_cos();
    assert!(close(went, [WALK * s, 0.0, WALK * c], 1e-3), "{went:?}");
    assert_eq!(runtime.world.resource::<Player>().yaw, 0.0);
    // Steering by the hand: the left controller points 30° right while the head looks ahead.
    runtime.world.resource_mut::<Vr>().steer = Steer::Hand;
    let mut poses = standing();
    poses.aims[0] = Some(pose([-0.2, 1.2, -0.3], -FRAC_PI_6));
    let went = walk(&mut runtime, poses);
    assert!(close(went, [-WALK * s, 0.0, WALK * c], 1e-3), "{went:?}");
}

#[test]
fn the_body_turns_after_the_head_past_45_degrees_and_the_room_stays() {
    let (mut runtime, _) = vr_player(WorldMesh::plane(), Vr::default());
    frame(&mut runtime, standing());
    let mut poses = standing();
    poses.head = Some(pose([0.0, EYES, 0.0], FRAC_PI_2));
    let before = eye(&runtime, &poses);
    frame(&mut runtime, poses);
    let yaw = runtime.world.resource::<Player>().yaw;
    assert!((yaw - FRAC_PI_2 / 2.0).abs() < 1e-4, "{yaw}");
    // The same head pose is seen from the same place: the play space turned back by what the body turned.
    assert!(close(eye(&runtime, &poses), before, 1e-4));
}

#[test]
fn snap_turns_step_30_degrees_per_push_about_the_head() {
    let (mut runtime, _) = vr_player(WorldMesh::plane(), Vr::new(Turning::Snap(SNAP), Steer::Head));
    // The user stands 0.4 m off the play space's centre: room scale puts the avatar under the head first.
    let mut poses = standing();
    poses.head = Some(pose([0.4, EYES, 0.0], 0.0));
    frame(&mut runtime, poses);
    let at = eye(&runtime, &poses);
    let push = Poses {
        sticks: [[0.0; 2], [1.0, 0.0]],
        ..poses
    };
    for _ in 0..10 {
        frame(&mut runtime, push);
    }
    let yaw = runtime.world.resource::<Player>().yaw;
    assert!((yaw - (TAU - SNAP)).abs() < 1e-4, "one step right while held: {yaw}");
    // The view turned about the head: it stays where it was, looking 30° right.
    assert!(close(eye(&runtime, &poses), at, 1e-4));
    let stage = runtime.world.resource::<Camera>().stage.expect("placed");
    let ahead = rotate(stage.rot, [0.0, 0.0, -1.0]);
    assert!(close(ahead, [-SNAP.sin(), 0.0, SNAP.cos()], 1e-4), "{ahead:?}");
    frame(&mut runtime, poses);
    frame(&mut runtime, push);
    let yaw = runtime.world.resource::<Player>().yaw;
    assert!(
        (yaw - (TAU - 2.0 * SNAP)).abs() < 1e-4,
        "a second push, a second step: {yaw}"
    );
}

#[test]
fn the_head_walks_into_a_wall_and_stops_with_the_view_on_the_capsule() {
    // A wall across +Z, 1 m ahead of the avatar.
    let mut world = WorldMesh::plane();
    let (a, b, c, d) = ([-5.0, 0.0, 1.0], [5.0, 0.0, 1.0], [5.0, 3.0, 1.0], [-5.0, 3.0, 1.0]);
    world.triangles.extend([[a, b, c], [a, c, d]]);
    let (mut runtime, _) = vr_player(world, Vr::default());
    // The user walks 2 m forward in the room (-Z in the play space), 1 cm a frame.
    let mut poses = standing();
    let mut nearest = f32::MAX;
    for i in 0..=200 {
        poses.head = Some(pose([0.0, EYES, -0.01 * i as f32], 0.0));
        frame(&mut runtime, poses);
        let (eye, pos) = (eye(&runtime, &poses), runtime.world.resource::<Player>().pos);
        assert!(
            close([eye[0], 0.0, eye[2]], pos, 1e-4),
            "frame {i}: the eye {eye:?} over the capsule {pos:?}"
        );
        nearest = nearest.min(1.0 - eye[2]);
    }
    let pos = runtime.world.resource::<Player>().pos;
    assert!((pos[2] - (1.0 - RADIUS)).abs() < 0.02, "stopped at the wall: {pos:?}");
    assert!(nearest > RADIUS - 0.02, "the view never passed the capsule: {nearest}");
}

#[test]
fn the_heads_height_sets_the_posture_and_the_platform_values_say_vr() {
    let (mut runtime, upright) = vr_player(WorldMesh::plane(), Vr::default());
    let params = |runtime: &mut Runtime| {
        (runtime.island_mut(0).animator_mut().expect("an animator").params[upright..upright + 3]).to_vec()
    };
    frame(&mut runtime, standing());
    // Before calibration: standing, VR, head and hands.
    assert_eq!(params(&mut runtime), [1.0, 1.0, 3.0]);
    assert!(runtime.islands()[0].hide_head);
    frame(
        &mut runtime,
        Poses {
            calibrate: true,
            ..standing()
        },
    );
    assert_eq!(runtime.world.resource::<Trackers>().calibrations.len(), 1);
    // The head at half its standing height: crouching; a fifth: prone.
    for (height, posture) in [
        (0.5, Posture::Crouching),
        (0.2, Posture::Prone),
        (1.0, Posture::Standing),
    ] {
        let mut poses = standing();
        poses.head = Some(pose([0.0, EYES * height, 0.0], 0.0));
        frame(&mut runtime, poses);
        assert_eq!(runtime.world.resource::<Player>().posture, posture);
        let upright_value = params(&mut runtime)[0];
        assert!((upright_value - height).abs() < 1e-4, "{upright_value}");
    }
    assert_eq!(runtime.world.resource::<Input>().height, Some(1.0));
}

#[test]
fn a_seat_holds_the_play_space_while_the_head_moves_freely() {
    let (mut runtime, _) = vr_player(WorldMesh::plane(), Vr::default());
    let seat = Station {
        seat: [0.5, 0.0, 0.5],
        yaw: FRAC_PI_2,
        exit: [0.5, 0.0, 1.5],
        seated: true,
    };
    runtime.world.spawn(seat);
    frame(&mut runtime, standing());
    runtime.world.resource_mut::<Input>().interact = true;
    frame(&mut runtime, standing());
    assert_eq!(runtime.world.resource::<Player>().pos, seat.seat);
    let at = eye(&runtime, &standing());
    // Leaning 20 cm to the right: the seat stays, the view moves 20 cm to the avatar's right (+Z, facing +X).
    let mut poses = standing();
    poses.head = Some(pose([0.2, EYES, 0.0], 0.0));
    frame(&mut runtime, poses);
    assert_eq!(runtime.world.resource::<Player>().pos, seat.seat);
    let moved = sub(eye(&runtime, &poses), at);
    assert!(close(moved, [0.0, 0.0, 0.2], 1e-4), "{moved:?}");
}
