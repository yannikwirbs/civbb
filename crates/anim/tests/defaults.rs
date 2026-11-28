//! Built-in locomotion (ADR 0108) on the generated humanoid: proxy names, the merged layers, and the poses they
//! play judged by where bones end up (feet on the floor, legs alternating, crouched, prone, seated, floor poses).

use civbb_anim::defaults::{Hand, Kind, POSE, Style, apply, proxy};
use civbb_anim::frames::derive;
use civbb_anim::muscle::find;
use civbb_anim::{Controller, Instance};
use civbb_core::math::{Pose, world_poses};
use civbb_format::animator::{Clip, HumanoidProperty, Target};
use civbb_format::hierarchy::{BoneNode, HumanBone, Node};
use civbb_format::math::Transform;
use civbb_format::state::Platform;
use civbb_format::{Caps, Document, validate};

const DT: f32 = 1.0 / 60.0;

/// The lean corpus avatar with index and middle fingers, muscle frames and an FX layer only.
fn humanoid() -> Document {
    use HumanBone as H;
    let mut doc = civbb_corpus::generate(&civbb_corpus::lean());
    let fingers = [
        (
            H::LeftHand,
            1.0,
            [H::LeftIndexProximal, H::LeftIndexIntermediate, H::LeftIndexDistal],
            0.02,
        ),
        (
            H::LeftHand,
            1.0,
            [H::LeftMiddleProximal, H::LeftMiddleIntermediate, H::LeftMiddleDistal],
            -0.02,
        ),
        (
            H::RightHand,
            -1.0,
            [H::RightIndexProximal, H::RightIndexIntermediate, H::RightIndexDistal],
            0.02,
        ),
        (
            H::RightHand,
            -1.0,
            [H::RightMiddleProximal, H::RightMiddleIntermediate, H::RightMiddleDistal],
            -0.02,
        ),
    ];
    let mut bones = Vec::new();
    for (hand, side, phalanges, z) in fingers {
        let h = doc.avatar().unwrap().humanoid.as_ref().unwrap();
        let mut parent = h.node(hand).unwrap();
        for (k, bone) in phalanges.into_iter().map(Some).chain([None]).enumerate() {
            let offset = if k == 0 {
                [side * 0.08, 0.0, z]
            } else {
                [side * 0.03, 0.0, 0.0]
            };
            doc.hierarchy.nodes.push(Node {
                name: format!("{bone:?}{parent}"),
                parent: Some(parent),
                rest: Transform {
                    translation: offset,
                    ..Transform::default()
                },
                active: true,
            });
            parent = (doc.hierarchy.nodes.len() - 1) as u32;
            if let Some(bone) = bone {
                bones.push(BoneNode { bone, node: parent });
            }
        }
    }
    let hierarchy = doc.hierarchy.clone();
    let h = doc.avatar_mut().unwrap().humanoid.as_mut().unwrap();
    h.bones.extend(bones);
    h.muscle_frames = derive(&hierarchy, h);
    doc
}

struct Run {
    doc: Document,
    c: Controller,
    i: Instance,
}

impl Run {
    fn new() -> Self {
        let mut doc = humanoid();
        assert!(apply(&mut doc));
        validate(&doc, &Caps::default(), |_| true).unwrap();
        let c = Controller::new(&doc).unwrap();
        let i = Instance::new(&c, true);
        Run { doc, c, i }
    }

    fn set(&mut self, p: Platform, v: f32) {
        self.i.set_platform(&self.c, p, v);
    }

    fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.i.update(&self.c, DT);
        }
    }

    /// World position of `bone` in the current pose.
    fn at(&self, bone: HumanBone) -> [f32; 3] {
        let nodes = &self.doc.hierarchy.nodes;
        let mut locals: Vec<_> = nodes.iter().map(|n| n.rest).collect();
        let mut active: Vec<_> = nodes.iter().map(|n| n.active).collect();
        self.i.write_nodes(&self.c, &mut locals, &mut active);
        let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
        let world = world_poses(&parents, Pose::IDENTITY, &locals);
        let h = self.doc.avatar().unwrap().humanoid.as_ref().unwrap();
        world[h.node(bone).unwrap() as usize].pos
    }

    /// The lowest human bone's height.
    fn low(&self) -> f32 {
        let h = self.doc.avatar().unwrap().humanoid.as_ref().unwrap();
        h.bones.iter().map(|b| self.at(b.bone)[1]).fold(f32::INFINITY, f32::min)
    }
}

/// The rest pose's lowest human bone: the floor every grounded pose keeps.
fn floor() -> f32 {
    let doc = humanoid();
    let nodes = &doc.hierarchy.nodes;
    let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
    let world = world_poses(
        &parents,
        Pose::IDENTITY,
        &nodes.iter().map(|n| n.rest).collect::<Vec<_>>(),
    );
    let h = doc.avatar().unwrap().humanoid.as_ref().unwrap();
    h.bones
        .iter()
        .map(|b| world[b.node as usize].pos[1])
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn proxy_names_map_to_generated_clips() {
    let gait = |style, forward, right| Some(Kind::Gait { style, forward, right });
    let d = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(proxy("proxy_stand_still"), Some(Kind::Idle));
    assert_eq!(proxy("proxy_stand_still3"), Some(Kind::Idle));
    assert_eq!(proxy("proxy_idle 1"), Some(Kind::Idle));
    assert_eq!(proxy("proxy_walk_forward"), gait(Style::Walk, 1.0, 0.0));
    assert_eq!(proxy("proxy_walk_backward"), gait(Style::Walk, -1.0, 0.0));
    assert_eq!(proxy("proxy_strafe_right_135"), gait(Style::Walk, -d, d));
    assert_eq!(proxy("proxy_run_strafe_right_45"), gait(Style::Run, d, d));
    assert_eq!(proxy("proxy_sprint_forward"), gait(Style::Sprint, 1.0, 0.0));
    assert_eq!(proxy("proxy_crouch_still"), gait(Style::Crouch, 0.0, 0.0));
    assert_eq!(proxy("proxy_crouch_walk_right"), gait(Style::Crouch, 0.0, 1.0));
    assert_eq!(proxy("proxy_low_crawl_still"), gait(Style::Crawl, 0.0, 0.0));
    assert_eq!(proxy("proxy_low_crawl_forward"), gait(Style::Crawl, 1.0, 0.0));
    assert_eq!(proxy("proxy_fall_long"), Some(Kind::Fall));
    assert_eq!(proxy("proxy_landing"), Some(Kind::Land { deep: true }));
    assert_eq!(proxy("proxy_afk"), Some(Kind::Afk));
    assert_eq!(proxy("proxy_sit"), Some(Kind::Sit));
    assert_eq!(proxy("proxy_hands_idle2"), Some(Kind::Hands(Hand::Idle)));
    assert_eq!(proxy("proxy_hands_thumbs_up"), Some(Kind::Hands(Hand::ThumbsUp)));
    // Emotes stay empty (#151); other names are not proxies.
    assert_eq!(proxy("proxy_dance"), None);
    assert_eq!(proxy("proxy_seated_clap"), None);
    assert_eq!(proxy("walk_forward"), None);
}

#[test]
fn empty_proxy_clips_are_filled_and_others_kept() {
    let mut doc = humanoid();
    let empty = |name: &str| Clip {
        name: name.into(),
        length: 0.0,
        looping: false,
        root: None,
        curves: Vec::new(),
    };
    doc.clips.push(empty("proxy_hands_fist"));
    doc.clips.push(empty("proxy_dance"));
    let mut own = doc.clips[0].clone();
    own.name = "proxy_walk_forward".into();
    let kept = own.clone();
    doc.clips.push(own);
    let n = doc.clips.len();
    apply(&mut doc);
    let fist = &doc.clips[n - 3];
    let index = find("LeftHand.Index.2 Stretched").unwrap();
    let curl = fist.curves.iter().find(|c| {
        c.target
            == Target::Humanoid {
                property: civbb_anim::muscle::property(index),
            }
    });
    assert_eq!(curl.unwrap().keys[0].value, -1.0, "a fist curls the fingers");
    assert!(!fist.curves.iter().any(|c| matches!(
        c.target,
        Target::Humanoid {
            property: HumanoidProperty::Body { .. }
        }
    )));
    assert!(doc.clips[n - 2].curves.is_empty(), "emotes stay empty");
    assert_eq!(doc.clips[n - 1], kept, "a creator's own clip under a proxy name stays");
}

#[test]
fn layers_are_added_only_to_empty_groups() {
    let mut doc = humanoid();
    let fx = doc.animator.as_ref().unwrap().layers.len();
    assert!(apply(&mut doc));
    let a = doc.animator.as_ref().unwrap();
    let names: Vec<_> = a.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names.len(), fx + 4);
    assert_eq!(names[..3], ["civbb/locomotion", "civbb/hand_left", "civbb/hand_right"]);
    assert_eq!(a.groups[a.layers[0].group as usize].name, "Base");
    assert_eq!(a.groups[a.layers[1].group as usize].name, "Gesture");
    let action = names.iter().position(|n| *n == "civbb/action").unwrap();
    assert_eq!(a.groups[a.layers[action].group as usize].name, "Action");
    validate(&doc, &Caps::default(), |_| true).unwrap();
    // A second merge finds both groups filled and only proxies to fill.
    let before = doc.clone();
    assert!(!apply(&mut doc));
    assert_eq!(doc, before);
}

#[test]
fn idle_stands_on_the_floor_with_the_arms_down() {
    let mut r = Run::new();
    r.step(30);
    assert!(
        (r.low() - floor()).abs() < 0.01,
        "lowest bone {} floor {}",
        r.low(),
        floor()
    );
    for (hand, shoulder) in [
        (HumanBone::LeftHand, HumanBone::LeftUpperArm),
        (HumanBone::RightHand, HumanBone::RightUpperArm),
    ] {
        assert!(r.at(hand)[1] < r.at(shoulder)[1] - 0.3, "{hand:?} hangs");
    }
}

#[test]
fn walking_alternates_the_feet_on_the_floor_and_swings_the_arms_against_them() {
    let mut r = Run::new();
    r.set(Platform::VelocityZ, 2.0);
    r.step(30);
    let (mut ahead, mut behind, mut against) = (0.0f32, 0.0f32, 0.0f32);
    for _ in 0..60 {
        r.step(1);
        let feet = r.at(HumanBone::LeftFoot)[2] - r.at(HumanBone::RightFoot)[2];
        let hands = r.at(HumanBone::LeftHand)[2] - r.at(HumanBone::RightHand)[2];
        ahead = ahead.max(feet);
        behind = behind.min(feet);
        against += feet * hands;
        assert!(
            (r.low() - floor()).abs() < 0.02,
            "lowest bone {} floor {}",
            r.low(),
            floor()
        );
    }
    assert!(ahead > 0.25 && behind < -0.25, "stride {behind}..{ahead}");
    assert!(against < 0.0, "arms swing with the legs");
}

#[test]
fn strafing_steps_the_legs_apart_without_crossing() {
    let mut r = Run::new();
    r.set(Platform::VelocityX, 2.0);
    r.step(30);
    let (mut widest, mut narrowest) = (0.0f32, f32::INFINITY);
    for _ in 0..60 {
        r.step(1);
        let apart = r.at(HumanBone::LeftFoot)[0] - r.at(HumanBone::RightFoot)[0];
        widest = widest.max(apart);
        narrowest = narrowest.min(apart);
    }
    assert!(widest > 0.35, "legs step out: {widest}");
    assert!(narrowest > 0.1, "legs do not cross: {narrowest}");
}

#[test]
fn crouching_lowers_the_hips_and_bends_the_knees_forward() {
    let mut r = Run::new();
    let standing = r.at(HumanBone::Hips)[1];
    r.set(Platform::Upright, 0.55);
    r.step(60);
    let hips = r.at(HumanBone::Hips);
    assert!(hips[1] < 0.8 * standing, "hips {} standing {standing}", hips[1]);
    assert!(r.at(HumanBone::LeftLowerLeg)[2] > hips[2] + 0.2, "knees forward");
    assert!((r.low() - floor()).abs() < 0.01);
}

#[test]
fn prone_lies_face_down_with_the_head_forward() {
    let mut r = Run::new();
    r.set(Platform::Upright, 0.25);
    r.step(60);
    let (head, hips) = (r.at(HumanBone::Head), r.at(HumanBone::Hips));
    assert!(head[1] < 0.35 && hips[1] < 0.35, "head {head:?} hips {hips:?}");
    assert!(head[2] > hips[2] + 0.4, "head ahead of the hips: {head:?} {hips:?}");
    assert!((r.low() - floor()).abs() < 0.01);
}

#[test]
fn falling_and_landing_follow_grounded() {
    let mut r = Run::new();
    r.set(Platform::Grounded, 0.0);
    r.step(30);
    assert_eq!(r.i.layer(0).unwrap().current, 3);
    r.set(Platform::Grounded, 1.0);
    r.step(30);
    assert_eq!(r.i.layer(0).unwrap().current, 0);
}

#[test]
fn afk_and_sitting_play_on_the_action_layer() {
    let mut r = Run::new();
    r.step(10);
    let head = r.at(HumanBone::Head);
    r.set(Platform::Afk, 1.0);
    r.step(60);
    assert!(r.at(HumanBone::Head)[1] < head[1] - 0.02, "the head drops");
    r.set(Platform::Afk, 0.0);
    r.step(60);
    assert!((r.at(HumanBone::Head)[1] - head[1]).abs() < 0.01, "and comes back");
    r.set(Platform::Seated, 1.0);
    r.step(60);
    let (knee, hips) = (r.at(HumanBone::LeftLowerLeg), r.at(HumanBone::Hips));
    assert!(
        knee[2] > hips[2] + 0.3 && (knee[1] - hips[1]).abs() < 0.15,
        "knee {knee:?} hips {hips:?}"
    );
}

#[test]
fn floor_poses_play_on_the_pose_entry_and_stay_on_the_floor() {
    let mut r = Run::new();
    let pose = r.doc.state.find(POSE).expect("the pose entry") as usize;
    r.step(10);
    let (head, standing) = (r.at(HumanBone::Head), r.at(HumanBone::Hips)[1]);
    // Sitting: hips down near the floor, knees ahead of them.
    r.i.params[pose] = 1.0;
    r.step(60);
    let (knee, hips) = (r.at(HumanBone::LeftLowerLeg), r.at(HumanBone::Hips));
    assert!(hips[1] < 0.4 * standing, "hips {hips:?} standing {standing}");
    assert!(knee[2] > hips[2] + 0.2, "knee {knee:?} hips {hips:?}");
    assert!((r.low() - floor()).abs() < 0.01);
    // Lying on the back: head and hips low, the head behind the hips (avatars face +Z).
    r.i.params[pose] = 2.0;
    r.step(60);
    let (h, hips) = (r.at(HumanBone::Head), r.at(HumanBone::Hips));
    assert!(h[1] < 0.35 && hips[1] < 0.35, "head {h:?} hips {hips:?}");
    assert!(h[2] < hips[2] - 0.4, "head behind the hips: {h:?} {hips:?}");
    assert!((r.low() - floor()).abs() < 0.01);
    r.i.params[pose] = 0.0;
    r.step(60);
    assert!((r.at(HumanBone::Head)[1] - head[1]).abs() < 0.01, "and stands again");
}

#[test]
fn gestures_pose_each_hand_and_the_fist_follows_its_weight() {
    let mut r = Run::new();
    let slot = |r: &Run, name: &str| {
        let m = find(name).unwrap();
        let targets = &r.c.bound.bindings.targets;
        let slot = targets
            .iter()
            .position(|t| *t == civbb_anim::Target::Muscle(m))
            .unwrap();
        r.i.pose()[slot]
    };
    let (left, right) = ("LeftHand.Index.2 Stretched", "RightHand.Index.2 Stretched");
    r.step(10);
    let idle = slot(&r, left);
    assert!(idle > 0.0 && slot(&r, right) == idle, "relaxed hands");
    r.set(Platform::GestureLeft, 1.0);
    r.set(Platform::GestureLeftWeight, 0.5);
    r.step(30);
    assert!(
        (slot(&r, left) - (idle - 1.0) / 2.0).abs() < 1e-4,
        "half a fist: {}",
        slot(&r, left)
    );
    assert_eq!(slot(&r, right), idle, "the other hand stays");
    r.set(Platform::GestureLeftWeight, 1.0);
    r.set(Platform::GestureRight, 3.0);
    r.step(30);
    assert_eq!(slot(&r, left), -1.0);
    assert_eq!(slot(&r, right), 1.0, "pointing");
    assert_eq!(slot(&r, "RightHand.Middle.2 Stretched"), -1.0);
}
