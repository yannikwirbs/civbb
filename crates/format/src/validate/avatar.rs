//! Validation of the avatar role: humanoid map, viewpoint, face, expression menu and metadata.

use std::collections::HashSet;

use crate::avatar::Avatar;
use crate::document::Document;
use crate::expressions::{ControlKind, Expressions, Menu};
use crate::face::{EyeDriver, Face, ShapeBind, VisemeBinds};
use crate::hierarchy::{BoneNode, Humanoid};
use crate::mesh::Mesh;
use crate::meta::Meta;

use super::V;

/// Authors plus references in [`crate::meta::Meta`].
const META_LISTS: usize = 256;

impl V {
    pub(super) fn avatar(&mut self, r: &Avatar, a: &Document) {
        let nodes = a.hierarchy.nodes.len();
        if let Some(h) = &r.humanoid {
            self.humanoid(h, nodes);
        }
        self.finite(&r.viewpoint, || "viewpoint".into());
        if let Some(f) = &r.face {
            self.face(f, &a.meshes, nodes);
        }
        if let Some(e) = &r.expressions {
            self.expressions(e, a.textures.len());
        }
        if let Some(m) = &r.meta {
            self.meta(m);
        }
    }

    pub(super) fn humanoid(&mut self, h: &Humanoid, nodes: usize) {
        let mut seen = HashSet::new();
        for &BoneNode { bone, node } in &h.bones {
            self.index(node as usize, nodes, || format!("humanoid.{bone:?}"), "node");
            self.ensure(seen.insert(bone), || format!("humanoid.{bone:?}"), "mapped twice");
        }
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

    pub(super) fn face(&mut self, f: &Face, meshes: &[Mesh], nodes: usize) {
        let mut seen = HashSet::new();
        for VisemeBinds { viseme: v, .. } in &f.visemes {
            self.ensure(seen.insert(*v), || format!("face.{v:?}"), "mapped twice");
        }
        let mut binds: Vec<&ShapeBind> = f.visemes.iter().flat_map(|v| &v.binds).chain(&f.blink).collect();
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

    pub(super) fn meta(&mut self, m: &Meta) {
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

    pub(super) fn expressions(&mut self, e: &Expressions, textures: usize) {
        let mut controls = 0;
        self.menu(&e.menu, 0, textures, &mut controls);
        self.cap(controls, self.caps.menu_controls, "expressions.menu controls");
    }

    pub(super) fn menu(&mut self, m: &Menu, depth: usize, textures: usize, controls: &mut usize) {
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
            for &e in ctl.parameter.iter().chain(ctl.sub_parameters.iter().flatten()) {
                self.written(e, p, "entry");
            }
            if let ControlKind::SubMenu(sub) = &ctl.kind {
                self.menu(sub, depth + 1, textures, controls);
            }
        }
    }
}
