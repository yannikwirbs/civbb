//! Tracker input on the frame schedule (ADR 0113): the scripted source calibrates the player's avatar on its first
//! frame, then the player island's IK targets follow the devices through that calibration; other islands keep none.
//! The headset source (ADR 0124) does the same from play-space poses, as the XR loop sets them.

use civbb_anim::ik::{Goal, Targets};
use civbb_anim::tracking::{self, Role};
use civbb_client::frame::Runtime;
use civbb_client::package::Package;
use civbb_client::player::Player;
use civbb_client::tracking::{HeadsetPoses, Scripted, Trackers, USER};
use civbb_core::math::{Pose, add, length, mul, scale, sub};
use civbb_xr::{Poses, xr};

#[test]
fn scripted_trackers_calibrate_then_drive_the_players_targets() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.add_trackers(Scripted::new(10));
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.spawn_package(&package, 1, false, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    runtime.step(1.0 / 60.0);
    let calibration = *(runtime.world.resource::<Trackers>().calibrations.get(&package.id)).expect("calibrated");
    assert!((calibration.ratio - 1.0 / USER).abs() < 1e-5, "{}", calibration.ratio);
    // The first frame is the rest pose: every goal is its bone at rest.
    let rest = tracking::rest(&package.avatar).expect("a humanoid");
    let targets = runtime.islands()[0].targets;
    for (role, goal) in [
        (Role::Head, targets.head),
        (Role::LeftHand, targets.hands[0]),
        (Role::Hips, targets.hips),
        (Role::Chest, targets.chest),
        (Role::RightFoot, targets.feet[1]),
    ] {
        let (got, want) = (goal.expect("tracked"), rest[role as usize].expect("mapped"));
        assert!(length(sub(got.pos, want.pos)) < 1e-4, "{role:?} at {:?}", got.pos);
    }
    assert!(targets.elbows.iter().chain(&targets.knees).all(Option::is_some));
    assert_eq!(runtime.islands()[1].targets, Targets::default());
    // Later frames follow the devices.
    for _ in 0..30 {
        runtime.step(1.0 / 60.0);
    }
    let devices = runtime.world.resource::<Trackers>().devices;
    assert_eq!(runtime.islands()[0].targets, calibration.targets(&devices));
    assert_ne!(runtime.islands()[0].targets, targets);
}

/// A play-space pose (OpenXR's, forward -Z) of a tracking-space device, the inverse of `from_play_space`.
fn play(device: Goal) -> xr::Posef {
    let [x, y, z] = device.pos;
    let [qx, qy, qz, qw] = mul([0.0, 1.0, 0.0, 0.0], device.rot);
    xr::Posef {
        orientation: xr::Quaternionf {
            x: qx,
            y: qy,
            z: qz,
            w: qw,
        },
        position: xr::Vector3f { x: -x, y, z: -z },
    }
}

#[test]
fn headset_poses_calibrate_then_drive_the_head_and_hands() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.add_headset();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    // A user 1.1 times the avatar's size in its rest pose, the headset at the eyes, presses calibrate.
    let mut rest = tracking::rest(&package.avatar).expect("a humanoid");
    rest[Role::Head as usize].as_mut().expect("mapped").pos = package.avatar.avatar().expect("an avatar").viewpoint;
    let worn = |role: Role| {
        let r = rest[role as usize].expect("mapped");
        Goal {
            pos: scale(r.pos, USER),
            rot: r.rot,
        }
    };
    let mut poses = Poses {
        head: Some(play(worn(Role::Head))),
        grips: [Some(play(worn(Role::LeftHand))), Some(play(worn(Role::RightHand)))],
        aims: [None; 2],
        calibrate: true,
    };
    runtime.world.insert_resource(HeadsetPoses(poses));
    runtime.step(1.0 / 60.0);
    let calibration = *(runtime.world.resource::<Trackers>().calibrations.get(&package.id)).expect("calibrated");
    assert!((calibration.ratio - 1.0 / USER).abs() < 1e-5, "{}", calibration.ratio);
    let close = |a: [f32; 3], b: [f32; 3]| length(sub(a, b)) < 1e-4;
    let bones = tracking::rest(&package.avatar).expect("a humanoid");
    let at_rest = runtime.islands()[0].targets;
    for (role, goal) in [
        (Role::Head, at_rest.head),
        (Role::LeftHand, at_rest.hands[0]),
        (Role::RightHand, at_rest.hands[1]),
    ] {
        let (got, want) = (goal.expect("tracked"), bones[role as usize].expect("mapped"));
        assert!(close(got.pos, want.pos), "{role:?} at {:?}", got.pos);
    }
    assert!(at_rest.hips.is_none() && at_rest.feet.iter().all(Option::is_none));

    // The user steps 10 cm right and 10 cm forward in the play space (+X, -Z): the avatar's head goal moves to its
    // right (-X) and forward (+Z) by the calibrated ratio; the button is no longer pressed. The left controller is
    // lost: its hand gets no goal and keeps the animated pose.
    let head = poses.head.as_mut().expect("tracked");
    (head.position.x, head.position.z) = (head.position.x + 0.1, head.position.z - 0.1);
    (poses.grips[0], poses.calibrate) = (None, false);
    runtime.world.insert_resource(HeadsetPoses(poses));
    runtime.step(1.0 / 60.0);
    let moved = runtime.islands()[0].targets;
    let step = scale([-0.1, 0.0, 0.1], calibration.ratio);
    let want = add(at_rest.head.expect("tracked").pos, step);
    assert!(close(moved.head.expect("tracked").pos, want), "{:?}", moved.head);
    assert!(moved.hands[0].is_none());
    assert!(close(
        moved.hands[1].expect("tracked").pos,
        at_rest.hands[1].expect("tracked").pos
    ));
    assert_eq!(
        runtime.world.resource::<Trackers>().calibrations[&package.id],
        calibration
    );
}
