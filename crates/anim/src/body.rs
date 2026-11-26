//! Unity's humanoid body pose (#52): a pose's centre of mass and body orientation from its bones' positions, the
//! quantities clips' `RootT` and `RootQ` curves set. Fitted to `HumanPoseHandler` recordings (`tests/unity/
//! body_pose.json`): orientation within 0.35 degrees, centre of mass within 6 mm at a 1.07 m human scale.

use civbb_core::math::{Pose, add, conj, cross, mul, normalize, qnormalize, rotate, scale, sub, world_poses};
use civbb_format::animator::RootSettings;
use civbb_format::hierarchy::HumanBone::{self, *};
use civbb_format::hierarchy::{BoneNode, Hierarchy, Humanoid, NodeIndex};
use civbb_format::math::{Quat, Transform, Vec3};

/// Places a humanoid's hips from a body pose (`RootT` then `RootQ`, Unity's units; ADR 0063).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyRig {
    hips: NodeIndex,
    /// The hips' ancestors, root first.
    above: Vec<NodeIndex>,
    /// Each bone the centre of mass and orientation read, with its nodes below the hips, parents first.
    chains: Vec<(HumanBone, Vec<NodeIndex>)>,
    /// [`orientation`] of the rest pose, the body rotation's reference.
    rest_axes: Quat,
    /// Metres per body position unit (Unity's `Animator.humanScale`).
    human_scale: f32,
    /// Added to the mass model's centre in the hips' frame, so that the rest pose's matches Unity's.
    anchor: Vec3,
    /// The rest pose's body pose: its centre of mass over the human scale, and no rotation.
    pub rest: [f32; 7],
}

impl BodyRig {
    /// The body rig of a validated humanoid with hips, upper arms and upper legs; `human_scale` 0 takes the rest
    /// pose's centre-of-mass height (Unity's definition, measured on its T-pose). With Unity's rest centre of mass
    /// the model's error at rest (up to 8 mm on corpus avatars) becomes a constant offset in the hips' frame, which
    /// leaves about 2 mm in other poses.
    pub fn new(hierarchy: &Hierarchy, h: &Humanoid) -> Option<Self> {
        let hips = h.node(Hips)?;
        let parent = |n: NodeIndex| hierarchy.nodes[n as usize].parent;
        let mut above = Vec::new();
        let mut n = parent(hips);
        while let Some(p) = n {
            above.push(p);
            n = parent(p);
        }
        above.reverse();
        let mut chains = Vec::new();
        for &BoneNode { bone, node } in &h.bones {
            if bone == Hips || !MASSES.iter().any(|m| m.0 == bone) {
                continue;
            }
            let mut chain = vec![node];
            let mut n = parent(node);
            while n != Some(hips) {
                chain.push(n?);
                n = parent(n?);
            }
            chain.reverse();
            chains.push((bone, chain));
        }
        let parents: Vec<_> = hierarchy.nodes.iter().map(|n| n.parent).collect();
        let locals: Vec<Transform> = hierarchy.nodes.iter().map(|n| n.rest).collect();
        let world = world_poses(&parents, Pose::IDENTITY, &locals);
        let at = |b: HumanBone| h.node(b).map(|n| world[n as usize].pos);
        let rest_axes = orientation(at)?;
        let modelled = center_of_mass(at);
        let com = h.center_of_mass.unwrap_or(modelled);
        let human_scale = if h.human_scale > 0.0 { h.human_scale } else { com[1] };
        if human_scale <= 0.0 {
            return None;
        }
        let w = world[hips as usize];
        let anchor = scale(rotate(conj(w.rot), sub(com, modelled)), 1.0 / w.scale);
        let c = scale(com, 1.0 / human_scale);
        Some(Self {
            hips,
            above,
            chains,
            rest_axes,
            human_scale,
            anchor,
            rest: [c[0], c[1], c[2], 0.0, 0.0, 0.0, 1.0],
        })
    }

    /// Writes the hips' local transform that puts the pose in `locals` (muscles applied) at `body`: the rotation
    /// that turns the pose's orientation into the body rotation, then the position that puts its centre of mass
    /// at the body position times the human scale.
    pub fn place(&self, body: [f32; 7], locals: &mut [Transform]) {
        let parent = self
            .above
            .iter()
            .fold(Pose::IDENTITY, |p, &n| p.then(&locals[n as usize]));
        let hips = &locals[self.hips as usize];
        let s = parent.scale * hips.scale[0];
        let below: Vec<(HumanBone, Vec3)> = (self.chains.iter())
            .map(|(b, chain)| {
                (
                    *b,
                    chain
                        .iter()
                        .fold(Pose::IDENTITY, |p, &n| p.then(&locals[n as usize]))
                        .pos,
                )
            })
            .collect();
        let at = |b: HumanBone| match b {
            Hips => Some([0.0; 3]),
            _ => below.iter().find(|(x, _)| *x == b).map(|(_, p)| *p),
        };
        let Some(axes) = orientation(at) else {
            return;
        };
        let rot = mul(
            mul(qnormalize([body[3], body[4], body[5], body[6]]), self.rest_axes),
            conj(axes),
        );
        let target = scale([body[0], body[1], body[2]], self.human_scale);
        let pos = sub(target, rotate(rot, scale(add(center_of_mass(at), self.anchor), s)));
        let hips = &mut locals[self.hips as usize];
        hips.rotation = mul(conj(parent.rot), rot);
        hips.translation = scale(rotate(conj(parent.rot), sub(pos, parent.pos)), 1.0 / parent.scale);
    }
}

/// A clip's body pose with its root-motion settings applied and root motion discarded, as Unity plays it with
/// `applyRootMotion` off (recorded, `tests/unity/root_motion.json`; ADR 0063). The pose plays relative to a root
/// that takes the body's yaw (twist about Y) and position, or their change since the clip's `start` when based upon
/// the original, or holds the start's when baked into the pose.
#[derive(Clone, Debug, PartialEq)]
pub struct RootMotion {
    settings: RootSettings,
    start: (Vec3, Quat),
    /// Loop pose: the root-relative pose's change from the clip's end back to its start, in the end's frame.
    cycle: Option<(Vec3, Quat)>,
}

impl RootMotion {
    /// The settings for a clip whose body pose is `start` at time 0 and `end` at its end; `looping` clips only get
    /// the loop pose.
    pub fn new(settings: RootSettings, start: (Vec3, Quat), end: (Vec3, Quat), looping: bool) -> Self {
        let mut r = Self {
            settings,
            start,
            cycle: None,
        };
        if looping && settings.loop_pose {
            let (p0, q0) = r.relative(start);
            let (p1, q1) = r.relative(end);
            let d = mul(conj(q1), q0);
            let d = if d[3] < 0.0 { d.map(|c| -c) } else { d };
            r.cycle = Some((rotate(conj(q1), sub(p0, p1)), d));
        }
        r
    }

    /// The pose at normalised clip time `w` (0 to 1) of the body pose `body` sampled there.
    pub fn apply(&self, body: (Vec3, Quat), w: f32) -> (Vec3, Quat) {
        let (mut p, mut q) = self.relative(body);
        if let Some((dp, d)) = self.cycle {
            // Unity weights the rotation by scaling its vector part.
            p = add(p, rotate(q, scale(dp, w)));
            q = qnormalize(mul(q, qnormalize([w * d[0], w * d[1], w * d[2], d[3]])));
        }
        p[1] -= self.settings.level;
        (p, q)
    }

    /// `body` relative to the root, before the loop pose and level.
    fn relative(&self, (p, q): (Vec3, Quat)) -> (Vec3, Quat) {
        let s = &self.settings;
        let (p0, q0) = self.start;
        let yaw = |q: Quat| qnormalize([0.0, q[1], 0.0, q[3]]);
        let mut root = if s.bake_orientation { yaw(q0) } else { yaw(q) };
        if s.original_orientation {
            root = mul(root, conj(yaw(q0)));
        }
        let half = s.orientation_offset.to_radians() / 2.0;
        root = mul(root, [0.0, half.sin(), 0.0, half.cos()]);
        let at = |bake: bool, original: bool, i: usize| {
            (if bake { p0[i] } else { p[i] }) - if original { p0[i] } else { 0.0 }
        };
        let origin = [
            at(s.bake_position_xz, s.original_position_xz, 0),
            at(s.bake_position_y, s.original_position_y, 1),
            at(s.bake_position_xz, s.original_position_xz, 2),
        ];
        let inv = conj(root);
        (rotate(inv, sub(p, origin)), mul(inv, q))
    }
}

/// Each bone's own mass, the bone its mass centre lies towards (none: at the bone) and how far. Unity's
/// `HumanTrait.GetBoneDefaultHierarchyMass` gives a subtree's mass; a bone's own is that minus its children's. The
/// fractions are fitted to `BodyPose.cs`'s recordings (#96, ADR 0073): halfway along most segments, feet at the joint,
/// the upper chest 7/12 of the way to the neck, and the left upper arm (not the right) 1/8 of the way to the elbow.
const MASSES: [(HumanBone, Option<HumanBone>, f32, f32); 22] = [
    (Hips, None, 12.0, 0.0),
    (Spine, Some(Chest), 2.5, 0.5),
    (Chest, Some(UpperChest), 12.0, 0.5),
    (UpperChest, Some(Neck), UPPER_CHEST_MASS, 7.0 / 12.0),
    (Neck, Some(Head), 1.0, 0.5),
    (Head, None, 4.0, 0.0),
    (LeftUpperLeg, Some(LeftLowerLeg), 10.0, 0.5),
    (LeftLowerLeg, Some(LeftFoot), 4.0, 0.5),
    (LeftFoot, None, 0.8, 0.0),
    (LeftToes, None, TOES_MASS, 0.0),
    (RightUpperLeg, Some(RightLowerLeg), 10.0, 0.5),
    (RightLowerLeg, Some(RightFoot), 4.0, 0.5),
    (RightFoot, None, 0.8, 0.0),
    (RightToes, None, TOES_MASS, 0.0),
    (LeftShoulder, Some(LeftUpperArm), 0.5, 0.5),
    (LeftUpperArm, Some(LeftLowerArm), 2.0, 0.125),
    (LeftLowerArm, Some(LeftHand), 1.5, 0.5),
    (LeftHand, None, 0.5, 0.0),
    (RightShoulder, Some(RightUpperArm), 0.5, 0.5),
    (RightUpperArm, Some(RightLowerArm), 2.0, 0.5),
    (RightLowerArm, Some(RightHand), 1.5, 0.5),
    (RightHand, None, 0.5, 0.0),
];

/// The mass-weighted centre of the bones `pos` places. A missing bone adds no mass and a missing child puts the
/// mass at the bone, except where Unity keeps the mass in the parent: missing toes weigh at their foot, and a missing
/// upper chest (9 of 13 corpus avatars) runs the chest's segment to the neck and puts the upper chest's mass 5/6 of
/// the way up it. Within 0.6 mm on the recordings with every bone, 1.3 mm without upper chest (and toes).
pub fn center_of_mass(pos: impl Fn(HumanBone) -> Option<Vec3>) -> Vec3 {
    let (mut sum, mut total) = ([0.0; 3], 0.0);
    let no_upper_chest = pos(UpperChest).is_none();
    for (bone, child, mut mass, f) in MASSES {
        if let Some(p) = pos(bone) {
            let child = child.map(|c| if c == UpperChest && no_upper_chest { Neck } else { c });
            let at = child.and_then(&pos).map_or(p, |c| add(p, scale(sub(c, p), f)));
            let toes = match bone {
                LeftFoot => Some(LeftToes),
                RightFoot => Some(RightToes),
                _ => None,
            };
            if toes.is_some_and(|t| pos(t).is_none()) {
                mass += TOES_MASS;
            }
            sum = add(sum, scale(at, mass));
            total += mass;
        }
    }
    if let (true, Some(chest), Some(neck)) = (no_upper_chest, pos(Chest), pos(Neck)) {
        let at = add(chest, scale(sub(neck, chest), 5.0 / 6.0));
        sum = add(sum, scale(at, UPPER_CHEST_MASS));
        total += UPPER_CHEST_MASS;
    }
    if total > 0.0 { scale(sum, 1.0 / total) } else { sum }
}

/// The upper chest's own mass in [`MASSES`].
const UPPER_CHEST_MASS: f32 = 12.0;
/// A toe's own mass in [`MASSES`].
const TOES_MASS: f32 = 0.2;

/// The body's axes as a rotation: Y from the upper legs' midpoint to the upper arms', X the sum of the upper arms'
/// and upper legs' left-to-right vectors made perpendicular to Y. Unity's body rotation is this relative to the
/// rest pose's: `orientation(pose) * conj(orientation(rest))`. `None` without both upper arms and upper legs.
pub fn orientation(pos: impl Fn(HumanBone) -> Option<Vec3>) -> Option<Quat> {
    let (la, ra) = (pos(LeftUpperArm)?, pos(RightUpperArm)?);
    let (ll, rl) = (pos(LeftUpperLeg)?, pos(RightUpperLeg)?);
    let up = normalize(sub(scale(add(la, ra), 0.5), scale(add(ll, rl), 0.5)));
    let side = add(sub(ra, la), sub(rl, ll));
    let z = normalize(cross(side, up));
    Some(basis(cross(up, z), up, z))
}

/// The rotation whose columns are these orthonormal axes.
pub(crate) fn basis(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    let trace = x[0] + y[1] + z[2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(y[2] - z[1]) / s, (z[0] - x[2]) / s, (x[1] - y[0]) / s, 0.25 * s]
    } else if x[0] > y[1] && x[0] > z[2] {
        let s = (1.0 + x[0] - y[1] - z[2]).sqrt() * 2.0;
        [0.25 * s, (y[0] + x[1]) / s, (z[0] + x[2]) / s, (y[2] - z[1]) / s]
    } else if y[1] > z[2] {
        let s = (1.0 + y[1] - x[0] - z[2]).sqrt() * 2.0;
        [(y[0] + x[1]) / s, 0.25 * s, (z[1] + y[2]) / s, (z[0] - x[2]) / s]
    } else {
        let s = (1.0 + z[2] - x[0] - y[1]).sqrt() * 2.0;
        [(z[0] + x[2]) / s, (z[1] + y[2]) / s, 0.25 * s, (x[1] - y[0]) / s]
    };
    civbb_core::math::qnormalize(q)
}
