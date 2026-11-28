//! Controller input (ADR 0124): the action set with a grip and an aim pose per hand and a calibrate button, its
//! suggested bindings for the common controllers, and each frame's head and hand poses in the session's space.

use crate::{Result, xr};

/// Hands in [`Poses`] order.
const HANDS: [&str; 2] = ["/user/hand/left", "/user/hand/right"];

/// Each interaction profile with the button that asks for calibration: the left hand's menu button, or B on the
/// Index (its menu button is the system's).
const PROFILES: [(&str, &str); 4] = [
    ("/interaction_profiles/khr/simple_controller", "menu/click"),
    ("/interaction_profiles/oculus/touch_controller", "menu/click"),
    ("/interaction_profiles/htc/vive_controller", "menu/click"),
    ("/interaction_profiles/valve/index_controller", "b/click"),
];

/// One frame's tracked poses in the session's space; `None` where the runtime has no valid position and orientation
/// (tracking lost, or the controller is off).
#[derive(Clone, Copy, Debug, Default)]
pub struct Poses {
    /// The headset: the point between the eyes, looking along -Z.
    pub head: Option<xr::Posef>,
    /// Where each hand holds its controller (left, right), the IK's hand device.
    pub grips: [Option<xr::Posef>; 2],
    /// Where each controller points (left, right), for pointers.
    pub aims: [Option<xr::Posef>; 2],
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
        let calibrate = set.create_action::<bool>("calibrate", "Calibrate", &[])?;
        for (profile, button) in PROFILES {
            let path = |p: &str| instance.string_to_path(p);
            let mut bindings = Vec::new();
            for hand in HANDS {
                bindings.push(xr::Binding::new(&grip, path(&format!("{hand}/input/grip/pose"))?));
                bindings.push(xr::Binding::new(&aim, path(&format!("{hand}/input/aim/pose"))?));
            }
            bindings.push(xr::Binding::new(
                &calibrate,
                path(&format!("{}/input/{button}", HANDS[0]))?,
            ));
            instance.suggest_interaction_profile_bindings(path(profile)?, &bindings)?;
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
        let pressed = self.calibrate.state(session, xr::Path::NULL)?;
        Ok(Poses {
            head: locate(&self.head)?,
            grips: [locate(&self.grip_spaces[0])?, locate(&self.grip_spaces[1])?],
            aims: [locate(&self.aim_spaces[0])?, locate(&self.aim_spaces[1])?],
            calibrate: pressed.changed_since_last_sync && pressed.current_state,
        })
    }
}
