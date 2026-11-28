//! Gamepad input for the desktop player (design doc §18, ADR 0115): gilrs buttons and sticks onto the player's
//! [`Input`], on top of keyboard and mouse, with snap or smooth turning.

use gilrs::{Axis, Button};

use crate::player::Input;

/// A stick deflected less than this counts as centred.
pub const DEAD_ZONE: f32 = 0.15;
/// Smooth turning's default yaw speed at full deflection: 120° per second.
pub const SMOOTH: f32 = 120.0 * std::f32::consts::PI / 180.0;
/// Snap turning's default step: 30°.
pub const SNAP: f32 = 30.0 * std::f32::consts::PI / 180.0;
/// The view's pitch speed at full deflection: 90° per second.
const PITCH: f32 = 90.0 * std::f32::consts::PI / 180.0;
/// A snap fires once the right stick is deflected past this sideways ...
const SNAP_AT: f32 = 0.7;
/// ... and the next one only after it came back below this.
const REARM: f32 = 0.3;

/// How the right stick turns the player (design doc §18 comfort; VR's thumbstick shares it later).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turning {
    /// Turns continuously, this many radians per second at full deflection.
    Smooth(f32),
    /// Turns this many radians at once per push.
    Snap(f32),
}

impl Default for Turning {
    fn default() -> Self {
        Turning::Smooth(SMOOTH)
    }
}

/// The state of the gamepads, merged: any pad drives the player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    /// How the right stick turns.
    pub turning: Turning,
    /// Left and right stick, x right and y up, -1 to 1.
    sticks: [[f32; 2]; 2],
    /// Running, from a click of the left stick until it is centred.
    run: bool,
    /// A snap turn fired and the stick has not come back yet.
    snapped: bool,
}

impl Pad {
    /// No pad touched yet, turning as `turning`.
    pub fn new(turning: Turning) -> Self {
        Pad {
            turning,
            ..Pad::default()
        }
    }

    /// A button pressed. South jumps, East crouches, North lies prone, West uses a station (as Space, C, Z and E),
    /// D-pad down cycles floor poses and up flies (as X and F), a click of the left stick runs. Returns whether it
    /// asks to cycle the view (Select, as V), the viewer's job. Start is left for the menu (#149).
    pub fn button(&mut self, button: Button, input: &mut Input) -> bool {
        match button {
            Button::South => input.jump = true,
            Button::East => input.crouch = true,
            Button::North => input.prone = true,
            Button::West => input.interact = true,
            Button::DPadDown => input.pose = true,
            Button::DPadUp => input.fly = true,
            Button::LeftThumb => self.run = true,
            Button::Select => return true,
            _ => {}
        }
        false
    }

    /// A stick axis moved to `value`, -1 to 1 (y up).
    pub fn axis(&mut self, axis: Axis, value: f32) {
        let (stick, i) = match axis {
            Axis::LeftStickX => (0, 0),
            Axis::LeftStickY => (0, 1),
            Axis::RightStickX => (1, 0),
            Axis::RightStickY => (1, 1),
            _ => return,
        };
        self.sticks[stick][i] = value.clamp(-1.0, 1.0);
    }

    /// Every pad is gone: sticks centred, nothing held.
    pub fn reset(&mut self) {
        *self = Pad::new(self.turning);
    }

    /// Adds the sticks over a frame of `dt` seconds to `input`, which holds the keyboard's and mouse's already: the left
    /// stick moves, the right one turns ([`Turning`]) and pitches the view.
    pub fn apply(&mut self, input: &mut Input, dt: f32) {
        let centred = |[x, y]: [f32; 2]| x.hypot(y) < DEAD_ZONE;
        let [[mx, my], [tx, ty]] = self.sticks.map(|s| if centred(s) { [0.0; 2] } else { s });
        if centred(self.sticks[0]) {
            self.run = false;
        }
        input.forward = (input.forward + my).clamp(-1.0, 1.0);
        input.right = (input.right + mx).clamp(-1.0, 1.0);
        input.run |= self.run;
        // The stick turns right with positive x; the input's yaw is positive to the left.
        input.look[0] -= match self.turning {
            Turning::Smooth(speed) => tx * speed * dt,
            Turning::Snap(step) => {
                if tx.abs() < REARM {
                    self.snapped = false;
                }
                if tx.abs() > SNAP_AT && !self.snapped {
                    self.snapped = true;
                    step.copysign(tx)
                } else {
                    0.0
                }
            }
        };
        input.look[1] += ty * PITCH * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn frame(pad: &mut Pad, input: Input) -> Input {
        let mut input = input;
        pad.apply(&mut input, DT);
        input
    }

    #[test]
    fn the_left_stick_moves_on_top_of_the_keys_and_its_click_runs_until_centred() {
        let mut pad = Pad::default();
        pad.axis(Axis::LeftStickY, 0.5);
        pad.axis(Axis::LeftStickX, -0.25);
        let input = frame(&mut pad, Input::default());
        assert_eq!((input.forward, input.right, input.run), (0.5, -0.25, false));
        // Keys held too: the sum, within -1 to 1.
        let keys = Input {
            forward: 1.0,
            right: 1.0,
            ..Input::default()
        };
        let input = frame(&mut pad, keys);
        assert_eq!((input.forward, input.right), (1.0, 0.75));
        pad.button(Button::LeftThumb, &mut Input::default());
        assert!(frame(&mut pad, Input::default()).run);
        // Within the dead zone the stick is centred: no movement, and running ends.
        pad.axis(Axis::LeftStickY, 0.1);
        pad.axis(Axis::LeftStickX, 0.0);
        let input = frame(&mut pad, Input::default());
        assert_eq!((input.forward, input.right, input.run), (0.0, 0.0, false));
        pad.axis(Axis::LeftStickY, 1.0);
        assert!(!frame(&mut pad, Input::default()).run);
    }

    #[test]
    fn buttons_ask_once_as_their_keys_do() {
        let mut pad = Pad::default();
        let mut input = Input::default();
        for b in [
            Button::South,
            Button::East,
            Button::North,
            Button::West,
            Button::DPadDown,
            Button::DPadUp,
        ] {
            assert!(!pad.button(b, &mut input));
        }
        assert!((input.jump, input.crouch, input.prone, input.interact) == (true, true, true, true));
        assert!(input.pose && input.fly);
        assert!(pad.button(Button::Select, &mut input));
        // Start (the menu, #149) and the rest change nothing yet.
        let mut other = Input::default();
        assert!(!pad.button(Button::Start, &mut other));
        assert_eq!(other, Input::default());
    }

    #[test]
    fn smooth_turning_turns_by_deflection_and_time_and_the_stick_pitches() {
        let mut pad = Pad::default();
        pad.axis(Axis::RightStickX, 1.0);
        pad.axis(Axis::RightStickY, 0.5);
        let mouse = Input {
            look: [0.01, 0.0],
            ..Input::default()
        };
        let input = frame(&mut pad, mouse);
        assert!((input.look[0] - (0.01 - SMOOTH * DT)).abs() < 1e-6, "{:?}", input.look);
        assert!((input.look[1] - 0.5 * PITCH * DT).abs() < 1e-6, "{:?}", input.look);
        pad.axis(Axis::RightStickX, -0.5);
        let input = frame(&mut pad, Input::default());
        assert!((input.look[0] - 0.5 * SMOOTH * DT).abs() < 1e-6, "{:?}", input.look);
    }

    #[test]
    fn snap_turning_steps_once_per_push() {
        let mut pad = Pad::new(Turning::Snap(SNAP));
        let mut yaw = Vec::new();
        for x in [0.5, 0.8, 1.0, 0.5, 0.8, 0.2, 0.8, 0.0, -0.9, -1.0] {
            pad.axis(Axis::RightStickX, x);
            yaw.push(frame(&mut pad, Input::default()).look[0]);
        }
        let (r, l) = (-SNAP, SNAP);
        assert_eq!(yaw, [0.0, r, 0.0, 0.0, 0.0, 0.0, r, 0.0, l, 0.0]);
        // A gone pad leaves the setting and nothing else.
        pad.reset();
        assert_eq!(pad, Pad::new(Turning::Snap(SNAP)));
    }

    #[test]
    fn the_pad_drives_the_player_through_the_input_resource() {
        let mut runtime = crate::frame::Runtime::default();
        runtime.add_player();
        let mut pad = Pad::new(Turning::Snap(SNAP));
        pad.axis(Axis::LeftStickY, 1.0);
        pad.axis(Axis::RightStickX, 1.0);
        for _ in 0..60 {
            pad.apply(&mut runtime.world.resource_mut::<Input>(), DT);
            runtime.step(DT);
        }
        // One snap to the right, then a second of walking that way.
        let player = runtime.world.resource::<crate::player::Player>();
        assert!(
            (player.yaw - (std::f32::consts::TAU - SNAP)).abs() < 1e-5,
            "{}",
            player.yaw
        );
        let walked = civbb_core::math::scale(player.forward(), crate::player::WALK);
        assert!(
            civbb_core::math::length(civbb_core::math::sub(player.pos, walked)) < 0.05,
            "{:?}",
            player.pos
        );
    }
}
