//! Exports an avatar's node, mesh, skin and morph subset as GLB, the importer's test input.
//!
//! Morph targets use sparse accessors, so the bloated avatar's 1,200 shape keys stay small. Materials export
//! their base colour only; textures and animations are left out. [`to_source_glb`] adds the source package's
//! `X_*` extensions (ADR 0041) for what glTF lacks.

use civbb_format::Document;
use civbb_format::animator::{
    Action, Animator, EulerOrder, HumanoidProperty, Key, MeshProperty, NodeProperty, Target, Unwritten,
};
use civbb_format::constraint::ConstraintType;
use civbb_format::face::{EyeDriver, ShapeBind};
use civbb_format::material::MaterialModel;
use civbb_format::mesh::Mesh;
use civbb_format::state::{EntryIndex, EntryKind, Platform, SyncMode};
use serde_json::{Value, json};

const FLOAT: u32 = 5126;
const U16: u32 = 5123;
const U32: u32 = 5125;
/// Unity dialect 1's states (the inverse of the importer's lowering): `unwritten` as `write_defaults`, actions as
/// behaviours.
fn raise_states(animator: &Animator, x: &mut Value) {
    for (layer, l) in animator.layers.iter().zip(x["layers"].as_array_mut().expect("array")) {
        let states = l["state_machine"]["states"].as_array_mut().expect("array");
        for (state, s) in layer.state_machine.states.iter().zip(states) {
            let o = s.as_object_mut().expect("object");
            o.remove("unwritten");
            o.remove("actions");
            o.insert("write_defaults".into(), json!(state.unwritten == Unwritten::Rest));
            let behaviours = state.actions.iter().map(|a| match a {
                Action::Entries { owner_only, ops } => {
                    json!({"ParameterDriver": {"local_only": owner_only, "ops": ops}})
                }
                Action::PoseSource(parts) => json!({ "TrackingControl": parts }),
                // Same shape in both.
                Action::LayerWeight { .. } | Action::GroupWeight { .. } | Action::Locomotion { .. } => json!(a),
            });
            o.insert("behaviours".into(), json!(behaviours.collect::<Vec<_>>()));
        }
    }
}

/// VRChat's PhysBone output suffixes, in `ChainOutputs::entries` order (the importer's list).
const CHAIN_SUFFIXES: [&str; 5] = ["_IsGrabbed", "_IsPosed", "_Angle", "_Stretch", "_Squish"];

#[derive(Default)]
struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    fn view(&mut self, bytes: &[u8]) -> usize {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        self.views
            .push(json!({"buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len()}));
        self.data.extend_from_slice(bytes);
        self.views.len() - 1
    }

    fn accessor(&mut self, a: Value) -> usize {
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn floats<const N: usize>(&mut self, items: &[[f32; N]], ty: &str, bounds: bool) -> usize {
        let bytes: Vec<u8> = items.as_flattened().iter().flat_map(|x| x.to_le_bytes()).collect();
        let view = self.view(&bytes);
        let mut a = json!({"bufferView": view, "componentType": FLOAT, "count": items.len(), "type": ty});
        if bounds {
            let fold = |f: fn(f32, f32) -> f32, init: f32| -> Vec<f32> {
                (0..N).map(|c| items.iter().map(|v| v[c]).fold(init, f)).collect()
            };
            a["min"] = json!(fold(f32::min, f32::INFINITY));
            a["max"] = json!(fold(f32::max, f32::NEG_INFINITY));
        }
        self.accessor(a)
    }

    fn u32s(&mut self, items: &[u32]) -> usize {
        let bytes: Vec<u8> = items.iter().flat_map(|x| x.to_le_bytes()).collect();
        let view = self.view(&bytes);
        self.accessor(json!({"bufferView": view, "componentType": U32, "count": items.len(), "type": "SCALAR"}))
    }

    /// A sparse VEC3 accessor over `count` vertices, zero except at `indices`.
    fn sparse(&mut self, count: usize, indices: &[u32], values: &[[f32; 3]]) -> usize {
        if indices.is_empty() {
            return self.floats(&vec![[0.0; 3]; count], "VEC3", false);
        }
        let ix: Vec<u8> = indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let vals: Vec<u8> = values.as_flattened().iter().flat_map(|x| x.to_le_bytes()).collect();
        let (iv, vv) = (self.view(&ix), self.view(&vals));
        self.accessor(json!({
            "componentType": FLOAT, "count": count, "type": "VEC3",
            "sparse": {
                "count": indices.len(),
                "indices": {"bufferView": iv, "componentType": U32},
                "values": {"bufferView": vv},
            },
        }))
    }
}

/// The avatar as a GLB file. Each node may carry at most one mesh.
pub fn to_glb(a: &Document) -> Vec<u8> {
    let (doc, bin) = document(a);
    glb(serde_json::to_vec(&doc).expect("json"), bin)
}

/// As [`to_glb`], plus `extras.active` on inactive nodes and the `X_*` extensions: humanoid map and viewpoint,
/// animator and clips, constraints, dynamics, contacts and expressions. Node indices are glTF indices, which
/// [`to_glb`] keeps equal to the avatar's.
pub fn to_source_glb(a: &Document) -> Vec<u8> {
    let (mut doc, bin) = document(a);
    for (i, n) in a.hierarchy.nodes.iter().enumerate() {
        if !n.active {
            doc["nodes"][i]["extras"] = json!({"active": false});
        }
    }
    let mut ext = serde_json::Map::new();
    if let Some(h) = a.avatar().and_then(|r| r.humanoid.as_ref()) {
        let bones: serde_json::Map<String, Value> = h
            .bones
            .iter()
            .map(|(b, n)| {
                (
                    serde_json::to_value(b)
                        .expect("bone")
                        .as_str()
                        .expect("name")
                        .to_owned(),
                    json!(n),
                )
            })
            .collect();
        let mut x = json!({"humanoid": bones, "viewpoint": a.avatar().map_or([0.0; 3], |r| r.viewpoint), "muscle_frames": h.muscle_frames});
        if let Some(face) = a.avatar().and_then(|r| r.face.as_ref()) {
            // Binds name the glTF node carrying the mesh (ADR 0042); node indices equal the avatar's.
            let mut face = face.clone();
            let binds = face.visemes.iter_mut().flat_map(|(_, b)| b.iter_mut());
            let eye_binds: Vec<&mut ShapeBind> = match face.eyes.as_mut().map(|e| &mut e.driver) {
                Some(EyeDriver::Shapes { up, down, left, right }) => {
                    [up, down, left, right].into_iter().flatten().collect()
                }
                _ => Vec::new(),
            };
            for b in binds.chain(face.blink.iter_mut()).chain(eye_binds) {
                b.mesh = a.meshes[b.mesh as usize].node;
            }
            x["face"] = json!(face);
        }
        ext.insert("X_avatar".into(), x);
    }
    // Unity dialect 1's parameters (the inverse of the importer's lowering): every entry an animator parameter with
    // its rest value as default, every external one an expression parameter, references as names.
    let entries = &a.state.entries;
    let name = |e: Option<EntryIndex>| e.map_or("", |i| dialect(&entries[i as usize].name));
    if let Some(animator) = &a.animator {
        let mut x = json!(animator);
        let params = entries
            .iter()
            .map(|e| json!({"name": dialect(&e.name), "kind": e.kind, "default": e.rest()}));
        x["parameters"] = json!(params.collect::<Vec<_>>());
        raise_states(animator, &mut x);
        let paths = node_paths(a);
        for layer in x["layers"].as_array_mut().expect("array") {
            if let Some(t) = layer.pointer_mut("/mask/transforms").filter(|t| t.is_array()) {
                let nodes = t.as_array().expect("array").iter().map(|n| n.as_u64().expect("node"));
                *t = json!(nodes.map(|n| paths[n as usize].as_str()).collect::<Vec<_>>());
            }
        }
        let mut clips = json!(a.clips);
        for (clip, v) in a.clips.iter().zip(clips.as_array_mut().expect("array")) {
            for (c, v) in clip.curves.iter().zip(v["curves"].as_array_mut().expect("array")) {
                let o = v.as_object_mut().expect("object");
                o.remove("target");
                o.insert("binding".into(), binding(a, &paths, &c.target));
                // Dialect 1's units (the inverse of the importer's conversion).
                let unit: Option<fn(f32) -> f32> = match c.target {
                    Target::Mesh {
                        property: MeshProperty::Weight(_),
                        ..
                    } => Some(|v| v * 100.0),
                    Target::Node {
                        property: NodeProperty::EulerRotation { .. },
                        ..
                    } => Some(f32::to_degrees),
                    _ => None,
                };
                if let Some(f) = unit {
                    let keys = c.keys.iter().map(|k| Key {
                        value: f(k.value),
                        in_tangent: f(k.in_tangent),
                        out_tangent: f(k.out_tangent),
                        ..*k
                    });
                    o.insert("keys".into(), json!(keys.collect::<Vec<_>>()));
                }
            }
        }
        ext.insert("X_animator".into(), json!({"animator": x, "clips": clips}));
    }
    if !a.constraints.is_empty() {
        ext.insert("X_constraints".into(), json!(a.constraints));
    }
    if !a.dynamics.chains.is_empty() || !a.dynamics.colliders.is_empty() {
        let mut x = json!(a.dynamics);
        let chains = x["chains"].as_array_mut().expect("array");
        for (c, v) in a.dynamics.chains.iter().zip(chains) {
            // The prefix the outputs' names share; an avatar from dialect 1 has no other kind.
            let names = c.outputs.entries().map(&name);
            let prefix = (names.iter().zip(CHAIN_SUFFIXES)).find_map(|(n, s)| n.strip_suffix(s));
            let o = v.as_object_mut().expect("object");
            o.remove("outputs");
            o.insert("parameter".into(), json!(prefix));
        }
        ext.insert("X_dynamics".into(), x);
    }
    if !a.contacts.senders.is_empty() || !a.contacts.receivers.is_empty() {
        let mut x = json!(a.contacts);
        for (r, v) in a
            .contacts
            .receivers
            .iter()
            .zip(x["receivers"].as_array_mut().expect("array"))
        {
            v["parameter"] = json!(name(r.parameter));
        }
        ext.insert("X_contacts".into(), x);
    }
    if let Some(e) = a.avatar().and_then(|r| r.expressions.as_ref()) {
        let external = entries.iter().filter(|e| e.external && e.kind != EntryKind::Trigger);
        let params = external.map(|e| {
            let synced = e.sync == SyncMode::Owner;
            json!({"name": e.name, "kind": e.kind, "default": e.default, "saved": e.saved, "synced": synced})
        });
        let mut menu = json!(e.menu);
        menu_names(&mut menu, &name);
        ext.insert(
            "X_expressions".into(),
            json!({"parameters": params.collect::<Vec<_>>(), "menu": menu}),
        );
    }
    doc["extensionsUsed"] = json!(ext.keys().collect::<Vec<_>>());
    doc["extensions"] = Value::Object(ext);
    glb(serde_json::to_vec(&doc).expect("json"), bin)
}

/// Each node's dialect 1 path: names below the root joined by `/`, the root's empty.
fn node_paths(a: &Document) -> Vec<String> {
    let mut paths: Vec<String> = Vec::with_capacity(a.hierarchy.nodes.len());
    for n in &a.hierarchy.nodes {
        paths.push(match n.parent {
            None => String::new(),
            Some(0) => n.name.clone(),
            Some(p) => format!("{}/{}", paths[p as usize], n.name),
        });
    }
    paths
}

/// A curve target as dialect 1's binding (the inverse of the importer's resolution): a path and a Unity property.
fn binding(a: &Document, paths: &[String], t: &Target) -> Value {
    let (path, property) = match t {
        Target::Node { node, property } => {
            let p = match *property {
                NodeProperty::Translation(i) => json!({"Position": i}),
                NodeProperty::Rotation(i) => json!({"Rotation": i}),
                NodeProperty::EulerRotation {
                    axis,
                    order: EulerOrder::Zxy,
                } => json!({"EulerRotation": axis}),
                NodeProperty::Scale(i) => json!({"Scale": i}),
                NodeProperty::Active => json!("Active"),
            };
            (paths[*node as usize].as_str(), p)
        }
        Target::Mesh { mesh, property } => {
            let m = &a.meshes[*mesh as usize];
            let p = match property {
                MeshProperty::Visible => json!("RendererEnabled"),
                MeshProperty::Weight(s) => json!({"BlendShape": m.blend_shapes[*s as usize].name}),
                // Unity names depend on the shader family, and V-up offsets on two curves at once.
                MeshProperty::Material { .. } => unimplemented!("the corpus generates no material curves"),
                MeshProperty::MaterialSlot(s) => json!({"MaterialSlot": s}),
            };
            (paths[m.node as usize].as_str(), p)
        }
        &Target::Humanoid(HumanoidProperty::Muscle { bone, axis }) => {
            let m = civbb_anim::muscle::index(bone, axis).expect("a muscle");
            ("", json!({"Muscle": civbb_anim::muscle::MUSCLES[m as usize].name}))
        }
        Target::Humanoid(HumanoidProperty::Body(i)) => ("", json!({"RootMotion": i})),
        Target::Entry(e) => ("", json!({"Parameter": dialect(&a.state.entries[*e as usize].name)})),
        Target::Constraint { constraint, field } => {
            let c = &a.constraints[*constraint as usize];
            let kind = ConstraintType::of(&c.kind).expect("an animatable constraint");
            (
                paths[c.target as usize].as_str(),
                json!({"Constraint": {"kind": kind, "field": field}}),
            )
        }
    };
    json!({"path": path, "property": property})
}

/// An entry's name in dialect 1: platform values under VRChat's names.
fn dialect(name: &str) -> &str {
    Platform::from_name(name).map_or(name, |p| p.vrchat_name())
}

/// A serialised menu's entry references as dialect 1's names.
fn menu_names<'a>(menu: &mut Value, name: &dyn Fn(Option<EntryIndex>) -> &'a str) {
    let index = |v: &Value| v.as_u64().map(|i| i as EntryIndex);
    for c in menu["controls"].as_array_mut().expect("array") {
        c["parameter"] = index(&c["parameter"]).map_or(Value::Null, |i| json!(name(Some(i))));
        let subs: Vec<&str> = (c["sub_parameters"].as_array().expect("array").iter())
            .map(|v| name(index(v)))
            .collect();
        c["sub_parameters"] = json!(subs);
        if let Some(sub) = c["kind"].get_mut("SubMenu") {
            menu_names(sub, name);
        }
    }
}

fn document(a: &Document) -> (Value, Vec<u8>) {
    let mut bin = Bin::default();
    let mut nodes: Vec<Value> = a
        .hierarchy
        .nodes
        .iter()
        .map(|n| {
            json!({
                "name": n.name,
                "translation": n.rest.translation,
                "rotation": n.rest.rotation,
                "scale": n.rest.scale,
            })
        })
        .collect();
    for (i, n) in a.hierarchy.nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            let children = &mut nodes[p as usize]["children"];
            if children.is_null() {
                *children = json!([]);
            }
            children.as_array_mut().expect("array").push(json!(i));
        }
    }
    let mut meshes = Vec::new();
    let mut skins = Vec::new();
    for m in &a.meshes {
        let node = &mut nodes[m.node as usize];
        assert!(node.get("mesh").is_none(), "node {} carries two meshes", m.node);
        node["mesh"] = json!(meshes.len());
        meshes.push(mesh(&mut bin, m));
        if let Some(s) = &m.skin {
            let ibm = bin.floats(&s.inverse_bind, "MAT4", false);
            node["skin"] = json!(skins.len());
            skins.push(json!({"joints": s.joints, "inverseBindMatrices": ibm}));
        }
    }
    let materials: Vec<Value> = a
        .materials
        .iter()
        .map(|m| {
            let color = match &m.model {
                MaterialModel::Pbr(p) => p.base_color,
                MaterialModel::Toon(t) => t.base_color,
                MaterialModel::Unlit(u) => u.color,
            };
            json!({
                "name": m.name,
                "pbrMetallicRoughness": {"baseColorFactor": color, "metallicFactor": 0.0, "roughnessFactor": 1.0},
                "doubleSided": m.double_sided,
            })
        })
        .collect();
    while !bin.data.len().is_multiple_of(4) {
        bin.data.push(0);
    }
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "civbb-corpus"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": bin.accessors,
        "bufferViews": bin.views,
        "buffers": [{"byteLength": bin.data.len()}],
    });
    if !skins.is_empty() {
        doc["skins"] = json!(skins);
    }
    (doc, bin.data)
}

fn mesh(bin: &mut Bin, m: &Mesh) -> Value {
    let mut attributes = json!({
        "POSITION": bin.floats(&m.positions, "VEC3", true),
        "NORMAL": bin.floats(&m.normals, "VEC3", false),
    });
    if !m.tangents.is_empty() {
        attributes["TANGENT"] = json!(bin.floats(&m.tangents, "VEC4", false));
    }
    if !m.uv0.is_empty() {
        attributes["TEXCOORD_0"] = json!(bin.floats(&m.uv0, "VEC2", false));
    }
    if !m.uv1.is_empty() {
        attributes["TEXCOORD_1"] = json!(bin.floats(&m.uv1, "VEC2", false));
    }
    if let Some(s) = &m.skin {
        let bytes: Vec<u8> = s
            .vertex_joints
            .as_flattened()
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        let view = bin.view(&bytes);
        let count = s.vertex_joints.len();
        attributes["JOINTS_0"] =
            json!(bin.accessor(json!({"bufferView": view, "componentType": U16, "count": count, "type": "VEC4"})));
        attributes["WEIGHTS_0"] = json!(bin.floats(&s.vertex_weights, "VEC4", false));
    }
    let n = m.positions.len();
    let targets: Vec<Value> = m
        .blend_shapes
        .iter()
        .map(|b| {
            let ix: Vec<u32> = b.deltas.iter().map(|d| d.vertex).collect();
            let pos: Vec<[f32; 3]> = b.deltas.iter().map(|d| d.position).collect();
            let nrm: Vec<[f32; 3]> = b.deltas.iter().map(|d| d.normal).collect();
            json!({"POSITION": bin.sparse(n, &ix, &pos), "NORMAL": bin.sparse(n, &ix, &nrm)})
        })
        .collect();
    let primitives: Vec<Value> = m
        .submeshes
        .iter()
        .map(|s| {
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            let mut p = json!({
                "attributes": attributes,
                "indices": bin.u32s(&m.indices[range]),
                "material": s.material,
            });
            if !targets.is_empty() {
                p["targets"] = json!(targets);
            }
            p
        })
        .collect();
    let mut out = json!({"name": m.name, "primitives": primitives});
    if !m.blend_shapes.is_empty() {
        let names: Vec<&str> = m.blend_shapes.iter().map(|b| b.name.as_str()).collect();
        let weights: Vec<f32> = m.blend_shapes.iter().map(|b| b.default_weight).collect();
        out["weights"] = json!(weights);
        out["extras"] = json!({"targetNames": names});
    }
    out
}

fn glb(mut json: Vec<u8>, bin: Vec<u8>) -> Vec<u8> {
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    for (kind, chunk) in [(b"JSON", &json), (b"BIN\0", &bin)] {
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(chunk);
    }
    out
}
