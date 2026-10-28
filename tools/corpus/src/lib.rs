//! Deterministic synthetic avatars: a lean control and a deliberately bloated worst case (design doc §21).

pub mod gltf;

use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::dynamics::*;
use civbb_format::expressions::*;
use civbb_format::face::{Face, ShapeBind, Viseme};
use civbb_format::hierarchy::{Hierarchy, HumanBone, Humanoid, Node, NodeIndex};
use civbb_format::material::{AlphaMode, Material, MaterialModel, Toon};
use civbb_format::math::{Transform, Vec3};
use civbb_format::mesh::{BlendShape, Mesh, ShapeDelta, Skin, Submesh};

/// What to generate.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Avatar name.
    pub name: String,
    /// Stacked outfit shells over the body, each toggleable.
    pub outfits: u32,
    /// Extra toggleable props on top of the outfit toggles.
    pub props: u32,
    /// Number of tails, each a dynamics chain.
    pub tails: u32,
    /// Bones per tail.
    pub tail_bones: u32,
    /// Blendshapes on the body (the first 15 are visemes).
    pub shape_keys: u32,
    /// Fraction of body vertices each non-viseme shape key moves.
    pub shape_key_coverage: f32,
    /// Cylinder sides per limb segment.
    pub sides: u32,
    /// Cylinder rings per limb segment.
    pub rings: u32,
}

/// A small, well-optimised control avatar: the compiler must never make it worse.
pub fn lean() -> Spec {
    Spec {
        name: "lean".into(),
        outfits: 1,
        props: 1,
        tails: 0,
        tail_bones: 0,
        shape_keys: 15,
        shape_key_coverage: 0.02,
        sides: 8,
        rings: 4,
    }
}

/// The worst case on purpose: stacked outfits, dozens of toggles, a nine-tail rig and 1,200 shape keys.
pub fn bloated() -> Spec {
    Spec {
        name: "bloated".into(),
        outfits: 8,
        props: 40,
        tails: 9,
        tail_bones: 8,
        shape_keys: 1200,
        shape_key_coverage: 0.05,
        sides: 24,
        rings: 16,
    }
}

const VISEMES: [&str; 15] = [
    "sil", "PP", "FF", "TH", "DD", "kk", "CH", "SS", "nn", "RR", "aa", "E", "ih", "oh", "ou",
];

/// Humanoid skeleton: name, parent name, offset from parent, humanoid bone.
const SKELETON: &[(&str, &str, Vec3, Option<HumanBone>)] = &[
    ("Hips", "Armature", [0.0, 0.95, 0.0], Some(HumanBone::Hips)),
    ("Spine", "Hips", [0.0, 0.1, 0.0], Some(HumanBone::Spine)),
    ("Chest", "Spine", [0.0, 0.15, 0.0], Some(HumanBone::Chest)),
    ("Neck", "Chest", [0.0, 0.2, 0.0], Some(HumanBone::Neck)),
    ("Head", "Neck", [0.0, 0.08, 0.0], Some(HumanBone::Head)),
    ("HeadTop", "Head", [0.0, 0.22, 0.0], None),
    ("Shoulder.L", "Chest", [0.05, 0.17, 0.0], Some(HumanBone::LeftShoulder)),
    (
        "UpperArm.L",
        "Shoulder.L",
        [0.1, 0.0, 0.0],
        Some(HumanBone::LeftUpperArm),
    ),
    (
        "LowerArm.L",
        "UpperArm.L",
        [0.25, 0.0, 0.0],
        Some(HumanBone::LeftLowerArm),
    ),
    ("Hand.L", "LowerArm.L", [0.23, 0.0, 0.0], Some(HumanBone::LeftHand)),
    ("HandTip.L", "Hand.L", [0.12, 0.0, 0.0], None),
    (
        "Shoulder.R",
        "Chest",
        [-0.05, 0.17, 0.0],
        Some(HumanBone::RightShoulder),
    ),
    (
        "UpperArm.R",
        "Shoulder.R",
        [-0.1, 0.0, 0.0],
        Some(HumanBone::RightUpperArm),
    ),
    (
        "LowerArm.R",
        "UpperArm.R",
        [-0.25, 0.0, 0.0],
        Some(HumanBone::RightLowerArm),
    ),
    ("Hand.R", "LowerArm.R", [-0.23, 0.0, 0.0], Some(HumanBone::RightHand)),
    ("HandTip.R", "Hand.R", [-0.12, 0.0, 0.0], None),
    ("UpperLeg.L", "Hips", [0.09, -0.05, 0.0], Some(HumanBone::LeftUpperLeg)),
    (
        "LowerLeg.L",
        "UpperLeg.L",
        [0.0, -0.42, 0.0],
        Some(HumanBone::LeftLowerLeg),
    ),
    ("Foot.L", "LowerLeg.L", [0.0, -0.4, 0.0], Some(HumanBone::LeftFoot)),
    ("Toes.L", "Foot.L", [0.0, -0.05, 0.12], Some(HumanBone::LeftToes)),
    (
        "UpperLeg.R",
        "Hips",
        [-0.09, -0.05, 0.0],
        Some(HumanBone::RightUpperLeg),
    ),
    (
        "LowerLeg.R",
        "UpperLeg.R",
        [0.0, -0.42, 0.0],
        Some(HumanBone::RightLowerLeg),
    ),
    ("Foot.R", "LowerLeg.R", [0.0, -0.4, 0.0], Some(HumanBone::RightFoot)),
    ("Toes.R", "Foot.R", [0.0, -0.05, 0.12], Some(HumanBone::RightToes)),
];

/// Limb segments drawn as cylinders: (from bone, to bone, radius).
const SEGMENTS: &[(&str, &str, f32)] = &[
    ("Hips", "Spine", 0.13),
    ("Spine", "Chest", 0.12),
    ("Chest", "Neck", 0.13),
    ("Neck", "Head", 0.05),
    ("Head", "HeadTop", 0.11),
    ("UpperArm.L", "LowerArm.L", 0.045),
    ("LowerArm.L", "Hand.L", 0.04),
    ("Hand.L", "HandTip.L", 0.035),
    ("UpperArm.R", "LowerArm.R", 0.045),
    ("LowerArm.R", "Hand.R", 0.04),
    ("Hand.R", "HandTip.R", 0.035),
    ("UpperLeg.L", "LowerLeg.L", 0.07),
    ("LowerLeg.L", "Foot.L", 0.055),
    ("Foot.L", "Toes.L", 0.045),
    ("UpperLeg.R", "LowerLeg.R", 0.07),
    ("LowerLeg.R", "Foot.R", 0.055),
    ("Foot.R", "Toes.R", 0.045),
];

/// xorshift64*, enough for deterministic test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % n.max(1) as u64) as u32
    }
}

struct Builder {
    nodes: Vec<Node>,
    world: Vec<Vec3>,
}

impl Builder {
    fn add(&mut self, name: &str, parent: Option<NodeIndex>, offset: Vec3, active: bool) -> NodeIndex {
        let base = parent.map_or([0.0; 3], |p| self.world[p as usize]);
        self.world.push(add(base, offset));
        self.nodes.push(Node {
            name: name.into(),
            parent,
            rest: Transform {
                translation: offset,
                ..Transform::IDENTITY
            },
            active,
        });
        (self.nodes.len() - 1) as NodeIndex
    }

    fn find(&self, name: &str) -> NodeIndex {
        self.nodes
            .iter()
            .position(|n| n.name == name)
            .unwrap_or_else(|| panic!("no node {name}")) as NodeIndex
    }

    /// Unity-style path relative to the root: names joined by `/`, root excluded.
    fn path(&self, mut node: NodeIndex) -> String {
        let mut parts = Vec::new();
        while let Some(parent) = self.nodes[node as usize].parent {
            parts.push(self.nodes[node as usize].name.as_str());
            node = parent;
        }
        parts.reverse();
        parts.join("/")
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn normalize(a: Vec3) -> Vec3 {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if len > 0.0 {
        scale(a, 1.0 / len)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Accumulates skinned cylinders into one mesh. Joints are skeleton nodes; the skin's joint list is built lazily.
#[derive(Default)]
struct MeshBuilder {
    mesh: Mesh,
    joint_nodes: Vec<NodeIndex>,
}

impl MeshBuilder {
    fn joint(&mut self, node: NodeIndex) -> u16 {
        let i = self.joint_nodes.iter().position(|&n| n == node).unwrap_or_else(|| {
            self.joint_nodes.push(node);
            self.joint_nodes.len() - 1
        });
        i as u16
    }

    /// A cylinder from `a` to `b`, each ring weighted linearly from joint `ja` to joint `jb`.
    #[allow(clippy::too_many_arguments)]
    fn cylinder(&mut self, a: Vec3, b: Vec3, radius: f32, ja: NodeIndex, jb: NodeIndex, sides: u32, rings: u32) {
        let (ja, jb) = (self.joint(ja), self.joint(jb));
        let axis = normalize(sub(b, a));
        let helper = if axis[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = normalize(cross(axis, helper));
        let v = cross(axis, u);
        let m = &mut self.mesh;
        let first = m.positions.len() as u32;
        for r in 0..=rings {
            let t = r as f32 / rings as f32;
            let center = add(a, scale(sub(b, a), t));
            for s in 0..sides {
                let angle = s as f32 / sides as f32 * std::f32::consts::TAU;
                let n = add(scale(u, angle.cos()), scale(v, angle.sin()));
                m.positions.push(add(center, scale(n, radius)));
                m.normals.push(n);
                m.uv0.push([s as f32 / sides as f32, t]);
                let skin = m.skin.get_or_insert_with(Skin::default);
                skin.vertex_joints.push([ja, jb, 0, 0]);
                skin.vertex_weights.push([1.0 - t, t, 0.0, 0.0]);
            }
        }
        for r in 0..rings {
            for s in 0..sides {
                let i0 = first + r * sides + s;
                let i1 = first + r * sides + (s + 1) % sides;
                let (i2, i3) = (i0 + sides, i1 + sides);
                m.indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }
    }

    fn finish(mut self, b: &Builder, name: &str, node: NodeIndex, material: u32) -> Mesh {
        let skin = self.mesh.skin.get_or_insert_with(Skin::default);
        skin.inverse_bind = self
            .joint_nodes
            .iter()
            .map(|&j| {
                let p = b.world[j as usize];
                [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -p[0], -p[1], -p[2], 1.0,
                ]
            })
            .collect();
        skin.joints = self.joint_nodes;
        self.mesh.name = name.into();
        self.mesh.node = node;
        self.mesh.submeshes = vec![Submesh {
            first_index: 0,
            index_count: self.mesh.indices.len() as u32,
            material,
        }];
        self.mesh
    }
}

fn toon(name: &str, color: [f32; 4]) -> Material {
    Material {
        name: name.into(),
        model: MaterialModel::Toon(Toon {
            base_color: color,
            base_color_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            shade_color: [color[0] * 0.7, color[1] * 0.6, color[2] * 0.7],
            shade_tex: None,
            shade_shift: 0.0,
            shade_toony: 0.9,
            rim_color: [0.0; 3],
            rim_power: 3.0,
            matcap_tex: None,
            matcap_strength: 0.0,
            emissive: [0.0; 3],
            emissive_tex: None,
            outline_width: 0.0,
            outline_color: [0.0; 3],
            outline_width_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        render_queue: 2000,
    }
}

fn graded(value: f32) -> Graded {
    Graded::constant(value)
}

fn toggle_clip(name: String, path: String, on: bool) -> Clip {
    let value = if on { 1.0 } else { 0.0 };
    Clip {
        name,
        length: 1.0 / 60.0,
        looping: false,
        curves: vec![Curve {
            binding: Binding {
                path,
                property: Property::Active,
            },
            keys: vec![Key {
                time: 0.0,
                value,
                in_tangent: f32::INFINITY,
                out_tangent: f32::INFINITY,
            }],
        }],
    }
}

/// Generates the avatar described by `spec`. Same spec, same avatar, on every platform.
pub fn generate(spec: &Spec) -> Avatar {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ spec.name.len() as u64);
    let mut b = Builder {
        nodes: Vec::new(),
        world: Vec::new(),
    };
    let root = b.add(&spec.name, None, [0.0; 3], true);
    let armature = b.add("Armature", Some(root), [0.0; 3], true);
    let mut humanoid = Humanoid {
        bones: Vec::new(),
        viewpoint: [0.0, 1.55, 0.08],
    };
    for (name, parent, offset, human) in SKELETON {
        let parent = if *parent == "Armature" {
            armature
        } else {
            b.find(parent)
        };
        let n = b.add(name, Some(parent), *offset, true);
        if let Some(h) = human {
            humanoid.bones.push((*h, n));
        }
    }

    // Body and outfits: the same segments, outfits as slightly larger shells on their own toggle nodes.
    let body_node = b.add("Body", Some(root), [0.0; 3], true);
    let mut body = MeshBuilder::default();
    for (from, to, radius) in SEGMENTS {
        let (fa, fb) = (b.find(from), b.find(to));
        body.cylinder(
            b.world[fa as usize],
            b.world[fb as usize],
            *radius,
            fa,
            fb,
            spec.sides,
            spec.rings,
        );
    }
    let mut meshes = vec![body.finish(&b, "Body", body_node, 0)];
    let mut materials = vec![toon("Skin", [1.0, 0.86, 0.78, 1.0])];
    let mut toggle_nodes = Vec::new();
    for o in 0..spec.outfits {
        let node = b.add(&format!("Outfit_{o}"), Some(root), [0.0; 3], o == 0);
        toggle_nodes.push(node);
        let mut shell = MeshBuilder::default();
        for (from, to, radius) in SEGMENTS {
            let (fa, fb) = (b.find(from), b.find(to));
            let r = radius * (1.06 + 0.02 * o as f32);
            shell.cylinder(
                b.world[fa as usize],
                b.world[fb as usize],
                r,
                fa,
                fb,
                spec.sides,
                spec.rings,
            );
        }
        let material = materials.len() as u32;
        materials.push(toon(
            &format!("Outfit_{o}"),
            [0.2 + 0.1 * (o % 8) as f32, 0.3, 0.6, 1.0],
        ));
        meshes.push(shell.finish(&b, &format!("Outfit_{o}"), node, material));
    }

    // Props: small rigid-looking items skinned to the chest, hidden by default.
    let chest = b.find("Chest");
    for p in 0..spec.props {
        let node = b.add(&format!("Prop_{p}"), Some(root), [0.0; 3], false);
        toggle_nodes.push(node);
        let angle = p as f32 / spec.props.max(1) as f32 * std::f32::consts::TAU;
        let base = add(b.world[chest as usize], [0.18 * angle.cos(), 0.1, 0.18 * angle.sin()]);
        let mut prop = MeshBuilder::default();
        prop.cylinder(base, add(base, [0.0, 0.06, 0.0]), 0.02, chest, chest, 8, 1);
        meshes.push(prop.finish(&b, &format!("Prop_{p}"), node, 0));
    }

    // Tails: one chain each, fanned out behind the hips.
    let hips = b.find("Hips");
    let mut dynamics = Dynamics::default();
    let collider_legs: Vec<u32> = ["UpperLeg.L", "UpperLeg.R"]
        .iter()
        .map(|leg| {
            dynamics.colliders.push(Collider {
                node: b.find(leg),
                shape: Shape::Capsule {
                    radius: 0.08,
                    height: 0.5,
                },
                position: [0.0, -0.2, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                inside: false,
            });
            (dynamics.colliders.len() - 1) as u32
        })
        .collect();
    if spec.tails > 0 {
        let tails_node = b.add("Tails", Some(root), [0.0; 3], true);
        let mut tails = MeshBuilder::default();
        for t in 0..spec.tails {
            let spread = (t as f32 / (spec.tails.max(2) - 1) as f32 - 0.5) * 1.6;
            let mut parent = hips;
            let mut first = None;
            for k in 0..spec.tail_bones {
                let offset = if k == 0 {
                    [0.0, -0.05, -0.12]
                } else {
                    [spread * 0.05, -0.01, -0.07]
                };
                let n = b.add(&format!("Tail_{t}_{k}"), Some(parent), offset, true);
                if k > 0 {
                    let radius = 0.04 * (1.0 - k as f32 / (spec.tail_bones as f32 + 1.0));
                    let (pa, pb) = (b.world[parent as usize], b.world[n as usize]);
                    tails.cylinder(pa, pb, radius, parent, n, spec.sides.min(12), 2);
                }
                first.get_or_insert(n);
                parent = n;
            }
            dynamics.chains.push(Chain {
                root: first.expect("tail_bones > 0"),
                ignore: vec![],
                endpoint: [0.0, 0.0, -0.05],
                multi_child: MultiChild::Ignore,
                integration: Integration::Advanced,
                pull: graded(0.2),
                spring: graded(0.4),
                stiffness: graded(0.1),
                gravity: graded(0.15),
                gravity_falloff: graded(0.5),
                immobile: graded(0.3),
                immobile_type: ImmobileType::AllMotion,
                limit: Limit::Angle {
                    max_angle: graded(60.0),
                    rotation: [0.0; 3],
                },
                radius: Graded {
                    value: 0.03,
                    curve: vec![[0.0, 1.0], [1.0, 0.3]],
                },
                colliders: collider_legs.clone(),
                max_stretch: graded(0.1),
                max_squish: graded(0.1),
                allow_grab: true,
                allow_posing: true,
                parameter: Some(format!("Tail_{t}")),
            });
        }
        let material = materials.len() as u32;
        materials.push(toon("Tails", [0.9, 0.6, 0.3, 1.0]));
        meshes.push(tails.finish(&b, "Tails", tails_node, material));
    }

    // Shape keys: visemes first, then face-tracking style keys, each moving a contiguous run of body vertices.
    let body_mesh = &mut meshes[0];
    let vertex_count = body_mesh.positions.len() as u32;
    let run = ((vertex_count as f32 * spec.shape_key_coverage) as u32).max(1);
    for i in 0..spec.shape_keys {
        let name = match VISEMES.get(i as usize) {
            Some(v) => format!("vrc.v_{v}"),
            None => format!("FT_Shape_{i}"),
        };
        let start = rng.below(vertex_count - run.min(vertex_count));
        let deltas = (start..start + run)
            .map(|v| ShapeDelta {
                vertex: v,
                position: scale(body_mesh.normals[v as usize], 0.004),
                normal: [0.0; 3],
            })
            .collect();
        body_mesh.blend_shapes.push(BlendShape {
            name,
            default_weight: 0.0,
            deltas,
        });
    }

    // Animator: one FX layer per toggle (the common legacy pattern the compiler later collapses).
    let mut animator = Animator {
        parameters: Vec::new(),
        groups: vec![LayerGroup {
            name: "FX".into(),
            weight: 1.0,
        }],
        layers: Vec::new(),
    };
    let mut clips = Vec::new();
    let mut expression_params = Vec::new();
    for &node in &toggle_nodes {
        let name = b.nodes[node as usize].name.clone();
        let default_on = b.nodes[node as usize].active;
        let param = animator.parameters.len() as ParamIndex;
        animator.parameters.push(Parameter {
            name: name.clone(),
            kind: ParamKind::Bool,
            default: if default_on { 1.0 } else { 0.0 },
        });
        expression_params.push(ExpressionParam {
            name: name.clone(),
            kind: ExpressionParamKind::Bool,
            default: if default_on { 1.0 } else { 0.0 },
            saved: true,
            synced: true,
        });
        let off_clip = clips.len() as ClipIndex;
        clips.push(toggle_clip(format!("{name}_Off"), b.path(node), false));
        clips.push(toggle_clip(format!("{name}_On"), b.path(node), true));
        let to = |state, mode| Transition {
            dest: TransitionDest::State(state),
            conditions: vec![Condition {
                param,
                mode,
                threshold: 0.0,
            }],
            has_exit_time: false,
            exit_time: 0.0,
            fixed_duration: true,
            duration: 0.0,
            offset: 0.0,
            interruption: InterruptionSource::None,
            ordered_interruption: true,
            can_transition_to_self: false,
        };
        let state = |label: &str, clip, transition| State {
            name: label.into(),
            motion: Some(Motion::Clip(clip)),
            speed: 1.0,
            speed_param: None,
            cycle_offset: 0.0,
            cycle_offset_param: None,
            time_param: None,
            write_defaults: false,
            transitions: vec![transition],
            behaviours: vec![],
        };
        animator.layers.push(Layer {
            name,
            group: 0,
            weight: 1.0,
            blending: LayerBlending::Override,
            mask: None,
            state_machine: StateMachine {
                states: vec![
                    state("Off", off_clip, to(1, ConditionMode::If)),
                    state("On", off_clip + 1, to(0, ConditionMode::IfNot)),
                ],
                default_state: if default_on { 1 } else { 0 },
                entry_transitions: vec![],
                any_state_transitions: vec![],
            },
        });
    }

    // Menu: toggles in pages of eight, as legacy menus are limited to eight controls.
    let pages: Vec<Control> = expression_params
        .chunks(8)
        .enumerate()
        .map(|(i, page)| Control {
            name: format!("Page {i}"),
            icon: None,
            kind: ControlKind::SubMenu(Menu {
                controls: page
                    .iter()
                    .map(|p| Control {
                        name: p.name.clone(),
                        icon: None,
                        kind: ControlKind::Toggle,
                        parameter: Some(p.name.clone()),
                        value: 1.0,
                        sub_parameters: vec![],
                    })
                    .collect(),
            }),
            parameter: None,
            value: 0.0,
            sub_parameters: vec![],
        })
        .collect();

    let volume = |node, radius| ContactVolume {
        node,
        shape: Shape::Sphere { radius },
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let contacts = Contacts {
        senders: ["Hand.L", "Hand.R"]
            .iter()
            .map(|h| ContactSender {
                volume: volume(b.find(h), 0.05),
                tags: vec!["Hand".into()],
            })
            .collect(),
        receivers: vec![ContactReceiver {
            volume: volume(b.find("Head"), 0.15),
            tags: vec!["Hand".into()],
            parameter: "HeadPat".into(),
            kind: ReceiverKind::Proximity,
            allow_self: false,
            allow_others: true,
            local_only: true,
            min_velocity: 0.0,
        }],
    };

    Avatar {
        name: spec.name.clone(),
        hierarchy: Hierarchy { nodes: b.nodes },
        humanoid: Some(humanoid),
        meshes,
        // The first 15 shape keys on the body are the visemes, in legacy order.
        face: Some(Face {
            visemes: Viseme::ALL
                .into_iter()
                .zip(0..spec.shape_keys.min(15))
                .map(|(v, shape)| {
                    (
                        v,
                        vec![ShapeBind {
                            mesh: 0,
                            shape,
                            weight: 1.0,
                        }],
                    )
                })
                .collect(),
            blink: vec![],
            eyes: None,
        }),
        meta: None,
        materials,
        textures: vec![],
        animator: Some(animator),
        clips,
        expressions: Some(Expressions {
            parameters: expression_params,
            menu: Menu { controls: pages },
        }),
        dynamics,
        contacts,
        constraints: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civbb_format::{Caps, validate};

    #[test]
    fn generated_avatars_are_valid() {
        for spec in [lean(), bloated()] {
            let a = generate(&spec);
            validate(&a, &Caps::default(), |_| true).unwrap_or_else(|e| panic!("{}: {e:?}", spec.name));
        }
    }

    #[test]
    fn bloated_hits_the_worst_case_targets() {
        let a = generate(&bloated());
        assert!(a.meshes[0].blend_shapes.len() >= 1000);
        assert_eq!(a.dynamics.chains.len(), 9);
        assert!(a.animator.as_ref().unwrap().layers.len() >= 40);
        assert!(a.meshes.len() >= 10);
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generate(&bloated()), generate(&bloated()));
    }

    #[test]
    fn toggle_paths_resolve_to_nodes() {
        let a = generate(&lean());
        let names: Vec<&str> = a.hierarchy.nodes.iter().map(|n| n.name.as_str()).collect();
        for clip in &a.clips {
            let path = &clip.curves[0].binding.path;
            assert!(names.contains(&path.rsplit('/').next().unwrap()), "{path}");
        }
    }
}
