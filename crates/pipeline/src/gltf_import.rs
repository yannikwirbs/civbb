//! glTF 2.0 importer: nodes, meshes, skins, morph targets, materials, textures and animations to a format [`Avatar`],
//! plus VRM 0.x and 1.0 extensions when present.
//!
//! Input is untrusted. Every count is checked against [`Caps`] before allocating, the total input against
//! [`Limits::bytes`], and the result passes [`civbb_format::validate`]. Coordinates and UVs keep glTF conventions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::hierarchy::{Hierarchy, Node, NodeIndex};
use civbb_format::material::{
    AlphaMode, ColorSpace, ImageFormat, Material, MaterialModel, Pbr, Texture, TextureSlot, Unlit,
};
use civbb_format::math::{Mat4, Transform, Vec3};
use civbb_format::mesh::{BlendShape, Mesh, ShapeDelta, Skin, Submesh};
use civbb_format::{Avatar, BlobId, Caps};
use gltf::Document;
use gltf::animation::{Interpolation, Property as GltfProperty};
use gltf::json::validation::Checked;
use gltf::mesh::{Mode, Semantic};

use crate::accessor::{arrays, floats, uints};

mod vrm;

/// Limits for one import.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Total bytes read: the file, external buffers and images, and decoded data URIs.
    pub bytes: usize,
    /// Count caps, the same ones validation applies.
    pub caps: Caps,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 512 << 20,
            caps: Caps::default(),
        }
    }
}

/// An imported avatar and the blobs its textures reference.
#[derive(Debug)]
pub struct Imported {
    /// The avatar. Humanoid, face, dynamics, animator and expressions come only from VRM extensions.
    pub avatar: Avatar,
    /// Encoded images by id.
    pub blobs: BTreeMap<BlobId, Vec<u8>>,
    /// Things skipped or approximated.
    pub warnings: Vec<String>,
}

/// Why an import failed.
#[derive(Debug, thiserror::Error)]
#[error("gltf import: {0}")]
pub struct ImportError(pub String);

impl From<String> for ImportError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Imports a `.gltf` or `.glb`. `resolve` loads a relative URI, or returns `None` to refuse it.
pub fn import(
    bytes: &[u8],
    limits: &Limits,
    resolve: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<Imported, ImportError> {
    if bytes.len() > limits.bytes {
        return Err(ImportError(format!(
            "{} bytes exceeds limit {}",
            bytes.len(),
            limits.bytes
        )));
    }
    // gltf subtracts the 12-byte header from the declared GLB length unchecked (fuzz crash d184ff83).
    let glb_length = bytes
        .get(8..12)
        .map(|l| u32::from_le_bytes(l.try_into().expect("4 bytes")));
    if bytes.starts_with(b"glTF") && glb_length.is_some_and(|l| l < 12) {
        return Err(ImportError("GLB length shorter than its header".into()));
    }
    let gltf = parse(bytes).map_err(|e| ImportError(e.to_string()))?;
    let mut im = Importer {
        doc: &gltf.document,
        resolve: &resolve,
        caps: limits.caps,
        budget: limits.bytes - bytes.len(),
        buffers: Vec::new(),
        map: Vec::new(),
        nodes: Vec::new(),
        node_meshes: HashMap::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        default_material: None,
        textures: Vec::new(),
        texture_map: HashMap::new(),
        blobs: BTreeMap::new(),
        warnings: Vec::new(),
        vertices_left: limits.caps.vertices,
        indices_left: limits.caps.indices,
        deltas_left: limits.caps.shape_deltas,
        shapes_left: limits.caps.blend_shapes,
    };
    im.buffers(gltf.blob.as_deref())?;
    let name = im.hierarchy()?;
    for m in im.doc.materials() {
        let material = im.material(&m);
        im.materials.push(material);
    }
    im.meshes()?;
    let mut clips = im.animations()?;
    let vrm = im.vrm(&mut clips)?;
    let avatar = Avatar {
        name: vrm.as_ref().and_then(|v| v.name.clone()).unwrap_or(name),
        hierarchy: Hierarchy {
            nodes: std::mem::take(&mut im.nodes),
        },
        meshes: std::mem::take(&mut im.meshes),
        materials: std::mem::take(&mut im.materials),
        textures: std::mem::take(&mut im.textures),
        clips,
        ..match vrm {
            Some(v) => Avatar {
                humanoid: Some(v.humanoid),
                face: v.face,
                dynamics: v.dynamics,
                animator: v.animator,
                expressions: v.expressions,
                meta: v.meta,
                constraints: v.constraints,
                ..Avatar::default()
            },
            None => Avatar::default(),
        }
    };
    civbb_format::validate(&avatar, &limits.caps, |id| im.blobs.contains_key(&id)).map_err(|errors| {
        let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
        ImportError(format!("invalid result: {}", list.join("; ")))
    })?;
    let mut seen = HashSet::new();
    im.warnings.retain(|w| seen.insert(w.clone()));
    Ok(Imported {
        avatar,
        blobs: im.blobs,
        warnings: im.warnings,
    })
}

/// Parses and validates the document. gltf-json's validation indexes each primitive's POSITION accessor before
/// checking that index (fuzz crash 703dba39), so that check runs first.
fn parse(bytes: &[u8]) -> Result<gltf::Gltf, gltf::Error> {
    let unchecked = gltf::Gltf::from_slice_without_validation(bytes)?;
    let json = unchecked.document.into_json();
    let positions = Checked::Valid(gltf::json::mesh::Semantic::Positions);
    let bad = json
        .meshes
        .iter()
        .flat_map(|m| &m.primitives)
        .filter_map(|p| p.attributes.get(&positions))
        .any(|i| i.value() >= json.accessors.len());
    if bad {
        let error = gltf::json::validation::Error::IndexOutOfBounds;
        return Err(gltf::Error::Validation(vec![(
            gltf::json::Path::new().field("POSITION accessor"),
            error,
        )]));
    }
    Ok(gltf::Gltf {
        document: Document::from_json(json)?,
        blob: unchecked.blob,
    })
}

/// A resolver reading relative URIs from `dir`. Absolute paths, schemes and `..` are refused, as are files
/// larger than `max_bytes`.
pub fn dir_resolver(dir: &Path, max_bytes: usize) -> impl Fn(&str) -> Option<Vec<u8>> {
    let dir = dir.to_path_buf();
    move |uri| {
        let path = percent_decode(uri)?;
        let unsafe_path =
            path.contains(':') || path.starts_with(['/', '\\']) || path.split(['/', '\\']).any(|c| c == "..");
        if unsafe_path {
            return None;
        }
        let file: PathBuf = dir.join(path);
        if std::fs::metadata(&file).ok()?.len() > max_bytes as u64 {
            return None;
        }
        std::fs::read(file).ok()
    }
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok().filter(|s| !s.contains('\0'))
}

pub(crate) struct Importer<'a> {
    pub(crate) doc: &'a Document,
    resolve: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    caps: Caps,
    /// Bytes still allowed from external files and data URIs.
    budget: usize,
    buffers: Vec<Vec<u8>>,
    /// glTF node index to format node index; `None` for nodes outside the scene.
    map: Vec<Option<NodeIndex>>,
    nodes: Vec<Node>,
    /// Format node index to the format mesh hanging from it.
    node_meshes: HashMap<NodeIndex, usize>,
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
    default_material: Option<u32>,
    textures: Vec<Texture>,
    texture_map: HashMap<(usize, bool), Option<u32>>,
    blobs: BTreeMap<BlobId, Vec<u8>>,
    warnings: Vec<String>,
    vertices_left: usize,
    indices_left: usize,
    deltas_left: usize,
    shapes_left: usize,
}

impl Importer<'_> {
    fn take_budget(&mut self, n: usize, what: &str) -> Result<(), String> {
        self.budget = self
            .budget
            .checked_sub(n)
            .ok_or_else(|| format!("{what}: input byte limit exceeded"))?;
        Ok(())
    }

    /// Loads a URI: base64 data URIs inline, anything else through `resolve`.
    fn load_uri(&mut self, uri: &str) -> Result<Vec<u8>, String> {
        let bytes = if let Some(rest) = uri.strip_prefix("data:") {
            let (_, data) = rest
                .split_once(";base64,")
                .ok_or_else(|| "only base64 data URIs are supported".to_owned())?;
            self.take_budget(data.len(), "data URI")?;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| format!("data URI: {e}"))?
        } else {
            let bytes = (self.resolve)(uri).ok_or_else(|| format!("cannot load `{uri}`"))?;
            self.take_budget(bytes.len(), uri)?;
            bytes
        };
        Ok(bytes)
    }

    fn buffers(&mut self, blob: Option<&[u8]>) -> Result<(), String> {
        for b in self.doc.buffers() {
            let data = match b.source() {
                gltf::buffer::Source::Bin => blob.ok_or("GLB binary chunk missing")?.to_vec(),
                gltf::buffer::Source::Uri(uri) => self.load_uri(uri)?,
            };
            if data.len() < b.length() {
                return Err(format!("buffer {}: shorter than byteLength", b.index()));
            }
            self.buffers.push(data);
        }
        Ok(())
    }

    /// Orders the scene's nodes parents first, keeping source order where it already is. Returns the root's name.
    fn hierarchy(&mut self) -> Result<String, String> {
        let doc = self.doc;
        let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;
        let all: Vec<gltf::Node> = doc.nodes().collect();
        let n = all.len();
        if n > self.caps.nodes {
            return Err(format!("{n} nodes exceeds cap {}", self.caps.nodes));
        }
        let mut parent = vec![None; n];
        for node in &all {
            for c in node.children() {
                if parent[c.index()].replace(node.index()).is_some() {
                    return Err(format!("node {} has two parents", c.index()));
                }
            }
        }
        // With unique parents and roots that have none, everything reachable from the roots is a forest.
        let roots: Vec<usize> = scene.nodes().map(|r| r.index()).collect();
        if let Some(r) = roots.iter().find(|&&r| parent[r].is_some()) {
            return Err(format!("scene root {r} has a parent"));
        }
        let mut reachable = vec![false; n];
        let mut stack = roots.clone();
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut reachable[i], true) {
                return Err(format!("node {i} listed twice in the scene"));
            }
            stack.extend(all[i].children().map(|c| c.index()));
        }

        let synthetic_root = roots.len() != 1;
        if synthetic_root {
            self.nodes.push(Node {
                name: scene.name().unwrap_or("Root").into(),
                parent: None,
                rest: Transform::IDENTITY,
                active: true,
            });
        }
        self.map = vec![None; n];
        let mut taken: HashSet<(Option<NodeIndex>, String)> = HashSet::new();
        for i in (0..n).filter(|&i| reachable[i]) {
            let mut chain = Vec::new();
            let mut cur = Some(i);
            while let Some(c) = cur.filter(|&c| self.map[c].is_none()) {
                chain.push(c);
                cur = parent[c];
            }
            for c in chain.into_iter().rev() {
                let node = &all[c];
                let parent = match parent[c] {
                    Some(p) => self.map[p],
                    None => synthetic_root.then_some(0),
                };
                let base = node.name().map_or_else(|| format!("node{c}"), str::to_owned);
                let mut name = base.clone();
                let mut k = 0;
                while !taken.insert((parent, name.clone())) {
                    k += 1;
                    name = format!("{base} ({k})");
                }
                let (translation, rotation, scale) = node.transform().decomposed();
                self.map[c] = Some(self.nodes.len() as NodeIndex);
                self.nodes.push(Node {
                    name,
                    parent,
                    rest: Transform {
                        translation,
                        rotation,
                        scale,
                    },
                    active: true,
                });
            }
        }
        Ok(self.nodes[0].name.clone())
    }

    /// The Unity-style binding path of a node: names below the root joined by `/`.
    fn path(&self, node: NodeIndex) -> Result<String, String> {
        let mut parts = Vec::new();
        let mut len = 0;
        let mut cur = node as usize;
        while let Some(p) = self.nodes[cur].parent {
            len += self.nodes[cur].name.len() + 1;
            if len > self.caps.string_len + 1 {
                return Err(format!(
                    "path of node {node} longer than {} bytes",
                    self.caps.string_len
                ));
            }
            parts.push(self.nodes[cur].name.as_str());
            cur = p as usize;
        }
        parts.reverse();
        Ok(parts.join("/"))
    }

    fn material(&mut self, m: &gltf::Material) -> Material {
        let pbr = m.pbr_metallic_roughness();
        let (alpha, render_queue) = match m.alpha_mode() {
            gltf::material::AlphaMode::Opaque => (AlphaMode::Opaque, 2000),
            gltf::material::AlphaMode::Mask => (
                AlphaMode::Mask {
                    cutoff: m.alpha_cutoff().unwrap_or(0.5),
                },
                2450,
            ),
            gltf::material::AlphaMode::Blend => (AlphaMode::Blend, 3000),
        };
        let base_tex = pbr
            .base_color_texture()
            .and_then(|t| self.info_slot(&t, ColorSpace::Srgb));
        let model = if m.unlit() {
            MaterialModel::Unlit(Unlit {
                color: pbr.base_color_factor(),
                color_tex: base_tex,
            })
        } else {
            let normal = m.normal_texture();
            let occlusion = m.occlusion_texture();
            MaterialModel::Pbr(Pbr {
                base_color: pbr.base_color_factor(),
                base_color_tex: base_tex,
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                metallic_roughness_tex: pbr
                    .metallic_roughness_texture()
                    .and_then(|t| self.info_slot(&t, ColorSpace::Linear)),
                normal_tex: normal
                    .as_ref()
                    .and_then(|t| self.slot(&t.texture(), t.tex_coord(), None, ColorSpace::Linear)),
                normal_scale: normal.as_ref().map_or(1.0, |t| t.scale()),
                occlusion_tex: occlusion
                    .as_ref()
                    .and_then(|t| self.slot(&t.texture(), t.tex_coord(), None, ColorSpace::Linear)),
                emissive: m.emissive_factor(),
                emissive_tex: m.emissive_texture().and_then(|t| self.info_slot(&t, ColorSpace::Srgb)),
            })
        };
        Material {
            name: m.name().unwrap_or("material").into(),
            model,
            alpha,
            double_sided: m.double_sided(),
            render_queue,
        }
    }

    fn info_slot(&mut self, info: &gltf::texture::Info, space: ColorSpace) -> Option<TextureSlot> {
        let transform = info.texture_transform();
        let tex_coord = transform
            .as_ref()
            .and_then(|t| t.tex_coord())
            .unwrap_or(info.tex_coord());
        if transform.as_ref().is_some_and(|t| t.rotation() != 0.0) {
            self.warnings
                .push("KHR_texture_transform rotation is not supported; ignored".into());
        }
        let scale_offset = transform.map(|t| {
            let (s, o) = (t.scale(), t.offset());
            [s[0], s[1], o[0], o[1]]
        });
        self.slot(&info.texture(), tex_coord, scale_offset, space)
    }

    fn slot(
        &mut self,
        texture: &gltf::Texture,
        tex_coord: u32,
        scale_offset: Option<[f32; 4]>,
        space: ColorSpace,
    ) -> Option<TextureSlot> {
        if tex_coord > 1 {
            self.warnings.push(format!(
                "texture {}: TEXCOORD_{tex_coord} unsupported; slot dropped",
                texture.index()
            ));
            return None;
        }
        let image = texture.source().index();
        let key = (image, space == ColorSpace::Srgb);
        let index = match self.texture_map.get(&key) {
            Some(i) => *i,
            None => {
                let name = texture.name().unwrap_or("texture");
                let i = match self.image(image, name, space) {
                    Ok(i) => Some(i),
                    Err(e) => {
                        self.warnings.push(format!("image {image}: {e}; slot dropped"));
                        None
                    }
                };
                self.texture_map.insert(key, i);
                i
            }
        }?;
        Some(TextureSlot {
            texture: index,
            uv_set: tex_coord as u8,
            scale_offset: scale_offset.unwrap_or([1.0, 1.0, 0.0, 0.0]),
        })
    }

    /// Stores an image's encoded bytes as a blob. Read from the JSON: the wrapper panics on malformed images.
    fn image(&mut self, image: usize, name: &str, space: ColorSpace) -> Result<u32, String> {
        let doc = self.doc;
        let json = &doc.as_json().images[image];
        let bytes = match (&json.buffer_view, &json.uri) {
            (Some(view), _) => {
                let view = doc.views().nth(view.value()).expect("validated index");
                let buffer = &self.buffers[view.buffer().index()];
                let end = view.offset().checked_add(view.length());
                end.and_then(|end| buffer.get(view.offset()..end))
                    .ok_or("bufferView out of bounds")?
                    .to_vec()
            }
            (None, Some(uri)) => self.load_uri(uri)?,
            (None, None) => return Err("no source".into()),
        };
        let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ImageFormat::Png
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            ImageFormat::Jpeg
        } else {
            return Err("not PNG or JPEG".into());
        };
        let blob = BlobId::of(&bytes);
        self.blobs.insert(blob, bytes);
        self.textures.push(Texture {
            name: json.name.clone().unwrap_or_else(|| name.into()),
            format,
            color_space: space,
            blob,
        });
        Ok(self.textures.len() as u32 - 1)
    }

    fn primitive_material(&mut self, p: &gltf::Primitive) -> u32 {
        if let Some(i) = p.material().index() {
            return i as u32;
        }
        *self.default_material.get_or_insert_with(|| {
            self.materials.push(Material {
                name: "default".into(),
                model: MaterialModel::Pbr(Pbr {
                    base_color: [1.0; 4],
                    base_color_tex: None,
                    metallic: 1.0,
                    roughness: 1.0,
                    metallic_roughness_tex: None,
                    normal_tex: None,
                    normal_scale: 1.0,
                    occlusion_tex: None,
                    emissive: [0.0; 3],
                    emissive_tex: None,
                }),
                alpha: AlphaMode::Opaque,
                double_sided: false,
                render_queue: 2000,
            });
            self.materials.len() as u32 - 1
        })
    }

    fn meshes(&mut self) -> Result<(), String> {
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

    fn animations(&mut self) -> Result<Vec<Clip>, String> {
        let doc = self.doc;
        let (mut curves_left, mut keys_left) = (self.caps.curves, self.caps.keys);
        let mut paths: HashMap<NodeIndex, String> = HashMap::new();
        let mut clips = Vec::new();
        for (ai, anim) in doc.animations().enumerate() {
            let mut clip = Clip {
                name: anim.name().map_or_else(|| format!("animation{ai}"), str::to_owned),
                ..Clip::default()
            };
            for ch in anim.channels() {
                let err = |e: String| format!("animation {ai} channel {}: {e}", ch.index());
                let Some(at) = self.map[ch.target().node().index()] else {
                    self.warnings.push(format!(
                        "animation {ai}: channel {} targets a node outside the scene",
                        ch.index()
                    ));
                    continue;
                };
                let properties: Vec<Property> = match ch.target().property() {
                    GltfProperty::Translation => (0..3).map(Property::Position).collect(),
                    GltfProperty::Rotation => (0..4).map(Property::Rotation).collect(),
                    GltfProperty::Scale => (0..3).map(Property::Scale).collect(),
                    GltfProperty::MorphTargetWeights => match self.node_meshes.get(&at) {
                        Some(&mi) => self.meshes[mi]
                            .blend_shapes
                            .iter()
                            .map(|s| Property::BlendShape(s.name.clone()))
                            .collect(),
                        None => return Err(err("morph weights on a node without a mesh".into())),
                    },
                };
                let s = ch.sampler();
                let times = floats(&s.input(), &self.buffers, keys_left).map_err(err)?;
                if s.input().dimensions().multiplicity() != 1 {
                    return Err(err("input is not scalar".into()));
                }
                if !times.iter().all(|t| t.is_finite()) || !times.windows(2).all(|w| w[0] < w[1]) {
                    return Err(err("key times not finite and strictly increasing".into()));
                }
                let cubic = s.interpolation() == Interpolation::CubicSpline;
                let stride = properties.len() * if cubic { 3 } else { 1 };
                let max = times.len().saturating_mul(stride);
                let values = floats(&s.output(), &self.buffers, max).map_err(err)?;
                if values.len() != max {
                    return Err(err("output count does not match input".into()));
                }
                let used = times.len() * properties.len();
                keys_left = keys_left
                    .checked_sub(used)
                    .ok_or_else(|| err("key cap exceeded".into()))?;
                curves_left = curves_left
                    .checked_sub(properties.len())
                    .ok_or_else(|| err("curve cap exceeded".into()))?;
                let path = match paths.get(&at) {
                    Some(p) => p.clone(),
                    None => {
                        let p = self.path(at).map_err(err)?;
                        paths.insert(at, p.clone());
                        p
                    }
                };
                // glTF weights are 0 to 1; blendshape curves use Unity's 0 to 100.
                let scale = if matches!(ch.target().property(), GltfProperty::MorphTargetWeights) {
                    100.0
                } else {
                    1.0
                };
                for (c, property) in properties.into_iter().enumerate() {
                    let keys = keys(&times, &values, c, stride, s.interpolation(), scale);
                    clip.curves.push(Curve {
                        binding: Binding {
                            path: path.clone(),
                            property,
                        },
                        keys,
                    });
                }
                clip.length = clip.length.max(times.last().copied().unwrap_or(0.0));
            }
            if !clip.curves.is_empty() {
                clips.push(clip);
            }
        }
        if clips.len() > self.caps.clips {
            return Err(format!("{} clips exceeds cap {}", clips.len(), self.caps.clips));
        }
        Ok(clips)
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

/// Hermite keys for component `c` of a sampler. Linear keys get the segment slopes, step keys infinite tangents.
fn keys(times: &[f32], values: &[f32], c: usize, stride: usize, interp: Interpolation, scale: f32) -> Vec<Key> {
    let cubic = interp == Interpolation::CubicSpline;
    let comps = if cubic { stride / 3 } else { stride };
    // Cubic spline outputs are (in tangent, value, out tangent) triples per key.
    let at = |k: usize, part: usize| values[k * stride + part * comps + c] * scale;
    let value = |k: usize| at(k, if cubic { 1 } else { 0 });
    let slope = |a: usize, b: usize| (value(b) - value(a)) / (times[b] - times[a]);
    (0..times.len())
        .map(|k| {
            let (in_tangent, out_tangent) = match interp {
                Interpolation::Step => (f32::INFINITY, f32::INFINITY),
                Interpolation::CubicSpline => (at(k, 0), at(k, 2)),
                Interpolation::Linear => (
                    if k > 0 { slope(k - 1, k) } else { 0.0 },
                    if k + 1 < times.len() { slope(k, k + 1) } else { 0.0 },
                ),
            };
            Key {
                time: times[k],
                value: value(k),
                in_tangent,
                out_tangent,
            }
        })
        .collect()
}
