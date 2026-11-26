//! glTF 2.0 importer: nodes, meshes, skins, morph targets, materials, textures and animations to a format [`Avatar`],
//! plus VRM 0.x and 1.0 extensions or the source package's `X_*` extensions (ADR 0041) when present.
//!
//! Input is untrusted. Every count is checked against [`Caps`] before allocating, the total input against
//! [`Limits::bytes`], and the result passes [`civbb_format::validate`]. Coordinates and UVs keep glTF conventions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use civbb_format::hierarchy::{Hierarchy, Node, NodeIndex};
use civbb_format::material::{Material, Texture};
use civbb_format::math::Transform;
use civbb_format::mesh::Mesh;
use civbb_format::{Avatar, BlobId, Caps};
use gltf::Document;
use gltf::json::validation::Checked;

mod animation;
mod material;
mod mesh;
mod source;
mod unity_clip;
mod unity_material;
mod unity_state;
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
    /// The avatar. Humanoid, face, dynamics, animator and expressions come only from VRM or `X_*` extensions.
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
    let gltf = parse(bytes).map_err(ImportError)?;
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
        families: Vec::new(),
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
    im.unity_materials()?;
    im.meshes()?;
    let mut clips = im.animations()?;
    let vrm = im.vrm(&mut clips)?;
    let source = im.source(clips.len())?;
    if vrm.is_some() && source.is_some() {
        return Err(ImportError("both VRM and X_* extensions present".into()));
    }
    let mut avatar = Avatar {
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
                state: v.state,
                meta: v.meta,
                constraints: v.constraints,
                ..Avatar::default()
            },
            None => Avatar::default(),
        }
    };
    if let Some(s) = source {
        avatar.humanoid = s.humanoid;
        avatar.animator = s.animator;
        avatar.clips.extend(s.clips);
        avatar.constraints = s.constraints;
        avatar.dynamics = s.dynamics;
        avatar.contacts = s.contacts;
        avatar.expressions = s.expressions;
        avatar.face = s.face;
        avatar.state = s.state;
    }
    let validate = |avatar: &Avatar| {
        civbb_format::validate(avatar, &limits.caps, |id| im.blobs.contains_key(&id)).map_err(|errors| {
            let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
            ImportError(format!("invalid result: {}", list.join("; ")))
        })
    };
    validate(&avatar)?;
    // Humanoids without Unity's muscle frames (glTF, VRM) get them derived from the rest pose (#53, ADR 0064).
    if let Some(h) = avatar.humanoid.as_ref().filter(|h| h.muscle_frames.is_empty()) {
        let frames = civbb_anim::frames::derive(&avatar.hierarchy, h);
        if !frames.is_empty() {
            avatar.humanoid.as_mut().expect("checked").muscle_frames = frames;
            validate(&avatar)?;
        }
    }
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
///
/// `KHR_mesh_quantization` is accepted (gltf-json refuses it as required): the accessor reader already converts
/// every component type. Required Draco or meshopt compression is refused (ADR 0076); when only used, both carry
/// uncompressed fallback data, which is read.
fn parse(bytes: &[u8]) -> Result<gltf::Gltf, String> {
    let unchecked = gltf::Gltf::from_slice_without_validation(bytes).map_err(|e| e.to_string())?;
    let mut json = unchecked.document.into_json();
    let compressed = ["KHR_draco_mesh_compression", "EXT_meshopt_compression"];
    if let Some(ext) = json
        .extensions_required
        .iter()
        .find(|e| compressed.contains(&e.as_str()))
    {
        return Err(format!(
            "{ext} is not supported: export the file without mesh compression"
        ));
    }
    json.extensions_required.retain(|e| e != "KHR_mesh_quantization");
    let positions = Checked::Valid(gltf::json::mesh::Semantic::Positions);
    let bad = json
        .meshes
        .iter()
        .flat_map(|m| &m.primitives)
        .filter_map(|p| p.attributes.get(&positions))
        .any(|i| i.value() >= json.accessors.len());
    if bad {
        let error = gltf::json::validation::Error::IndexOutOfBounds;
        return Err(
            gltf::Error::Validation(vec![(gltf::json::Path::new().field("POSITION accessor"), error)]).to_string(),
        );
    }
    Ok(gltf::Gltf {
        document: Document::from_json(json).map_err(|e| e.to_string())?,
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
    /// Each material's Unity shader family (`X_toon_material`); missing or `None` for the others.
    families: Vec<Option<unity_material::Family>>,
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
                // `extras.active: false` marks an inactive node (ADR 0041).
                let active = node.extras().as_ref().is_none_or(|raw| {
                    serde_json::from_str::<serde_json::Value>(raw.get())
                        .map_or(true, |v| v.get("active") != Some(&serde_json::Value::Bool(false)))
                });
                self.map[c] = Some(self.nodes.len() as NodeIndex);
                self.nodes.push(Node {
                    name,
                    parent,
                    rest: Transform {
                        translation,
                        rotation,
                        scale,
                    },
                    active,
                });
            }
        }
        Ok(self.nodes[0].name.clone())
    }
}
