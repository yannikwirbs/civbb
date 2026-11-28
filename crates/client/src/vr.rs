//! The VR player (design doc §18, ADR 0125): the controllers' sticks move and turn the player as a gamepad's do, the
//! head's room-scale motion carries the capsule through the world, its height sets the posture, and the play space
//! follows the avatar, which the headset's eyes are drawn from.

use std::f32::consts::FRAC_PI_4;

use bevy_ecs::prelude::*;
use civbb_anim::ik::Goal;
use civbb_anim::tracking::{Calibration, ROLES, Role};
use civbb_core::math::{Pose, mul, qnormalize, rotate, scale, sub};
use civbb_core::schedule::Dt;
use civbb_format::math::Quat;
use gilrs::{Axis, Button};

use crate::extract::Camera;
use crate::frame::Shows;
use crate::gamepad::{Pad, Turning};
use crate::player::{Input, Player, View};
use crate::tracking::Trackers;

/// The head turns this far from the body before the body turns after it.
pub const FOLLOW: f32 = FRAC_PI_4;

/// The headset's poses and controls for the frame being stepped (ADR 0124), in the play space: the XR loop writes
/// them after `xrWaitFrame`, for its predicted display time; [`vr_system`] reads them.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HeadsetPoses(pub civbb_xr::Poses);

/// What the left stick's forward is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Steer {
    /// Where the head looks.
    #[default]
    Head,
    /// Where the left controller points.
    Hand,
}

/// The VR player's state beside [`Player`].
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Vr {
    /// What the left stick's forward is.
    pub steer: Steer,
    /// The sticks as a gamepad's: dead zones, turning, running until the left stick is centred.
    pub pad: Pad,
    /// Tracking space from the play space: where the play space lies relative to the avatar's origin and facing, in
    /// the user's metres (ADR 0124's half turn about +Y at first). Room-scale motion shifts it, the body turning
    /// after the head turns it.
    pub play: Pose,
}

impl Vr {
    /// The play space at the avatar, its forward the avatar's.
    pub fn new(turning: Turning, steer: Steer) -> Self {
        Self {
            steer,
            pad: Pad::new(turning),
            play: Pose {
                rot: [0.0, 1.0, 0.0, 0.0],
                ..Pose::IDENTITY
            },
        }
    }
}

impl Default for Vr {
    fn default() -> Self {
        Self::new(Turning::default(), Steer::Head)
    }
}

/// A pose in the play space (OpenXR's: floor at y 0, forward -Z) as a device in tracking space, through `play`.
pub fn from_play_space(play: &Pose, pose: &civbb_xr::xr::Posef) -> Goal {
    let (p, o) = (pose.position, pose.orientation);
    Goal {
        pos: play.point([p.x, p.y, p.z]),
        rot: qnormalize(mul(play.rot, [o.x, o.y, o.z, o.w])),
    }
}

/// Where the play space sits in the world: the avatar's origin, carrying `play` at `ratio` avatar metres per user
/// metre (the calibration's, 1 before one) and the player's scale.
pub fn stage(player: &Player, play: &Pose, ratio: f32) -> Pose {
    let origin = Pose {
        pos: player.pos,
        rot: player.rotation(),
        scale: player.scale,
    };
    Pose {
        pos: origin.point(scale(play.pos, ratio)),
        rot: mul(origin.rot, play.rot),
        scale: player.scale * ratio,
    }
}

/// The yaw of a device in tracking space: 0 when its forward (OpenXR's -Z) is the avatar's, positive to the left.
fn heading(rot: Quat) -> f32 {
    let [x, _, z] = rotate(rot, [0.0, 0.0, -1.0]);
    if x.hypot(z) < 1e-6 { 0.0 } else { x.atan2(z) }
}

fn about_y(angle: f32) -> Quat {
    let (s, c) = (angle / 2.0).sin_cos();
    [0.0, s, 0.0, c]
}

/// The legacy `TrackingType` of a calibration: 6 with hips and feet, 4 with hips, else 3 (head and hands).
pub fn tracking_type(calibration: &Calibration) -> f32 {
    let has = |role: Role| calibration.offsets[role as usize].is_some();
    match (has(Role::Hips), has(Role::LeftFoot) && has(Role::RightFoot)) {
        (true, true) => 6.0,
        (true, false) => 4.0,
        _ => 3.0,
    }
}

/// The VR system of `PosesIn`, before the player's: sets the player's [`Input`] from the headset (sticks, jump,
/// run, the body turning after the head, room-scale motion, the head's height), keeps the play space on the
/// avatar, and writes the head and grips as the [`Trackers`]' devices, asking to calibrate on the button.
pub(crate) fn vr_system(
    dt: Res<Dt>,
    poses: Res<HeadsetPoses>,
    mut vr: ResMut<Vr>,
    mut trackers: ResMut<Trackers>,
    (mut input, mut player): (ResMut<Input>, ResMut<Player>),
    shows: Query<&Shows>,
) {
    let (poses, vr) = (&poses.0, &mut *vr);
    let shows = player.island.and_then(|e| shows.get(e).ok());
    let calibration = shows.and_then(|s| trackers.calibrations.get(&s.0.id)).copied();
    let ratio = calibration.map_or(1.0, |c| c.ratio);
    (player.view, player.vr) = (View::First, Some(calibration.as_ref().map_or(3.0, tracking_type)));
    // Seated, posed or about to be, the body holds still and the head moves freely in the play space.
    let free = player.station.is_none() && player.floor_pose.is_none() && !input.interact && !input.pose;
    if let Some(head) = poses.head.filter(|_| free) {
        // The body turns after the head past FOLLOW; the play space turns back, so the room stays where it was.
        let yaw = heading(from_play_space(&vr.play, &head).rot);
        let follow = yaw - yaw.clamp(-FOLLOW, FOLLOW);
        let back = about_y(-follow);
        (vr.play.pos, vr.play.rot) = (rotate(back, vr.play.pos), mul(back, vr.play.rot));
        input.look[0] += follow;
        // Room scale: the body goes where the head went, and the play space shifts back to keep the head above it.
        let h = from_play_space(&vr.play, &head).pos;
        let across = [h[0], 0.0, h[2]];
        vr.play.pos = sub(vr.play.pos, across);
        input.step = rotate(about_y(player.yaw + follow), scale(across, ratio * player.scale));
    }
    let eyes = shows.and_then(|s| s.0.avatar.avatar().map(|a| a.viewpoint[1]));
    input.height = match (calibration, eyes, poses.head) {
        (Some(c), Some(eyes), Some(head)) => Some((head.position.y * c.ratio / eyes).clamp(0.0, 1.0)),
        _ => None,
    };
    // The left stick moves along the steering direction: as the body's forward and right.
    let steer = match vr.steer {
        Steer::Head => poses.head,
        Steer::Hand => poses.aims[0],
    };
    let (s, c) = steer
        .map_or(0.0, |p| heading(from_play_space(&vr.play, &p).rot))
        .sin_cos();
    let [[x, y], [turn, _]] = poses.sticks;
    vr.pad.axis(Axis::LeftStickX, x * c - y * s);
    vr.pad.axis(Axis::LeftStickY, y * c + x * s);
    vr.pad.axis(Axis::RightStickX, turn);
    if poses.run {
        vr.pad.button(Button::LeftThumb, &mut input);
    }
    input.jump |= poses.jump;
    vr.pad.apply(&mut input, dt.0, true);
    let mut devices = [None; ROLES];
    devices[Role::Head as usize] = poses.head.as_ref().map(|p| from_play_space(&vr.play, p));
    devices[Role::LeftHand as usize] = poses.grips[0].as_ref().map(|p| from_play_space(&vr.play, p));
    devices[Role::RightHand as usize] = poses.grips[1].as_ref().map(|p| from_play_space(&vr.play, p));
    trackers.devices = devices;
    trackers.calibrate |= poses.calibrate;
}

/// The stage system of `PosesIn`, after the trackers': places the headset's play space at the player for the
/// render thread's eyes ([`Camera::stage`]).
pub(crate) fn stage_system(
    vr: Res<Vr>,
    trackers: Res<Trackers>,
    player: Res<Player>,
    shows: Query<&Shows>,
    mut camera: ResMut<Camera>,
) {
    let shows = player.island.and_then(|e| shows.get(e).ok());
    let ratio = (shows.and_then(|s| trackers.calibrations.get(&s.0.id))).map_or(1.0, |c| c.ratio);
    camera.stage = Some(stage(&player, &vr.play, ratio));
}
