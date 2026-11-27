//! Full-body IK (ADRs 0111, 0112) on the lean corpus humanoid: reachable targets are met within 1 mm with bone
//! lengths kept, unreachable ones stretch the limb straight, the view pitch turns the head, tracking control skips
//! parts, and 6- and 10-point goals taken from a scripted pose sequence put every tracked bone back where it was.

use civbb_anim::action::{PoseSources, part_index};
use civbb_anim::ik::{Goal, Solver, Targets};
use civbb_core::math::{Pose, dot, euler_to_quat, length, mul, rotate, sub, world_poses};
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

/// How far two rotations differ: the largest component difference with `b`'s sign matched to `a` (about half the angle
/// in radians; `acos` of the dot product is too noisy in f32 near zero).
fn angle(a: Quat, b: Quat) -> f32 {
    let d: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let s = if d < 0.0 { -1.0 } else { 1.0 };
    a.iter().zip(&b).map(|(x, y)| (x - s * y).abs()).fold(0.0, f32::max)
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
        hands: [Some(left), Some(right)],
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
            hands: [Some(goal), None],
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
        hands: [Some(goal); 2],
        look: 0.5,
        ..Targets::default()
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

/// A scripted pose `t` seconds in: hips lowered, shifted and turned, the spine bent, arms and legs bent, all moving.
fn truth(a: &Avatar, t: f32) -> Vec<Pose> {
    use HumanBone as H;
    let s = t.sin();
    let mut locals = a.rest.clone();
    let hips = a.node(H::Hips);
    locals[hips].translation[0] += 0.05 * s;
    locals[hips].translation[1] -= 0.15 + 0.05 * s;
    locals[hips].translation[2] += 0.1;
    // Degrees about X, Y, Z (Unity's order): +X bends the spine forward, legs forward are -X, knees +X.
    let turns = [
        (H::Hips, [0.0, 20.0 * s, 0.0]),
        (H::Spine, [10.0, 0.0, 0.0]),
        (H::Chest, [10.0 + 5.0 * s, 5.0 * s, 0.0]),
        (H::Neck, [-10.0, 0.0, 0.0]),
        (H::Head, [0.0, 20.0 * s, 0.0]),
        (H::LeftUpperArm, [0.0, -30.0, -50.0 + 10.0 * s]),
        (H::LeftLowerArm, [0.0, -60.0 - 20.0 * s, 0.0]),
        (H::RightUpperArm, [0.0, 30.0, 50.0]),
        (H::RightLowerArm, [0.0, 60.0 + 20.0 * s, 0.0]),
        (H::LeftUpperLeg, [-40.0 - 10.0 * s, 0.0, 0.0]),
        (H::LeftLowerLeg, [60.0 + 10.0 * s, 0.0, 0.0]),
        (H::LeftFoot, [10.0 * s, 0.0, 0.0]),
        (H::RightUpperLeg, [-20.0, 0.0, 0.0]),
        (H::RightLowerLeg, [30.0, 0.0, 0.0]),
    ];
    for (bone, deg) in turns {
        let i = a.node(bone);
        locals[i].rotation = mul(locals[i].rotation, euler_to_quat(deg));
    }
    a.world(&locals)
}

/// Solves from the rest pose towards `targets(world)` for a sequence of scripted poses and checks that every bone in
/// `met` lands where the scripted pose has it, within 1 mm and 1 mrad, with bone lengths kept.
fn follows_the_script(targets: impl Fn(&Avatar, &[Pose]) -> Targets, met: &[(HumanBone, bool)]) {
    let a = avatar();
    let rest = a.world(&a.rest);
    for frame in 0..20 {
        let truth = truth(&a, frame as f32 * 0.3);
        let (_, world) = a.solve(&targets(&a, &truth), &TRACKED);
        for &(bone, rotation) in met {
            let (got, want) = (world[a.node(bone)], truth[a.node(bone)]);
            assert!(
                distance(got.pos, want.pos) < 1e-3,
                "frame {frame}: {bone:?} at {:?}, scripted {:?}",
                got.pos,
                want.pos
            );
            let turned = angle(got.rot, want.rot);
            assert!(!rotation || turned < 1e-3, "frame {frame}: {bone:?} turned {turned}");
        }
        for (i, (r, s)) in a.lengths(&rest).iter().zip(a.lengths(&world)).enumerate() {
            assert!((r - s).abs() < 1e-4, "frame {frame}: node {i}: {r} became {s}");
        }
    }
}

fn goal(a: &Avatar, world: &[Pose], bone: HumanBone) -> Option<Goal> {
    let p = world[a.node(bone)];
    Some(Goal { pos: p.pos, rot: p.rot })
}

fn six_point(a: &Avatar, world: &[Pose]) -> Targets {
    use HumanBone as H;
    Targets {
        head: goal(a, world, H::Head),
        hands: [goal(a, world, H::LeftHand), goal(a, world, H::RightHand)],
        hips: goal(a, world, H::Hips),
        feet: [goal(a, world, H::LeftFoot), goal(a, world, H::RightFoot)],
        ..Targets::default()
    }
}

#[test]
fn six_point_targets_follow_a_scripted_pose() {
    use HumanBone as H;
    let met = [H::Hips, H::Head, H::LeftHand, H::RightHand, H::LeftFoot, H::RightFoot].map(|b| (b, true));
    follows_the_script(six_point, &met);
}

#[test]
fn ten_point_targets_place_the_chest_elbows_and_knees_too() {
    use HumanBone as H;
    let ten_point = |a: &Avatar, world: &[Pose]| {
        let at = |b| Some(world[a.node(b)].pos);
        Targets {
            chest: goal(a, world, H::Chest),
            elbows: [at(H::LeftLowerArm), at(H::RightLowerArm)],
            knees: [at(H::LeftLowerLeg), at(H::RightLowerLeg)],
            ..six_point(a, world)
        }
    };
    let mut met = vec![(H::Chest, true)];
    met.extend([H::Hips, H::Head, H::LeftHand, H::RightHand, H::LeftFoot, H::RightFoot].map(|b| (b, true)));
    met.extend([H::LeftLowerArm, H::RightLowerArm, H::LeftLowerLeg, H::RightLowerLeg].map(|b| (b, false)));
    follows_the_script(ten_point, &met);
}
