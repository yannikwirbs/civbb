//! The native `CIVBB_*` extensions (ADRs 0086, 0101): document parts at the root, node flags on nodes, toon
//! materials on materials, each the format model's JSON with glTF indices and a `version`.
//!
//! A malformed extension, an unknown version or an index outside the glTF document fails the import; `validate`
//! then checks the rest.

use civbb_format::animator::{Animator, Clip, Motion, Target};
use civbb_format::constraint::{Constraint, ConstraintKind, WorldUp};
use civbb_format::dynamics::{Contacts, Dynamics};
use civbb_format::face::EyeDriver;
use civbb_format::hierarchy::NodeIndex;
use civbb_format::material::{ColorSpace, MaterialModel, TextureSlot, Toon};
use civbb_format::mesh::FirstPerson;
use civbb_format::state::StateTable;
use civbb_format::{Avatar, Document, Role};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Importer;

/// Newest version of every `CIVBB_*` extension this importer reads (ADR 0087; migrations come with version 2).
const VERSION: u64 = 1;

/// Root-level extensions.
pub(super) const NAMES: [&str; 6] = [
    "CIVBB_avatar",
    "CIVBB_state",
    "CIVBB_behaviour",
    "CIVBB_constraints",
    "CIVBB_dynamics",
    "CIVBB_contacts",
];

#[derive(Deserialize)]
struct Behaviour {
    #[serde(default)]
    animator: Option<Animator>,
    #[serde(default)]
    clips: Vec<Clip>,
}

#[derive(Deserialize)]
struct Constraints {
    constraints: Vec<Constraint>,
}

#[derive(Deserialize)]
struct NodeFlags {
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    first_person: FirstPerson,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct ToonMaterial {
    #[serde(default)]
    order: i32,
    #[serde(flatten)]
    toon: Toon,
}

/// Parses one extension object after checking and removing its `version`.
fn parse<T: DeserializeOwned>(name: &str, v: &Value) -> Result<T, String> {
    let mut v = v.clone();
    let o = v.as_object_mut().ok_or_else(|| format!("{name}: not an object"))?;
    match o.remove("version").map(|n| n.as_u64()) {
        Some(Some(1..=VERSION)) => {}
        Some(_) => return Err(format!("{name}: this importer reads version 1 to {VERSION}")),
        None => return Err(format!("{name}: no version")),
    }
    T::deserialize(v).map_err(|e| format!("{name}: {e}"))
}

impl Importer<'_> {
    /// Reads the root-level `CIVBB_*` extensions into a document's parts (role, state, behaviour, constraints,
    /// dynamics, contacts) if any is present. `clip_base` is the number of clips imported from glTF animations.
    pub(super) fn native(&mut self, clip_base: usize) -> Result<Option<Document>, String> {
        let doc = self.doc;
        let ext = |name| doc.extension_value(name);
        if NAMES.iter().all(|n| ext(n).is_none()) {
            return Ok(None);
        }
        let mut d = Document::default();
        if let Some(v) = ext("CIVBB_avatar") {
            d.role = Role::Avatar(parse::<Avatar>("CIVBB_avatar", v)?);
        }
        if let Some(v) = ext("CIVBB_state") {
            d.state = parse::<StateTable>("CIVBB_state", v)?;
        }
        if let Some(v) = ext("CIVBB_behaviour") {
            let b: Behaviour = parse("CIVBB_behaviour", v)?;
            d.animator = b.animator;
            d.clips = b.clips;
        }
        if let Some(v) = ext("CIVBB_constraints") {
            d.constraints = parse::<Constraints>("CIVBB_constraints", v)?.constraints;
        }
        if let Some(v) = ext("CIVBB_dynamics") {
            d.dynamics = parse::<Dynamics>("CIVBB_dynamics", v)?;
        }
        if let Some(v) = ext("CIVBB_contacts") {
            d.contacts = parse::<Contacts>("CIVBB_contacts", v)?;
        }
        self.native_indices(&mut d, clip_base)?;
        Ok(Some(d))
    }

    /// Applies `CIVBB_node` (activity, first-person visibility) and `CIVBB_materials_toon` to the imported nodes,
    /// meshes and materials.
    pub(super) fn native_nodes_and_materials(&mut self) -> Result<(), String> {
        let doc = self.doc;
        for node in doc.nodes() {
            let Some(v) = node.extension_value("CIVBB_node") else {
                continue;
            };
            let flags: NodeFlags = parse("CIVBB_node", v)?;
            let Some(at) = self.map[node.index()] else {
                continue;
            };
            self.nodes[at as usize].active = flags.active;
            if let Some(&m) = self.node_meshes.get(&at) {
                self.meshes[m].first_person = flags.first_person;
            } else if flags.first_person != FirstPerson::default() {
                return Err(format!("CIVBB_node: node {} has no mesh", node.index()));
            }
        }
        for (i, m) in doc.materials().enumerate() {
            let Some(v) = m.extension_value("CIVBB_materials_toon") else {
                continue;
            };
            let mut t: ToonMaterial = parse("CIVBB_materials_toon", v)?;
            for (slot, space) in toon_slots(&mut t.toon) {
                let n = slot.texture;
                let texture = (doc.textures().nth(n as usize))
                    .ok_or_else(|| format!("CIVBB_materials_toon: texture {n} does not exist"))?;
                *slot = self
                    .slot(&texture, slot.uv_set.into(), Some(slot.scale_offset), space)
                    .ok_or_else(|| format!("CIVBB_materials_toon: texture {n} unreadable"))?;
            }
            self.materials[i].model = MaterialModel::Toon(t.toon);
            self.materials[i].order = t.order;
        }
        Ok(())
    }

    /// Maps the parts' glTF indices: nodes to hierarchy nodes, mesh references (the node carrying the mesh) to
    /// format meshes, clip indices past the glTF animations', menu icons to format textures.
    fn native_indices(&mut self, d: &mut Document, clip_base: usize) -> Result<(), String> {
        let Document {
            role: Role::Avatar(avatar),
            animator,
            clips,
            constraints,
            dynamics,
            contacts,
            ..
        } = d;
        let mut nodes: Vec<(&str, &mut NodeIndex)> = Vec::new();
        let mut meshes: Vec<(&str, &mut u32)> = Vec::new();
        let a = "CIVBB_avatar";
        if let Some(h) = &mut avatar.humanoid {
            nodes.extend(h.bones.iter_mut().map(|(_, n)| (a, n)));
        }
        if let Some(f) = &mut avatar.face {
            match f.eyes.as_mut().map(|e| &mut e.driver) {
                Some(EyeDriver::Bones { left, right }) => nodes.extend(left.iter_mut().chain(right).map(|n| (a, n))),
                Some(EyeDriver::Shapes { up, down, left, right }) => {
                    meshes.extend([up, down, left, right].into_iter().flatten().map(|b| (a, &mut b.mesh)));
                }
                None => {}
            }
            let binds = f.visemes.iter_mut().flat_map(|(_, b)| b).chain(&mut f.blink);
            meshes.extend(binds.map(|b| (a, &mut b.mesh)));
        }
        let b = "CIVBB_behaviour";
        let masks = animator.iter_mut().flat_map(|an| &mut an.layers);
        for m in masks.filter_map(|l| l.mask.as_mut()?.transforms.as_mut()) {
            nodes.extend(m.iter_mut().map(|n| (b, n)));
        }
        for c in clips.iter_mut().flat_map(|c| &mut c.curves) {
            match &mut c.target {
                Target::Node { node, .. } => nodes.push((b, node)),
                Target::Mesh { mesh, .. } => meshes.push((b, mesh)),
                _ => {}
            }
        }
        let c = "CIVBB_constraints";
        for k in constraints.iter_mut() {
            nodes.push((c, &mut k.target));
            nodes.extend(k.sources.iter_mut().map(|s| (c, &mut s.node)));
            match &mut k.kind {
                ConstraintKind::Aim {
                    world_up: WorldUp::ObjectUp(n) | WorldUp::ObjectRotationUp(n, _),
                    ..
                }
                | ConstraintKind::LookAt {
                    world_up_node: Some(n), ..
                } => nodes.push((c, n)),
                _ => {}
            }
        }
        let y = "CIVBB_dynamics";
        for ch in &mut dynamics.chains {
            nodes.push((y, &mut ch.root));
            nodes.extend(ch.ignore.iter_mut().map(|n| (y, n)));
        }
        nodes.extend(dynamics.colliders.iter_mut().map(|c| (y, &mut c.node)));
        let volumes = contacts.senders.iter_mut().map(|s| &mut s.volume);
        for v in volumes.chain(contacts.receivers.iter_mut().map(|r| &mut r.volume)) {
            nodes.push(("CIVBB_contacts", &mut v.node));
            nodes.extend(v.component.as_mut().map(|n| ("CIVBB_contacts", n)));
        }
        for (ext, m) in meshes {
            let at = self.x_node(ext, *m)?;
            *m = self
                .node_meshes
                .get(&at)
                .map(|&i| i as u32)
                .ok_or_else(|| format!("{ext}: node {m} has no mesh"))?;
        }
        for (ext, n) in nodes {
            *n = self.x_node(ext, *n)?;
        }
        let base = u32::try_from(clip_base).map_err(|_| "CIVBB_behaviour: too many clips")?;
        let states = animator
            .iter_mut()
            .flat_map(|an| &mut an.layers)
            .flat_map(|l| &mut l.state_machine.states);
        for m in states.filter_map(|s| s.motion.as_mut()) {
            shift(m, base)?;
        }
        let icons = avatar.expressions.iter_mut().flat_map(|e| icons(&mut e.menu));
        for icon in icons {
            let texture = self.doc.textures().nth(*icon as usize);
            let texture = texture.ok_or_else(|| format!("CIVBB_avatar: icon texture {icon} does not exist"))?;
            *icon = (self.slot(&texture, 0, None, ColorSpace::Srgb))
                .ok_or_else(|| format!("CIVBB_avatar: icon texture {icon} unreadable"))?
                .texture;
        }
        Ok(())
    }
}

/// Every menu icon, submenus included.
fn icons(menu: &mut civbb_format::expressions::Menu) -> Vec<&mut u32> {
    let mut out = Vec::new();
    for c in &mut menu.controls {
        if let civbb_format::expressions::ControlKind::SubMenu(m) = &mut c.kind {
            out.extend(icons(m));
        }
        out.extend(c.icon.as_mut());
    }
    out
}

/// Moves a motion's clip indices past the clips imported from glTF animations.
fn shift(m: &mut Motion, base: u32) -> Result<(), String> {
    match m {
        Motion::Clip(i) => *i = i.checked_add(base).ok_or("CIVBB_behaviour: clip index overflows")?,
        Motion::BlendTree(t) => {
            for c in &mut t.children {
                shift(&mut c.motion, base)?;
            }
        }
    }
    Ok(())
}

/// A toon material's texture slots and the colour space each samples.
fn toon_slots(t: &mut Toon) -> Vec<(&mut TextureSlot, ColorSpace)> {
    let (s, l) = (ColorSpace::Srgb, ColorSpace::Linear);
    let mut out: Vec<(&mut TextureSlot, ColorSpace)> = Vec::new();
    out.extend(t.base_color_tex.as_mut().map(|x| (x, s)));
    out.extend(t.shade_tex.as_mut().map(|x| (x, s)));
    out.extend(t.normal.as_mut().map(|n| (&mut n.tex, l)));
    out.extend(t.emission.as_mut().and_then(|e| e.tex.as_mut()).map(|x| (x, s)));
    for m in &mut t.matcaps {
        out.extend(m.tex.as_mut().map(|x| (x, s)));
        out.extend(m.mask_tex.as_mut().map(|x| (x, l)));
    }
    if let Some(o) = &mut t.outline {
        out.extend(o.width_tex.as_mut().map(|x| (x, l)));
        out.extend(o.tex.as_mut().map(|x| (x, s)));
    }
    out
}
