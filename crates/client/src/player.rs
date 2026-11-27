//! The local player in desktop mode: input moves one island through the world (ADR 0110) and places the camera at its
//! viewpoint (design doc §18, ADR 0106), and provides its platform values (ADR 0107).

use std::f32::consts::TAU;

use bevy_ecs::prelude::*;
use civbb_core::math::{add, dot, length, scale, sub};
use civbb_core::schedule::Dt;
use civbb_format::math::{Quat, Vec3};
use civbb_format::state::Platform;

use crate::controller::{Body, WorldMesh};
use crate::extract::Camera;
use crate::frame::Shows;
use crate::island::Island;
use crate::station::{self, REACH, Station};

/// Walking speed forward and back in m/s, the legacy world default (ADR 0106).
pub const WALK: f32 = 2.0;
/// Running speed forward and back in m/s.
pub const RUN: f32 = 4.0;
/// Sideways speed in m/s, running or not.
pub const STRAFE: f32 = 2.0;
pub use crate::controller::{GRAVITY, JUMP};
/// The view's pitch stays within ±89°.
const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;
/// The third-person camera's distance behind the viewpoint, in viewpoint heights.
const THIRD_PERSON: f32 = 1.5;
/// The lowest eye height in metres the player can choose (ADR 0114).
pub const EYE_MIN: f32 = 0.2;
/// The highest eye height in metres the player can choose.
pub const EYE_MAX: f32 = 5.0;

/// What the player asks for, written before each frame; the frame consumes `jump` and `look`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// Forward (+1) to back (-1).
    pub forward: f32,
    /// Right (+1) to left (-1).
    pub right: f32,
    /// Run instead of walk.
    pub run: bool,
    /// Jump once.
    pub jump: bool,
    /// Turn since the last frame in radians: yaw (positive turns left) and pitch (positive looks up).
    pub look: [f32; 2],
    /// Switch between standing and crouching once.
    pub crouch: bool,
    /// Switch between standing and lying prone once.
    pub prone: bool,
    /// Left and right hand gestures, 0 (neutral) to 7 (thumbs up), as the legacy `GestureLeft` and `GestureRight`.
    pub gestures: [u8; 2],
    /// Away from keyboard: the window lost focus.
    pub afk: bool,
    /// Multiply the eye height by this once (ADR 0114).
    pub resize: Option<f32>,
    /// Use the nearest station, or leave the one sat in, once.
    pub interact: bool,
}

/// How upright the player is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Posture {
    /// On their feet.
    #[default]
    Standing,
    /// Crouching: half speed.
    Crouching,
    /// Lying prone: quarter speed.
    Prone,
}

impl Posture {
    /// The `Upright` value: 1 standing, within the usual crouching and prone bands of locomotion layers otherwise.
    pub fn upright(self) -> f32 {
        match self {
            Posture::Standing => 1.0,
            Posture::Crouching => 0.55,
            Posture::Prone => 0.25,
        }
    }

    /// The factor on movement speeds.
    fn speed(self) -> f32 {
        match self {
            Posture::Standing => 1.0,
            Posture::Crouching => 0.5,
            Posture::Prone => 0.25,
        }
    }

    /// `self` after a crouch toggle, then a prone toggle, as asked.
    fn toggled(self, crouch: bool, prone: bool) -> Self {
        let p = match (crouch, self) {
            (false, p) => p,
            (true, Posture::Crouching) => Posture::Standing,
            (true, _) => Posture::Crouching,
        };
        match (prone, p) {
            (false, p) => p,
            (true, Posture::Prone) => Posture::Standing,
            (true, _) => Posture::Prone,
        }
    }
}

/// Where the camera is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// The viewer's own camera; the player does not place it.
    #[default]
    Orbit,
    /// At the avatar's viewpoint.
    First,
    /// Behind the viewpoint, looking at it.
    Third,
}

impl View {
    /// The next mode in the cycle orbit, first person, third person.
    pub fn next(self) -> Self {
        match self {
            View::Orbit => View::First,
            View::First => View::Third,
            View::Third => View::Orbit,
        }
    }
}

/// The local player: which island it moves and where.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Player {
    /// The island moved; `None` moves nothing.
    pub island: Option<Entity>,
    /// Where the avatar stands (its origin).
    pub pos: Vec3,
    /// Facing in radians about +Y; 0 faces +Z.
    pub yaw: f32,
    /// The view's elevation in radians.
    pub pitch: f32,
    /// Upward speed in m/s.
    pub vertical: f32,
    /// Velocity over the last step in m/s, in the world.
    pub velocity: Vec3,
    /// Turn rate over the last step in radians per second, positive to the left.
    pub turn: f32,
    /// Standing on the ground.
    pub grounded: bool,
    /// Standing, crouching or prone.
    pub posture: Posture,
    /// The camera mode.
    pub view: View,
    /// The chosen eye height in metres; `None` is the avatar's own (ADR 0114).
    pub eye_height: Option<f32>,
    /// The avatar's scale: the eye height over the avatar's own.
    pub scale: f32,
    /// The station sat in.
    pub station: Option<Station>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            island: None,
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            vertical: 0.0,
            velocity: [0.0; 3],
            turn: 0.0,
            grounded: true,
            posture: Posture::Standing,
            view: View::Orbit,
            eye_height: None,
            scale: 1.0,
            station: None,
        }
    }
}

impl Player {
    /// Facing as a unit vector on the ground.
    pub fn forward(&self) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        [s, 0.0, c]
    }

    /// Facing as a rotation about +Y.
    pub fn rotation(&self) -> Quat {
        let (s, c) = (self.yaw / 2.0).sin_cos();
        [0.0, s, 0.0, c]
    }

    /// Applies `factor` to the eye height, kept within [`EYE_MIN`] and [`EYE_MAX`], and sets the scale for an avatar
    /// whose own eye height is `own` metres; without one (`own` not positive) the scale is 1.
    pub fn rescale(&mut self, factor: Option<f32>, own: f32) {
        if own <= 0.0 {
            self.scale = 1.0;
            return;
        }
        if let Some(f) = factor {
            self.eye_height = Some((self.eye_height.unwrap_or(own) * f).clamp(EYE_MIN, EYE_MAX));
        }
        self.scale = self.eye_height.map_or(1.0, |e| e / own);
    }

    /// Sits in `station`: at its seat, facing its way, still.
    pub fn sit(&mut self, station: Station) {
        (self.pos, self.yaw, self.station) = (station.seat, station.yaw, Some(station));
        (self.vertical, self.velocity, self.turn, self.grounded) = (0.0, [0.0; 3], 0.0, true);
    }

    /// Moves and turns by `input` over `dt` seconds through `world` (ADR 0110), at the player's scale (ADR 0114). In a
    /// station only the view pitches; jumping or using leaves it.
    pub fn step(&mut self, input: &Input, dt: f32, world: &WorldMesh) {
        let start = self.pos;
        self.pitch = (self.pitch + input.look[1]).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        if let Some(station) = self.station {
            if input.jump || input.interact {
                (self.pos, self.station) = (station.exit, None);
            }
            (self.velocity, self.turn) = ([0.0; 3], 0.0);
            return;
        }
        self.yaw = (self.yaw + input.look[0]).rem_euclid(TAU);
        self.turn = if dt > 0.0 { input.look[0] / dt } else { 0.0 };
        self.posture = self.posture.toggled(input.crouch, input.prone);
        let (f, r) = (input.forward.clamp(-1.0, 1.0), input.right.clamp(-1.0, 1.0));
        // Diagonal input is not faster than straight input.
        let n = (f * f + r * r).sqrt().max(1.0) / (self.posture.speed() * self.scale);
        let along = f / n * if input.run { RUN } else { WALK };
        let side = r / n * STRAFE;
        let (forward, right) = (self.forward(), self.right());
        let mut body = Body {
            pos: self.pos,
            vertical: self.vertical,
            grounded: self.grounded,
            scale: self.scale,
        };
        body.step(world, add(scale(forward, along), scale(right, side)), input.jump, dt);
        (self.pos, self.vertical, self.grounded) = (body.pos, body.vertical, body.grounded);
        self.velocity = if dt > 0.0 {
            scale(sub(self.pos, start), 1.0 / dt)
        } else {
            [0.0; 3]
        };
    }

    /// The avatar's right on the ground: avatars face +Z, so it is forward x up.
    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        [-f[2], 0.0, f[0]]
    }

    /// The platform values the player provides (ADR 0107), given what `input` holds and the avatar's own eye height
    /// `own`: velocity in avatar space (x right, z forward, avatar metres), turn rate (positive to the right), ground
    /// contact, posture, gestures, AFK, desktop tracking, stations and scale (ADR 0114).
    pub fn platform_values(&self, input: &Input, own: f32) -> [(Platform, f32); 20] {
        use Platform::*;
        let flag = |b: bool| f32::from(u8::from(b));
        let v = scale(self.velocity, 1.0 / self.scale);
        let eye = own * self.scale;
        let [left, right] = input.gestures;
        [
            (VelocityX, dot(v, self.right())),
            (VelocityY, v[1]),
            (VelocityZ, dot(v, self.forward())),
            (VelocityMagnitude, length(v)),
            (AngularY, -self.turn),
            (Grounded, flag(self.grounded)),
            (Upright, self.posture.upright()),
            (Afk, flag(input.afk)),
            (TrackingType, 3.0),
            (VrMode, 0.0),
            (GestureLeft, f32::from(left)),
            (GestureRight, f32::from(right)),
            // A key press closes the fist all the way.
            (GestureLeftWeight, flag(left == 1)),
            (GestureRightWeight, flag(right == 1)),
            (InStation, flag(self.station.is_some())),
            (Seated, flag(self.station.is_some_and(|s| s.seated))),
            (ScaleFactor, self.scale),
            (ScaleFactorInverse, 1.0 / self.scale),
            (EyeHeight, eye),
            (
                EyeHeightPercent,
                ((eye - EYE_MIN) / (EYE_MAX - EYE_MIN)).clamp(0.0, 1.0),
            ),
        ]
    }

    /// The camera's eye and the point it looks at, for an avatar whose viewpoint is `eye` in the world and
    /// `height` metres above its origin; `None` in orbit mode.
    pub fn camera(&self, eye: Vec3, height: f32) -> Option<(Vec3, Vec3)> {
        let (sp, cp) = self.pitch.sin_cos();
        let forward = self.forward();
        let look = [forward[0] * cp, sp, forward[2] * cp];
        match self.view {
            View::Orbit => None,
            View::First => Some((eye, add(eye, look))),
            View::Third => Some((sub(eye, scale(look, THIRD_PERSON * height.max(0.5))), eye)),
        }
    }
}

/// The player system of [`civbb_core::schedule::Step::PosesIn`]: scales the [`Player`], seats it in the nearest
/// [`Station`] when asked, steps it by the [`Input`], places its island and, outside orbit mode, the [`Camera`].
pub(crate) fn player_system(
    dt: Res<Dt>,
    mut input: ResMut<Input>,
    mut player: ResMut<Player>,
    mut camera: ResMut<Camera>,
    world: Res<WorldMesh>,
    stations: Query<&Station>,
    mut islands: Query<(&mut Island, &Shows)>,
) {
    let viewpoint = (player.island.and_then(|e| islands.get(e).ok()))
        .and_then(|(_, shows)| shows.0.avatar.avatar().map(|a| a.viewpoint))
        .unwrap_or([0.0; 3]);
    player.rescale(input.resize, viewpoint[1]);
    if input.interact
        && player.station.is_none()
        && let Some(s) = station::nearest(stations, player.pos, REACH * player.scale)
    {
        player.sit(s);
        input.interact = false;
    }
    player.step(&input, dt.0, &world);
    (input.jump, input.look, input.crouch, input.prone) = (false, [0.0; 2], false, false);
    (input.resize, input.interact) = (None, false);
    let Some((mut island, _)) = player.island.and_then(|e| islands.get_mut(e).ok()) else {
        return;
    };
    for (p, v) in player.platform_values(&input, viewpoint[1]) {
        island.set_platform(p, v);
    }
    island.origin.pos = player.pos;
    island.origin.rot = player.rotation();
    island.origin.scale = player.scale;
    island.hide_head = player.view == View::First;
    island.targets.look = player.pitch;
    let eye = island.origin.point(viewpoint);
    if let Some((eye, target)) = player.camera(eye, viewpoint[1] * island.origin.scale) {
        (camera.eye, camera.target) = (eye, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn step(p: &mut Player, input: &Input, dt: f32) {
        p.step(input, dt, &WorldMesh::plane());
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn walks_runs_and_strafes_at_the_set_speeds() {
        let mut p = Player::default();
        let walk = Input {
            forward: 1.0,
            ..Default::default()
        };
        for _ in 0..60 {
            step(&mut p, &walk, DT);
        }
        assert!(close(p.pos, [0.0, 0.0, WALK]), "{:?}", p.pos);
        let run = Input { run: true, ..walk };
        for _ in 0..60 {
            step(&mut p, &run, DT);
        }
        assert!(close(p.pos, [0.0, 0.0, WALK + RUN]), "{:?}", p.pos);
        // Right of an avatar facing +Z is -X.
        let mut p = Player::default();
        step(
            &mut p,
            &Input {
                right: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!(close(p.pos, [-STRAFE, 0.0, 0.0]), "{:?}", p.pos);
        // Diagonal: no faster than straight.
        let mut p = Player::default();
        step(
            &mut p,
            &Input {
                forward: 1.0,
                right: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!((p.pos[0].hypot(p.pos[2]) - WALK).abs() < 1e-4, "{:?}", p.pos);
    }

    #[test]
    fn turning_left_then_walking_heads_to_plus_x() {
        let mut p = Player::default();
        step(
            &mut p,
            &Input {
                look: [std::f32::consts::FRAC_PI_2, 0.0],
                ..Default::default()
            },
            0.0,
        );
        step(
            &mut p,
            &Input {
                forward: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!(close(p.pos, [WALK, 0.0, 0.0]), "{:?}", p.pos);
        // The rotation turns +Z to the facing.
        assert!(close(
            civbb_core::math::rotate(p.rotation(), [0.0, 0.0, 1.0]),
            p.forward()
        ));
    }

    #[test]
    fn a_jump_rises_and_lands() {
        let mut p = Player::default();
        step(
            &mut p,
            &Input {
                jump: true,
                ..Default::default()
            },
            DT,
        );
        assert!(!p.grounded && p.pos[1] > 0.0);
        let mut top = 0.0_f32;
        for _ in 0..120 {
            step(&mut p, &Input::default(), DT);
            top = top.max(p.pos[1]);
        }
        // v² / 2g, give or take the integration.
        assert!((top - JUMP * JUMP / (2.0 * GRAVITY)).abs() < 0.03, "{top}");
        assert!(p.grounded && p.pos[1].abs() < 1e-5 && p.vertical == 0.0, "{p:?}");
    }

    #[test]
    fn crouching_and_prone_toggle_and_slow_down() {
        let mut p = Player::default();
        let toggle = |crouch, prone| Input {
            crouch,
            prone,
            ..Default::default()
        };
        step(&mut p, &toggle(true, false), 0.0);
        assert_eq!(p.posture, Posture::Crouching);
        step(&mut p, &toggle(false, true), 0.0);
        assert_eq!(p.posture, Posture::Prone);
        step(&mut p, &toggle(false, true), 0.0);
        assert_eq!(p.posture, Posture::Standing);
        step(&mut p, &toggle(true, false), 0.0);
        step(
            &mut p,
            &Input {
                forward: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!(close(p.pos, [0.0, 0.0, WALK / 2.0]), "{:?}", p.pos);
    }

    #[test]
    fn platform_values_are_in_avatar_space() {
        use Platform::*;
        let mut p = Player {
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let input = Input {
            right: 1.0,
            look: [-0.1, 0.0],
            gestures: [1, 4],
            afk: true,
            ..Default::default()
        };
        step(&mut p, &input, 0.5);
        let values = p.platform_values(&input, 1.5);
        let value = |q| values.iter().find(|(p, _)| *p == q).expect("provided").1;
        // Strafing right at strafe speed while turning right at 0.2 rad/s.
        assert!((value(VelocityX) - STRAFE).abs() < 1e-3, "{values:?}");
        assert!(value(VelocityZ).abs() < 1e-3 && value(VelocityY) == 0.0);
        assert!((value(VelocityMagnitude) - STRAFE).abs() < 1e-3);
        assert!((value(AngularY) - 0.2).abs() < 1e-5);
        assert_eq!(
            [Grounded, Upright, Afk, TrackingType, VrMode].map(value),
            [1.0, 1.0, 1.0, 3.0, 0.0]
        );
        assert_eq!(
            [GestureLeft, GestureRight, GestureLeftWeight, GestureRightWeight].map(value),
            [1.0, 4.0, 1.0, 0.0]
        );
        // In the air: falling, not grounded.
        step(
            &mut p,
            &Input {
                jump: true,
                ..Default::default()
            },
            DT,
        );
        // Past the top of the jump (0.31 s), before landing (0.61 s).
        for _ in 0..25 {
            step(&mut p, &Input::default(), DT);
        }
        let values = p.platform_values(&Input::default(), 1.5);
        let value = |q| values.iter().find(|(p, _)| *p == q).expect("provided").1;
        assert!(value(VelocityY) < 0.0 && value(Grounded) == 0.0, "{values:?}");
    }

    #[test]
    fn pitch_stays_short_of_straight_up_and_down() {
        let mut p = Player::default();
        let up = Input {
            look: [0.0, 10.0],
            ..Default::default()
        };
        step(&mut p, &up, DT);
        assert_eq!(p.pitch, PITCH_LIMIT);
    }

    #[test]
    fn cameras_by_view() {
        let mut p = Player::default();
        let eye = [0.0, 1.5, 0.1];
        assert_eq!(p.camera(eye, 1.5), None);
        p.view = View::First;
        assert_eq!(p.camera(eye, 1.5), Some((eye, [0.0, 1.5, 1.1])));
        p.view = View::Third;
        let (from, at) = p.camera(eye, 1.5).expect("third person places the camera");
        assert_eq!(at, eye);
        assert!(close(from, [0.0, 1.5, 0.1 - THIRD_PERSON * 1.5]));
        assert_eq!(p.view.next(), View::Orbit);
    }

    #[test]
    fn eye_height_scales_speeds_and_the_jump_within_limits() {
        use Platform::*;
        let mut p = Player::default();
        // An avatar with its eyes at 1.5 m, doubled.
        p.rescale(Some(2.0), 1.5);
        assert_eq!((p.eye_height, p.scale), (Some(3.0), 2.0));
        let walk = Input {
            forward: 1.0,
            ..Default::default()
        };
        step(&mut p, &walk, 1.0);
        assert!(close(p.pos, [0.0, 0.0, 2.0 * WALK]), "{:?}", p.pos);
        // Platform velocities stay in avatar metres, so the gait matches the speed.
        let values = p.platform_values(&walk, 1.5);
        let value = |q| values.iter().find(|(p, _)| *p == q).expect("provided").1;
        assert!((value(VelocityZ) - WALK).abs() < 1e-4, "{values:?}");
        assert_eq!([ScaleFactor, ScaleFactorInverse, EyeHeight].map(value), [2.0, 0.5, 3.0]);
        assert!((value(EyeHeightPercent) - (3.0 - EYE_MIN) / (EYE_MAX - EYE_MIN)).abs() < 1e-6);
        // The jump reaches twice its height.
        let mut top = 0.0_f32;
        step(
            &mut p,
            &Input {
                jump: true,
                ..Default::default()
            },
            DT,
        );
        for _ in 0..120 {
            step(&mut p, &Input::default(), DT);
            top = top.max(p.pos[1]);
        }
        assert!((top - 2.0 * JUMP * JUMP / (2.0 * GRAVITY)).abs() < 0.05, "{top}");
        // Chosen heights stay within the limits; another avatar keeps the eye height and takes its own scale.
        p.rescale(Some(100.0), 1.5);
        assert_eq!(p.eye_height, Some(EYE_MAX));
        p.rescale(Some(1e-3), 1.5);
        assert_eq!(p.eye_height, Some(EYE_MIN));
        p.rescale(None, 0.8);
        assert_eq!(p.scale, EYE_MIN / 0.8);
        // No avatar, no scale.
        p.rescale(Some(2.0), 0.0);
        assert_eq!(p.scale, 1.0);
    }

    #[test]
    fn a_station_holds_the_player_until_a_jump_or_use() {
        use Platform::*;
        let station = Station {
            seat: [1.0, 0.0, 2.0],
            yaw: 1.0,
            exit: [1.0, 0.0, 3.0],
            seated: true,
        };
        let mut p = Player::default();
        p.sit(station);
        let walk = Input {
            forward: 1.0,
            look: [0.5, 0.2],
            crouch: true,
            ..Default::default()
        };
        step(&mut p, &walk, 1.0);
        assert_eq!((p.pos, p.yaw, p.posture), (station.seat, 1.0, Posture::Standing));
        assert_eq!((p.velocity, p.pitch), ([0.0; 3], 0.2));
        let values = p.platform_values(&walk, 1.5);
        let value = |q| values.iter().find(|(p, _)| *p == q).expect("provided").1;
        assert_eq!(
            [InStation, Seated, VelocityZ, Grounded].map(value),
            [1.0, 1.0, 0.0, 1.0]
        );
        for leave in [
            Input {
                jump: true,
                ..Default::default()
            },
            Input {
                interact: true,
                ..Default::default()
            },
        ] {
            p.sit(station);
            step(&mut p, &leave, DT);
            assert_eq!((p.pos, p.station), (station.exit, None));
        }
        // A station without sitting.
        p.sit(Station {
            seated: false,
            ..station
        });
        let values = p.platform_values(&Input::default(), 1.5);
        let value = |q| values.iter().find(|(p, _)| *p == q).expect("provided").1;
        assert_eq!([InStation, Seated].map(value), [1.0, 0.0]);
    }
}
