//! Full-body IK (design doc §7, ADR 0111): head and hand targets in avatar space reached by an iterative spine and
//! hips and analytic two-bone limbs, on top of the animated pose.

use civbb_core::math::{
    IDENTITY, Pose, add, conj, dot, from_to, length, mul, qnormalize, rotate, scale, slerp, sub, unit,
};
use civbb_format::Document;
use civbb_format::animator::{PosePart, PoseSource};
use civbb_format::hierarchy::{HumanBone, NodeIndex};
use civbb_format::math::{Quat, Transform, Vec3};

use crate::action::{PoseSources, part_index};

/// Passes of the spine solver; the head's remaining offset moves the hips.
const SPINE_PASSES: usize = 2;
/// The view pitch's share taken by the upper chest (or chest) and the neck; the head turns the rest.
const LOOK_SHARES: [f32; 2] = [0.2, 0.3];
/// A limb whose middle joint is closer than this fraction of its upper bone to the root-to-end line counts as
/// straight: its bend direction comes from the rest frame.
const STRAIGHT: f32 = 1e-3;

/// Where a bone should be, in avatar space (the island origin's frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Goal {
    /// Position.
    pub pos: Vec3,
    /// Rotation.
    pub rot: Quat,
}

/// One island's IK targets for its next step, in avatar space. The default sets none, which leaves the animated pose.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Targets {
    /// The head bone.
    pub head: Option<Goal>,
    /// The left hand bone.
    pub left_hand: Option<Goal>,
    /// The right hand bone.
    pub right_hand: Option<Goal>,
    /// Desktop view pitch in radians, positive up: turns the animated head about the avatar's side axis, shared with
    /// the chest and neck. Applied before `head`.
    pub look: f32,
}

/// A two-bone limb: root, middle and end joints.
#[derive(Clone, Copy, Debug)]
struct Limb {
    upper: NodeIndex,
    lower: NodeIndex,
    end: NodeIndex,
    /// The bend direction for a straight limb, in the frame of `upper`'s parent.
    pole: Vec3,
}

/// A humanoid's IK solver, compiled once per avatar.
#[derive(Clone, Debug)]
pub struct Solver {
    parents: Vec<Option<NodeIndex>>,
    hips: NodeIndex,
    /// Mapped spine bones from the hips up to the neck, root first.
    spine: Vec<NodeIndex>,
    head: NodeIndex,
    /// Bones turned by the view pitch with their share; the head last, taking what is left.
    look: Vec<(NodeIndex, f32)>,
    /// Left, right.
    arms: [Limb; 2],
    /// Left, right.
    legs: [Limb; 2],
}

impl Solver {
    /// The solver for a humanoid with hips, spine, head, arms and legs mapped; `None` otherwise.
    pub fn new(avatar: &Document) -> Option<Self> {
        use HumanBone as H;
        let humanoid = avatar.avatar()?.humanoid.as_ref()?;
        let node = |b| humanoid.node(b);
        let nodes = &avatar.hierarchy.nodes;
        let parents: Vec<Option<NodeIndex>> = nodes.iter().map(|n| n.parent).collect();
        let rest: Vec<Transform> = nodes.iter().map(|n| n.rest).collect();
        let limb = |upper, lower, end, pole: Vec3| -> Option<Limb> {
            let upper = node(upper)?;
            let parent = parents[upper as usize].map_or(IDENTITY, |p| place(&parents, &rest, ROOT, p).rot);
            Some(Limb {
                upper,
                lower: node(lower)?,
                end: node(end)?,
                pole: rotate(conj(parent), pole),
            })
        };
        // Avatars face +Z with +Y up: elbows bend down and back, knees forward.
        let elbow = [0.0, -1.0, -1.0];
        let knee = [0.0, 0.0, 1.0];
        let spine: Vec<NodeIndex> = [H::Spine, H::Chest, H::UpperChest, H::Neck]
            .into_iter()
            .filter_map(node)
            .collect();
        let head = node(H::Head)?;
        let mut look: Vec<(NodeIndex, f32)> = [node(H::UpperChest).or(node(H::Chest)), node(H::Neck)]
            .into_iter()
            .zip(LOOK_SHARES)
            .filter_map(|(n, s)| Some((n?, s)))
            .collect();
        look.push((head, 1.0 - look.iter().map(|l| l.1).sum::<f32>()));
        Some(Self {
            hips: node(H::Hips)?,
            spine: (!spine.is_empty()).then_some(spine)?,
            head,
            look,
            arms: [
                limb(H::LeftUpperArm, H::LeftLowerArm, H::LeftHand, elbow)?,
                limb(H::RightUpperArm, H::RightLowerArm, H::RightHand, elbow)?,
            ],
            legs: [
                limb(H::LeftUpperLeg, H::LeftLowerLeg, H::LeftFoot, knee)?,
                limb(H::RightUpperLeg, H::RightLowerLeg, H::RightFoot, knee)?,
            ],
            parents,
        })
    }

    /// Moves the animated pose in `locals` towards `targets`, skipping parts whose pose source is the animator
    /// (tracking control). Rotates bones only, apart from the hips, which move when the head needs them to; the feet
    /// then keep their animated placement. Allocates nothing.
    pub fn solve(&self, locals: &mut [Transform], targets: &Targets, sources: &PoseSources) {
        let tracked = |p: PosePart| sources[part_index(p)] == PoseSource::Tracking;
        let head = targets.head.filter(|_| tracked(PosePart::Head));
        if targets.look != 0.0 && tracked(PosePart::Head) {
            // About the avatar's side axis: right is -X, so a positive pitch turns +Z towards +Y about -X.
            for &(node, share) in &self.look {
                let (s, c) = (-0.5 * targets.look * share).sin_cos();
                self.rotate(locals, ROOT, node, [s, 0.0, 0.0, c]);
            }
        }
        if let Some(goal) = head {
            let feet = self.legs.map(|l| self.place(locals, ROOT, l.end));
            self.reach_head(locals, goal);
            for (leg, foot) in self.legs.iter().zip(feet) {
                self.limb(locals, leg, foot.pos, foot.rot);
            }
        }
        let hands = [
            (targets.left_hand, PosePart::LeftHand),
            (targets.right_hand, PosePart::RightHand),
        ];
        for (arm, (goal, part)) in self.arms.iter().zip(hands) {
            if let Some(goal) = goal.filter(|_| tracked(part)) {
                self.limb(locals, arm, goal.pos, goal.rot);
            }
        }
    }

    /// The hips follow the head's height change, the spine bends the head towards `goal` (each bone from the root up
    /// taking its share of the remaining turn), the hips take the offset left, then the head takes the rotation.
    fn reach_head(&self, locals: &mut [Transform], goal: Goal) {
        let at = self.anchor(locals, self.hips);
        let rise = goal.pos[1] - self.place(locals, at, self.head).pos[1];
        self.translate(locals, at, self.hips, [0.0, rise, 0.0]);
        let n = self.spine.len();
        for _ in 0..SPINE_PASSES {
            for (k, &bone) in self.spine.iter().enumerate() {
                let joint = self.place(locals, at, bone).pos;
                let head = self.place(locals, at, self.head).pos;
                if let (Some(from), Some(to)) = (unit(sub(head, joint)), unit(sub(goal.pos, joint))) {
                    let turn = slerp(IDENTITY, from_to(from, to), 1.0 / (n - k) as f32);
                    self.rotate(locals, at, bone, turn);
                }
            }
        }
        let left = sub(goal.pos, self.place(locals, at, self.head).pos);
        self.translate(locals, at, self.hips, left);
        self.set_rotation(locals, at, self.head, goal.rot);
    }

    /// Two-bone IK: the limb's end towards `target`, stretched straight at it when out of reach, then turned to
    /// `rot`. The middle joint bends towards where it was, or towards the limb's rest pole when the limb is straight.
    fn limb(&self, locals: &mut [Transform], limb: &Limb, target: Vec3, rot: Quat) {
        let at = self.anchor(locals, limb.upper);
        let (a, b, c) = (
            self.place(locals, at, limb.upper).pos,
            self.place(locals, at, limb.lower).pos,
            self.place(locals, at, limb.end).pos,
        );
        let (upper, lower) = (length(sub(b, a)), length(sub(c, b)));
        let Some(dir) = unit(sub(target, a)) else { return };
        if upper <= f32::EPSILON || lower <= f32::EPSILON {
            return;
        }
        let d = length(sub(target, a)).clamp((upper - lower).abs(), upper + lower);
        let across = |v: Vec3, d: Vec3| sub(v, scale(d, dot(v, d)));
        let hint = unit(sub(c, a))
            .map(|ac| across(sub(b, a), ac))
            .filter(|v| length(*v) > STRAIGHT * upper)
            .unwrap_or_else(|| rotate(at.1.rot, limb.pole));
        let Some(bend) = unit(across(hint, dir)) else { return };
        let cos = ((upper * upper + d * d - lower * lower) / (2.0 * upper * d)).clamp(-1.0, 1.0);
        let sin = (1.0 - cos * cos).max(0.0).sqrt();
        let elbow = add(a, add(scale(dir, upper * cos), scale(bend, upper * sin)));
        if let (Some(from), Some(to)) = (unit(sub(b, a)), unit(sub(elbow, a))) {
            self.rotate(locals, at, limb.upper, from_to(from, to));
        }
        let (b, c) = (
            self.place(locals, at, limb.lower).pos,
            self.place(locals, at, limb.end).pos,
        );
        if let (Some(from), Some(to)) = (unit(sub(c, b)), unit(sub(add(a, scale(dir, d)), b))) {
            self.rotate(locals, at, limb.lower, from_to(from, to));
        }
        self.set_rotation(locals, at, limb.end, rot);
    }

    /// The parent of `node` placed, as the anchor for placing `node` and the nodes below it.
    fn anchor(&self, locals: &[Transform], node: NodeIndex) -> Anchor {
        let parent = self.parents[node as usize];
        (parent, parent.map_or(Pose::IDENTITY, |p| self.place(locals, ROOT, p)))
    }

    /// `node`'s placement in avatar space, walking up from `node` to the anchor `at`.
    fn place(&self, locals: &[Transform], at: Anchor, node: NodeIndex) -> Pose {
        place(&self.parents, locals, at, node)
    }

    /// The rotation of `node`'s parent in avatar space.
    fn parent_rotation(&self, locals: &[Transform], at: Anchor, node: NodeIndex) -> Quat {
        self.parents[node as usize].map_or(IDENTITY, |p| self.place(locals, at, p).rot)
    }

    /// Turns `node` (and everything below it) by `q` about its joint, `q` in avatar space.
    fn rotate(&self, locals: &mut [Transform], at: Anchor, node: NodeIndex, q: Quat) {
        let p = self.parent_rotation(locals, at, node);
        let l = &mut locals[node as usize].rotation;
        *l = qnormalize(mul(conj(p), mul(q, mul(p, *l))));
    }

    /// Sets `node`'s rotation in avatar space.
    fn set_rotation(&self, locals: &mut [Transform], at: Anchor, node: NodeIndex, rot: Quat) {
        let p = self.parent_rotation(locals, at, node);
        locals[node as usize].rotation = qnormalize(mul(conj(p), rot));
    }

    /// Moves `node` by `delta` in avatar space.
    fn translate(&self, locals: &mut [Transform], at: Anchor, node: NodeIndex, delta: Vec3) {
        let p = self.parents[node as usize].map_or(Pose::IDENTITY, |p| self.place(locals, at, p));
        if p.scale.abs() <= f32::EPSILON {
            return;
        }
        let t = &mut locals[node as usize].translation;
        *t = add(*t, scale(rotate(conj(p.rot), delta), 1.0 / p.scale));
    }
}

/// A node whose placement is known, or `None` for the avatar root's parent, with that placement: placing a node
/// below it walks up only to there.
type Anchor = (Option<NodeIndex>, Pose);

/// The avatar root's parent, at the avatar-space origin.
const ROOT: Anchor = (None, Pose::IDENTITY);

/// `node`'s placement in avatar space from its ancestors' local transforms, walking up to the anchor `at` (or to the
/// root when `at` is not an ancestor).
fn place(parents: &[Option<NodeIndex>], locals: &[Transform], at: Anchor, node: NodeIndex) -> Pose {
    let parent = parents[node as usize];
    let p = if parent == at.0 {
        at.1
    } else {
        parent.map_or(Pose::IDENTITY, |p| place(parents, locals, at, p))
    };
    p.then(&locals[node as usize])
}
