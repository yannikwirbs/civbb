//! Tracker input on the frame schedule (ADR 0113): the scripted source calibrates the player's avatar on its first
//! frame, then the player island's IK targets follow the devices through that calibration; other islands keep none.

use civbb_anim::ik::Targets;
use civbb_anim::tracking::{self, Role};
use civbb_client::frame::Runtime;
use civbb_client::package::Package;
use civbb_client::player::Player;
use civbb_client::tracking::{Scripted, Trackers, USER};
use civbb_core::math::{Pose, length, sub};

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
