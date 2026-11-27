//! Tracker input and calibration (design doc §7 and §17, ADR 0113): device poses by role in tracking space, and the
//! per-user, per-avatar calibration that turns them into IK targets.

use civbb_core::math::{Pose, add, conj, mul, qnormalize, rotate, scale, sub, world_poses};
use civbb_format::Document;
use civbb_format::hierarchy::HumanBone;

use crate::ik::{Goal, Targets};

/// What a tracked device is worn on; pairs are left, right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The headset, at the eyes.
    Head,
    /// Left controller.
    LeftHand,
    /// Right controller.
    RightHand,
    /// Waist tracker.
    Hips,
    /// Chest tracker.
    Chest,
    /// Left foot tracker.
    LeftFoot,
    /// Right foot tracker.
    RightFoot,
    /// Left elbow tracker.
    LeftElbow,
    /// Right elbow tracker.
    RightElbow,
    /// Left knee tracker.
    LeftKnee,
    /// Right knee tracker.
    RightKnee,
}

/// The number of [`Role`]s.
pub const ROLES: usize = 11;

impl Role {
    /// Every role, in [`Devices`] order.
    pub const ALL: [Role; ROLES] = [
        Role::Head,
        Role::LeftHand,
        Role::RightHand,
        Role::Hips,
        Role::Chest,
        Role::LeftFoot,
        Role::RightFoot,
        Role::LeftElbow,
        Role::RightElbow,
        Role::LeftKnee,
        Role::RightKnee,
    ];

    /// The bone a role places, first mapped wins: the chest is the IK solver's (upper chest, else chest), elbows and
    /// knees their limb's middle bone.
    fn bones(self) -> &'static [HumanBone] {
        use HumanBone as H;
        match self {
            Role::Head => &[H::Head],
            Role::LeftHand => &[H::LeftHand],
            Role::RightHand => &[H::RightHand],
            Role::Hips => &[H::Hips],
            Role::Chest => &[H::UpperChest, H::Chest],
            Role::LeftFoot => &[H::LeftFoot],
            Role::RightFoot => &[H::RightFoot],
            Role::LeftElbow => &[H::LeftLowerArm],
            Role::RightElbow => &[H::RightLowerArm],
            Role::LeftKnee => &[H::LeftLowerLeg],
            Role::RightKnee => &[H::RightLowerLeg],
        }
    }
}

/// One frame's device poses by role (index `Role as usize`; `None` for a role not tracked), in tracking space: the
/// user's metres, floor at y 0, mapped by the source so that the avatar's origin and forward (+Z) are its own.
pub type Devices = [Option<Goal>; ROLES];

/// Each role's bone in the avatar's rest pose, in avatar space: the pose the calibration step asks the user to match
/// (a T-pose for most avatars). `None` without a humanoid.
pub fn rest(avatar: &Document) -> Option<Devices> {
    let humanoid = avatar.avatar()?.humanoid.as_ref()?;
    let nodes = &avatar.hierarchy.nodes;
    let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
    let locals: Vec<_> = nodes.iter().map(|n| n.rest).collect();
    let world = world_poses(&parents, Pose::IDENTITY, &locals);
    Some(Role::ALL.map(|role| {
        let node = role.bones().iter().find_map(|&b| humanoid.node(b))?;
        let p = world[node as usize];
        Some(Goal { pos: p.pos, rot: p.rot })
    }))
}

/// One user's calibration for one avatar: how tracking space scales to the avatar and where each tracked bone sits on
/// its device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    /// Avatar metres per tracking-space metre: the avatar's eye height over the head device's at calibration.
    pub ratio: f32,
    /// Each calibrated role's bone in its device's frame, in avatar metres.
    pub offsets: Devices,
}

impl Calibration {
    /// The calibration step: `devices` are worn by a user standing in the avatar's [`rest`] pose, the head device at
    /// the eyes. `None` without a humanoid or a head device above the floor.
    pub fn new(avatar: &Document, devices: &Devices) -> Option<Self> {
        let eyes = avatar.avatar()?.viewpoint[1];
        let head = devices[Role::Head as usize]?;
        if !(head.pos[1] > 0.0 && eyes > 0.0) {
            return None;
        }
        let ratio = eyes / head.pos[1];
        let rest = rest(avatar)?;
        let mut offsets = [None; ROLES];
        for (offset, (device, bone)) in offsets.iter_mut().zip(devices.iter().zip(rest)) {
            if let (Some(d), Some(bone)) = (device, bone) {
                *offset = Some(Goal {
                    pos: rotate(conj(d.rot), sub(bone.pos, scale(d.pos, ratio))),
                    rot: qnormalize(mul(conj(d.rot), bone.rot)),
                });
            }
        }
        Some(Self { ratio, offsets })
    }

    /// The IK targets for this frame's `devices`: each calibrated role's device scaled by [`Self::ratio`] and carried
    /// with its offset; elbows and knees give the bend points. A role not calibrated or not tracked sets nothing.
    /// Allocates nothing.
    pub fn targets(&self, devices: &Devices) -> Targets {
        let goal = |role: Role| {
            let (d, o) = (devices[role as usize]?, self.offsets[role as usize]?);
            Some(Goal {
                pos: add(scale(d.pos, self.ratio), rotate(d.rot, o.pos)),
                rot: qnormalize(mul(d.rot, o.rot)),
            })
        };
        let point = |role| goal(role).map(|g| g.pos);
        use Role as R;
        Targets {
            head: goal(R::Head),
            hands: [goal(R::LeftHand), goal(R::RightHand)],
            hips: goal(R::Hips),
            chest: goal(R::Chest),
            feet: [goal(R::LeftFoot), goal(R::RightFoot)],
            elbows: [point(R::LeftElbow), point(R::RightElbow)],
            knees: [point(R::LeftKnee), point(R::RightKnee)],
            look: 0.0,
        }
    }
}
