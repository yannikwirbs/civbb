//! Golden-file conformance: runs every `tests/golden/*.json` case frame by frame and compares the pose.
//!
//! A case is a small controller (nodes, parameters, clips, layers) plus frames: parameter changes, then expected
//! values (`"<node>.x|y|z|active"`, a material's property as its shader receives it in Unity's units
//! `"<node>:<material>:<property>"`, `null` when the material lacks it) and current states (`"<layer>": "<state>"`).
//! Material curves target `"<node>:<property>"` by Unity name and are lowered as the importer does (ADR 0099). Every
//! expectation is Unity's:
//! `tools/unity-truth/Assets/Editor/Goldens.cs` plays the cases and records `tests/unity/goldens.json` (#54), which
//! `golden_cases` checks them against.

use std::collections::BTreeMap;
use std::path::Path;

use civbb_anim::material::{self, to_linear, to_srgb};
use civbb_anim::{Controller, Instance, Target};
use civbb_format::Document;
use civbb_format::animator::Target as CurveTarget;
use civbb_format::animator::*;
use civbb_format::blob::BlobId;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{
    AlphaMode, ColorSpace, ImageFormat, Material, MaterialField, MaterialModel, Pbr, Texture, TextureSlot, Unlit,
};
use civbb_format::math::Transform;
use civbb_format::mesh::{Mesh, Submesh};
use civbb_format::state::{Entry, EntryKind, StateTable};
use serde::Deserialize;
use serde_json::Value;

/// A state entry of a case: the animator parameter Unity recorded.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParamSpec {
    name: String,
    kind: EntryKind,
    default: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    source: String,
    nodes: Vec<NodeSpec>,
    params: Vec<ParamSpec>,
    clips: BTreeMap<String, ClipSpec>,
    layers: Vec<LayerSpec>,
    dt: f32,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSpec {
    name: String,
    #[serde(default = "yes")]
    active: bool,
    /// A renderer's materials, each its saved Unity properties: `_Color` (RGBA, gamma) alone is Unity's
    /// `Unlit/Color`; with `_Metallic`, `_Glossiness` or `_MainTex_ST` (scale xy, offset zw) it is `Standard`.
    #[serde(default)]
    materials: Vec<BTreeMap<String, Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipSpec {
    length: f32,
    #[serde(default)]
    looping: bool,
    /// `[path.property, [[time, value], ...]]`, linear keys.
    curves: Vec<(String, Vec<(f32, f32)>)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerSpec {
    name: String,
    #[serde(default = "one")]
    weight: f32,
    #[serde(default)]
    additive: bool,
    /// Transform paths the layer may write (an avatar mask); absent writes all.
    mask: Option<Vec<String>>,
    states: Vec<StateSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateSpec {
    name: String,
    motion: Option<MotionSpec>,
    wd: bool,
    #[serde(default)]
    transitions: Vec<TransitionSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum MotionSpec {
    Clip(String),
    /// Parameter and `[threshold, clip]` children.
    Simple1d(String, Vec<(f32, String)>),
    /// `[parameter, clip]` children.
    Direct(Vec<(String, String)>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionSpec {
    to: String,
    #[serde(default, rename = "if")]
    if_: Vec<String>,
    #[serde(default)]
    if_not: Vec<String>,
    #[serde(default)]
    duration: f32,
    exit_time: Option<f32>,
    /// Which transitions may interrupt this one (Unity's names); absent is `None`.
    interruption: Option<InterruptionSource>,
    #[serde(default = "yes")]
    ordered: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    #[serde(default)]
    set: BTreeMap<String, f32>,
    expect: BTreeMap<String, Value>,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

impl Case {
    fn param(&self, name: &str) -> u32 {
        self.params
            .iter()
            .position(|p| p.name == name)
            .unwrap_or_else(|| panic!("no parameter {name}")) as u32
    }

    fn clip(&self, name: &str) -> u32 {
        self.clips
            .keys()
            .position(|k| k == name)
            .unwrap_or_else(|| panic!("no clip {name}")) as u32
    }

    fn motion(&self, m: &MotionSpec) -> Motion {
        let child = |motion, threshold, direct_param| ChildMotion {
            motion,
            threshold,
            position: [0.0; 2],
            direct_param,
            time_scale: 1.0,
            mirror: false,
            cycle_offset: 0.0,
        };
        let tree = |kind, children| {
            Motion::BlendTree(Box::new(BlendTree {
                kind,
                children,
                normalize: false,
            }))
        };
        match m {
            MotionSpec::Clip(c) => Motion::Clip { clip: self.clip(c) },
            MotionSpec::Simple1d(p, children) => tree(
                BlendTreeKind::Simple1D { param: self.param(p) },
                children
                    .iter()
                    .map(|(t, c)| child(Motion::Clip { clip: self.clip(c) }, *t, None))
                    .collect(),
            ),
            MotionSpec::Direct(children) => tree(
                BlendTreeKind::Direct,
                children
                    .iter()
                    .map(|(p, c)| child(Motion::Clip { clip: self.clip(c) }, 0.0, Some(self.param(p))))
                    .collect(),
            ),
        }
    }

    fn avatar(&self) -> Document {
        let mut nodes = vec![Node {
            name: "Root".into(),
            parent: None,
            rest: Transform::IDENTITY,
            active: true,
        }];
        nodes.extend(self.nodes.iter().map(|n| Node {
            name: n.name.clone(),
            parent: Some(0),
            rest: Transform::IDENTITY,
            active: n.active,
        }));
        let clips: Vec<Clip> = self
            .clips
            .iter()
            .map(|(name, c)| Clip {
                name: name.clone(),
                length: c.length,
                looping: c.looping,
                root: None,
                curves: (c.curves.iter())
                    .map(|(target, keys)| curve(&self.nodes, target, keys))
                    .collect(),
            })
            .collect();
        let layers = self
            .layers
            .iter()
            .map(|l| {
                let index = |name: &str| {
                    l.states
                        .iter()
                        .position(|s| s.name == name)
                        .unwrap_or_else(|| panic!("no state {name}")) as u32
                };
                let states = l
                    .states
                    .iter()
                    .map(|s| State {
                        name: s.name.clone(),
                        motion: s.motion.as_ref().map(|m| self.motion(m)),
                        speed: 1.0,
                        speed_param: None,
                        cycle_offset: 0.0,
                        cycle_offset_param: None,
                        time_param: None,
                        unwritten: if s.wd { Unwritten::Rest } else { Unwritten::Hold },
                        mirror: false,
                        mirror_param: None,
                        transitions: s.transitions.iter().map(|t| self.transition(t, index(&t.to))).collect(),
                        actions: Vec::new(),
                    })
                    .collect();
                Layer {
                    name: l.name.clone(),
                    group: 0,
                    weight: l.weight,
                    blending: if l.additive {
                        LayerBlending::Additive
                    } else {
                        LayerBlending::Override
                    },
                    mask: l.mask.as_ref().map(|t| Mask {
                        body_parts: Vec::new(),
                        transforms: Some(t.iter().map(|name| node_index(&self.nodes, name)).collect()),
                    }),
                    state_machine: StateMachine {
                        states,
                        default_state: 0,
                        entry_transitions: Vec::new(),
                        any_state_transitions: Vec::new(),
                    },
                }
            })
            .collect();
        let (mut meshes, mut materials) = (Vec::new(), Vec::new());
        for (i, n) in self.nodes.iter().enumerate().filter(|(_, n)| !n.materials.is_empty()) {
            let submeshes = (n.materials.iter())
                .map(|spec| {
                    materials.push(unity_material(spec));
                    Submesh {
                        first_index: 0,
                        index_count: 0,
                        material: materials.len() as u32 - 1,
                    }
                })
                .collect();
            meshes.push(Mesh {
                name: n.name.clone(),
                node: i as u32 + 1,
                submeshes,
                ..Mesh::default()
            });
        }
        bake(&clips, &meshes, &mut materials);
        let textures = if materials.is_empty() {
            Vec::new()
        } else {
            vec![Texture {
                name: "white".into(),
                format: ImageFormat::Png,
                color_space: ColorSpace::Srgb,
                blob: BlobId([0; 32]),
            }]
        };
        Document {
            hierarchy: Hierarchy { nodes },
            meshes,
            materials,
            textures,
            state: StateTable {
                entries: (self.params.iter())
                    .map(|p| Entry::new(p.name.clone(), p.kind, p.default))
                    .collect(),
            },
            animator: Some(Animator {
                groups: vec![LayerGroup {
                    name: "All".into(),
                    weight: 1.0,
                }],
                layers,
            }),
            clips,
            ..Document::default()
        }
    }

    fn transition(&self, t: &TransitionSpec, dest: u32) -> Transition {
        let mut conditions = Vec::new();
        for (names, mode) in [(&t.if_, ConditionMode::If), (&t.if_not, ConditionMode::IfNot)] {
            conditions.extend(names.iter().map(|n| Condition {
                param: self.param(n),
                mode,
                threshold: 0.0,
            }));
        }
        Transition {
            dest: TransitionDest::State { state: dest },
            conditions,
            has_exit_time: t.exit_time.is_some(),
            exit_time: t.exit_time.unwrap_or(0.0),
            fixed_duration: true,
            duration: t.duration,
            offset: 0.0,
            interruption: t.interruption.unwrap_or(InterruptionSource::None),
            ordered_interruption: t.ordered,
            can_transition_to_self: false,
        }
    }
}

/// A saved Unity material as the importer maps it: `Standard` to PBR (its `_MainTex` always present, so a texture
/// slot), `Unlit/Color` to Unlit; colours gamma to linear.
fn unity_material(spec: &BTreeMap<String, Value>) -> Material {
    let num = |k: &str, default: f32| spec.get(k).map_or(default, |v| v.as_f64().unwrap() as f32);
    let vec4 = |k: &str, default: [f32; 4]| {
        spec.get(k).map_or(default, |v| {
            serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{k}: {e}"))
        })
    };
    for k in spec.keys() {
        assert!(
            ["_Color", "_Metallic", "_Glossiness", "_MainTex_ST"].contains(&k.as_str()),
            "unsupported material property {k}"
        );
    }
    let c = vec4("_Color", [1.0; 4]);
    let color = [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3]];
    let model = if spec.keys().any(|k| k != "_Color") {
        let st = vec4("_MainTex_ST", [1.0, 1.0, 0.0, 0.0]);
        MaterialModel::Pbr(Pbr {
            base_color: color,
            base_color_tex: Some(TextureSlot {
                texture: 0,
                uv_set: 0,
                scale_offset: [st[0], st[1], st[2], 1.0 - st[1] - st[3]],
            }),
            metallic: num("_Metallic", 0.0),
            roughness: 1.0 - num("_Glossiness", 0.5),
            metallic_roughness_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            occlusion_tex: None,
            emissive: [0.0; 3],
            emissive_tex: None,
        })
    } else {
        MaterialModel::Unlit(Unlit { color, color_tex: None })
    };
    Material {
        name: "m".into(),
        model,
        alpha: AlphaMode::Opaque,
        double_sided: false,
        order: 0,
    }
}

/// The native field and component of the Unity material properties the cases use, as the importer lowers them
/// (ADR 0099).
fn unity_property(prop: &str) -> (MaterialField, u8) {
    let (base, c) = prop.rsplit_once('.').unwrap_or((prop, "x"));
    let c = "xyzw"
        .find(c)
        .or_else(|| "rgba".find(c))
        .unwrap_or_else(|| panic!("bad component {prop}")) as u8;
    match base {
        "_Color" => (MaterialField::BaseColor, c),
        "_Metallic" => (MaterialField::Metallic, 0),
        "_MainTex_ST" => (MaterialField::BaseColorTransform, c),
        _ => panic!("unsupported material property {prop}"),
    }
}

/// What the importer bakes (ADR 0099): components of an animated field that no curve animates are 0 in the
/// renderer's materials, in Unity's units (so the stored V offset is 1 with Unity's V scale and V offset both 0).
fn bake(clips: &[Clip], meshes: &[Mesh], materials: &mut [Material]) {
    for (mi, mesh) in meshes.iter().enumerate() {
        let mut animated: Vec<(MaterialField, [bool; 4])> = Vec::new();
        for c in clips.iter().flat_map(|c| &c.curves) {
            if let CurveTarget::Mesh {
                mesh,
                property: MeshProperty::Material { field, component, .. },
            } = c.target
                && mesh as usize == mi
            {
                match animated.iter_mut().find(|a| a.0 == field) {
                    Some(a) => a.1[component as usize] = true,
                    None => {
                        let mut a = [false; 4];
                        a[component as usize] = true;
                        animated.push((field, a));
                    }
                }
            }
        }
        for (field, a) in animated {
            for c in (0..field.components()).filter(|&c| !a[c as usize]) {
                let v = if field == MaterialField::BaseColorTransform && c == 3 {
                    1.0
                } else {
                    0.0
                };
                for s in &mesh.submeshes {
                    let m = &mut materials[s.material as usize];
                    // Unity's V offset stays when its V scale becomes 0.
                    if field == MaterialField::BaseColorTransform
                        && c == 1
                        && let Some(st) = material::get(m, field)
                    {
                        material::set(m, field, 3, st[3] + st[1]);
                    }
                    material::set(m, field, c, v);
                }
            }
        }
    }
}

/// The index of the node named `name`: spec nodes follow the root.
fn node_index(nodes: &[NodeSpec], name: &str) -> u32 {
    let i = nodes.iter().position(|n| n.name == name);
    i.unwrap_or_else(|| panic!("no node {name}")) as u32 + 1
}

/// `"<node>.<x|y|z|active>"` or `"<node>:<material property>"` (a mesh per node with materials, in node order).
fn curve(nodes: &[NodeSpec], target: &str, keys: &[(f32, f32)]) -> Curve {
    let target = match target.split_once(':') {
        Some((name, prop)) => {
            let n = node_index(nodes, name) as usize - 1;
            let mesh = nodes[..n].iter().filter(|n| !n.materials.is_empty()).count() as u32;
            let (field, component) = unity_property(prop);
            assert!(
                prop != "_MainTex_ST.y",
                "the harness lowers V offsets without a V scale curve only"
            );
            let target = CurveTarget::Mesh {
                mesh,
                property: MeshProperty::Material {
                    field,
                    component,
                    srgb: field.is_color(),
                },
            };
            // Unity's V offset is from the bottom: stored, it is 1 - V scale (no curve: 0) - offset.
            let mut c = curve_keys(target, keys);
            if field == MaterialField::BaseColorTransform && component == 3 {
                for k in &mut c.keys {
                    (k.value, k.in_tangent, k.out_tangent) = (1.0 - k.value, -k.in_tangent, -k.out_tangent);
                }
            }
            return c;
        }
        None => {
            let (name, prop) = target
                .rsplit_once('.')
                .unwrap_or_else(|| panic!("bad curve target {target}"));
            let property = match prop {
                "x" => NodeProperty::Translation { component: 0 },
                "y" => NodeProperty::Translation { component: 1 },
                "z" => NodeProperty::Translation { component: 2 },
                "active" => NodeProperty::Active,
                _ => panic!("unknown property {prop}"),
            };
            CurveTarget::Node {
                node: node_index(nodes, name),
                property,
            }
        }
    };
    curve_keys(target, keys)
}

/// Keys with linear tangents.
fn curve_keys(target: CurveTarget, keys: &[(f32, f32)]) -> Curve {
    let slope = |i: usize, j: usize| (keys[j].1 - keys[i].1) / (keys[j].0 - keys[i].0);
    Curve {
        target,
        keys: (0..keys.len())
            .map(|i| Key {
                time: keys[i].0,
                value: keys[i].1,
                in_tangent: if i > 0 { slope(i - 1, i) } else { 0.0 },
                out_tangent: if i + 1 < keys.len() { slope(i, i + 1) } else { 0.0 },
            })
            .collect(),
    }
}

/// The current value of `"<node>.<property>"`: its slot, or the node's rest value when nothing animates it.
fn value(c: &Controller, a: &Document, i: &Instance, key: &str) -> f32 {
    let (path, prop) = key.rsplit_once('.').unwrap_or_else(|| panic!("bad key {key}"));
    let node = a
        .hierarchy
        .nodes
        .iter()
        .position(|n| n.name == path)
        .unwrap_or_else(|| panic!("no node {path}")) as u32;
    let target = match prop {
        "x" => Target::Position(node, 0),
        "y" => Target::Position(node, 1),
        "z" => Target::Position(node, 2),
        "active" => Target::Active(node),
        _ => panic!("unknown property {prop}"),
    };
    match c.bound.bindings.targets.iter().position(|t| *t == target) {
        Some(s) => i.pose()[s],
        None => match target {
            Target::Active(n) => f32::from(u8::from(a.hierarchy.nodes[n as usize].active)),
            _ => 0.0,
        },
    }
}

/// What material `<index>` of the renderer at `<node>` receives for `<property>` after the step, in Unity's units: the
/// avatar's material with every slot of the renderer set, as `Island::materials` applies them (ADRs 0025, 0099).
fn material_value(c: &Controller, a: &Document, i: &Instance, key: &str) -> Option<f32> {
    let mut parts = key.splitn(3, ':');
    let (path, index, prop) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
    let node = a.hierarchy.nodes.iter().position(|n| n.name == path).unwrap() as u32;
    let mesh = a.meshes.iter().position(|m| m.node == node).unwrap();
    let sub = &a.meshes[mesh].submeshes[index.parse::<usize>().unwrap()];
    let mut m = a.materials[sub.material as usize].clone();
    for (slot, t) in c.bound.bindings.targets.iter().enumerate() {
        if let Target::Material(on, field, component, srgb) = *t
            && on as usize == mesh
        {
            let v = i.pose()[slot];
            let v = if srgb && component < 3 { to_linear(v) } else { v };
            material::set(&mut m, field, component, v);
        }
    }
    // Back to Unity's units: sRGB-encoded colours, the V offset from the bottom.
    let (field, c) = unity_property(prop);
    let v = material::get(&m, field)?;
    Some(match (field, c) {
        (MaterialField::BaseColor, 0..3) => to_srgb(v[c as usize]),
        (MaterialField::BaseColorTransform, 3) => 1.0 - v[1] - v[3],
        _ => v[c as usize],
    })
}

fn run(path: &Path, unity: &Value) {
    let name = path.file_name().unwrap().to_string_lossy();
    let recorded = unity["cases"][path.file_stem().unwrap().to_string_lossy().as_ref()]
        .as_array()
        .unwrap_or_else(|| panic!("{name}: not recorded in Unity; run Goldens.Record"));
    let case: Case =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(!case.source.is_empty(), "{name}: cite a source");
    let a = case.avatar();
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true)
        .unwrap_or_else(|e| panic!("{name}: invalid case: {e:?}"));
    let c = Controller::new(&a).unwrap();
    assert!(c.bound.unbound.is_empty(), "{name}: {:?}", c.bound.unbound);
    let mut inst = Instance::new(&c, true);
    assert_eq!(recorded.len(), case.frames.len(), "{name}: re-record in Unity");
    for (f, frame) in case.frames.iter().enumerate() {
        for (key, want) in &frame.expect {
            let got = &recorded[f][key];
            let same = match (want.as_f64(), got.as_f64()) {
                (Some(w), Some(g)) => (w - g).abs() < 1e-4,
                _ => want == got,
            };
            assert!(same, "{name}: frame {f}: {key} expects {want}, Unity recorded {got}");
        }
        for (p, v) in &frame.set {
            inst.params[case.param(p) as usize] = *v;
        }
        inst.update(&c, case.dt);
        for (key, want) in &frame.expect {
            if key.contains(':') {
                let got = material_value(&c, &a, &inst, key);
                let same = match (want.as_f64(), got) {
                    (Some(w), Some(g)) => (w as f32 - g).abs() < 1e-4,
                    (None, None) => want.is_null(),
                    _ => false,
                };
                assert!(same, "{name}: frame {f}: {key} = {got:?}, expected {want}");
            } else if let Some(state) = want.as_str() {
                let li = case
                    .layers
                    .iter()
                    .position(|l| l.name == *key)
                    .unwrap_or_else(|| panic!("no layer {key}"));
                let got = &case.layers[li].states[inst.layer(li).unwrap().current as usize].name;
                assert_eq!(got, state, "{name}: frame {f}: layer {key}");
            } else {
                let want = want.as_f64().unwrap_or_else(|| panic!("{name}: bad expectation {key}")) as f32;
                let got = value(&c, &a, &inst, key);
                assert!(
                    (got - want).abs() < 1e-4,
                    "{name}: frame {f}: {key} = {got}, expected {want}"
                );
            }
        }
    }
}

#[test]
fn golden_cases() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    assert!(paths.len() >= 6, "golden cases missing");
    let unity: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/goldens.json")).unwrap(),
    )
    .unwrap();
    for p in &paths {
        run(p, &unity);
    }
}
