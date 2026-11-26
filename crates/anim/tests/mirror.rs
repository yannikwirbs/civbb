//! Humanoid mirroring (ADR 0060): the muscle map against Unity's recording (`tools/unity-truth/Assets/Editor/Mirror.cs`
//! writes `unity/mirror.json`), and the animator mirroring a state, a blend tree child and both.

use std::path::Path;

use civbb_anim::muscle::{COUNT, find, mirror, property};
use civbb_anim::{Controller, Instance, Target};
use civbb_format::Document;
use civbb_format::animator::Target as CurveTarget;
use civbb_format::animator::*;
use civbb_format::hierarchy::{BoneNode, Hierarchy, HumanBone, Humanoid, MuscleFrame, Node};
use civbb_format::math::Transform;
use civbb_format::state::{Entry, EntryIndex, EntryKind, StateTable};
use civbb_format::{Avatar, Role};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    plain: Vec<f32>,
    mirrored_state: Vec<f32>,
    mirrored_child: Vec<f32>,
    mirrored_both: Vec<f32>,
}

#[test]
fn the_mirror_map_matches_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/mirror.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(r.plain.len(), COUNT);
    for i in 0..COUNT {
        let (j, sign) = mirror(i as u8);
        let want = sign * r.plain[j as usize];
        assert!(
            (r.mirrored_state[i] - want).abs() < 1e-5,
            "muscle {i}: {} != {want}",
            r.mirrored_state[i]
        );
    }
    // A mirrored child in a mirrored state plays plain: Unity combines them by exclusive or.
    assert_eq!(r.mirrored_child, r.mirrored_state);
    assert_eq!(r.mirrored_both, r.plain);
}

/// Hips with the spine and both upper arms under it, every bone with a frame, the bool entry `m` at 1, and `animator`.
fn humanoid(clips: Vec<Clip>, animator: Animator) -> Document {
    let bones = [
        HumanBone::Hips,
        HumanBone::Spine,
        HumanBone::LeftUpperArm,
        HumanBone::RightUpperArm,
    ];
    let names = ["Root", "Hips", "Spine", "LeftArm", "RightArm"];
    let parents = [None, Some(0), Some(1), Some(1), Some(1)];
    Document {
        state: StateTable {
            entries: vec![Entry::new("m", EntryKind::Bool, 1.0)],
        },
        hierarchy: Hierarchy {
            nodes: names
                .iter()
                .zip(parents)
                .map(|(n, parent)| Node {
                    name: (*n).into(),
                    parent,
                    rest: Transform::IDENTITY,
                    active: true,
                })
                .collect(),
        },
        role: Role::Avatar(Avatar {
            humanoid: Some(Humanoid {
                bones: bones
                    .iter()
                    .enumerate()
                    .map(|(i, b)| BoneNode {
                        bone: *b,
                        node: i as u32 + 1,
                    })
                    .collect(),
                human_scale: 0.0,
                center_of_mass: None,
                muscle_frames: bones
                    .iter()
                    .map(|&bone| MuscleFrame {
                        bone,
                        pre: Transform::IDENTITY.rotation,
                        post: Transform::IDENTITY.rotation,
                        sign: [1.0; 3],
                        min: [-60.0; 3],
                        max: [60.0; 3],
                        twist: 0.5,
                    })
                    .collect(),
            }),
            viewpoint: [0.0; 3],
            ..Default::default()
        }),
        clips,
        animator: Some(animator),
        ..Document::default()
    }
}

fn clip(curves: &[(&str, f32)]) -> Clip {
    Clip {
        name: "c".into(),
        length: 1.0,
        looping: true,
        root: None,
        curves: curves
            .iter()
            .map(|(m, v)| Curve {
                target: CurveTarget::Humanoid {
                    property: property(find(m).expect("a Unity muscle")),
                },
                keys: vec![Key {
                    time: 0.0,
                    value: *v,
                    in_tangent: 0.0,
                    out_tangent: 0.0,
                }],
            })
            .collect(),
    }
}

/// One layer playing `motion` in a state with `mirror` and `mirror_param`; entry 0 is the bool `m` ([`humanoid`]).
fn animator(motion: Motion, mirror: bool, mirror_param: Option<EntryIndex>) -> Animator {
    Animator {
        groups: vec![LayerGroup {
            name: "All".into(),
            weight: 1.0,
        }],
        layers: vec![Layer {
            name: "l".into(),
            group: 0,
            weight: 1.0,
            blending: LayerBlending::Override,
            mask: None,
            state_machine: StateMachine {
                states: vec![State {
                    name: "s".into(),
                    motion: Some(motion),
                    speed: 1.0,
                    speed_param: None,
                    cycle_offset: 0.0,
                    cycle_offset_param: None,
                    time_param: None,
                    unwritten: Unwritten::Rest,
                    mirror,
                    mirror_param,
                    transitions: Vec::new(),
                    actions: Vec::new(),
                }],
                default_state: 0,
                entry_transitions: Vec::new(),
                any_state_transitions: Vec::new(),
            },
        }],
    }
}

/// The muscles named, after one update of `a`.
fn play(a: &Document, names: &[&str]) -> Vec<f32> {
    let c = Controller::new(a).unwrap();
    let mut i = Instance::new(&c, true);
    i.update(&c, 1.0 / 60.0);
    let targets = &c.bound.bindings.targets;
    names
        .iter()
        .map(|n| {
            let s = targets.iter().position(|t| *t == Target::Muscle(find(n).unwrap()));
            s.map_or(0.0, |s| i.pose()[s])
        })
        .collect()
}

#[test]
fn mirrored_states_and_children_swap_sides_and_negate_the_body_by_exclusive_or() {
    let names = [
        "Left Arm Down-Up",
        "Right Arm Down-Up",
        "Spine Left-Right",
        "Spine Front-Back",
    ];
    let clips = || {
        vec![clip(&[
            ("Left Arm Down-Up", 0.4),
            ("Spine Left-Right", 0.2),
            ("Spine Front-Back", 0.1),
        ])]
    };
    let plain = [0.4, 0.0, 0.2, 0.1];
    let mirrored = [0.0, 0.4, -0.2, 0.1];
    let child = |mirror| {
        Motion::BlendTree(Box::new(BlendTree {
            kind: BlendTreeKind::Direct,
            children: vec![ChildMotion {
                motion: Motion::Clip { clip: 0 },
                threshold: 0.0,
                position: [0.0, 0.0],
                direct_param: Some(0),
                time_scale: 1.0,
                cycle_offset: 0.0,
                mirror,
            }],
            normalize: false,
        }))
    };
    let cases = [
        ("plain", Motion::Clip { clip: 0 }, false, None, plain),
        ("mirrored state", Motion::Clip { clip: 0 }, true, None, mirrored),
        (
            "mirror parameter on",
            Motion::Clip { clip: 0 },
            false,
            Some(0),
            mirrored,
        ),
        ("mirrored child", child(true), false, None, mirrored),
        ("mirrored child in a mirrored state", child(true), true, None, plain),
    ];
    for (what, motion, flag, param, want) in cases {
        let got = play(&humanoid(clips(), animator(motion, flag, param)), &names);
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6),
            "{what}: {got:?} != {want:?}"
        );
    }
}
