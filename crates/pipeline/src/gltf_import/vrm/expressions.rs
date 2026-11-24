//! VRM expressions to visemes, blink and a Direct blend tree of toggles, and look-at to eye look.

use civbb_format::animator::*;
use civbb_format::expressions::{Control, ControlKind, ExpressionParam, ExpressionParamKind, Expressions, Menu};
use civbb_format::face::{EyeDriver, EyeLook, Face, RangeMap, ShapeBind, Viseme};
use civbb_format::hierarchy::{HumanBone, Humanoid};
use serde_json::Value;

use super::{Vrm, index, num};
use crate::gltf_import::Importer;

/// One VRM expression after resolving its binds.
pub(super) struct Expression {
    name: String,
    preset: Option<String>,
    binds: Vec<ShapeBind>,
}

impl Importer<'_> {
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

    pub(super) fn expressions_v0(&mut self, groups: &Value) -> Vec<Expression> {
        let mut out = Vec::new();
        for g in groups.as_array().into_iter().flatten() {
            let mut binds = Vec::new();
            for b in g["binds"].as_array().into_iter().flatten() {
                // 0.x binds name a glTF mesh; every node instancing it gets the bind.
                let targets = self.meshes_of(index(&b["mesh"]));
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

    pub(super) fn expressions_v1(&mut self, e: &Value) -> Vec<Expression> {
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
    /// Returns the look up, down, left and right binds for eye look.
    pub(super) fn expression_parts(
        &mut self,
        exprs: Vec<Expression>,
        clips: &mut Vec<Clip>,
        out: &mut Vrm,
    ) -> Result<[Vec<ShapeBind>; 4], String> {
        let mut face = Face::default();
        let mut look: [Vec<ShapeBind>; 4] = Default::default();
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
                // One-eye blinks have no format field yet.
                (None, Some("blink_l" | "blink_r" | "blinkleft" | "blinkright")) => {}
                (None, Some("lookup")) => look[0] = e.binds,
                (None, Some("lookdown")) => look[1] = e.binds,
                (None, Some("lookleft")) => look[2] = e.binds,
                (None, Some("lookright")) => look[3] = e.binds,
                _ if e.binds.is_empty() => {}
                _ if toggles.iter().any(|(n, _)| *n == e.name) => {
                    self.warnings.push(format!("VRM expression `{}` defined twice", e.name));
                }
                _ => toggles.push((e.name, e.binds)),
            }
        }
        out.face = (!face.visemes.is_empty() || !face.blink.is_empty()).then_some(face);
        if toggles.is_empty() {
            return Ok(look);
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
                mirror: false,
                cycle_offset: 0.0,
            });
            clips.push(Clip {
                name: name.clone(),
                length: 1.0 / 60.0,
                looping: false,
                root: None,
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
                        mirror: false,
                        mirror_param: None,
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
        Ok(look)
    }

    /// Eye look from `lookAt` (1.0) or `firstPerson.lookAt*` (0.x); `look` holds the up, down, left and right
    /// expression binds for blendshape eye look.
    pub(super) fn eye_look(
        &mut self,
        root: &Value,
        v1: bool,
        humanoid: &Humanoid,
        look: [Vec<ShapeBind>; 4],
    ) -> Option<EyeLook> {
        let (kind, maps, names) = if v1 {
            let la = &root["lookAt"];
            let names = [
                "rangeMapHorizontalInner",
                "rangeMapHorizontalOuter",
                "rangeMapVerticalDown",
                "rangeMapVerticalUp",
            ];
            (la["type"].as_str()?.to_ascii_lowercase(), la, names)
        } else {
            let fp = &root["firstPerson"];
            let names = [
                "lookAtHorizontalInner",
                "lookAtHorizontalOuter",
                "lookAtVerticalDown",
                "lookAtVerticalUp",
            ];
            (fp["lookAtTypeName"].as_str()?.to_ascii_lowercase(), fp, names)
        };
        let bones = kind == "bone";
        if !bones && kind != "expression" && kind != "blendshape" {
            self.warnings.push(format!("VRM eye look type `{kind}` not supported"));
            return None;
        }
        // VRM defaults: 90 degrees in, 10 degrees (bones) or full weight (expressions) out.
        let default_out = if bones { 10.0 } else { 1.0 };
        let [inner, outer, down, up] = names.map(|n| {
            let m = &maps[n];
            let linear = |c: &Vec<Value>| c.iter().map(Value::as_f64).eq(LINEAR_CURVE.map(Some));
            if !v1 && m["curve"].as_array().is_some_and(|c| !linear(c)) {
                self.warnings.push("VRM eye look curve treated as linear".into());
            }
            let (input, output) = if v1 {
                ("inputMaxValue", "outputScale")
            } else {
                ("xRange", "yRange")
            };
            RangeMap {
                input_max: num(&m[input], 90.0),
                output_scale: num(&m[output], default_out),
            }
        });
        let driver = if bones {
            let (left, right) = (humanoid.node(HumanBone::LeftEye), humanoid.node(HumanBone::RightEye));
            if left.is_none() && right.is_none() {
                self.warnings.push("VRM bone eye look without eye bones skipped".into());
                return None;
            }
            EyeDriver::Bones { left, right }
        } else {
            if look.iter().all(Vec::is_empty) {
                self.warnings
                    .push("VRM expression eye look without look expressions skipped".into());
                return None;
            }
            let [up, down, left, right] = look;
            EyeDriver::Shapes { up, down, left, right }
        };
        Some(EyeLook {
            driver,
            horizontal_inner: inner,
            horizontal_outer: outer,
            vertical_down: down,
            vertical_up: up,
        })
    }
}

/// UniVRM's default lookAt curve, a straight line: `[time, value, in, out]` for two keys.
const LINEAR_CURVE: [f64; 8] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0];
