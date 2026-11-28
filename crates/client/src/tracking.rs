//! Tracker input for the player (ADR 0113): the [`Trackers`] resource that sources fill, the system that calibrates
//! and turns it into the player island's IK targets, and its sources: the headset's and the viewer's scripted one.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy_ecs::prelude::*;
use civbb_anim::ik::Goal;
use civbb_anim::tracking::{self, Calibration, Devices, ROLES, Role};
use civbb_core::math::{add, euler_to_quat, mul, qnormalize, scale};
use civbb_core::schedule::Dt;

use crate::frame::Shows;
use crate::island::Island;
use crate::player::Player;

/// The player's trackers: sources (OpenXR in M1.4, OSC later, [`Scripted`] in the viewer) write `devices` in
/// `PosesIn`; [`trackers_system`] then calibrates when asked and sets the player island's IK targets.
#[derive(Resource, Clone, Debug, Default)]
pub struct Trackers {
    /// This frame's device poses in tracking space.
    pub devices: Devices,
    /// Calibrate from this frame's `devices`: the user stands in the avatar's rest pose.
    pub calibrate: bool,
    /// The user's calibration per avatar, by package id; the player island's targets come from its avatar's.
    pub calibrations: HashMap<u64, Calibration>,
}

/// The tracker system of `PosesIn`, after the player's: calibrates the player's avatar if asked, then sets its
/// island's IK targets from the devices once that avatar is calibrated.
pub(crate) fn trackers_system(
    mut trackers: ResMut<Trackers>,
    player: Res<Player>,
    mut islands: Query<(&mut Island, &Shows)>,
) {
    let Some((mut island, shows)) = player.island.and_then(|e| islands.get_mut(e).ok()) else {
        return;
    };
    let trackers = &mut *trackers;
    if std::mem::take(&mut trackers.calibrate)
        && let Some(c) = Calibration::new(&shows.0.avatar, &trackers.devices)
    {
        trackers.calibrations.insert(shows.0.id, c);
    }
    if let Some(c) = trackers.calibrations.get(&shows.0.id) {
        island.targets = c.targets(&trackers.devices);
    }
}

/// The headset's poses for the frame being stepped (ADR 0124), in the play space: the XR loop writes them after
/// `xrWaitFrame`, for its predicted display time; [`headset_system`] maps them into [`Trackers`].
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HeadsetPoses(pub civbb_xr::Poses);

/// A pose in the play space (OpenXR's: floor at y 0, forward -Z) as a device in tracking space: the play space's
/// origin is the avatar's and its forward the avatar's (+Z), a half turn about +Y (ADR 0124).
pub fn from_play_space(pose: &civbb_xr::xr::Posef) -> Goal {
    let (p, o) = (pose.position, pose.orientation);
    Goal {
        pos: [-p.x, p.y, -p.z],
        rot: qnormalize(mul([0.0, 1.0, 0.0, 0.0], [o.x, o.y, o.z, o.w])),
    }
}

/// The headset source of `PosesIn`, before [`trackers_system`]: the head and both grips as the head and hand
/// devices, nothing for the other roles; the calibrate button asks for calibration.
pub(crate) fn headset_system(poses: Res<HeadsetPoses>, mut trackers: ResMut<Trackers>) {
    let poses = &poses.0;
    let mut devices = [None; ROLES];
    devices[Role::Head as usize] = poses.head.as_ref().map(from_play_space);
    devices[Role::LeftHand as usize] = poses.grips[0].as_ref().map(from_play_space);
    devices[Role::RightHand as usize] = poses.grips[1].as_ref().map(from_play_space);
    trackers.devices = devices;
    trackers.calibrate |= poses.calibrate;
}

/// The scripted wearer is this many times the avatar's size.
pub const USER: f32 = 1.1;
/// Seconds per cycle of the scripted paths.
const PERIOD: f32 = 4.0;

/// Scripted tracker paths, the viewer's debug source (`view --trackers N`): a user [`USER`] times the avatar's size
/// stands in its rest pose on the first frame, which calibrates, then squats, sways and nods, circles the hands and
/// lifts the feet in turn.
#[derive(Resource, Clone, Debug)]
pub struct Scripted {
    roles: &'static [Role],
    /// The devices at rest, from the player's avatar on the first frame it is shown.
    rest: Option<Devices>,
    t: f32,
}

impl Scripted {
    /// 3 points (head and hands), 6 (plus hips and feet) or 10 (plus chest, elbows and knees); `None` for other counts.
    pub fn new(points: usize) -> Option<Self> {
        use Role as R;
        let roles: &'static [Role] = match points {
            3 => &[R::Head, R::LeftHand, R::RightHand],
            6 => &[R::Head, R::LeftHand, R::RightHand, R::Hips, R::LeftFoot, R::RightFoot],
            10 => &Role::ALL,
            _ => return None,
        };
        Some(Self {
            roles,
            rest: None,
            t: 0.0,
        })
    }

    /// The devices `t` seconds into the script.
    fn at(&self, rest: &Devices, t: f32) -> Devices {
        let (s, c) = (TAU * t / PERIOD).sin_cos();
        let (s2, c2) = (2.0 * TAU * t / PERIOD).sin_cos();
        let body = [0.05 * s, -0.1 * (1.0 - c), 0.0];
        let mut devices = [None; ROLES];
        for &role in self.roles {
            let Some(r) = rest[role as usize] else { continue };
            let lift = |side: f32| 0.1 * (side * s).max(0.0);
            let (offset, rot) = match role {
                Role::Head => (body, mul(euler_to_quat([15.0 * s2, 0.0, 0.0]), r.rot)),
                Role::LeftHand => (add(body, [0.0, 0.1 * s2, 0.1 * c2]), r.rot),
                Role::RightHand => (add(body, [0.0, -0.1 * s2, 0.1 * c2]), r.rot),
                Role::LeftFoot => ([0.0, lift(1.0), 0.5 * lift(1.0)], r.rot),
                Role::RightFoot => ([0.0, lift(-1.0), 0.5 * lift(-1.0)], r.rot),
                Role::LeftElbow | Role::RightElbow => (add(body, [0.0, -0.3, -0.2]), r.rot),
                Role::LeftKnee | Role::RightKnee => (add(body, [0.0, 0.0, 0.4]), r.rot),
                Role::Hips | Role::Chest => (body, r.rot),
            };
            devices[role as usize] = Some(Goal {
                pos: add(r.pos, offset),
                rot,
            });
        }
        devices
    }
}

/// The scripted source of `PosesIn`, before [`trackers_system`]: on the first frame showing the player's avatar,
/// devices at rest and a calibration; then the paths.
pub(crate) fn scripted_system(
    dt: Res<Dt>,
    mut scripted: ResMut<Scripted>,
    mut trackers: ResMut<Trackers>,
    player: Res<Player>,
    shows: Query<&Shows>,
) {
    let rest = match scripted.rest {
        Some(rest) => rest,
        None => {
            let Some(shows) = player.island.and_then(|e| shows.get(e).ok()) else {
                return;
            };
            let avatar = &shows.0.avatar;
            let (Some(mut rest), Some(a)) = (tracking::rest(avatar), avatar.avatar()) else {
                return;
            };
            // The headset at the eyes, then everything at the user's size.
            if let Some(head) = &mut rest[Role::Head as usize] {
                head.pos = a.viewpoint;
            }
            for device in rest.iter_mut().flatten() {
                device.pos = scale(device.pos, USER);
            }
            trackers.calibrate = true;
            *scripted.rest.insert(rest)
        }
    };
    trackers.devices = scripted.at(&rest, scripted.t);
    scripted.t += dt.0;
}
