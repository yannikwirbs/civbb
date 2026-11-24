//! Structural validation and hard caps for untrusted avatars. After [`validate`] passes, every index is in range.
//!
//! Caps here are safety limits against hostile input, not performance ratings; the cost profile handles cost.

use std::collections::HashSet;

use crate::BlobId;
use crate::avatar::Avatar;
use crate::constraint::{Constraint, ConstraintKind, WorldUp};
use crate::dynamics::{ContactShape, ContactVolume, Contacts, Dynamics, Graded, Limit, Shape};
use crate::expressions::{ControlKind, Expressions, Menu};
use crate::face::{EyeDriver, Face, ShapeBind};
use crate::hierarchy::{Hierarchy, Humanoid};
use crate::material::{MaterialModel, TextureSlot};
use crate::math::Transform;
use crate::mesh::{Mesh, Skin};
use crate::meta::Meta;

mod animator;

/// Hard limits for one avatar.
#[allow(missing_docs)] // Each field caps the count its name says.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub nodes: usize,
    pub meshes: usize,
    pub vertices: usize,
    pub indices: usize,
    pub blend_shapes: usize,
    pub shape_deltas: usize,
    pub materials: usize,
    pub textures: usize,
    pub parameters: usize,
    pub layers: usize,
    pub states: usize,
    pub transitions: usize,
    pub clips: usize,
    pub curves: usize,
    pub keys: usize,
    pub blend_tree_depth: usize,
    pub menu_depth: usize,
    pub menu_controls: usize,
    pub synced_bits: u32,
    pub chains: usize,
    pub colliders: usize,
    pub contacts: usize,
    pub constraints: usize,
    pub graded_points: usize,
    pub string_len: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            nodes: 8192,
            meshes: 512,
            vertices: 4_000_000,
            indices: 12_000_000,
            blend_shapes: 16_384,
            shape_deltas: 40_000_000,
            materials: 512,
            textures: 512,
            parameters: 2048,
            layers: 512,
            states: 16_384,
            transitions: 65_536,
            clips: 16_384,
            curves: 500_000,
            keys: 10_000_000,
            blend_tree_depth: 16,
            menu_depth: 16,
            menu_controls: 4096,
            synced_bits: 1024,
            chains: 1024,
            colliders: 512,
            contacts: 1024,
            constraints: 4096,
            graded_points: 64,
            string_len: 1024,
        }
    }
}

/// One validation failure, with a path to the offending item.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{path}: {problem}")]
pub struct ValidationError {
    /// Location, e.g. `meshes[3].submeshes[0]`.
    pub path: String,
    /// What is wrong.
    pub problem: String,
}

/// Stop collecting after this many errors; one bad array can otherwise produce millions.
const MAX_ERRORS: usize = 100;
/// Authors plus references in [`crate::meta::Meta`].
const META_LISTS: usize = 256;
/// Levels of detail below the full mesh.
const MAX_LODS: usize = 8;

/// Checks every cross reference, stream length, ordering and cap. `has_blob` reports whether a texture's blob exists.
pub fn validate(a: &Avatar, caps: &Caps, has_blob: impl Fn(BlobId) -> bool) -> Result<(), Vec<ValidationError>> {
    let mut v = V {
        errors: Vec::new(),
        caps: *caps,
    };
    v.avatar(a, &has_blob);
    if v.errors.is_empty() { Ok(()) } else { Err(v.errors) }
}

struct V {
    errors: Vec<ValidationError>,
    caps: Caps,
}

/// Counts summed over every mesh, capped once all are seen.
#[derive(Default)]
struct Totals {
    vertices: usize,
    indices: usize,
    shapes: usize,
    deltas: usize,
}

impl V {
    fn fail(&mut self, path: impl FnOnce() -> String, problem: impl Into<String>) {
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(ValidationError {
                path: path(),
                problem: problem.into(),
            });
        }
    }

    fn ensure(&mut self, ok: bool, path: impl FnOnce() -> String, problem: impl Into<String>) {
        if !ok {
            self.fail(path, problem);
        }
    }

    fn index(&mut self, i: usize, len: usize, path: impl FnOnce() -> String, what: &str) {
        self.ensure(i < len, path, format!("{what} index {i} out of range ({len})"));
    }

    fn cap(&mut self, n: usize, cap: usize, what: &str) {
        self.ensure(n <= cap, || what.to_owned(), format!("{n} exceeds cap {cap}"));
    }

    fn string(&mut self, s: &str, path: impl FnOnce() -> String) {
        let cap = self.caps.string_len;
        self.ensure(s.len() <= cap, path, format!("string longer than {cap} bytes"));
    }

    fn finite(&mut self, values: &[f32], path: impl FnOnce() -> String) {
        self.ensure(values.iter().all(|x| x.is_finite()), path, "non-finite number");
    }

    fn transform(&mut self, t: &Transform, path: impl FnOnce() -> String) {
        let all = [t.translation.as_slice(), &t.rotation, &t.scale].concat();
        self.finite(&all, path);
    }

    /// Every part of the avatar, in this order: errors come out in it and stop at [`MAX_ERRORS`].
    fn avatar(&mut self, a: &Avatar, has_blob: &impl Fn(BlobId) -> bool) {
        let c = self.caps;
        let nodes = a.hierarchy.nodes.len();
        self.string(&a.name, || "name".into());
        self.hierarchy(&a.hierarchy);
        if let Some(h) = &a.humanoid {
            self.humanoid(h, nodes);
        }
        self.cap(a.meshes.len(), c.meshes, "meshes");
        self.cap(a.materials.len(), c.materials, "materials");
        self.cap(a.textures.len(), c.textures, "textures");
        let mut totals = Totals::default();
        for (mi, m) in a.meshes.iter().enumerate() {
            self.mesh(mi, m, nodes, a.materials.len(), &mut totals);
        }
        if let Some(f) = &a.face {
            self.face(f, &a.meshes, nodes);
        }
        if let Some(m) = &a.meta {
            self.meta(m);
        }
        self.cap(totals.vertices, c.vertices, "total vertices");
        self.cap(totals.indices, c.indices, "total indices");
        self.cap(totals.shapes, c.blend_shapes, "total blend shapes");
        self.cap(totals.deltas, c.shape_deltas, "total blend shape deltas");
        self.materials(a, has_blob);
        self.clips(&a.clips);
        if let Some(anim) = &a.animator {
            self.animator(anim, a.clips.len());
        }
        if let Some(e) = &a.expressions {
            self.expressions(e, a.textures.len());
        }
        self.dynamics(&a.dynamics, nodes);
        self.contacts(&a.contacts, nodes);
        self.constraints(&a.constraints, nodes);
    }

    fn hierarchy(&mut self, h: &Hierarchy) {
        let nodes = h.nodes.len();
        self.cap(nodes, self.caps.nodes, "hierarchy.nodes");
        self.ensure(nodes > 0, || "hierarchy".into(), "no root node");
        for (i, n) in h.nodes.iter().enumerate() {
            let p = || format!("hierarchy.nodes[{i}]");
            self.string(&n.name, p);
            self.transform(&n.rest, p);
            match (i, n.parent) {
                (0, None) => {}
                (0, Some(_)) => self.fail(p, "root has a parent"),
                (_, None) => self.fail(p, "only node 0 may lack a parent"),
                (_, Some(parent)) => self.ensure((parent as usize) < i, p, "parent must precede child"),
            }
        }
    }

    fn humanoid(&mut self, h: &Humanoid, nodes: usize) {
        let mut seen = HashSet::new();
        for (bone, node) in &h.bones {
            self.index(*node as usize, nodes, || format!("humanoid.{bone:?}"), "node");
            self.ensure(seen.insert(*bone), || format!("humanoid.{bone:?}"), "mapped twice");
        }
        self.finite(&h.viewpoint, || "humanoid.viewpoint".into());
        self.ensure(
            h.human_scale.is_finite() && h.human_scale >= 0.0,
            || "humanoid.human_scale".into(),
            "must be finite and 0 or more",
        );
        if let Some(c) = &h.center_of_mass {
            self.finite(c, || "humanoid.center_of_mass".into());
        }
        let mut framed = HashSet::new();
        for (i, f) in h.muscle_frames.iter().enumerate() {
            let p = || format!("humanoid.muscle_frames[{i}]");
            self.ensure(h.node(f.bone).is_some(), p, "bone not mapped");
            self.ensure(framed.insert(f.bone), p, "bone framed twice");
            let all = [f.pre.as_slice(), &f.post, &f.sign, &f.min, &f.max, &[f.twist]].concat();
            self.finite(&all, p);
            let unit = |q: &[f32; 4]| (q.iter().map(|c| c * c).sum::<f32>() - 1.0).abs() < 1e-3;
            self.ensure(
                unit(&f.pre) && unit(&f.post),
                p,
                "pre and post must be unit quaternions",
            );
            self.ensure(f.sign.iter().all(|s| s.abs() == 1.0), p, "signs must be 1 or -1");
            self.ensure(
                f.min.iter().all(|m| *m <= 0.0) && f.max.iter().all(|m| *m >= 0.0),
                p,
                "min above 0 or max below 0",
            );
            let twist_ok = if f.bone.twist_child().is_some() {
                (0.0..=1.0).contains(&f.twist)
            } else {
                f.twist == 1.0
            };
            self.ensure(twist_ok, p, "twist must be in 0..=1 on limb bones and 1 elsewhere");
        }
    }

    /// Mesh `mi`: streams, skin, submeshes, levels of detail and blend shapes. Adds its counts to `totals`.
    fn mesh(&mut self, mi: usize, m: &Mesh, nodes: usize, materials: usize, totals: &mut Totals) {
        let p = |s: &str| format!("meshes[{mi}]{s}");
        let n = m.positions.len();
        totals.vertices += n;
        totals.indices += m.indices.len();
        totals.shapes += m.blend_shapes.len();
        self.string(&m.name, || p(".name"));
        self.index(m.node as usize, nodes, || p(".node"), "node");
        self.ensure(m.normals.len() == n, || p(".normals"), "length differs from positions");
        for (name, len) in [
            ("tangents", m.tangents.len()),
            ("uv0", m.uv0.len()),
            ("uv1", m.uv1.len()),
        ] {
            self.ensure(
                len == 0 || len == n,
                || p(&format!(".{name}")),
                "length differs from positions",
            );
        }
        self.finite(m.positions.as_flattened(), || p(".positions"));
        self.finite(m.normals.as_flattened(), || p(".normals"));
        if let Some(s) = &m.skin {
            self.skin(mi, s, n, nodes);
        }
        self.ensure(
            m.indices.len().is_multiple_of(3),
            || p(".indices"),
            "not a triangle list",
        );
        self.ensure(
            m.indices.iter().all(|&i| (i as usize) < n),
            || p(".indices"),
            "vertex index out of range",
        );
        for (si, s) in m.submeshes.iter().enumerate() {
            let sp = || p(&format!(".submeshes[{si}]"));
            let end = s.first_index as u64 + s.index_count as u64;
            self.ensure(end <= m.indices.len() as u64, sp, "index range out of bounds");
            self.ensure(s.index_count % 3 == 0, sp, "index count not a multiple of 3");
            self.index(s.material as usize, materials, sp, "material");
        }
        self.lods(mi, m, totals);
        for (bi, b) in m.blend_shapes.iter().enumerate() {
            let bp = || p(&format!(".blend_shapes[{bi}]"));
            totals.deltas += b.deltas.len();
            self.string(&b.name, bp);
            self.ensure(
                b.deltas.windows(2).all(|w| w[0].vertex < w[1].vertex),
                bp,
                "deltas not in strictly ascending vertex order",
            );
            self.ensure(
                b.deltas.last().is_none_or(|d| (d.vertex as usize) < n),
                bp,
                "vertex out of range",
            );
            self.ensure(
                b.deltas
                    .iter()
                    .all(|d| d.position.iter().chain(&d.normal).all(|x| x.is_finite())),
                bp,
                "non-finite delta",
            );
        }
    }

    /// The skin of mesh `mi`, which has `n` vertices.
    fn skin(&mut self, mi: usize, s: &Skin, n: usize, nodes: usize) {
        let p = |field: &str| format!("meshes[{mi}]{field}");
        for j in &s.joints {
            self.index(*j as usize, nodes, || p(".skin.joints"), "node");
        }
        self.ensure(
            s.inverse_bind.len() == s.joints.len(),
            || p(".skin.inverse_bind"),
            "one per joint",
        );
        self.ensure(
            s.vertex_joints.len() == n,
            || p(".skin.vertex_joints"),
            "one per vertex",
        );
        self.ensure(
            s.vertex_weights.len() == n,
            || p(".skin.vertex_weights"),
            "one per vertex",
        );
        let joints = s.joints.len();
        self.ensure(
            s.vertex_joints.as_flattened().iter().all(|&j| (j as usize) < joints),
            || p(".skin.vertex_joints"),
            "joint index out of range",
        );
        self.finite(s.vertex_weights.as_flattened(), || p(".skin.vertex_weights"));
        self.finite(s.inverse_bind.as_flattened(), || p(".skin.inverse_bind"));
    }

    /// The levels of detail of mesh `mi`: each a subset of the level above with the same submeshes.
    fn lods(&mut self, mi: usize, m: &Mesh, totals: &mut Totals) {
        let p = |s: &str| format!("meshes[{mi}]{s}");
        self.ensure(m.lods.len() <= MAX_LODS, || p(".lods"), "too many levels");
        let (mut above_v, mut above_i) = (m.positions.len(), m.indices.len());
        for (li, l) in m.lods.iter().enumerate() {
            let lp = || p(&format!(".lods[{li}]"));
            let v = l.vertices as usize;
            totals.indices += l.indices.len();
            self.ensure(v <= above_v, lp, "more vertices than the level above");
            self.ensure(l.indices.len() < above_i, lp, "not fewer indices than the level above");
            self.ensure(l.indices.len() % 3 == 0, lp, "not a triangle list");
            self.ensure(
                l.indices.iter().all(|&i| (i as usize) < v),
                lp,
                "vertex index out of range",
            );
            self.ensure(
                l.submeshes.len() == m.submeshes.len()
                    && l.submeshes
                        .iter()
                        .zip(&m.submeshes)
                        .all(|(s, t)| s.material == t.material),
                lp,
                "submeshes differ from the mesh's",
            );
            self.ensure(
                l.submeshes.iter().all(|s| {
                    s.index_count % 3 == 0 && s.first_index as u64 + s.index_count as u64 <= l.indices.len() as u64
                }),
                lp,
                "submesh index range out of bounds",
            );
            self.ensure(
                l.error.is_finite() && l.error >= 0.0,
                lp,
                "error not finite and non-negative",
            );
            (above_v, above_i) = (v.min(above_v), l.indices.len());
        }
    }

    fn face(&mut self, f: &Face, meshes: &[Mesh], nodes: usize) {
        let mut seen = HashSet::new();
        for (v, _) in &f.visemes {
            self.ensure(seen.insert(*v), || format!("face.{v:?}"), "mapped twice");
        }
        let mut binds: Vec<&ShapeBind> = f.visemes.iter().flat_map(|(_, b)| b).chain(&f.blink).collect();
        if let Some(e) = &f.eyes {
            let maps = [e.horizontal_inner, e.horizontal_outer, e.vertical_down, e.vertical_up];
            self.finite(&maps.map(|m| [m.input_max, m.output_scale]).concat(), || {
                "face.eyes".into()
            });
            match &e.driver {
                EyeDriver::Bones { left, right } => {
                    for n in left.iter().chain(right) {
                        self.index(*n as usize, nodes, || "face.eyes".into(), "eye node");
                    }
                }
                EyeDriver::Shapes { up, down, left, right } => {
                    binds.extend(up.iter().chain(down).chain(left).chain(right))
                }
            }
        }
        for (i, b) in binds.into_iter().enumerate() {
            let p = || format!("face.binds[{i}]");
            match meshes.get(b.mesh as usize) {
                Some(m) => self.index(b.shape as usize, m.blend_shapes.len(), p, "blend shape"),
                None => self.fail(p, format!("mesh index {} out of range", b.mesh)),
            }
            self.finite(&[b.weight], p);
        }
    }

    fn meta(&mut self, m: &Meta) {
        let l = &m.licence;
        let strings = m.authors.iter().chain(&m.references);
        self.ensure(
            m.authors.len() + m.references.len() <= META_LISTS,
            || "meta".into(),
            "too many authors or references",
        );
        for s in strings.chain([&m.version, &m.copyright, &m.contact, &l.url, &l.other_terms_url]) {
            self.string(s, || "meta".into());
        }
    }

    /// Materials' texture slots, then each texture's blob.
    fn materials(&mut self, a: &Avatar, has_blob: &impl Fn(BlobId) -> bool) {
        for (i, m) in a.materials.iter().enumerate() {
            let p = || format!("materials[{i}]");
            self.string(&m.name, p);
            let slots: Vec<&Option<TextureSlot>> = match &m.model {
                MaterialModel::Pbr(x) => vec![
                    &x.base_color_tex,
                    &x.metallic_roughness_tex,
                    &x.normal_tex,
                    &x.occlusion_tex,
                    &x.emissive_tex,
                ],
                MaterialModel::Toon(x) => {
                    let mut slots = vec![
                        &x.base_color_tex,
                        &x.normal_tex,
                        &x.shade_tex,
                        &x.matcap_tex,
                        &x.matcap_mask_tex,
                        &x.emissive_tex,
                        &x.outline_width_tex,
                        &x.outline_tex,
                    ];
                    if let Some(m) = &x.second_matcap {
                        slots.extend([&m.tex, &m.mask_tex]);
                    }
                    slots
                }
                MaterialModel::Unlit(x) => vec![&x.color_tex],
            };
            for s in slots.into_iter().flatten() {
                self.index(s.texture as usize, a.textures.len(), p, "texture");
                self.ensure(s.uv_set <= 1, p, "uv_set must be 0 or 1");
            }
        }
        for (i, t) in a.textures.iter().enumerate() {
            self.string(&t.name, || format!("textures[{i}]"));
            self.ensure(
                has_blob(t.blob),
                || format!("textures[{i}]"),
                format!("blob {} missing", t.blob),
            );
        }
    }

    fn expressions(&mut self, e: &Expressions, textures: usize) {
        let c = self.caps;
        let bits: u32 = e.parameters.iter().filter(|p| p.synced).map(|p| p.kind.bits()).sum();
        self.ensure(
            bits <= c.synced_bits,
            || "expressions".into(),
            format!("{bits} synced bits over cap"),
        );
        self.cap(e.parameters.len(), c.parameters, "expressions.parameters");
        for (i, p) in e.parameters.iter().enumerate() {
            self.string(&p.name, || format!("expressions.parameters[{i}]"));
        }
        let mut controls = 0;
        self.menu(&e.menu, 0, textures, &mut controls);
        self.cap(controls, c.menu_controls, "expressions.menu controls");
    }

    fn dynamics(&mut self, d: &Dynamics, nodes: usize) {
        let colliders = d.colliders.len();
        self.cap(d.chains.len(), self.caps.chains, "dynamics.chains");
        self.cap(colliders, self.caps.colliders, "dynamics.colliders");
        for (i, ch) in d.chains.iter().enumerate() {
            let p = || format!("dynamics.chains[{i}]");
            self.index(ch.root as usize, nodes, p, "root node");
            for n in &ch.ignore {
                self.index(*n as usize, nodes, p, "ignored node");
            }
            for col in &ch.colliders {
                self.index(*col as usize, colliders, p, "collider");
            }
            self.finite(&ch.endpoint, p);
            let mut graded = vec![
                &ch.pull,
                &ch.spring,
                &ch.stiffness,
                &ch.gravity,
                &ch.gravity_falloff,
                &ch.immobile,
                &ch.radius,
                &ch.max_stretch,
                &ch.max_squish,
            ];
            match &ch.limit {
                Limit::None => {}
                Limit::Angle { max_angle, rotation } | Limit::Hinge { max_angle, rotation } => {
                    graded.push(max_angle);
                    self.finite(rotation, p);
                }
                Limit::Polar {
                    max_pitch,
                    max_yaw,
                    rotation,
                } => {
                    graded.extend([max_pitch, max_yaw]);
                    self.finite(rotation, p);
                }
            }
            for g in graded {
                self.graded(g, p);
            }
            if let Some(s) = &ch.parameter {
                self.string(s, p);
            }
        }
        for (i, col) in d.colliders.iter().enumerate() {
            let p = || format!("dynamics.colliders[{i}]");
            self.index(col.node as usize, nodes, p, "node");
            let dims = match col.shape {
                Shape::Sphere { radius } => vec![radius],
                Shape::Capsule { radius, height } => vec![radius, height],
                Shape::Plane => vec![],
            };
            self.finite(&[col.position.as_slice(), &col.rotation, &dims].concat(), p);
        }
    }

    fn contacts(&mut self, c: &Contacts, nodes: usize) {
        self.cap(c.senders.len() + c.receivers.len(), self.caps.contacts, "contacts");
        let senders = c.senders.iter().map(|s| (&s.volume, &s.tags, None));
        let receivers = c.receivers.iter().map(|r| (&r.volume, &r.tags, Some(&r.parameter)));
        for (i, (vol, tags, param)) in senders.chain(receivers).enumerate() {
            let p = || format!("contacts[{i}]");
            self.volume(vol, nodes, p);
            for t in tags {
                self.string(t, p);
            }
            if let Some(s) = param {
                self.string(s, p);
            }
        }
    }

    fn constraints(&mut self, constraints: &[Constraint], nodes: usize) {
        self.cap(constraints.len(), self.caps.constraints, "constraints");
        for (i, k) in constraints.iter().enumerate() {
            let p = || format!("constraints[{i}]");
            self.index(k.target as usize, nodes, p, "target node");
            for s in &k.sources {
                self.index(s.node as usize, nodes, p, "source node");
                self.transform(&s.offset, p);
                self.finite(&[s.weight], p);
            }
            self.transform(&k.rest, p);
            self.finite(&[k.weight], p);
            let (numbers, up) = match &k.kind {
                ConstraintKind::Position { offset }
                | ConstraintKind::Rotation { offset }
                | ConstraintKind::Scale { offset } => (offset.to_vec(), None),
                ConstraintKind::Parent { .. } | ConstraintKind::RestRotation => (vec![], None),
                ConstraintKind::RestRoll { axis } | ConstraintKind::RestAim { axis } => (axis.to_vec(), None),
                ConstraintKind::Aim {
                    offset,
                    aim_axis,
                    up_axis,
                    world_up,
                } => {
                    let (node, v) = match *world_up {
                        WorldUp::Vector(v) => (None, v),
                        WorldUp::ObjectUp(n) => (Some(n), [0.0; 3]),
                        WorldUp::ObjectRotationUp(n, v) => (Some(n), v),
                        WorldUp::SceneUp | WorldUp::None => (None, [0.0; 3]),
                    };
                    ([offset.as_slice(), aim_axis, up_axis, &v].concat(), node)
                }
                ConstraintKind::LookAt {
                    offset,
                    roll,
                    world_up_node,
                } => ([offset.as_slice(), &[*roll]].concat(), *world_up_node),
            };
            self.finite(&numbers, p);
            if let Some(n) = up {
                self.index(n as usize, nodes, p, "up node");
            }
        }
    }

    fn graded(&mut self, g: &Graded, path: impl Fn() -> String) {
        let cap = self.caps.graded_points;
        self.ensure(g.curve.len() <= cap, &path, format!("graded curve over {cap} points"));
        self.finite(&[&[g.value], g.curve.as_flattened()].concat(), path);
    }

    fn volume(&mut self, v: &ContactVolume, nodes: usize, path: impl Fn() -> String) {
        self.index(v.node as usize, nodes, &path, "node");
        if let Some(c) = v.component {
            self.index(c as usize, nodes, &path, "component node");
        }
        let dims = match v.shape {
            ContactShape::Sphere { radius } => vec![radius],
            ContactShape::Capsule { radius, height } => vec![radius, height],
            ContactShape::Box { size } => size.to_vec(),
        };
        let ok = dims.iter().all(|d| *d >= 0.0);
        self.ensure(ok, &path, "negative or non-finite contact size");
        self.finite(&[v.position.as_slice(), &v.rotation, &dims].concat(), path);
    }

    fn menu(&mut self, m: &Menu, depth: usize, textures: usize, controls: &mut usize) {
        let cap = self.caps.menu_depth;
        if depth > cap {
            self.fail(|| "expressions.menu".into(), format!("nested deeper than {cap}"));
            return;
        }
        for (i, ctl) in m.controls.iter().enumerate() {
            *controls += 1;
            let p = || format!("expressions.menu(depth {depth})[{i}]");
            self.string(&ctl.name, p);
            if let Some(icon) = ctl.icon {
                self.index(icon as usize, textures, p, "icon texture");
            }
            for s in ctl.parameter.iter().chain(&ctl.sub_parameters) {
                self.string(s, p);
            }
            if let ControlKind::SubMenu(sub) = &ctl.kind {
                self.menu(sub, depth + 1, textures, controls);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animator::TransitionDest;
    use crate::testutil::sample_avatar;

    fn check(a: &Avatar) -> Result<(), Vec<ValidationError>> {
        validate(a, &Caps::default(), |_| true)
    }

    fn expect_error(a: &Avatar, path_part: &str) {
        let errors = check(a).expect_err("expected a validation error");
        assert!(
            errors.iter().any(|e| e.path.contains(path_part)),
            "no error at {path_part}: {errors:?}"
        );
    }

    #[test]
    fn sample_is_valid() {
        check(&sample_avatar()).unwrap();
    }

    #[test]
    fn levels_are_checked() {
        use crate::mesh::{Lod, Submesh};
        let mut a = sample_avatar();
        a.meshes[0].lods = vec![Lod {
            vertices: 2,
            indices: Vec::new(),
            submeshes: vec![Submesh {
                first_index: 0,
                index_count: 0,
                material: 0,
            }],
            error: 0.01,
        }];
        check(&a).unwrap();
        // The level keeps two vertices, the shape's delta on vertex 2 goes, its name stays.
        let l = a.meshes[0].level(1);
        assert_eq!((l.positions.len(), l.indices.len(), l.lods.len()), (2, 0, 0));
        assert_eq!(l.skin.as_ref().unwrap().vertex_weights.len(), 2);
        assert!(l.blend_shapes[0].deltas.is_empty());
        assert_eq!(a.meshes[0].level(5), l);
        assert_eq!(a.meshes[0].level(0), a.meshes[0]);
        type Mutation = fn(&mut Lod);
        let cases: &[Mutation] = &[
            |l| l.vertices = 4,
            |l| l.indices = vec![0, 1, 2],
            |l| l.indices = vec![0, 1],
            |l| l.submeshes.clear(),
            |l| l.submeshes[0].material = 1,
            |l| l.submeshes[0].index_count = 3,
            |l| l.error = f32::NAN,
        ];
        for mutate in cases {
            let mut b = a.clone();
            mutate(&mut b.meshes[0].lods[0]);
            expect_error(&b, "meshes[0].lods[0]");
        }
        // A second level may not have more vertices or as many indices as the first.
        let mut b = a.clone();
        let again = b.meshes[0].lods[0].clone();
        b.meshes[0].lods.push(again);
        expect_error(&b, "meshes[0].lods[1]");
    }

    #[test]
    fn missing_blob_is_rejected() {
        assert!(validate(&sample_avatar(), &Caps::default(), |_| false).is_err());
    }

    #[test]
    fn out_of_range_indices_are_rejected() {
        type Mutation = fn(&mut Avatar);
        let cases: &[(&str, Mutation)] = &[
            ("hierarchy.nodes[1]", |a| a.hierarchy.nodes[1].parent = Some(5)),
            ("humanoid", |a| a.humanoid.as_mut().unwrap().bones[0].1 = 99),
            ("humanoid.muscle_frames[0]", |a| {
                a.humanoid.as_mut().unwrap().muscle_frames[0].bone = crate::hierarchy::HumanBone::Head
            }),
            ("humanoid.muscle_frames[0]", |a| {
                a.humanoid.as_mut().unwrap().muscle_frames[0].twist = 0.5
            }),
            ("humanoid.muscle_frames[0]", |a| {
                a.humanoid.as_mut().unwrap().muscle_frames[0].pre = [0.0; 4]
            }),
            ("meshes[0].node", |a| a.meshes[0].node = 99),
            ("meshes[0].indices", |a| a.meshes[0].indices[0] = 3),
            ("meshes[0].submeshes[0]", |a| a.meshes[0].submeshes[0].material = 1),
            ("meshes[0].submeshes[0]", |a| a.meshes[0].submeshes[0].index_count = 6),
            ("meshes[0].skin.joints", |a| {
                a.meshes[0].skin.as_mut().unwrap().joints[0] = 2
            }),
            ("meshes[0].skin.vertex_joints", |a| {
                a.meshes[0].skin.as_mut().unwrap().vertex_joints[0][1] = 1
            }),
            ("meshes[0].blend_shapes[0]", |a| {
                a.meshes[0].blend_shapes[0].deltas[0].vertex = 3
            }),
            ("face.binds[0]", |a| a.face.as_mut().unwrap().visemes[0].1[0].shape = 1),
            ("face.binds[0]", |a| a.face.as_mut().unwrap().visemes[0].1[0].mesh = 1),
            ("face.eyes", |a| {
                a.face.as_mut().unwrap().eyes.as_mut().unwrap().driver = crate::face::EyeDriver::Bones {
                    left: Some(9),
                    right: None,
                }
            }),
            ("face.Aa", |a| {
                let f = a.face.as_mut().unwrap();
                f.visemes.push(f.visemes[0].clone())
            }),
            ("materials[0]", |a| a.textures.clear()),
            ("animator.layers[0]", |a| {
                a.animator.as_mut().unwrap().layers[0].group = 1
            }),
            ("animator.layers[0]", |a| {
                a.animator.as_mut().unwrap().layers[0].state_machine.states[0].transitions[0].dest =
                    TransitionDest::State(9)
            }),
            ("animator.layers[0]", |a| {
                a.animator.as_mut().unwrap().layers[0].state_machine.states[0].transitions[0].conditions[0].param = 9
            }),
            ("animator.layers[0].states[1]", |a| a.clips.clear()),
            ("dynamics.chains[0]", |a| a.dynamics.chains[0].colliders[0] = 4),
            ("dynamics.colliders[0]", |a| a.dynamics.colliders[0].node = 40),
            ("contacts[0]", |a| a.contacts.receivers[0].volume.node = 40),
            ("contacts[0]", |a| a.contacts.receivers[0].volume.component = Some(40)),
            ("constraints[0]", |a| a.constraints[0].sources[0].node = 40),
            ("constraints[0]", |a| {
                a.constraints[0].kind = ConstraintKind::LookAt {
                    offset: [0.0; 3],
                    roll: 0.0,
                    world_up_node: Some(40),
                }
            }),
        ];
        for (path, mutate) in cases {
            let mut a = sample_avatar();
            mutate(&mut a);
            expect_error(&a, path);
        }
    }

    #[test]
    fn non_finite_and_unordered_data_is_rejected() {
        let mut a = sample_avatar();
        a.meshes[0].positions[0][1] = f32::NAN;
        expect_error(&a, "meshes[0].positions");

        let mut a = sample_avatar();
        let curve = &mut a.clips[0].curves[0];
        curve.keys.push(curve.keys[0]);
        expect_error(&a, "clips[0].curves[0]");

        for shape in [
            Shape::Sphere { radius: f32::NAN },
            Shape::Capsule {
                radius: 0.1,
                height: f32::INFINITY,
            },
        ] {
            let mut a = sample_avatar();
            a.dynamics.colliders[0].shape = shape;
            expect_error(&a, "dynamics.colliders[0]");
        }
    }

    #[test]
    fn contact_shapes_are_checked() {
        for (shape, problem) in [
            (
                ContactShape::Sphere { radius: -1.0 },
                "negative or non-finite contact size",
            ),
            (
                ContactShape::Box { size: [0.1, -0.2, 0.3] },
                "negative or non-finite contact size",
            ),
            (
                ContactShape::Capsule {
                    radius: 0.1,
                    height: f32::NAN,
                },
                "negative or non-finite contact size",
            ),
        ] {
            let mut a = sample_avatar();
            a.contacts.receivers[0].volume.shape = shape;
            let errors = check(&a).expect_err("expected a validation error");
            assert!(errors.iter().any(|e| e.problem == problem), "{shape:?}: {errors:?}");
        }
    }

    #[test]
    fn caps_are_enforced() {
        let caps = Caps {
            nodes: 1,
            ..Caps::default()
        };
        let errors = validate(&sample_avatar(), &caps, |_| true).unwrap_err();
        assert!(errors.iter().any(|e| e.path == "hierarchy.nodes"));
    }

    #[test]
    fn deep_menus_are_rejected_without_recursing_forever() {
        let mut a = sample_avatar();
        let mut menu = Menu::default();
        for _ in 0..100 {
            let mut parent = Menu::default();
            parent.controls.push(crate::expressions::Control {
                name: "sub".into(),
                icon: None,
                kind: ControlKind::SubMenu(menu),
                parameter: None,
                value: 0.0,
                sub_parameters: vec![],
            });
            menu = parent;
        }
        a.expressions.as_mut().unwrap().menu = menu;
        expect_error(&a, "expressions.menu");
    }

    #[test]
    fn error_count_is_bounded() {
        let mut a = sample_avatar();
        a.meshes[0].indices = vec![99; 3000];
        a.meshes[0].submeshes = vec![a.meshes[0].submeshes[0]; 1000];
        a.meshes[0].submeshes.iter_mut().for_each(|s| s.material = 7);
        assert_eq!(check(&a).unwrap_err().len(), MAX_ERRORS);
    }
}
