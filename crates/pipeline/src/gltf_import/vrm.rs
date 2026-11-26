//! VRM 0.x (`VRM`) and 1.0 (`VRMC_vrm`, `VRMC_springBone`, `VRMC_materials_mtoon`, `VRMC_node_constraint`) to native
//! types (ADRs 0006, 0018).
//!
//! Runs after the glTF import and reads the extension JSON tolerantly: a missing or malformed field takes the
//! VRM default, an unresolvable reference is skipped with a warning. Validation still checks the result.

use civbb_core::math::{add, conj, cross, dot};
use civbb_format::animator::*;
use civbb_format::constraint::{Constraint, ConstraintKind, ConstraintSource};
use civbb_format::dynamics::Dynamics;
use civbb_format::expressions::Expressions;
use civbb_format::face::Face;
use civbb_format::hierarchy::{HumanBone, Humanoid, NodeIndex};
use civbb_format::math::{Quat, Transform, Vec3};
use civbb_format::mesh::FirstPerson;
use civbb_format::meta::Meta;
use civbb_format::state::StateTable;
use serde_json::Value;

use super::Importer;
use meta::meta;

mod expressions;
mod meta;
mod mtoon;
mod springs;

/// The avatar parts VRM adds to a plain glTF import.
pub(super) struct Vrm {
    pub name: Option<String>,
    pub humanoid: Humanoid,
    pub viewpoint: Vec3,
    pub face: Option<Face>,
    pub dynamics: Dynamics,
    pub animator: Option<Animator>,
    pub expressions: Option<Expressions>,
    pub state: StateTable,
    pub meta: Option<Meta>,
    pub constraints: Vec<Constraint>,
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
        let (humanoid, viewpoint) = self.vrm_humanoid(root, v1);
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
        self.first_person(&root["firstPerson"]["meshAnnotations"], v1);
        let constraints = self.node_constraints();
        let title = if v1 {
            &root["meta"]["name"]
        } else {
            &root["meta"]["title"]
        };
        let mut vrm = Vrm {
            name: title.as_str().filter(|s| !s.is_empty()).map(str::to_owned),
            humanoid,
            viewpoint,
            face: None,
            dynamics,
            animator: None,
            expressions: None,
            state: StateTable::default(),
            meta: meta(&root["meta"], v1),
            constraints,
        };
        let look = self.expression_parts(exprs, clips, &mut vrm)?;
        if let Some(eyes) = self.eye_look(root, v1, &vrm.humanoid, look) {
            vrm.face.get_or_insert_with(Face::default).eyes = Some(eyes);
        }
        Ok(Some(vrm))
    }

    /// `VRMC_node_constraint` on nodes: roll, aim and rotation as rest-relative constraints with one source.
    fn node_constraints(&mut self) -> Vec<Constraint> {
        let doc = self.doc;
        let mut out = Vec::new();
        for n in doc.nodes() {
            let Some(ext) = n.extension_value("VRMC_node_constraint") else {
                continue;
            };
            let Some(target) = self.map.get(n.index()).copied().flatten() else {
                continue;
            };
            let c = &ext["constraint"];
            let (kind, body) = if let Some(b) = c.get("rotation") {
                (Some(ConstraintKind::RestRotation), b)
            } else if let Some(b) = c.get("roll") {
                (axis(&b["rollAxis"]).map(|axis| ConstraintKind::RestRoll { axis }), b)
            } else if let Some(b) = c.get("aim") {
                (axis(&b["aimAxis"]).map(|axis| ConstraintKind::RestAim { axis }), b)
            } else {
                (None, c)
            };
            let source = self.vrm_node(&body["source"]);
            let (Some(kind), Some(source)) = (kind, source.filter(|&s| s != target)) else {
                self.warnings
                    .push(format!("node {}: unusable VRMC_node_constraint skipped", n.index()));
                continue;
            };
            if out.len() == self.caps.constraints {
                self.warnings
                    .push("VRMC_node_constraint: over the constraint cap, rest skipped".into());
                break;
            }
            out.push(Constraint {
                kind,
                target,
                sources: vec![ConstraintSource {
                    node: source,
                    weight: 1.0,
                    offset: Transform::IDENTITY,
                }],
                weight: num(&body["weight"], 1.0).clamp(0.0, 1.0),
                active: true,
                axes: [true; 3],
                rest: self.nodes[target as usize].rest,
                options: Default::default(),
            });
        }
        out
    }

    fn vrm_node(&self, v: &Value) -> Option<NodeIndex> {
        index(v).and_then(|i| self.map.get(i).copied().flatten())
    }

    /// The humanoid map and the viewpoint.
    fn vrm_humanoid(&mut self, root: &Value, v1: bool) -> (Humanoid, Vec3) {
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
        let viewpoint = match bone {
            Some(b) => {
                let (t, r, s) = world[b as usize];
                add(t, rotate(r, [offset[0] * s[0], offset[1] * s[1], offset[2] * s[2]]))
            }
            None => {
                self.warnings.push("VRM has no head bone; viewpoint at origin".into());
                [0.0; 3]
            }
        };
        (h, viewpoint)
    }

    /// World translation, rotation and scale of every node (scale and rotation composed per axis, which is exact
    /// without non-uniform scale under rotation).
    pub(super) fn world(&self) -> Vec<(Vec3, Quat, Vec3)> {
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

    /// Every format mesh instancing glTF mesh `mesh` (VRM 0.x names meshes, not nodes).
    fn meshes_of(&self, mesh: Option<usize>) -> Vec<usize> {
        let Some(mesh) = mesh else {
            return Vec::new();
        };
        self.doc
            .nodes()
            .filter(|n| n.mesh().map(|m| m.index()) == Some(mesh))
            .filter_map(|n| self.node_meshes.get(&self.map[n.index()]?).copied())
            .collect()
    }

    /// First-person visibility per mesh (`firstPerson.meshAnnotations`).
    fn first_person(&mut self, list: &Value, v1: bool) {
        for a in list.as_array().into_iter().flatten() {
            let flag = if v1 { &a["type"] } else { &a["firstPersonFlag"] };
            let fp = match flag.as_str().map(str::to_ascii_lowercase).as_deref() {
                Some("auto") => FirstPerson::Auto,
                Some("both") => FirstPerson::Both,
                Some("firstpersononly") => FirstPerson::FirstPersonOnly,
                Some("thirdpersononly") => FirstPerson::ThirdPersonOnly,
                _ => {
                    self.warnings
                        .push("VRM first-person annotation with an unknown type skipped".into());
                    continue;
                }
            };
            let meshes = if v1 {
                let mesh = self
                    .vrm_node(&a["node"])
                    .and_then(|n| self.node_meshes.get(&n).copied());
                mesh.into_iter().collect()
            } else {
                self.meshes_of(index(&a["mesh"]))
            };
            if meshes.is_empty() {
                self.warnings
                    .push("VRM first-person annotation of a missing mesh skipped".into());
            }
            for m in meshes {
                self.meshes[m].first_person = fp;
            }
        }
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

fn index(v: &Value) -> Option<usize> {
    v.as_u64().and_then(|x| usize::try_from(x).ok())
}

/// A VRM axis name (`X`, `PositiveX`, `NegativeY` ...) as a unit vector.
fn axis(v: &Value) -> Option<Vec3> {
    let s = v.as_str()?;
    let (sign, name) = match s.strip_prefix("Negative") {
        Some(rest) => (-1.0, rest),
        None => (1.0, s.strip_prefix("Positive").unwrap_or(s)),
    };
    let k = ["X", "Y", "Z"].iter().position(|a| *a == name)?;
    let mut out = [0.0; 3];
    out[k] = sign;
    Some(out)
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

// `qmul`, `rotate` and `from_to` stay apart from `civbb_core::math`: core's round differently (imported VRM values
// change in the last bit) and core's `from_to` turns a non-finite input into the identity where this one stays NaN
// and validation rejects the collider. Switching is a deliberate change with re-baselined imports.

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
}
