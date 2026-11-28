//! Controller input (ADRs 0124, 0125): the action set with a grip and an aim pose and a stick per hand, jump, run and
//! calibrate buttons, its suggested bindings for the common controllers, and each frame's head and hand poses in the
//! session's space with the controls' state.

use crate::{Result, xr};

/// Hands in [`Poses`] order.
const HANDS: [&str; 2] = ["/user/hand/left", "/user/hand/right"];

/// One interaction profile's controls under `.../input/`; `None` where it has none.
struct Profile {
    path: &'static str,
    /// Each hand's thumbstick or trackpad.
    stick: Option<&'static str>,
    /// The left stick's click: run.
    run: Option<&'static str>,
    /// The right hand's jump button.
    jump: Option<&'static str>,
    /// The left hand's button that asks for calibration: the menu button, or B on the Index (its menu button is
    /// the system's).
    calibrate: &'static str,
}

const PROFILES: [Profile; 4] = [
    Profile {
        path: "/interaction_profiles/khr/simple_controller",
        stick: None,
        run: None,
        jump: None,
        calibrate: "menu/click",
    },
    Profile {
        path: "/interaction_profiles/oculus/touch_controller",
        stick: Some("thumbstick"),
        run: Some("thumbstick/click"),
        jump: Some("a/click"),
        calibrate: "menu/click",
    },
    Profile {
        path: "/interaction_profiles/htc/vive_controller",
        stick: Some("trackpad"),
        run: Some("trackpad/click"),
        jump: Some("menu/click"),
        calibrate: "menu/click",
    },
    Profile {
        path: "/interaction_profiles/valve/index_controller",
        stick: Some("thumbstick"),
        run: Some("thumbstick/click"),
        jump: Some("a/click"),
        calibrate: "b/click",
    },
];

/// One frame's tracked poses in the session's space and the controls' state; a pose is `None` where the runtime has
/// no valid position and orientation (tracking lost, or the controller is off).
#[derive(Clone, Copy, Debug, Default)]
pub struct Poses {
    /// The headset: the point between the eyes, looking along -Z.
    pub head: Option<xr::Posef>,
    /// Where each hand holds its controller (left, right), the IK's hand device.
    pub grips: [Option<xr::Posef>; 2],
    /// Where each controller points (left, right), for pointers.
    pub aims: [Option<xr::Posef>; 2],
    /// Each hand's stick (left, right), x right and y up, -1 to 1; centred on controllers without one.
    pub sticks: [[f32; 2]; 2],
    /// The jump button went down since the last frame.
    pub jump: bool,
    /// The left stick was clicked since the last frame.
    pub run: bool,
    /// The calibrate button went down since the last frame.
    pub calibrate: bool,
}

/// The action set and the spaces its poses are located from.
pub(crate) struct Input {
    // Spaces before their actions and the set.
    grip_spaces: [xr::Space; 2],
    aim_spaces: [xr::Space; 2],
    head: xr::Space,
    space: xr::Space,
    hands: [xr::Path; 2],
    stick: xr::Action<xr::Vector2f>,
    jump: xr::Action<bool>,
    run: xr::Action<bool>,
    calibrate: xr::Action<bool>,
    _grip: xr::Action<xr::Posef>,
    _aim: xr::Action<xr::Posef>,
    set: xr::ActionSet,
}

impl Input {
    /// Creates the actions, suggests their bindings and attaches them to `session` (once per session, before any
    /// sync); poses are located in `space`.
    pub(crate) fn new(instance: &xr::Instance, session: &xr::Session<xr::Vulkan>, space: xr::Space) -> Result<Self> {
        let set = instance.create_action_set("player", "Player", 0)?;
        let hands = HANDS.map(|h| instance.string_to_path(h));
        let hands = [hands[0]?, hands[1]?];
        let grip = set.create_action::<xr::Posef>("grip", "Hand grip", &hands)?;
        let aim = set.create_action::<xr::Posef>("aim", "Hand aim", &hands)?;
        let stick = set.create_action::<xr::Vector2f>("stick", "Move and turn", &hands)?;
        let jump = set.create_action::<bool>("jump", "Jump", &[])?;
        let run = set.create_action::<bool>("run", "Run", &[])?;
        let calibrate = set.create_action::<bool>("calibrate", "Calibrate", &[])?;
        for profile in PROFILES {
            let path = |p: &str| instance.string_to_path(p);
            let [left, right] = HANDS;
            let mut bindings = Vec::new();
            for hand in HANDS {
                bindings.push(xr::Binding::new(&grip, path(&format!("{hand}/input/grip/pose"))?));
                bindings.push(xr::Binding::new(&aim, path(&format!("{hand}/input/aim/pose"))?));
                if let Some(s) = profile.stick {
                    bindings.push(xr::Binding::new(&stick, path(&format!("{hand}/input/{s}"))?));
                }
            }
            if let Some(r) = profile.run {
                bindings.push(xr::Binding::new(&run, path(&format!("{left}/input/{r}"))?));
            }
            if let Some(j) = profile.jump {
                bindings.push(xr::Binding::new(&jump, path(&format!("{right}/input/{j}"))?));
            }
            let c = profile.calibrate;
            bindings.push(xr::Binding::new(&calibrate, path(&format!("{left}/input/{c}"))?));
            instance.suggest_interaction_profile_bindings(path(profile.path)?, &bindings)?;
        }
        session.attach_action_sets(&[&set])?;
        let spaces = |action: &xr::Action<xr::Posef>| -> Result<[xr::Space; 2]> {
            let [l, r] = hands.map(|h| action.create_space(session, h, xr::Posef::IDENTITY));
            Ok([l?, r?])
        };
        Ok(Self {
            grip_spaces: spaces(&grip)?,
            aim_spaces: spaces(&aim)?,
            head: session.create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)?,
            space,
            hands,
            stick,
            jump,
            run,
            calibrate,
            _grip: grip,
            _aim: aim,
            set,
        })
    }

    /// Syncs the actions and locates the head and hands at `time`.
    pub(crate) fn poses(&self, session: &xr::Session<xr::Vulkan>, time: xr::Time) -> Result<Poses> {
        session.sync_actions(&[xr::ActiveActionSet::new(&self.set)])?;
        let locate = |space: &xr::Space| -> Result<Option<xr::Posef>> {
            let at = space.locate(&self.space, time)?;
            let valid = xr::SpaceLocationFlags::ORIENTATION_VALID | xr::SpaceLocationFlags::POSITION_VALID;
            Ok(at.location_flags.contains(valid).then_some(at.pose))
        };
        let pressed = |action: &xr::Action<bool>| -> Result<bool> {
            let s = action.state(session, xr::Path::NULL)?;
            Ok(s.changed_since_last_sync && s.current_state)
        };
        let stick = |hand: xr::Path| -> Result<[f32; 2]> {
            let s = self.stick.state(session, hand)?;
            Ok(if s.is_active {
                [s.current_state.x, s.current_state.y]
            } else {
                [0.0; 2]
            })
        };
        Ok(Poses {
            head: locate(&self.head)?,
            grips: [locate(&self.grip_spaces[0])?, locate(&self.grip_spaces[1])?],
            aims: [locate(&self.aim_spaces[0])?, locate(&self.aim_spaces[1])?],
            sticks: [stick(self.hands[0])?, stick(self.hands[1])?],
            jump: pressed(&self.jump)?,
            run: pressed(&self.run)?,
            calibrate: pressed(&self.calibrate)?,
        })
    }
}
