//! glTF meshes: one format mesh per node (its primitives become submeshes), vertex streams, skins and morph targets.

use std::collections::HashMap;

use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Mat4, Vec3};
use civbb_format::mesh::{BlendShape, Mesh, ShapeDelta, Skin, Submesh};
use gltf::mesh::{Mode, Semantic};

use super::Importer;
use crate::accessor::{arrays, floats, uints};

impl Importer<'_> {
    pub(super) fn meshes(&mut self) -> Result<(), String> {
        let doc = self.doc;
        let mut order: Vec<(NodeIndex, gltf::Node)> = doc
            .nodes()
            .filter_map(|n| Some((self.map[n.index()]?, n)))
            .filter(|(_, n)| n.mesh().is_some())
            .collect();
        order.sort_by_key(|(i, _)| *i);
        if order.len() > self.caps.meshes {
            return Err(format!("{} meshes exceeds cap {}", order.len(), self.caps.meshes));
        }
        for (at, node) in order {
            let mesh = self
                .mesh(&node, at)
                .map_err(|e| format!("node {}: {e}", node.index()))?;
            self.node_meshes.insert(at, self.meshes.len());
            self.meshes.push(mesh);
        }
        Ok(())
    }

    fn skin(&self, skin: &gltf::Skin) -> Result<Skin, String> {
        let joints = skin
            .joints()
            .map(|j| self.map[j.index()].ok_or_else(|| format!("joint node {} is not in the scene", j.index())))
            .collect::<Result<Vec<_>, _>>()?;
        const IDENTITY: Mat4 = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let inverse_bind = match skin.inverse_bind_matrices() {
            Some(acc) => {
                let mut m = arrays::<16>(&acc, &self.buffers, self.caps.nodes.max(joints.len()))?;
                if m.len() < joints.len() {
                    return Err("fewer inverse bind matrices than joints".into());
                }
                m.truncate(joints.len());
                m
            }
            None => vec![IDENTITY; joints.len()],
        };
        Ok(Skin {
            joints,
            inverse_bind,
            ..Skin::default()
        })
    }

    fn mesh(&mut self, node: &gltf::Node, at: NodeIndex) -> Result<Mesh, String> {
        let gmesh = node.mesh().expect("filtered");
        let names = target_names(&gmesh);
        let weights = node.weights().or(gmesh.weights()).unwrap_or(&[]);
        let mut m = Mesh {
            name: self.nodes[at as usize].name.clone(),
            node: at,
            skin: node.skin().map(|s| self.skin(&s)).transpose()?,
            ..Mesh::default()
        };
        let any = |s: Semantic| gmesh.primitives().any(|p| p.get(&s).is_some());
        let streams = Streams {
            tangents: any(Semantic::Tangents),
            uv0: any(Semantic::TexCoords(0)),
            uv1: any(Semantic::TexCoords(1)),
        };
        let mut sets: HashMap<Vec<Option<usize>>, (u32, u32)> = HashMap::new();
        let mut missing_normals = Vec::new();
        let mut shapes: Option<Vec<BlendShape>> = None;
        for p in gmesh.primitives() {
            if !matches!(p.mode(), Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan) {
                self.warnings
                    .push(format!("node {}: {:?} primitive skipped", node.index(), p.mode()));
                continue;
            }
            // Primitives sharing accessors (UniVRM submeshes) share one vertex range.
            let key = vertex_key(&p);
            let (base, count) = match sets.get(&key) {
                Some(s) => *s,
                None => {
                    let base = m.positions.len() as u32;
                    self.vertices(&p, &mut m, &streams, &mut missing_normals)?;
                    let count = m.positions.len() as u32 - base;
                    let shapes = shapes.get_or_insert_with(|| {
                        (0..p.morph_targets().len())
                            .map(|i| BlendShape {
                                name: names.get(i).cloned().unwrap_or_else(|| format!("shape{i}")),
                                default_weight: weights.get(i).copied().unwrap_or(0.0),
                                deltas: Vec::new(),
                            })
                            .collect()
                    });
                    self.targets(&p, base, count, shapes)?;
                    sets.insert(key, (base, count));
                    (base, count)
                }
            };
            let raw = match p.indices() {
                Some(acc) => uints(&acc, &self.buffers, self.indices_left)?,
                None => (0..count).collect(),
            };
            if raw.iter().any(|&i| i >= count) {
                return Err("vertex index out of range".into());
            }
            let tris = triangulate(&raw, p.mode());
            self.indices_left = self.indices_left.checked_sub(tris.len()).ok_or("index cap exceeded")?;
            let first_index = m.indices.len() as u32;
            m.indices.extend(tris.iter().map(|i| i + base));
            let material = self.primitive_material(&p);
            m.submeshes.push(Submesh {
                first_index,
                index_count: tris.len() as u32,
                material,
            });
        }
        m.blend_shapes = shapes.unwrap_or_default();
        self.shapes_left = self
            .shapes_left
            .checked_sub(m.blend_shapes.len())
            .ok_or("blend shape cap exceeded")?;
        if !missing_normals.is_empty() {
            compute_normals(&mut m, &missing_normals);
        }
        Ok(m)
    }

    /// Appends one vertex set's streams to `m`.
    fn vertices(
        &mut self,
        p: &gltf::Primitive,
        m: &mut Mesh,
        streams: &Streams,
        missing_normals: &mut Vec<std::ops::Range<usize>>,
    ) -> Result<(), String> {
        let bufs = &self.buffers;
        let positions = arrays::<3>(
            &p.get(&Semantic::Positions).ok_or("primitive without POSITION")?,
            bufs,
            self.vertices_left,
        )?;
        let n = positions.len();
        self.vertices_left -= n;
        let start = m.positions.len();
        let read = |s: Semantic| -> Result<Option<Vec<f32>>, String> {
            p.get(&s)
                .map(|acc| {
                    let v = floats(&acc, bufs, n)?;
                    if v.len() != n * acc.dimensions().multiplicity() {
                        return Err(format!("{s:?} count differs from POSITION"));
                    }
                    Ok(v)
                })
                .transpose()
        };
        fn chunk<const N: usize>(v: Vec<f32>) -> Vec<[f32; N]> {
            v.as_chunks::<N>().0.to_vec()
        }
        let check = |s: Semantic, want: usize| -> Result<(), String> {
            match p.get(&s) {
                Some(acc) if acc.dimensions().multiplicity() != want => {
                    Err(format!("{s:?}: expected {want} components"))
                }
                _ => Ok(()),
            }
        };
        check(Semantic::Normals, 3)?;
        check(Semantic::Tangents, 4)?;
        check(Semantic::TexCoords(0), 2)?;
        check(Semantic::TexCoords(1), 2)?;
        check(Semantic::Joints(0), 4)?;
        check(Semantic::Weights(0), 4)?;
        m.positions.extend(positions);
        match read(Semantic::Normals)? {
            Some(v) => m.normals.extend(chunk::<3>(v)),
            None => {
                missing_normals.push(start..start + n);
                m.normals.resize(start + n, [0.0; 3]);
            }
        }
        if streams.tangents {
            let v = read(Semantic::Tangents)?.map_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; n], chunk::<4>);
            m.tangents.extend(v);
        }
        if streams.uv0 {
            let v = read(Semantic::TexCoords(0))?.map_or_else(|| vec![[0.0; 2]; n], chunk::<2>);
            m.uv0.extend(v);
        }
        if streams.uv1 {
            let v = read(Semantic::TexCoords(1))?.map_or_else(|| vec![[0.0; 2]; n], chunk::<2>);
            m.uv1.extend(v);
        }
        if let Some(skin) = &mut m.skin {
            match (p.get(&Semantic::Joints(0)), read(Semantic::Weights(0))?) {
                (Some(acc), Some(w)) => {
                    let j = uints(&acc, bufs, n)?;
                    if j.len() != n * 4 {
                        return Err("JOINTS_0 count differs from POSITION".into());
                    }
                    for c in j.as_chunks::<4>().0 {
                        let mut out = [0u16; 4];
                        for (o, &x) in out.iter_mut().zip(c) {
                            *o = u16::try_from(x).map_err(|_| "joint index over 65535")?;
                        }
                        skin.vertex_joints.push(out);
                    }
                    skin.vertex_weights
                        .extend(chunk::<4>(w).into_iter().map(normalize_weights));
                }
                _ => {
                    skin.vertex_joints.resize(start + n, [0; 4]);
                    skin.vertex_weights.resize(start + n, [1.0, 0.0, 0.0, 0.0]);
                }
            }
        }
        if p.get(&Semantic::Joints(1)).is_some() {
            self.warnings
                .push("JOINTS_1/WEIGHTS_1 ignored: at most four influences per vertex".into());
        }
        Ok(())
    }

    /// Appends one vertex set's morph target deltas, sparse, to `shapes`.
    fn targets(&mut self, p: &gltf::Primitive, base: u32, count: u32, shapes: &mut [BlendShape]) -> Result<(), String> {
        if p.morph_targets().len() != shapes.len() {
            return Err("primitives disagree on morph target count".into());
        }
        let n = count as usize;
        for (t, shape) in p.morph_targets().zip(shapes) {
            let read = |acc: Option<gltf::Accessor>| -> Result<Option<Vec<[f32; 3]>>, String> {
                acc.map(|a| {
                    let v = arrays::<3>(&a, &self.buffers, n)?;
                    if v.len() != n {
                        return Err("morph target count differs from POSITION".into());
                    }
                    Ok(v)
                })
                .transpose()
            };
            let (pos, nrm) = (read(t.positions())?, read(t.normals())?);
            let at = |v: &Option<Vec<[f32; 3]>>, i: usize| v.as_ref().map_or([0.0; 3], |v| v[i]);
            for i in 0..n {
                let (position, normal) = (at(&pos, i), at(&nrm, i));
                if position != [0.0; 3] || normal != [0.0; 3] {
                    self.deltas_left = self.deltas_left.checked_sub(1).ok_or("shape delta cap exceeded")?;
                    shape.deltas.push(ShapeDelta {
                        vertex: base + i as u32,
                        position,
                        normal,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Which optional vertex streams any primitive of a mesh has; primitives lacking one get defaults.
struct Streams {
    tangents: bool,
    uv0: bool,
    uv1: bool,
}

/// Accessor indices identifying a primitive's vertex data, including morph targets.
fn vertex_key(p: &gltf::Primitive) -> Vec<Option<usize>> {
    let semantics = [
        Semantic::Positions,
        Semantic::Normals,
        Semantic::Tangents,
        Semantic::TexCoords(0),
        Semantic::TexCoords(1),
        Semantic::Joints(0),
        Semantic::Weights(0),
    ];
    let mut key: Vec<Option<usize>> = semantics.iter().map(|s| p.get(s).map(|a| a.index())).collect();
    for t in p.morph_targets() {
        key.extend([t.positions(), t.normals()].map(|a| a.map(|a| a.index())));
    }
    key
}

fn target_names(mesh: &gltf::Mesh) -> Vec<String> {
    let Some(raw) = mesh.extras() else {
        return Vec::new();
    };
    let value: serde_json::Value = serde_json::from_str(raw.get()).unwrap_or_default();
    value["targetNames"]
        .as_array()
        .map(|a| a.iter().map(|n| n.as_str().unwrap_or_default().to_owned()).collect())
        .unwrap_or_default()
}

fn triangulate(raw: &[u32], mode: Mode) -> Vec<u32> {
    let n = raw.len();
    match mode {
        Mode::TriangleStrip => (0..n.saturating_sub(2))
            .flat_map(|i| {
                let odd = i % 2;
                [raw[i], raw[i + 1 + odd], raw[i + 2 - odd]]
            })
            .collect(),
        Mode::TriangleFan => (0..n.saturating_sub(2))
            .flat_map(|i| [raw[i + 1], raw[i + 2], raw[0]])
            .collect(),
        _ => raw[..n - n % 3].to_vec(),
    }
}

/// Rescales weights to sum to 1 when they are off by more than rounding; all-zero weights bind to joint 0.
fn normalize_weights(w: [f32; 4]) -> [f32; 4] {
    let sum: f32 = w.iter().sum();
    if !(sum.is_finite() && sum > 0.0) {
        [1.0, 0.0, 0.0, 0.0]
    } else if (sum - 1.0).abs() > 1e-5 {
        w.map(|x| x / sum)
    } else {
        w
    }
}

/// Area-weighted smooth normals for the vertex ranges that came without them.
fn compute_normals(m: &mut Mesh, ranges: &[std::ops::Range<usize>]) {
    let missing = |v: usize| ranges.iter().any(|r| r.contains(&v));
    let sub = |a: Vec3, b: Vec3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| i as usize);
        let (e1, e2) = (sub(m.positions[b], m.positions[a]), sub(m.positions[c], m.positions[a]));
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for v in [a, b, c].into_iter().filter(|&v| missing(v)) {
            for (acc, x) in m.normals[v].iter_mut().zip(n) {
                *acc += x;
            }
        }
    }
    for r in ranges {
        for n in &mut m.normals[r.clone()] {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            *n = if len > 0.0 && len.is_finite() {
                n.map(|x| x / len)
            } else {
                [0.0, 0.0, 1.0]
            };
        }
    }
}
