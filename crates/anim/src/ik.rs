//! Full-body IK (design doc §7, ADRs 0111, 0112): 3-, 6- and 10-point goals in avatar space reached by a FABRIK
//! spine, the hips and analytic two-bone limbs, on top of the animated pose.

use civbb_core::math::{IDENTITY, Pose, add, conj, dot, from_to, length, mul, qnormalize, rotate, scale, sub, unit};
use civbb_format::Document;
use civbb_format::animator::{PosePart, PoseSource};
use civbb_format::hierarchy::{HumanBone, NodeIndex};
use civbb_format::math::{Quat, Transform, Vec3};

use crate::action::{PoseSources, part_index};

/// Joint positions a spine chain holds: spine, chest, upper chest, neck and the bone they move.
const CHAIN: usize = 5;
/// Most FABRIK passes of the spine; it stops earlier once the end is within [`FABRIK_DONE`] metres of its goal.
const FABRIK_PASSES: usize = 40;
const FABRIK_DONE: f32 = 1e-4;
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

/// One island's IK targets for its next step, in avatar space; pairs are left, right. The default sets none, which
/// leaves the animated pose. 3-point sets the head and hands, 6-point adds the hips and feet, 10+ the chest, elbows
/// and knees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Targets {
    /// The head bone.
    pub head: Option<Goal>,
    /// The hand bones.
    pub hands: [Option<Goal>; 2],
    /// The hips bone.
    pub hips: Option<Goal>,
    /// The upper chest bone, or the chest without one.
    pub chest: Option<Goal>,
    /// The foot bones.
    pub feet: [Option<Goal>; 2],
    /// Points the elbows bend towards.
    pub elbows: [Option<Vec3>; 2],
    /// Points the knees bend towards.
    pub knees: [Option<Vec3>; 2],
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
    /// The bone in `spine` a chest goal places: the upper chest, or the chest without one.
    chest: Option<usize>,
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
        let chest = node(H::UpperChest)
            .or(node(H::Chest))
            .and_then(|c| spine.iter().position(|&s| s == c));
        Some(Self {
            hips: node(H::Hips)?,
            chest,
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
    /// (tracking control: `Hip` gates the hips and chest, a hand or foot its elbow or knee). Rotates bones only, apart
    /// from the hips, which take their goal or, without one, move when the head needs them to; feet without a goal
    /// then keep their animated placement. Allocates nothing.
    pub fn solve(&self, locals: &mut [Transform], targets: &Targets, sources: &PoseSources) {
        use PosePart as P;
        let tracked = |p: PosePart| sources[part_index(p)] == PoseSource::Tracking;
        let head = targets.head.filter(|_| tracked(P::Head));
        let hips = targets.hips.filter(|_| tracked(P::Hip));
        let chest = (targets.chest.filter(|_| tracked(P::Hip))).zip(self.chest);
        if targets.look != 0.0 && tracked(P::Head) {
            // About the avatar's side axis: right is -X, so a positive pitch turns +Z towards +Y about -X.
            for &(node, share) in &self.look {
                let (s, c) = (-0.5 * targets.look * share).sin_cos();
                self.rotate(locals, ROOT, node, [s, 0.0, 0.0, c]);
            }
        }
        let animated_feet = self.legs.map(|l| self.place(locals, ROOT, l.end));
        let at = self.anchor(locals, self.hips);
        if let Some(goal) = hips {
            let left = sub(goal.pos, self.place(locals, at, self.hips).pos);
            self.translate(locals, at, self.hips, left);
            self.set_rotation(locals, at, self.hips, goal.rot);
        }
        let mut spine = &self.spine[..];
        if let Some((goal, i)) = chest {
            self.bend(locals, at, &self.spine[..i], self.spine[i], goal.pos);
            self.set_rotation(locals, at, self.spine[i], goal.rot);
            spine = &self.spine[i + 1..];
        }
        if let Some(goal) = head {
            if hips.is_none() && chest.is_none() {
                // 3-point: the hips follow the head's height, then take what the spine leaves.
                let rise = goal.pos[1] - self.place(locals, at, self.head).pos[1];
                self.translate(locals, at, self.hips, [0.0, rise, 0.0]);
                self.bend(locals, at, spine, self.head, goal.pos);
                let left = sub(goal.pos, self.place(locals, at, self.head).pos);
                self.translate(locals, at, self.hips, left);
            } else {
                self.bend(locals, at, spine, self.head, goal.pos);
            }
            self.set_rotation(locals, at, self.head, goal.rot);
        }
        let moved = hips.is_some() || head.is_some();
        let feet = [P::LeftFoot, P::RightFoot].map(&tracked);
        for (k, leg) in self.legs.iter().enumerate() {
            let goal = (targets.feet[k].filter(|_| feet[k])).map(|g| (g.pos, g.rot));
            let animated = moved.then_some((animated_feet[k].pos, animated_feet[k].rot));
            if let Some((pos, rot)) = goal.or(animated) {
                self.limb(locals, leg, pos, rot, targets.knees[k].filter(|_| feet[k]));
            }
        }
        for (k, part) in [P::LeftHand, P::RightHand].into_iter().enumerate() {
            if let Some(goal) = targets.hands[k].filter(|_| tracked(part)) {
                self.limb(locals, &self.arms[k], goal.pos, goal.rot, targets.elbows[k]);
            }
        }
    }

    /// Bends `bones` (a chain from the root up, at most four) so that `tip`, a node below the last, moves towards
    /// `goal`: FABRIK on the joint positions, then each bone swung onto its new direction, root first. A goal out of
    /// reach straightens the chain towards it.
    fn bend(&self, locals: &mut [Transform], at: Anchor, bones: &[NodeIndex], tip: NodeIndex, goal: Vec3) {
        let n = bones.len();
        if n == 0 || n >= CHAIN {
            return;
        }
        let mut p = [[0.0; 3]; CHAIN];
        for (k, &bone) in bones.iter().enumerate() {
            p[k] = self.place(locals, at, bone).pos;
        }
        p[n] = self.place(locals, at, tip).pos;
        let mut len = [0.0; CHAIN];
        for k in 0..n {
            len[k] = length(sub(p[k + 1], p[k]));
        }
        // `b` at `len` from `a`, on the line from `a` through where `b` was.
        let toward = |a: Vec3, b: Vec3, len: f32| add(a, scale(unit(sub(b, a)).unwrap_or([0.0, 1.0, 0.0]), len));
        let root = p[0];
        let passes = if length(sub(goal, root)) >= len.iter().sum() {
            // Out of reach: straight at the goal.
            for k in 0..n {
                p[k + 1] = toward(p[k], goal, len[k]);
            }
            0
        } else {
            FABRIK_PASSES
        };
        for _ in 0..passes {
            p[n] = goal;
            for k in (0..n).rev() {
                p[k] = toward(p[k + 1], p[k], len[k]);
            }
            p[0] = root;
            for k in 0..n {
                p[k + 1] = toward(p[k], p[k + 1], len[k]);
            }
            if length(sub(p[n], goal)) < FABRIK_DONE {
                break;
            }
        }
        for (k, &bone) in bones.iter().enumerate() {
            let child = bones.get(k + 1).copied().unwrap_or(tip);
            let (joint, next) = (self.place(locals, at, bone).pos, self.place(locals, at, child).pos);
            if let (Some(from), Some(to)) = (unit(sub(next, joint)), unit(sub(p[k + 1], joint))) {
                self.rotate(locals, at, bone, from_to(from, to));
            }
        }
    }

    /// Two-bone IK: the limb's end towards `target`, stretched straight at it when out of reach, then turned to
    /// `rot`. The middle joint bends towards `hint` if given, else towards where it was, or towards the limb's rest
    /// pole when the limb is straight.
    fn limb(&self, locals: &mut [Transform], limb: &Limb, target: Vec3, rot: Quat, hint: Option<Vec3>) {
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
        let off_line = |v: &Vec3| length(across(*v, dir)) > STRAIGHT * upper;
        let hint = (hint.map(|h| sub(h, a)).filter(off_line))
            .or_else(|| unit(sub(c, a)).map(|ac| across(sub(b, a), ac)).filter(off_line))
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

/// `v` without its part along unit `d`.
fn across(v: Vec3, d: Vec3) -> Vec3 {
    sub(v, scale(d, dot(v, d)))
}
