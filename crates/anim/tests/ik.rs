//! Full-body IK (ADR 0111) on the lean corpus humanoid: reachable targets are met within 1 mm with bone lengths
//! kept, unreachable ones stretch the limb straight, the view pitch turns the head, tracking control skips parts.

use civbb_anim::action::{PoseSources, part_index};
use civbb_anim::ik::{Goal, Solver, Targets};
use civbb_core::math::{Pose, dot, length, rotate, sub, world_poses};
use civbb_format::Document;
use civbb_format::animator::{PosePart, PoseSource};
use civbb_format::hierarchy::{HumanBone, NodeIndex};
use civbb_format::math::{Quat, Transform, Vec3};

const TRACKED: PoseSources = [PoseSource::Tracking; 10];

struct Avatar {
    doc: Document,
    solver: Solver,
    parents: Vec<Option<NodeIndex>>,
    rest: Vec<Transform>,
}

fn avatar() -> Avatar {
    let doc = civbb_corpus::generate(&civbb_corpus::lean());
    let solver = Solver::new(&doc).expect("the lean avatar is a full humanoid");
    let nodes = &doc.hierarchy.nodes;
    Avatar {
        parents: nodes.iter().map(|n| n.parent).collect(),
        rest: nodes.iter().map(|n| n.rest).collect(),
        solver,
        doc,
    }
}

impl Avatar {
    fn node(&self, bone: HumanBone) -> usize {
        let h = self.doc.avatar().and_then(|a| a.humanoid.as_ref());
        h.and_then(|h| h.node(bone)).expect("mapped") as usize
    }

    fn world(&self, locals: &[Transform]) -> Vec<Pose> {
        world_poses(&self.parents, Pose::IDENTITY, locals)
    }

    fn solve(&self, targets: &Targets, sources: &PoseSources) -> (Vec<Transform>, Vec<Pose>) {
        let mut locals = self.rest.clone();
        self.solver.solve(&mut locals, targets, sources);
        let world = self.world(&locals);
        (locals, world)
    }

    /// Every node's distance to its parent, the hips' excepted (they move).
    fn lengths(&self, world: &[Pose]) -> Vec<f32> {
        let hips = self.node(HumanBone::Hips);
        (self.parents.iter().enumerate())
            .map(|(i, p)| match p {
                Some(p) if i != hips => length(sub(world[i].pos, world[*p as usize].pos)),
                _ => 0.0,
            })
            .collect()
    }
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    length(sub(a, b))
}

/// The angle between two rotations, radians.
fn angle(a: Quat, b: Quat) -> f32 {
    let d: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    2.0 * d.abs().min(1.0).acos()
}

#[test]
fn reachable_hand_targets_are_met_within_a_millimetre_and_keep_bone_lengths() {
    let a = avatar();
    let rest = a.world(&a.rest);
    let turned: Quat = [0.0, 0.3826834, 0.0, 0.9238795];
    let left = Goal {
        pos: [0.25, 1.2, 0.3],
        rot: turned,
    };
    let right = Goal {
        pos: [-0.2, 1.0, 0.25],
        rot: [0.0, 0.0, 0.0, 1.0],
    };
    let targets = Targets {
        left_hand: Some(left),
        right_hand: Some(right),
        ..Targets::default()
    };
    let (_, world) = a.solve(&targets, &TRACKED);
    for (bone, goal) in [(HumanBone::LeftHand, left), (HumanBone::RightHand, right)] {
        let hand = world[a.node(bone)];
        assert!(distance(hand.pos, goal.pos) < 1e-3, "{bone:?} at {:?}", hand.pos);
        assert!(angle(hand.rot, goal.rot) < 1e-3);
    }
    for (i, (r, s)) in a.lengths(&rest).iter().zip(a.lengths(&world)).enumerate() {
        assert!((r - s).abs() < 1e-4, "node {i}: {r} became {s}");
    }
    // The elbows bend down and back, not up.
    for (upper, lower, hand) in [
        (HumanBone::LeftUpperArm, HumanBone::LeftLowerArm, HumanBone::LeftHand),
        (HumanBone::RightUpperArm, HumanBone::RightLowerArm, HumanBone::RightHand),
    ] {
        let (u, l, h) = (
            world[a.node(upper)].pos,
            world[a.node(lower)].pos,
            world[a.node(hand)].pos,
        );
        let mid = [(u[0] + h[0]) / 2.0, (u[1] + h[1]) / 2.0, (u[2] + h[2]) / 2.0];
        assert!(l[1] < mid[1], "{lower:?} at {l:?}, line middle {mid:?}");
    }
    // Nothing else moved: the legs and head keep their rest placement.
    for bone in [HumanBone::Head, HumanBone::LeftFoot, HumanBone::RightFoot] {
        assert!(distance(world[a.node(bone)].pos, rest[a.node(bone)].pos) < 1e-6);
    }
}

#[test]
fn unreachable_targets_stretch_the_arm_straight_at_them() {
    let a = avatar();
    for target in [[2.0, 1.4, 1.0], [0.0, 3.0, 0.0], [0.15, 1.4, 0.0]] {
        let goal = Goal {
            pos: target,
            rot: [0.0, 0.0, 0.0, 1.0],
        };
        let targets = Targets {
            left_hand: Some(goal),
            ..Targets::default()
        };
        let (locals, world) = a.solve(&targets, &TRACKED);
        assert!(
            locals
                .iter()
                .all(|l| l.rotation.iter().chain(&l.translation).all(|v| v.is_finite()))
        );
        let (u, l, h) = (
            world[a.node(HumanBone::LeftUpperArm)].pos,
            world[a.node(HumanBone::LeftLowerArm)].pos,
            world[a.node(HumanBone::LeftHand)].pos,
        );
        let reach = distance(l, u) + distance(h, l);
        let toward = sub(target, u);
        let wanted = length(toward).clamp((distance(l, u) - distance(h, l)).abs(), reach);
        let expected = civbb_core::math::add(u, civbb_core::math::scale(toward, wanted / length(toward)));
        assert!(
            distance(h, expected) < 1e-3,
            "{target:?}: hand at {h:?}, expected {expected:?}"
        );
    }
}

#[test]
fn the_head_reaches_its_target_and_the_feet_stay_planted() {
    let a = avatar();
    let rest = a.world(&a.rest);
    let head = rest[a.node(HumanBone::Head)];
    // Crouched and leaning forward, looking a little down.
    let goal = Goal {
        pos: [head.pos[0], head.pos[1] - 0.4, head.pos[2] + 0.15],
        rot: [0.13052619, 0.0, 0.0, 0.9914449],
    };
    let targets = Targets {
        head: Some(goal),
        ..Targets::default()
    };
    let (_, world) = a.solve(&targets, &TRACKED);
    let reached = world[a.node(HumanBone::Head)];
    assert!(distance(reached.pos, goal.pos) < 1e-3, "head at {:?}", reached.pos);
    assert!(angle(reached.rot, goal.rot) < 1e-3);
    for foot in [HumanBone::LeftFoot, HumanBone::RightFoot] {
        let (now, was) = (world[a.node(foot)], rest[a.node(foot)]);
        assert!(distance(now.pos, was.pos) < 1e-3, "{foot:?} at {:?}", now.pos);
        assert!(angle(now.rot, was.rot) < 1e-3);
    }
    // The knees bend forward.
    let knee = world[a.node(HumanBone::LeftLowerLeg)].pos;
    assert!(knee[2] > 0.05, "knee at {knee:?}");
    let hips = world[a.node(HumanBone::Hips)].pos;
    assert!(hips[1] < rest[a.node(HumanBone::Hips)].pos[1] - 0.2, "hips at {hips:?}");
    for (i, (r, s)) in a.lengths(&rest).iter().zip(a.lengths(&world)).enumerate() {
        assert!((r - s).abs() < 1e-4, "node {i}: {r} became {s}");
    }
}

#[test]
fn the_view_pitch_turns_the_head_up_and_down() {
    let a = avatar();
    let rest = a.world(&a.rest);
    let forward = |p: &Pose, rest: &Pose| rotate(p.rot, rotate(civbb_core::math::conj(rest.rot), [0.0, 0.0, 1.0]));
    for pitch in [0.6_f32, -0.6] {
        let targets = Targets {
            look: pitch,
            ..Targets::default()
        };
        let (_, world) = a.solve(&targets, &TRACKED);
        let h = a.node(HumanBone::Head);
        let f = forward(&world[h], &rest[h]);
        assert!((f[1] - pitch.sin()).abs() < 1e-4, "pitch {pitch}: head forward {f:?}");
        assert!((dot(f, [0.0, 0.0, 1.0]) - pitch.cos()).abs() < 1e-4);
        // The neck took part of it.
        let n = a.node(HumanBone::Neck);
        let neck = forward(&world[n], &rest[n]);
        assert!(neck[1] * pitch > 0.0 && neck[1].abs() < f[1].abs());
    }
}

#[test]
fn no_targets_and_animation_sources_leave_the_pose_untouched() {
    let a = avatar();
    let (locals, _) = a.solve(&Targets::default(), &TRACKED);
    assert_eq!(locals, a.rest);
    let goal = Goal {
        pos: [0.3, 1.0, 0.3],
        rot: [0.0, 0.0, 0.0, 1.0],
    };
    let targets = Targets {
        head: Some(goal),
        left_hand: Some(goal),
        right_hand: Some(goal),
        look: 0.5,
    };
    let mut animated = TRACKED;
    for part in [PosePart::Head, PosePart::LeftHand, PosePart::RightHand] {
        animated[part_index(part)] = PoseSource::Animation;
    }
    let (locals, _) = a.solve(&targets, &animated);
    assert_eq!(locals, a.rest);
    // Only the left hand tracked: only the left arm moves.
    animated[part_index(PosePart::LeftHand)] = PoseSource::Tracking;
    let (locals, _) = a.solve(&targets, &animated);
    let moved: Vec<usize> = (0..locals.len()).filter(|&i| locals[i] != a.rest[i]).collect();
    let arm = [HumanBone::LeftUpperArm, HumanBone::LeftLowerArm, HumanBone::LeftHand].map(|b| a.node(b));
    assert_eq!(moved, arm);
}
