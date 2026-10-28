//! VRM 0.x (`VRM`) and 1.0 (`VRMC_vrm`, `VRMC_springBone`, `VRMC_materials_mtoon`) to native types (ADR 0006).
//!
//! Runs after the glTF import and reads the extension JSON tolerantly: a missing or malformed field takes the
//! VRM default, an unresolvable reference is skipped with a warning. Validation still checks the result.

use civbb_format::animator::*;
use civbb_format::dynamics::{Chain, Collider, Dynamics, Graded, ImmobileType, Integration, Limit, MultiChild, Shape};
use civbb_format::expressions::{Control, ControlKind, ExpressionParam, ExpressionParamKind, Expressions, Menu};
use civbb_format::face::{Face, ShapeBind, Viseme};
use civbb_format::hierarchy::{HumanBone, Humanoid, NodeIndex};
use civbb_format::material::{AlphaMode, ColorSpace, Material, MaterialModel, TextureSlot, Toon};
use civbb_format::math::{Quat, Vec3};
use serde_json::Value;

use super::Importer;

/// The avatar parts VRM adds to a plain glTF import.
pub(super) struct Vrm {
    pub name: Option<String>,
    pub humanoid: Humanoid,
    pub face: Option<Face>,
    pub dynamics: Dynamics,
    pub animator: Option<Animator>,
    pub expressions: Option<Expressions>,
}

/// One VRM expression after resolving its binds.
struct Expression {
    name: String,
    preset: Option<String>,
    binds: Vec<ShapeBind>,
}

impl Importer<'_> {
    /// Applies VRM extensions if present. Expression clips are appended to `clips`.
    pub(super) fn vrm(&mut self, clips: &mut Vec<Clip>) -> Result<Option<Vrm>, String> {
        let doc = self.doc;
        let (v1, root) = match (doc.extension_value("VRMC_vrm"), doc.extension_value("VRM")) {
            (Some(v), _) => (true, v),
            (None, Some(v)) => (false, v),
            (None, None) => return Ok(None),
        };
        if !v1 {
            // VRM 0.x models face -Z; turn the whole avatar to +Z.
            let r = &mut self.nodes[0].rest;
            r.rotation = qmul([0.0, 1.0, 0.0, 0.0], r.rotation);
            r.translation = [-r.translation[0], r.translation[1], -r.translation[2]];
        }
        let humanoid = self.vrm_humanoid(root, v1);
        let dynamics = if v1 {
            doc.extension_value("VRMC_springBone")
                .map(|s| self.springs_v1(s))
                .unwrap_or_default()
        } else {
            self.springs_v0(&root["secondaryAnimation"])
        };
        self.mtoon(root, v1);
        let exprs = if v1 {
            self.expressions_v1(&root["expressions"])
        } else {
            self.expressions_v0(&root["blendShapeMaster"]["blendShapeGroups"])
        };
        if root["firstPerson"]["meshAnnotations"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            self.warnings
                .push("VRM first-person mesh visibility not imported yet (#44)".into());
        }
        if doc.nodes().any(|n| n.extension_value("VRMC_node_constraint").is_some()) {
            self.warnings.push("VRMC_node_constraint not imported yet (#46)".into());
        }
        let look = if v1 {
            &root["lookAt"]
        } else {
            &root["firstPerson"]["lookAtTypeName"]
        };
        if !look.is_null() {
            self.warnings.push("VRM eye look not imported yet (#44)".into());
        }
        let title = if v1 {
            &root["meta"]["name"]
        } else {
            &root["meta"]["title"]
        };
        let mut vrm = Vrm {
            name: title.as_str().filter(|s| !s.is_empty()).map(str::to_owned),
            humanoid,
            face: None,
            dynamics,
            animator: None,
            expressions: None,
        };
        self.expression_parts(exprs, clips, &mut vrm)?;
        Ok(Some(vrm))
    }

    fn vrm_node(&self, v: &Value) -> Option<NodeIndex> {
        index(v).and_then(|i| self.map.get(i).copied().flatten())
    }

    fn vrm_humanoid(&mut self, root: &Value, v1: bool) -> Humanoid {
        let bones: Vec<(String, &Value)> = if v1 {
            let map = root["humanoid"]["humanBones"].as_object();
            map.into_iter()
                .flatten()
                .map(|(k, b)| (k.clone(), &b["node"]))
                .collect()
        } else {
            let list = root["humanoid"]["humanBones"].as_array();
            list.into_iter()
                .flatten()
                .map(|b| (b["bone"].as_str().unwrap_or_default().to_owned(), &b["node"]))
                .collect()
        };
        let mut h = Humanoid::default();
        for (name, node) in bones {
            let (Some(bone), Some(node)) = (human_bone(&name, v1), self.vrm_node(node)) else {
                self.warnings.push(format!("VRM humanoid bone `{name}` skipped"));
                continue;
            };
            if h.node(bone).is_some() {
                self.warnings.push(format!("VRM humanoid bone `{name}` mapped twice"));
                continue;
            }
            h.bones.push((bone, node));
        }
        let world = self.world();
        let (bone, offset) = if v1 {
            (
                h.node(HumanBone::Head),
                arr(&root["lookAt"]["offsetFromHeadBone"], [0.0, 0.06, 0.0]),
            )
        } else {
            let fp = &root["firstPerson"];
            let bone = self.vrm_node(&fp["firstPersonBone"]).or(h.node(HumanBone::Head));
            (bone, xyz(&fp["firstPersonBoneOffset"], [0.0, 0.06, 0.0]))
        };
        match bone {
            Some(b) => {
                let (t, r, s) = world[b as usize];
                h.viewpoint = add(t, rotate(r, [offset[0] * s[0], offset[1] * s[1], offset[2] * s[2]]));
            }
            None => self.warnings.push("VRM has no head bone; viewpoint at origin".into()),
        }
        h
    }

    /// World translation, rotation and scale of every node (scale and rotation composed per axis, which is exact
    /// without non-uniform scale under rotation).
    fn world(&self) -> Vec<(Vec3, Quat, Vec3)> {
        let mut w: Vec<(Vec3, Quat, Vec3)> = Vec::with_capacity(self.nodes.len());
        for n in &self.nodes {
            let l = &n.rest;
            w.push(match n.parent {
                None => (l.translation, l.rotation, l.scale),
                Some(p) => {
                    let (pt, pr, ps) = w[p as usize];
                    let t = l.translation;
                    (
                        add(pt, rotate(pr, [t[0] * ps[0], t[1] * ps[1], t[2] * ps[2]])),
                        qmul(pr, l.rotation),
                        [ps[0] * l.scale[0], ps[1] * l.scale[1], ps[2] * l.scale[2]],
                    )
                }
            });
        }
        w
    }

    fn children(&self) -> Vec<Vec<NodeIndex>> {
        let mut c = vec![Vec::new(); self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(p) = n.parent {
                c[p as usize].push(i as NodeIndex);
            }
        }
        c
    }

    fn springs_v0(&mut self, sa: &Value) -> Dynamics {
        let mut d = Dynamics::default();
        let mut groups = Vec::new();
        for g in sa["colliderGroups"].as_array().into_iter().flatten() {
            let mut ids = Vec::new();
            if let Some(node) = self.vrm_node(&g["node"]) {
                for c in g["colliders"].as_array().into_iter().flatten() {
                    ids.push(d.colliders.len() as u32);
                    d.colliders
                        .push(sphere(node, xyz(&c["offset"], [0.0; 3]), num(&c["radius"], 0.0)));
                }
            } else {
                self.warnings
                    .push("VRM collider group without a valid node skipped".into());
            }
            groups.push(ids);
        }
        let children = self.children();
        for bg in sa["boneGroups"].as_array().into_iter().flatten() {
            let colliders = collider_list(&bg["colliderGroups"], &groups);
            let gravity_dir = xyz(&bg["gravityDir"], [0.0, -1.0, 0.0]);
            for b in bg["bones"].as_array().into_iter().flatten() {
                let Some(root) = self.vrm_node(b) else {
                    self.warnings
                        .push("VRM spring bone without a valid node skipped".into());
                    continue;
                };
                // UniVRM 0.x gives each leaf a 7 cm virtual tail continuing the bone; use the first leaf's direction.
                let mut leaf = root;
                while let Some(&c) = children[leaf as usize].first() {
                    leaf = c;
                }
                let rest = &self.nodes[leaf as usize].rest;
                let dir = rotate(conj(rest.rotation), rest.translation);
                let len = dot(dir, dir).sqrt();
                let endpoint = if len > 1e-6 {
                    dir.map(|x| x / len * 0.07)
                } else {
                    [0.0, 0.07, 0.0]
                };
                d.chains.push(chain(
                    root,
                    Vec::new(),
                    endpoint,
                    &Params {
                        stiffness: Graded::constant(num(&bg["stiffiness"], 1.0)),
                        drag: Graded::constant(num(&bg["dragForce"], 0.4)),
                        gravity: Graded::constant(num(&bg["gravityPower"], 0.0) * -gravity_dir[1]),
                        radius: Graded::constant(num(&bg["hitRadius"], 0.02)),
                    },
                    colliders.clone(),
                ));
            }
        }
        d
    }

    fn springs_v1(&mut self, sb: &Value) -> Dynamics {
        let mut d = Dynamics::default();
        let mut collider_ids = Vec::new();
        for c in sb["colliders"].as_array().into_iter().flatten() {
            let (Some(node), shape) = (self.vrm_node(&c["node"]), &c["shape"]) else {
                self.warnings.push("VRM collider without a valid node skipped".into());
                collider_ids.push(None);
                continue;
            };
            let collider = if !shape["capsule"].is_null() {
                let s = &shape["capsule"];
                capsule(
                    node,
                    arr(&s["offset"], [0.0; 3]),
                    arr(&s["tail"], [0.0; 3]),
                    num(&s["radius"], 0.0),
                )
            } else {
                let s = &shape["sphere"];
                sphere(node, arr(&s["offset"], [0.0; 3]), num(&s["radius"], 0.0))
            };
            collider_ids.push(Some(d.colliders.len() as u32));
            d.colliders.push(collider);
        }
        let groups: Vec<Vec<u32>> = sb["colliderGroups"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| {
                let ids = g["colliders"].as_array().into_iter().flatten();
                ids.filter_map(|i| collider_ids.get(index(i)?).copied().flatten())
                    .collect()
            })
            .collect();
        let children = self.children();
        let cap = self.caps.graded_points;
        for s in sb["springs"].as_array().into_iter().flatten() {
            let joints: Vec<(NodeIndex, &Value)> = s["joints"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|j| Some((self.vrm_node(&j["node"])?, j)))
                .collect();
            let Some(&(root, _)) = joints.first() else {
                self.warnings.push("VRM spring without valid joints skipped".into());
                continue;
            };
            // Only the listed path is simulated: branches off it are ignored, and the last joint is the tail.
            let mut ignore = Vec::new();
            for (i, (node, _)) in joints.iter().enumerate() {
                let next = joints.get(i + 1).map(|j| j.0);
                ignore.extend(children[*node as usize].iter().filter(|&&c| Some(c) != next));
            }
            // Parameters on a joint drive the segment to the next joint, so the tail's are unused.
            let segs = &joints[..joints.len().saturating_sub(1).max(1)];
            let per =
                |key: &str, default: f32| -> Vec<f32> { segs.iter().map(|(_, j)| num(&j[key], default)).collect() };
            let gravity: Vec<f32> = segs
                .iter()
                .map(|(_, j)| num(&j["gravityPower"], 0.0) * -arr(&j["gravityDir"], [0.0, -1.0, 0.0])[1])
                .collect();
            d.chains.push(chain(
                root,
                ignore,
                [0.0; 3],
                &Params {
                    stiffness: graded(&per("stiffness", 1.0), cap),
                    drag: graded(&per("dragForce", 0.5), cap),
                    gravity: graded(&gravity, cap),
                    radius: graded(&per("hitRadius", 0.0), cap),
                },
                collider_list(&s["colliderGroups"], &groups),
            ));
        }
        d
    }

    fn mtoon_slot(&mut self, v: &Value, space: ColorSpace, scale_offset: Option<[f32; 4]>) -> Option<TextureSlot> {
        let doc = self.doc;
        let i = index(if v.is_object() { &v["index"] } else { v })?;
        let Some(texture) = doc.textures().nth(i) else {
            self.warnings
                .push(format!("MToon texture {i} does not exist; slot dropped"));
            return None;
        };
        let tex_coord = index(&v["texCoord"]).unwrap_or(0) as u32;
        self.slot(&texture, tex_coord, scale_offset, space)
    }

    /// Replaces MToon materials with the Toon model; other materials keep their glTF import.
    fn mtoon(&mut self, root: &Value, v1: bool) {
        let doc = self.doc;
        for (i, m) in doc.materials().enumerate() {
            let toon = if v1 {
                match m.extension_value("VRMC_materials_mtoon") {
                    Some(x) => self.mtoon_v1(i, x),
                    None => continue,
                }
            } else {
                let p = &root["materialProperties"][i];
                if p["shader"].as_str() != Some("VRM/MToon") {
                    continue;
                }
                self.mtoon_v0(i, p)
            };
            self.materials[i] = toon;
        }
    }

    fn mtoon_v1(&mut self, i: usize, x: &Value) -> Material {
        let base = self.materials[i].clone();
        let (base_color, base_color_tex, normal_tex, normal_scale, emissive, emissive_tex) = match &base.model {
            MaterialModel::Pbr(p) => (
                p.base_color,
                p.base_color_tex,
                p.normal_tex,
                p.normal_scale,
                p.emissive,
                p.emissive_tex,
            ),
            MaterialModel::Unlit(u) => (u.color, u.color_tex, None, 1.0, [0.0; 3], None),
            MaterialModel::Toon(t) => (t.base_color, t.base_color_tex, None, 1.0, [0.0; 3], None),
        };
        let matcap_tex = self.mtoon_slot(&x["matcapTexture"], ColorSpace::Srgb, None);
        let outline_width = match x["outlineWidthMode"].as_str() {
            Some("worldCoordinates") => num(&x["outlineWidthFactor"], 0.0),
            Some("screenCoordinates") => {
                self.warnings
                    .push(format!("material {i}: screen-space MToon outline dropped"));
                0.0
            }
            _ => 0.0,
        };
        let uv_anim = [
            "uvAnimationScrollXSpeedFactor",
            "uvAnimationScrollYSpeedFactor",
            "uvAnimationRotationSpeedFactor",
        ];
        if uv_anim.iter().any(|k| num(&x[*k], 0.0) != 0.0) {
            self.warnings.push(format!("material {i}: MToon UV animation dropped"));
        }
        let offset = x["renderQueueOffsetNumber"].as_i64().unwrap_or(0).clamp(-9, 9) as i32;
        Material {
            model: MaterialModel::Toon(Toon {
                base_color,
                base_color_tex,
                normal_tex,
                normal_scale,
                shade_color: arr(&x["shadeColorFactor"], [0.0; 3]),
                shade_tex: self.mtoon_slot(&x["shadeMultiplyTexture"], ColorSpace::Srgb, None),
                shade_shift: num(&x["shadingShiftFactor"], 0.0),
                shade_toony: num(&x["shadingToonyFactor"], 0.9),
                rim_color: arr(&x["parametricRimColorFactor"], [0.0; 3]),
                rim_power: num(&x["parametricRimFresnelPowerFactor"], 5.0),
                matcap_strength: if matcap_tex.is_some() {
                    arr(&x["matcapFactor"], [1.0; 3]).into_iter().fold(0.0, f32::max)
                } else {
                    0.0
                },
                matcap_tex,
                emissive,
                emissive_tex,
                outline_width,
                outline_color: arr(&x["outlineColorFactor"], [0.0; 3]),
                outline_width_tex: self.mtoon_slot(&x["outlineWidthMultiplyTexture"], ColorSpace::Linear, None),
            }),
            render_queue: base.render_queue + offset,
            ..base
        }
    }

    fn mtoon_v0(&mut self, i: usize, p: &Value) -> Material {
        let (f, v, t) = (&p["floatProperties"], &p["vectorProperties"], &p["textureProperties"]);
        // UniVRM 0.x stores texture tiling as [offset x, offset y, scale x, scale y].
        let main = arr(&v["_MainTex"], [0.0, 0.0, 1.0, 1.0]);
        let main_so = [main[2], main[3], main[0], main[1]];
        let base_color_tex = self.mtoon_slot(&t["_MainTex"], ColorSpace::Srgb, Some(main_so));
        let matcap_tex = self.mtoon_slot(&t["_SphereAdd"], ColorSpace::Srgb, None);
        let outline_width = match num(&f["_OutlineWidthMode"], 0.0) as i32 {
            // MToon 0.x world outline width is in centimetres.
            1 => num(&f["_OutlineWidth"], 0.0) * 0.01,
            2 => {
                self.warnings
                    .push(format!("material {i}: screen-space MToon outline dropped"));
                0.0
            }
            _ => 0.0,
        };
        let (alpha, default_queue) = match num(&f["_BlendMode"], 0.0) as i32 {
            1 => (
                AlphaMode::Mask {
                    cutoff: num(&f["_Cutoff"], 0.5),
                },
                2450,
            ),
            2 | 3 => (AlphaMode::Blend, 3000),
            _ => (AlphaMode::Opaque, 2000),
        };
        let color = |key: &str, default: [f32; 4]| srgb(arr(&v[key], default));
        let rgb = |c: [f32; 4]| [c[0], c[1], c[2]];
        Material {
            name: self.materials[i].name.clone(),
            model: MaterialModel::Toon(Toon {
                base_color: color("_Color", [1.0; 4]),
                base_color_tex,
                normal_tex: self.mtoon_slot(&t["_BumpMap"], ColorSpace::Linear, None),
                normal_scale: num(&f["_BumpScale"], 1.0),
                shade_color: rgb(color("_ShadeColor", [0.97, 0.81, 0.86, 1.0])),
                shade_tex: self.mtoon_slot(&t["_ShadeTexture"], ColorSpace::Srgb, None),
                shade_shift: num(&f["_ShadeShift"], 0.0),
                shade_toony: num(&f["_ShadeToony"], 0.9),
                rim_color: rgb(color("_RimColor", [0.0, 0.0, 0.0, 1.0])),
                rim_power: num(&f["_RimFresnelPower"], 1.0),
                matcap_strength: if matcap_tex.is_some() { 1.0 } else { 0.0 },
                matcap_tex,
                emissive: rgb(color("_EmissionColor", [0.0, 0.0, 0.0, 1.0])),
                emissive_tex: self.mtoon_slot(&t["_EmissionMap"], ColorSpace::Srgb, None),
                outline_width,
                outline_color: rgb(color("_OutlineColor", [0.0, 0.0, 0.0, 1.0])),
                outline_width_tex: self.mtoon_slot(&t["_OutlineWidthTexture"], ColorSpace::Linear, None),
            }),
            alpha,
            double_sided: num(&f["_CullMode"], 2.0) as i32 == 0,
            render_queue: p["renderQueue"]
                .as_i64()
                .map_or(default_queue, |q| q.clamp(0, 5000) as i32),
        }
    }

    /// Resolves a morph bind to a format mesh and shape, warning when it does not exist.
    fn bind(&mut self, mesh: Option<usize>, shape: &Value, weight: f32, out: &mut Vec<ShapeBind>) {
        let shape = index(shape);
        match (mesh, shape) {
            (Some(m), Some(s)) if s < self.meshes[m].blend_shapes.len() => out.push(ShapeBind {
                mesh: m as u32,
                shape: s as u32,
                weight,
            }),
            _ => self
                .warnings
                .push("VRM expression bind to a missing blendshape skipped".into()),
        }
    }

    fn expressions_v0(&mut self, groups: &Value) -> Vec<Expression> {
        let doc = self.doc;
        let mut out = Vec::new();
        for g in groups.as_array().into_iter().flatten() {
            let mut binds = Vec::new();
            for b in g["binds"].as_array().into_iter().flatten() {
                // 0.x binds name a glTF mesh; every node instancing it gets the bind.
                let mesh = index(&b["mesh"]);
                let targets: Vec<usize> = doc
                    .nodes()
                    .filter(|n| n.mesh().map(|m| m.index()) == mesh && mesh.is_some())
                    .filter_map(|n| self.node_meshes.get(&self.map[n.index()]?).copied())
                    .collect();
                if targets.is_empty() {
                    self.warnings
                        .push("VRM expression bind to a missing mesh skipped".into());
                }
                for m in targets {
                    self.bind(Some(m), &b["index"], num(&b["weight"], 100.0) / 100.0, &mut binds);
                }
            }
            if g["materialValues"].as_array().is_some_and(|a| !a.is_empty()) {
                self.warnings.push("VRM expression material binds dropped".into());
            }
            let preset = g["presetName"].as_str().filter(|p| *p != "unknown");
            out.push(Expression {
                name: g["name"].as_str().or(preset).unwrap_or("expression").to_owned(),
                preset: preset.map(|p| p.to_lowercase()),
                binds,
            });
        }
        out
    }

    fn expressions_v1(&mut self, e: &Value) -> Vec<Expression> {
        let mut out = Vec::new();
        for (kind, map) in [("preset", &e["preset"]), ("custom", &e["custom"])] {
            for (name, x) in map.as_object().into_iter().flatten() {
                let mut binds = Vec::new();
                for b in x["morphTargetBinds"].as_array().into_iter().flatten() {
                    let mesh = self
                        .vrm_node(&b["node"])
                        .and_then(|n| self.node_meshes.get(&n).copied());
                    self.bind(mesh, &b["index"], num(&b["weight"], 0.0), &mut binds);
                }
                let material_binds = ["materialColorBinds", "textureTransformBinds"]
                    .iter()
                    .any(|k| x[*k].as_array().is_some_and(|a| !a.is_empty()));
                if material_binds {
                    self.warnings.push("VRM expression material binds dropped".into());
                }
                out.push(Expression {
                    name: name.clone(),
                    preset: (kind == "preset").then(|| name.to_lowercase()),
                    binds,
                });
            }
        }
        out
    }

    /// Visemes and blink go to the face; the rest become clips in one Direct blend tree, toggled from the menu.
    fn expression_parts(&mut self, exprs: Vec<Expression>, clips: &mut Vec<Clip>, out: &mut Vrm) -> Result<(), String> {
        let mut face = Face::default();
        let mut toggles = Vec::new();
        for e in exprs {
            let viseme = match e.preset.as_deref() {
                Some("a" | "aa") => Some(Viseme::Aa),
                Some("i" | "ih") => Some(Viseme::I),
                Some("u" | "ou") => Some(Viseme::U),
                Some("e" | "ee") => Some(Viseme::E),
                Some("o" | "oh") => Some(Viseme::O),
                _ => None,
            };
            match (viseme, e.preset.as_deref()) {
                (Some(v), _) if face.visemes.iter().all(|(x, _)| *x != v) => face.visemes.push((v, e.binds)),
                (Some(_), _) => {}
                (None, Some("blink")) => face.blink = e.binds,
                // Eye look and one-eye blinks belong to the eye-look setup (#44).
                (None, Some("blink_l" | "blink_r" | "blinkleft" | "blinkright")) => {}
                (None, Some("lookup" | "lookdown" | "lookleft" | "lookright")) => {}
                _ if e.binds.is_empty() => {}
                _ if toggles.iter().any(|(n, _)| *n == e.name) => {
                    self.warnings.push(format!("VRM expression `{}` defined twice", e.name));
                }
                _ => toggles.push((e.name, e.binds)),
            }
        }
        out.face = (!face.visemes.is_empty() || !face.blink.is_empty()).then_some(face);
        if toggles.is_empty() {
            return Ok(());
        }

        let mut children = Vec::new();
        let mut params = Vec::new();
        let mut expr_params = Vec::new();
        let mut controls = Vec::new();
        let mut bits = 0;
        for (i, (name, binds)) in toggles.into_iter().enumerate() {
            let mut curves: Vec<Curve> = Vec::new();
            for b in binds {
                let m = &self.meshes[b.mesh as usize];
                let binding = Binding {
                    path: self.path(m.node)?,
                    property: Property::BlendShape(m.blend_shapes[b.shape as usize].name.clone()),
                };
                if curves.iter().any(|c| c.binding == binding) {
                    continue;
                }
                curves.push(Curve {
                    binding,
                    keys: vec![Key {
                        time: 0.0,
                        value: b.weight * 100.0,
                        in_tangent: f32::INFINITY,
                        out_tangent: f32::INFINITY,
                    }],
                });
            }
            children.push(ChildMotion {
                motion: Motion::Clip(clips.len() as ClipIndex),
                threshold: 0.0,
                position: [0.0; 2],
                direct_param: Some(i as ParamIndex),
                time_scale: 1.0,
                cycle_offset: 0.0,
            });
            clips.push(Clip {
                name: name.clone(),
                length: 1.0 / 60.0,
                looping: false,
                curves,
            });
            params.push(Parameter {
                name: name.clone(),
                kind: ParamKind::Float,
                default: 0.0,
            });
            // Beyond the synced budget expressions still work, just locally.
            let synced = bits + ExpressionParamKind::Float.bits() <= self.caps.synced_bits;
            if synced {
                bits += ExpressionParamKind::Float.bits();
            }
            expr_params.push(ExpressionParam {
                name: name.clone(),
                kind: ExpressionParamKind::Float,
                default: 0.0,
                saved: false,
                synced,
            });
            controls.push(Control {
                name: name.clone(),
                icon: None,
                kind: ControlKind::Toggle,
                parameter: Some(name),
                value: 1.0,
                sub_parameters: Vec::new(),
            });
        }
        let animator = Animator {
            parameters: params,
            groups: vec![LayerGroup {
                name: "FX".into(),
                weight: 1.0,
            }],
            layers: vec![Layer {
                name: "VRM Expressions".into(),
                group: 0,
                weight: 1.0,
                blending: LayerBlending::Override,
                mask: None,
                state_machine: StateMachine {
                    states: vec![State {
                        name: "Expressions".into(),
                        motion: Some(Motion::BlendTree(Box::new(BlendTree {
                            kind: BlendTreeKind::Direct,
                            children,
                            normalize: false,
                        }))),
                        speed: 1.0,
                        speed_param: None,
                        cycle_offset: 0.0,
                        cycle_offset_param: None,
                        time_param: None,
                        write_defaults: true,
                        transitions: Vec::new(),
                        behaviours: Vec::new(),
                    }],
                    default_state: 0,
                    entry_transitions: Vec::new(),
                    any_state_transitions: Vec::new(),
                },
            }],
        };
        out.animator = Some(animator);
        out.expressions = Some(Expressions {
            parameters: expr_params,
            menu: Menu { controls },
        });
        Ok(())
    }
}

/// Maps a VRM bone name to a humanoid bone. VRM 1.0 renamed the thumb: its metacarpal is Unity's proximal.
fn human_bone(name: &str, v1: bool) -> Option<HumanBone> {
    let renamed = match (v1, name) {
        (true, "leftThumbMetacarpal") => "LeftThumbProximal",
        (true, "leftThumbProximal") => "LeftThumbIntermediate",
        (true, "rightThumbMetacarpal") => "RightThumbProximal",
        (true, "rightThumbProximal") => "RightThumbIntermediate",
        _ => "",
    };
    let pascal = if renamed.is_empty() {
        let mut c = name.chars();
        c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str())?
    } else {
        renamed.to_owned()
    };
    serde_json::from_value(Value::String(pascal)).ok()
}

struct Params {
    stiffness: Graded,
    drag: Graded,
    gravity: Graded,
    radius: Graded,
}

/// A chain with the ADR 0006 mapping: pull from stiffness, momentum from drag, gravity from its vertical part.
fn chain(root: NodeIndex, ignore: Vec<NodeIndex>, endpoint: Vec3, p: &Params, colliders: Vec<u32>) -> Chain {
    let map = |g: &Graded, f: fn(f32) -> f32| {
        let value = f(g.value);
        // Curves scale the base value; recompute them so the mapped value at each point stays exact.
        let curve = g
            .curve
            .iter()
            .map(|&[d, k]| [d, if value != 0.0 { f(g.value * k) / value } else { 0.0 }])
            .collect();
        Graded { value, curve }
    };
    let constant = Graded::constant;
    Chain {
        root,
        ignore,
        endpoint,
        multi_child: MultiChild::First,
        integration: Integration::Simplified,
        pull: map(&p.stiffness, |s| (s / 4.0).clamp(0.0, 1.0)),
        spring: map(&p.drag, |d| (1.0 - d).clamp(0.0, 1.0)),
        stiffness: constant(0.0),
        gravity: map(&p.gravity, |g| g.clamp(-1.0, 1.0)),
        gravity_falloff: constant(0.0),
        immobile: constant(0.0),
        immobile_type: ImmobileType::AllMotion,
        limit: Limit::None,
        radius: p.radius.clone(),
        colliders,
        max_stretch: constant(0.0),
        max_squish: constant(0.0),
        allow_grab: true,
        allow_posing: true,
        parameter: None,
    }
}

/// Per-joint values as a graded value: the largest magnitude scaled by a curve over depth, at most `cap` points.
fn graded(values: &[f32], cap: usize) -> Graded {
    let max = values.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max == 0.0 || values.windows(2).all(|w| w[0] == w[1]) {
        return Graded::constant(values.first().copied().unwrap_or(0.0));
    }
    let n = values.len();
    let points = n.min(cap.max(2));
    let curve = (0..points)
        .map(|k| {
            let i = k * (n - 1) / (points - 1);
            [i as f32 / (n - 1) as f32, values[i] / max]
        })
        .collect();
    Graded { value: max, curve }
}

fn collider_list(groups: &Value, resolved: &[Vec<u32>]) -> Vec<u32> {
    let mut out: Vec<u32> = groups
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| resolved.get(index(g)?))
        .flatten()
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn sphere(node: NodeIndex, offset: Vec3, radius: f32) -> Collider {
    Collider {
        node,
        shape: Shape::Sphere { radius },
        position: offset,
        rotation: [0.0, 0.0, 0.0, 1.0],
        inside: false,
    }
}

/// A capsule from `a` to `b`; ours run along local Y, so it is centred and rotated onto that segment.
fn capsule(node: NodeIndex, a: Vec3, b: Vec3, radius: f32) -> Collider {
    let axis = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = dot(axis, axis).sqrt();
    if len < 1e-6 {
        return sphere(node, a, radius);
    }
    let dir = axis.map(|x| x / len);
    Collider {
        node,
        shape: Shape::Capsule {
            radius,
            height: len + 2.0 * radius,
        },
        position: [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0],
        rotation: from_to([0.0, 1.0, 0.0], dir),
        inside: false,
    }
}

fn index(v: &Value) -> Option<usize> {
    v.as_u64().and_then(|x| usize::try_from(x).ok())
}

fn num(v: &Value, default: f32) -> f32 {
    v.as_f64().map_or(default, |x| x as f32)
}

/// A JSON array of at least `N` numbers, else `default`.
fn arr<const N: usize>(v: &Value, default: [f32; N]) -> [f32; N] {
    match v.as_array() {
        Some(a) if a.len() >= N => std::array::from_fn(|i| num(&a[i], default[i])),
        _ => default,
    }
}

/// A VRM 0.x `{x, y, z}` object, else `default`.
fn xyz(v: &Value, default: Vec3) -> Vec3 {
    if v.is_object() {
        [
            num(&v["x"], default[0]),
            num(&v["y"], default[1]),
            num(&v["z"], default[2]),
        ]
    } else {
        default
    }
}

/// sRGB colour to linear; alpha stays as is.
fn srgb(c: [f32; 4]) -> [f32; 4] {
    let f = |x: f32| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn qmul(a: Quat, b: Quat) -> Quat {
    let (av, bv) = ([a[0], a[1], a[2]], [b[0], b[1], b[2]]);
    let c = cross(av, bv);
    [
        a[3] * b[0] + b[3] * a[0] + c[0],
        a[3] * b[1] + b[3] * a[1] + c[1],
        a[3] * b[2] + b[3] * a[2] + c[2],
        a[3] * b[3] - dot(av, bv),
    ]
}

fn conj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let r = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), conj(q));
    [r[0], r[1], r[2]]
}

/// The shortest rotation taking unit vector `a` to unit vector `b`.
fn from_to(a: Vec3, b: Vec3) -> Quat {
    let d = dot(a, b);
    if d < -0.999_999 {
        // Opposite: half a turn about any axis perpendicular to `a`.
        let axis = if a[0].abs() < 0.9 {
            cross(a, [1.0, 0.0, 0.0])
        } else {
            cross(a, [0.0, 1.0, 0.0])
        };
        let l = dot(axis, axis).sqrt();
        return [axis[0] / l, axis[1] / l, axis[2] / l, 0.0];
    }
    let c = cross(a, b);
    let q = [c[0], c[1], c[2], 1.0 + d];
    let l = (q.iter().map(|x| x * x).sum::<f32>()).sqrt();
    q.map(|x| x / l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vrm1_thumb_names_shift_one_segment() {
        assert_eq!(
            human_bone("leftThumbMetacarpal", true),
            Some(HumanBone::LeftThumbProximal)
        );
        assert_eq!(
            human_bone("leftThumbProximal", false),
            Some(HumanBone::LeftThumbProximal)
        );
        assert_eq!(human_bone("upperChest", true), Some(HumanBone::UpperChest));
        assert_eq!(human_bone("tail", true), None);
    }

    #[test]
    fn rotations() {
        let q = from_to([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        let r = rotate(q, [0.0, 1.0, 0.0]);
        assert!((r[0] - 1.0).abs() < 1e-6 && r[1].abs() < 1e-6, "{r:?}");
        let flip = from_to([0.0, 1.0, 0.0], [0.0, -1.0, 0.0]);
        assert!((rotate(flip, [0.0, 1.0, 0.0])[1] + 1.0).abs() < 1e-6);
        assert_eq!(rotate([0.0, 1.0, 0.0, 0.0], [1.0, 2.0, 3.0]), [-1.0, 2.0, -3.0]);
    }

    #[test]
    fn graded_keeps_per_joint_values() {
        let g = graded(&[0.5, 1.0, 0.25], 64);
        assert_eq!(g.value, 1.0);
        assert_eq!(g.curve, [[0.0, 0.5], [0.5, 1.0], [1.0, 0.25]]);
        assert_eq!(graded(&vec![1.0; 100], 64), Graded::constant(1.0));
        assert_eq!(
            graded(&(0..100).map(|i| i as f32).collect::<Vec<_>>(), 8).curve.len(),
            8
        );
    }
}
