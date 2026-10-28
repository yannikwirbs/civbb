//! Exports an avatar's node, mesh, skin and morph subset as GLB, the importer's test input.
//!
//! Morph targets use sparse accessors, so the bloated avatar's 1,200 shape keys stay small. Materials export
//! their base colour only; textures, animations and everything outside glTF are left out.

use civbb_format::Avatar;
use civbb_format::material::MaterialModel;
use civbb_format::mesh::Mesh;
use serde_json::{Value, json};

const FLOAT: u32 = 5126;
const U16: u32 = 5123;
const U32: u32 = 5125;

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
pub fn to_glb(a: &Avatar) -> Vec<u8> {
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
    glb(serde_json::to_vec(&doc).expect("json"), bin.data)
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
