//! PhysBone chains (ADR 0015) against the VRChat SDK's own PhysBone runtime (`tests/unity/vrc_physbones.json`,
//! `tools/unity-truth/Assets/Editor/VrcPhysBones.cs`, #75): a four-bone chain on a root moving along a scripted path,
//! versions 1.0 and 1.1, both integration types, one parameter at a time. Replayed frame by frame; each case's error
//! is the largest distance of any bone from where the SDK put it.

use std::collections::BTreeMap;

use civbb_core::math::{Pose, length, sub};
use civbb_dynamics::{Rig, State};
use civbb_format::Avatar;
use civbb_format::dynamics::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Quat, Transform, Vec3};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Recording {
    bone_length: f32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    settings: BTreeMap<String, Value>,
    colliders: Vec<ColliderRec>,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
struct ColliderRec {
    shape: String,
    radius: f32,
    height: f32,
    position: Vec3,
    rotation: Quat,
    inside: bool,
}

#[derive(Deserialize)]
struct Frame {
    root: RootPose,
    dt: f32,
    bones: Vec<Vec3>,
}

#[derive(Deserialize)]
struct RootPose {
    position: Vec3,
    rotation: Quat,
}

/// Unity's left-handed space to ours, as the exporter mirrors it (`Json.Vec`, `Json.Quat`).
fn mirror(v: Vec3) -> Vec3 {
    [-v[0], v[1], v[2]]
}

fn mirror_q(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], q[3]]
}

impl Case {
    fn f(&self, k: &str) -> f32 {
        self.settings[k]
            .as_f64()
            .unwrap_or_else(|| panic!("{}: {k}", self.name)) as f32
    }

    fn s(&self, k: &str) -> &str {
        self.settings[k]
            .as_str()
            .unwrap_or_else(|| panic!("{}: {k}", self.name))
    }

    fn v(&self, k: &str) -> Vec3 {
        let a = self.settings[k]
            .as_array()
            .unwrap_or_else(|| panic!("{}: {k}", self.name));
        std::array::from_fn(|i| a[i].as_f64().unwrap() as f32)
    }

    /// Root, B0 on it (the chain root), then the bones along Unity's +X.
    fn avatar(&self, bone: f32) -> Avatar {
        let node = |name: &str, parent: Option<u32>, t: Vec3| Node {
            name: name.into(),
            parent,
            rest: Transform {
                translation: t,
                ..Transform::IDENTITY
            },
            active: true,
        };
        let mut nodes = vec![node("Root", None, [0.0; 3]), node("B0", Some(0), [0.0; 3])];
        for i in 1..=4 {
            nodes.push(node(&format!("B{i}"), Some(i), mirror([bone, 0.0, 0.0])));
        }
        let g = |k: &str| Graded::constant(self.f(k));
        let rotation = {
            let r = self.v("limitRotation");
            [r[0], -r[1], -r[2]]
        };
        let limit = match self.s("limitType") {
            "Angle" => Limit::Angle {
                max_angle: g("maxAngleX"),
                rotation,
            },
            "Hinge" => Limit::Hinge {
                max_angle: g("maxAngleX"),
                rotation,
            },
            "Polar" => Limit::Polar {
                max_pitch: g("maxAngleX"),
                max_yaw: g("maxAngleZ"),
                rotation,
            },
            _ => Limit::None,
        };
        let colliders: Vec<Collider> = self
            .colliders
            .iter()
            .map(|c| Collider {
                node: 0,
                shape: match c.shape.as_str() {
                    "Sphere" => Shape::Sphere { radius: c.radius },
                    "Capsule" => Shape::Capsule {
                        radius: c.radius,
                        height: c.height,
                    },
                    _ => Shape::Plane,
                },
                position: mirror(c.position),
                rotation: mirror_q(c.rotation),
                inside: c.inside,
            })
            .collect();
        let chain = Chain {
            root: 1,
            ignore: Vec::new(),
            endpoint: mirror(self.v("endpointPosition")),
            multi_child: MultiChild::First,
            integration: if self.s("integrationType") == "Advanced" {
                Integration::Advanced
            } else {
                Integration::Simplified
            },
            pull: g("pull"),
            spring: g("spring"),
            stiffness: g("stiffness"),
            gravity: g("gravity"),
            gravity_falloff: g("gravityFalloff"),
            immobile: g("immobile"),
            immobile_type: if self.s("immobileType") == "World" {
                ImmobileType::World
            } else {
                ImmobileType::AllMotion
            },
            limit,
            radius: g("radius"),
            colliders: (0..colliders.len() as u32).collect(),
            max_stretch: g("maxStretch"),
            max_squish: g("maxSquish"),
            allow_grab: false,
            allow_posing: false,
            parameter: None,
        };
        Avatar {
            hierarchy: Hierarchy { nodes },
            dynamics: Dynamics {
                chains: vec![chain],
                colliders,
            },
            ..Avatar::default()
        }
    }

    /// The largest distance of a bone from the SDK's, over all frames, and the frame where it occurs.
    fn error(&self, bone: f32) -> (f32, usize) {
        let a = self.avatar(bone);
        let rig = Rig::new(&a);
        let mut state = State::new(&rig);
        let mut worst = (0.0f32, 0);
        for (f, frame) in self.frames.iter().enumerate() {
            let origin = Pose {
                pos: mirror(frame.root.position),
                rot: mirror_q(frame.root.rotation),
                scale: 1.0,
            };
            let mut locals: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
            // The SDK's first recorded frame already holds one step.
            if f == 0 {
                state.step(&rig, origin, &mut locals.clone(), 0.0);
            }
            state.step(&rig, origin, &mut locals, frame.dt);
            let mut world: Vec<Pose> = Vec::new();
            for (i, n) in a.hierarchy.nodes.iter().enumerate() {
                let p = n.parent.map_or(origin, |p| world[p as usize]);
                world.push(p.then(&locals[i]));
            }
            for (b, want) in frame.bones.iter().enumerate() {
                let d = length(sub(world[b + 1].pos, mirror(*want)));
                if d > worst.0 {
                    worst = (d, f);
                }
            }
        }
        worst
    }
}

fn recording() -> Recording {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unity/vrc_physbones.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Our solver's largest bone error per case today, rounded up to the next centimetre. The model of ADR 0015 is
/// hand-derived and far from the SDK's (#75); a solver change may only lower these, and lowers them here.
const BOUNDS: [(&str, f32); 30] = [
    ("sag_v10_simplified", 0.23),
    ("sway_v10_simplified", 0.31),
    ("turn_v10_simplified", 0.24),
    ("stiff_v10_simplified", 0.33),
    ("sag_v10_advanced", 0.16),
    ("sway_v10_advanced", 0.16),
    ("turn_v10_advanced", 0.24),
    ("stiff_v10_advanced", 0.19),
    ("sag_v11_simplified", 0.16),
    ("sway_v11_simplified", 0.31),
    ("turn_v11_simplified", 0.14),
    ("stiff_v11_simplified", 0.33),
    ("sag_v11_advanced", 0.18),
    ("sway_v11_advanced", 0.16),
    ("turn_v11_advanced", 0.12),
    ("stiff_v11_advanced", 0.15),
    ("pull_high", 0.04),
    ("pull_zero_gravity", 0.57),
    ("falloff", 0.12),
    ("gravity_negative", 0.16),
    ("immobile_all", 0.18),
    ("immobile_world", 0.18),
    ("immobile_world_turn", 0.21),
    ("limit_angle", 0.14),
    ("limit_hinge", 0.17),
    ("limit_polar", 0.18),
    ("endpoint", 0.31),
    ("collider_sphere", 0.57),
    ("collider_capsule", 0.57),
    ("collider_plane", 0.17),
];

/// What the recording itself shows, whatever our model: the chain root follows the root, motion never stretches or
/// squishes a bone by more than a millimetre, and at the first frame the chain is still straight along +X unless
/// gravity already acted.
#[test]
fn the_sdk_keeps_bone_lengths_and_pins_the_root() {
    let r = recording();
    for c in &r.cases {
        for (f, frame) in c.frames.iter().enumerate() {
            assert!(
                length(sub(frame.bones[0], frame.root.position)) < 1e-4,
                "{} frame {f}",
                c.name
            );
            for b in 1..frame.bones.len() {
                let l = length(sub(frame.bones[b], frame.bones[b - 1]));
                assert!((l - r.bone_length).abs() < 1e-3, "{} frame {f} bone {b}: {l}", c.name);
            }
        }
    }
}

#[test]
fn chains_stay_within_todays_distance_from_the_sdk() {
    let r = recording();
    assert_eq!(r.cases.len(), BOUNDS.len());
    let mut over = Vec::new();
    for (c, (name, bound)) in r.cases.iter().zip(BOUNDS) {
        assert_eq!(c.name, name);
        let (e, f) = c.error(r.bone_length);
        println!("{:24} {:8.4} m at frame {f}", c.name, e);
        if e > bound {
            over.push(format!("{name}: {e} m at frame {f}, bound {bound}"));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}
