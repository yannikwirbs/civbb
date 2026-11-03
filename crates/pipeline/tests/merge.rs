//! Mesh merging (ADR 0027): meshes always visible together merge and same-material submeshes join, everything
//! else stays apart; merged avatars stay valid and show the same triangles, materials and shape weights in every
//! toggle state as before, on the corpus and on local avatars.

use std::cmp::Ordering;
use std::sync::Arc;

use civbb_client::{Island, Model, frame};
use civbb_core::math::Pose;
use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::face::{Face, ShapeBind};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{AlphaMode, Material, MaterialModel, Unlit};
use civbb_format::math::{Mat4, Quat, Transform, Vec3};
use civbb_format::mesh::{BlendShape, FirstPerson, Mesh, ShapeDelta, Skin, Submesh};
use civbb_format::{Avatar, Caps, validate};
use civbb_pipeline::gltf_import::{Limits, dir_resolver, import};
use civbb_pipeline::{Merged, merge, prune};

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn valid(a: &Avatar) {
    if let Err(e) = validate(a, &Caps::default(), |_| true) {
        panic!("{e:?}");
    }
}

/// Root; Hips (a joint); Toggle (animated) with ShirtA (skinned, shape `Lift`) and ShirtB (rigid, shape `Fold`,
/// animated); Body (skinned, `Smile`, face-bound); Hat (rigid, also `Smile`); Glow (animated material); Veil
/// (blended); Hidden (inactive, never animated); Visor (third person only). Every mesh draws material 0 twice
/// except Veil.
fn avatar() -> Avatar {
    let node = |name: &str, parent: Option<u32>, translation: Vec3, rotation: Quat, active: bool| Node {
        name: name.into(),
        parent,
        rest: Transform {
            translation,
            rotation,
            scale: [1.0; 3],
        },
        active,
    };
    let still = [0.0, 0.0, 0.0, 1.0];
    let turned = [0.0, 0.382_683_43, 0.0, 0.923_879_5];
    let mesh = |name: &str, node: u32, skinned: bool, shape: Option<&str>, material: u32| Mesh {
        name: name.into(),
        node,
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.5], [1.0, 1.0, 0.5]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        skin: skinned.then(|| Skin {
            joints: vec![1],
            inverse_bind: vec![IDENTITY],
            vertex_joints: vec![[0; 4]; 4],
            vertex_weights: vec![[1.0, 0.0, 0.0, 0.0]; 4],
        }),
        indices: vec![0, 1, 2, 2, 1, 3],
        submeshes: vec![
            Submesh {
                first_index: 0,
                index_count: 3,
                material,
            },
            Submesh {
                first_index: 3,
                index_count: 3,
                material,
            },
        ],
        blend_shapes: shape
            .into_iter()
            .map(|s| BlendShape {
                name: s.into(),
                default_weight: 0.5,
                deltas: vec![ShapeDelta {
                    vertex: 3,
                    position: [0.0, 0.0, 0.25],
                    normal: [0.0; 3],
                }],
            })
            .collect(),
        ..Mesh::default()
    };
    let material = |alpha| Material {
        name: String::new(),
        model: MaterialModel::Unlit(Unlit {
            color: [1.0; 4],
            color_tex: None,
        }),
        alpha,
        double_sided: false,
        render_queue: 2000,
    };
    let curve = |path: &str, property| Curve {
        binding: Binding {
            path: path.into(),
            property,
        },
        keys: vec![Key {
            time: 0.0,
            value: 1.0,
            in_tangent: 0.0,
            out_tangent: 0.0,
        }],
    };
    let mut visor = mesh("Visor", 10, false, None, 0);
    visor.first_person = FirstPerson::ThirdPersonOnly;
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![
                node("Root", None, [0.0; 3], still, true),
                node("Hips", Some(0), [0.0, 1.0, 0.0], turned, true),
                node("Toggle", Some(0), [0.0; 3], still, true),
                node("ShirtA", Some(2), [0.0; 3], still, true),
                node("ShirtB", Some(2), [0.3, 0.2, 0.1], turned, true),
                node("Body", Some(0), [0.0; 3], still, true),
                node("Hat", Some(1), [0.0, 0.5, 0.0], turned, true),
                node("Glow", Some(0), [0.0; 3], still, true),
                node("Veil", Some(0), [0.0; 3], still, true),
                node("Hidden", Some(0), [0.0; 3], still, false),
                node("Visor", Some(0), [0.0; 3], still, true),
            ],
        },
        meshes: vec![
            mesh("ShirtA", 3, true, Some("Lift"), 0),
            mesh("ShirtB", 4, false, Some("Fold"), 0),
            mesh("Body", 5, true, Some("Smile"), 0),
            mesh("Hat", 6, false, Some("Smile"), 0),
            mesh("Glow", 7, false, None, 0),
            mesh("Veil", 8, false, None, 1),
            mesh("Hidden", 9, false, None, 0),
            visor,
        ],
        materials: vec![material(AlphaMode::Opaque), material(AlphaMode::Blend)],
        face: Some(Face {
            blink: vec![ShapeBind {
                mesh: 1,
                shape: 0,
                weight: 1.0,
            }],
            ..Face::default()
        }),
        clips: vec![Clip {
            name: "c".into(),
            length: 1.0,
            looping: false,
            curves: vec![
                curve("Toggle", Property::Active),
                curve("Toggle/ShirtB", Property::BlendShape("Fold".into())),
                // ShirtB has no `Lift`: unbound before, and it must not reach ShirtA's after.
                curve("Toggle/ShirtB", Property::BlendShape("Lift".into())),
                curve("Glow", Property::Material("_Color.r".into())),
            ],
        }],
        ..Avatar::default()
    }
}

#[test]
fn merges_meshes_that_show_together_and_keeps_the_rest_apart() {
    let mut a = avatar();
    valid(&a);
    let before = a.clone();
    let merged = merge(&mut a);
    valid(&a);
    // ShirtB joins ShirtA (same toggle), their four material-0 submeshes become one; Body, Hat and Visor each join
    // nothing but lose a submesh; Glow (animated material), Veil (blended) and Hidden (never shown) are untouched.
    assert_eq!(
        merged,
        Merged {
            meshes: 1,
            submeshes: 6
        }
    );
    let names: Vec<_> = a.meshes.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["ShirtA", "Body", "Hat", "Glow", "Veil", "Hidden", "Visor"]);
    let shirt = &a.meshes[0];
    assert_eq!((shirt.node, shirt.positions.len(), shirt.indices.len()), (3, 8, 12));
    assert_eq!(
        shirt.submeshes,
        [Submesh {
            first_index: 0,
            index_count: 12,
            material: 0
        }]
    );
    // The rigid shirt is skinned to its own node with an identity inverse bind.
    let skin = shirt.skin.as_ref().unwrap();
    assert_eq!(skin.joints, [1, 4]);
    assert_eq!(skin.inverse_bind, [IDENTITY; 2]);
    assert_eq!(skin.vertex_joints[4..], [[1; 4]; 4]);
    let shapes: Vec<_> = shirt
        .blend_shapes
        .iter()
        .map(|s| (s.name.as_str(), s.deltas[0].vertex))
        .collect();
    assert_eq!(shapes, [("Lift", 3), ("Fold", 7)]);
    // Fold's curve follows it to ShirtA's path; the unbound Lift curve stays where it was.
    let paths: Vec<_> = a.clips[0].curves.iter().map(|c| c.binding.path.as_str()).collect();
    assert_eq!(paths, ["Toggle", "Toggle/ShirtA", "Toggle/ShirtB", "Glow"]);
    assert_eq!(a.meshes[3..6], before.meshes[4..7]);
    assert_eq!(a.face.as_ref().unwrap().blink[0].mesh, 0);
    assert_eq!(a.face.as_ref().unwrap().blink[0].shape, 1);
    let bound = |a: &Avatar| civbb_anim::bind(a).unbound.len();
    assert_eq!(bound(&a), bound(&before));
    same_look(&before, &a, "hand-built");

    // A second pass finds nothing.
    assert_eq!(merge(&mut a), Merged::default());

    // A curve on ShirtA's path naming ShirtB's shape is unbound; merging would bind it, so they stay apart.
    let mut a = before.clone();
    let mut c = a.clips[0].curves[1].clone();
    c.binding.path = "Toggle/ShirtA".into();
    a.clips[0].curves.push(c);
    assert_eq!(merge(&mut a).meshes, 0);
}

#[test]
fn corpus_avatars_look_the_same() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let mut a = civbb_corpus::generate(&spec);
        prune(&mut a);
        let before = a.clone();
        let merged = merge(&mut a);
        valid(&a);
        eprintln!("{}: {merged:?}", spec.name);
        same_look(&before, &a, &spec.name);
    }
}

/// Builds every local avatar in `corpus/local/glb/` and `corpus/local/vrm/` (gitignored) as `civbb build` does
/// and checks it looks the same before and after merging. Skips what is absent.
#[test]
fn local_avatars_look_the_same() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local");
    for sub in ["glb", "vrm"] {
        let Ok(entries) = std::fs::read_dir(root.join(sub)) else {
            eprintln!("skipped: corpus/local/{sub} not present");
            continue;
        };
        for f in entries.flatten().map(|e| e.path()) {
            if !f.extension().is_some_and(|x| x == "glb" || x == "vrm") {
                continue;
            }
            let bytes = std::fs::read(&f).unwrap();
            let resolve = dir_resolver(f.parent().unwrap(), Limits::default().bytes);
            let mut a = import(&bytes, &Limits::default(), resolve).unwrap().avatar;
            prune(&mut a);
            let before = a.clone();
            let merged = merge(&mut a);
            valid(&a);
            let name = f.file_name().unwrap().to_string_lossy();
            eprintln!("{name}: {merged:?}");
            same_look(&before, &a, &name);
        }
    }
}

/// Steps both avatars through the same states, the rest pose, then each parameter set away from its default in
/// turn, and compares what they show.
fn same_look(before: &Avatar, after: &Avatar, name: &str) {
    let island = |a: &Avatar| Island::new(Arc::new(Model::new(a)), 1, true, Pose::IDENTITY);
    let (mut b, mut a) = (island(before), island(after));
    let params: Vec<(String, f32)> = before
        .animator
        .iter()
        .flat_map(|x| &x.parameters)
        .map(|p| (p.name.clone(), if p.default == 0.0 { 1.0 } else { 0.0 }))
        .collect();
    for state in 0..=params.len() {
        for island in [&mut b, &mut a] {
            if state > 0 {
                let (p, v) = &params[state - 1];
                let i = island.model().controller().unwrap().param(p).unwrap();
                island.animator_mut().unwrap().params[i as usize] = *v;
            }
            for _ in 0..3 {
                frame::step(std::slice::from_mut(island), |_, _| true, 1.0 / 60.0);
            }
        }
        let shown = look(before, &b);
        assert!(shown == look(after, &a), "{name}: state {state} differs");
        let (mut mb, mut ma) = (Vec::new(), Vec::new());
        b.materials(&mut mb);
        a.materials(&mut ma);
        assert_eq!(mb, ma, "{name}: state {state} animated materials differ");
    }
}

/// What an avatar shows: every visible triangle with its material, its vertices in world space after shape
/// weights and skinning, sorted.
fn look(avatar: &Avatar, island: &Island) -> Vec<(u32, [[f32; 3]; 3])> {
    let mut weights = Vec::new();
    island.shape_weights(&mut weights);
    let world = island.world();
    let mut out = Vec::new();
    for (mesh, weights) in avatar.meshes.iter().zip(&weights) {
        if !island.active()[mesh.node as usize] {
            continue;
        }
        let mut pos = mesh.positions.clone();
        for (s, &w) in mesh.blend_shapes.iter().zip(weights) {
            if w != 0.0 {
                for d in &s.deltas {
                    (0..3).for_each(|k| pos[d.vertex as usize][k] += w * d.position[k]);
                }
            }
        }
        let placed: Vec<Vec3> = match &mesh.skin {
            Some(s) if !s.joints.is_empty() => pos
                .iter()
                .zip(&s.vertex_joints)
                .zip(&s.vertex_weights)
                .map(|((p, js), ws)| {
                    let mut v = [0.0; 3];
                    for (&j, &w) in js.iter().zip(ws).filter(|(_, w)| **w != 0.0) {
                        let q = world[s.joints[j as usize] as usize].point(transform(&s.inverse_bind[j as usize], *p));
                        (0..3).for_each(|k| v[k] += w * q[k]);
                    }
                    v
                })
                .collect(),
            _ => pos.iter().map(|p| world[mesh.node as usize].point(*p)).collect(),
        };
        // Signed zeros compare equal but sort apart.
        let placed: Vec<Vec3> = placed.iter().map(|p| p.map(|x| x + 0.0)).collect();
        for s in &mesh.submeshes {
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            for t in mesh.indices[range].chunks(3) {
                out.push((s.material, std::array::from_fn(|k| placed[t[k] as usize])));
            }
        }
    }
    out.sort_by(|x, y| {
        x.0.cmp(&y.0).then_with(|| {
            x.1.as_flattened()
                .iter()
                .zip(y.1.as_flattened())
                .map(|(a, b)| a.total_cmp(b))
                .find(|o| *o != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
    });
    out
}

/// A column-major matrix applied to a point.
fn transform(m: &Mat4, p: Vec3) -> Vec3 {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}
