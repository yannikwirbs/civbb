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

    /// Moves and turns by `input` over `dt` seconds through `world` (ADR 0110).
    pub fn step(&mut self, input: &Input, dt: f32, world: &WorldMesh) {
        let start = self.pos;
        self.yaw = (self.yaw + input.look[0]).rem_euclid(TAU);
        self.turn = if dt > 0.0 { input.look[0] / dt } else { 0.0 };
        self.pitch = (self.pitch + input.look[1]).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.posture = self.posture.toggled(input.crouch, input.prone);
        let (f, r) = (input.forward.clamp(-1.0, 1.0), input.right.clamp(-1.0, 1.0));
        // Diagonal input is not faster than straight input.
        let n = (f * f + r * r).sqrt().max(1.0) / self.posture.speed();
        let along = f / n * if input.run { RUN } else { WALK };
        let side = r / n * STRAFE;
        let (forward, right) = (self.forward(), self.right());
        let mut body = Body {
            pos: self.pos,
            vertical: self.vertical,
            grounded: self.grounded,
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

    /// The platform values the player provides (ADR 0107), given what `input` holds: velocity in avatar space
    /// (x right, z forward), turn rate (positive to the right), ground contact, posture, gestures, AFK and desktop
    /// tracking.
    pub fn platform_values(&self, input: &Input) -> [(Platform, f32); 14] {
        use Platform::*;
        let flag = |b: bool| f32::from(u8::from(b));
        let v = self.velocity;
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

/// The player system of [`civbb_core::schedule::Step::PosesIn`]: steps the [`Player`] by the [`Input`], places its
/// island and, outside orbit mode, the [`Camera`].
pub(crate) fn player_system(
    dt: Res<Dt>,
    mut input: ResMut<Input>,
    mut player: ResMut<Player>,
    mut camera: ResMut<Camera>,
    world: Res<WorldMesh>,
    mut islands: Query<(&mut Island, &Shows)>,
) {
    player.step(&input, dt.0, &world);
    (input.jump, input.look, input.crouch, input.prone) = (false, [0.0; 2], false, false);
    let Some((mut island, shows)) = player.island.and_then(|e| islands.get_mut(e).ok()) else {
        return;
    };
    for (p, v) in player.platform_values(&input) {
        island.set_platform(p, v);
    }
    island.origin.pos = player.pos;
    island.origin.rot = player.rotation();
    island.hide_head = player.view == View::First;
    island.targets.look = player.pitch;
    let viewpoint = shows.0.avatar.avatar().map_or([0.0; 3], |a| a.viewpoint);
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
        let values = p.platform_values(&input);
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
        let values = p.platform_values(&Input::default());
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
}
