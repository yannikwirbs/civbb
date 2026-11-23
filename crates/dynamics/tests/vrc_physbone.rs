//! PhysBone chains (ADR 0049) against the VRChat SDK's own PhysBone runtime (`tests/unity/vrc_physbones*.json`,
//! `tools/unity-truth/Assets/Editor/VrcPhysBones.cs`, #75): a four-bone chain on a root moving along a scripted path,
//! versions 1.0 and 1.1, both integration types, one parameter at a time, at 60 Hz and some cases at 90 and 30 Hz.
//! Replayed frame by frame; each case's error is the largest distance of any bone from where the SDK put it.

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
    dt: f32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    settings: BTreeMap<String, Value>,
    /// B0 to B4's local scales; all 1 when absent.
    #[serde(default)]
    bone_scales: Vec<f32>,
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
    scale: f32,
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
        for (b, &s) in self.bone_scales.iter().enumerate() {
            nodes[b + 1].rest.scale = [s; 3];
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
            version: if self.s("version") == "Version_1_0" {
                Version::V1_0
            } else {
                Version::V1_1
            },
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
                scale: frame.root.scale,
            };
            let mut locals: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
            // The SDK counts a chain's first frame twice.
            let dt = if f == 0 { 2.0 * frame.dt } else { frame.dt };
            state.step(&rig, origin, &mut locals, dt);
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

fn recording(file: &str) -> Recording {
    let path = format!("{}/tests/unity/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

const FILES: [&str; 3] = [
    "vrc_physbones.json",
    "vrc_physbones_90hz.json",
    "vrc_physbones_30hz.json",
];

/// What the recordings themselves show, whatever our model: the chain root follows the root, motion never stretches
/// or squishes a bone by more than a millimetre.
#[test]
fn the_sdk_keeps_bone_lengths_and_pins_the_root() {
    for file in FILES {
        let r = recording(file);
        for c in &r.cases {
            for (f, frame) in c.frames.iter().enumerate() {
                assert!(
                    length(sub(frame.bones[0], frame.root.position)) < 1e-4,
                    "{file} {} frame {f}",
                    c.name
                );
                let mut bone = r.bone_length * frame.root.scale;
                for b in 1..frame.bones.len() {
                    bone *= c.bone_scales.get(b - 1).copied().unwrap_or(1.0);
                    let l = length(sub(frame.bones[b], frame.bones[b - 1]));
                    assert!((l - bone).abs() < 1e-3, "{file} {} frame {f} bone {b}: {l}", c.name);
                }
            }
        }
    }
}

/// Every case's largest bone error (metres), file by file. Cases not listed must replay within [`EXACT`].
const BOUNDS: &[(&str, &str, f32)] = &[
    // Version 1.1 residuals at 60 Hz: stiffness 0.8 with momentum, the polar box's corners, velocity after a
    // collider push, spring 1 (#84).
    ("vrc_physbones.json", "stiff_v11_advanced", 0.0031),
    ("vrc_physbones.json", "limit_polar_diagonal", 0.008),
    ("vrc_physbones.json", "collider_sway", 0.016),
    ("vrc_physbones.json", "simplified_p10_s100", 0.00075),
    ("vrc_physbones.json", "simplified_p40_s100", 0.001),
    // A bone fatter than its inside capsule: the SDK puts the tip on the far side of the core (negative room, exact
    // for the first frames), then the tip sits near the axis and flips sides on float noise (#90).
    ("vrc_physbones.json", "collider_inside_capsule_fat", 0.0194),
    ("vrc_physbones.json", "collider_inside_capsule_short_fat", 0.0211),
    // Other frame rates: the output's partial step is an approximation (#84); full steps (30 Hz) are exact.
    ("vrc_physbones_90hz.json", "sag_v10_simplified", 0.002),
    ("vrc_physbones_90hz.json", "sag_v11_simplified", 0.0014),
    ("vrc_physbones_90hz.json", "sway_v11_simplified", 0.0023),
    ("vrc_physbones_90hz.json", "sway_v11_advanced", 0.0011),
    ("vrc_physbones_90hz.json", "turn_v11_advanced", 0.0007),
    ("vrc_physbones_90hz.json", "stiff_v11_advanced", 0.0063),
    ("vrc_physbones_90hz.json", "immobile_all", 0.0009),
    ("vrc_physbones_90hz.json", "immobile_advanced", 0.0008),
    ("vrc_physbones_90hz.json", "scaled", 0.0024),
    ("vrc_physbones_90hz.json", "limit_angle", 0.0035),
    ("vrc_physbones_90hz.json", "limit_angle_sway", 0.0018),
    ("vrc_physbones_90hz.json", "collider_sphere", 0.0151),
    ("vrc_physbones_90hz.json", "collider_sway", 0.0639),
    ("vrc_physbones_90hz.json", "simplified_p40_s70", 0.0036),
    ("vrc_physbones_90hz.json", "advanced_p20_m50_k50", 0.0025),
    ("vrc_physbones_90hz.json", "gravity_v11_sway", 0.0016),
    ("vrc_physbones_90hz.json", "falloff_v11_50", 0.0021),
    ("vrc_physbones_30hz.json", "stiff_v11_advanced", 0.0028),
    ("vrc_physbones_30hz.json", "collider_sway", 0.0164),
];

/// What "exact" means here: the goldens hold positions to the micrometre and Unity steps in single precision.
const EXACT: f32 = 0.0005;

#[test]
fn chains_replay_the_sdk() {
    let mut over = Vec::new();
    for file in FILES {
        let r = recording(file);
        for c in &r.cases {
            let bound = BOUNDS
                .iter()
                .find(|b| b.0 == file && b.1 == c.name)
                .map_or(EXACT, |b| b.2);
            let (e, f) = c.error(r.bone_length);
            println!(
                "{file:24} {:28} {:8.5} m at frame {f} (bound {bound}, dt {})",
                c.name, e, r.dt
            );
            if e > bound {
                over.push(format!("{file} {}: {e} m at frame {f}, bound {bound}", c.name));
            }
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}
