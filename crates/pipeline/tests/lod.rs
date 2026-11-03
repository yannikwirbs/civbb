//! LOD chain (ADR 0031): levels fall in triangles within their error caps, keep submesh borders and UV seams, keep
//! hard skin and shape splits, are deterministic, leave the full mesh looking the same, and skin like the full mesh
//! at their kept vertices, on a hand-built grid, the corpus and local avatars.

mod common;

use std::collections::{HashMap, HashSet};

use civbb_format::Avatar;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{AlphaMode, Material, MaterialModel, Unlit};
use civbb_format::math::{Mat4, Transform};
use civbb_format::mesh::{BlendShape, Mesh, ShapeDelta, Skin, Submesh};
use civbb_pipeline::gltf_import::{Limits, dir_resolver, import};
use civbb_pipeline::lod::{KEEP, LEVELS};
use civbb_pipeline::{Lods, lod, merge, prune};
use common::{placed, same_look, states, valid};

/// Grid cells per side, each 0.1 wide.
const CELLS: u32 = 16;

fn translation(t: [f32; 3]) -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t[0], t[1], t[2], 1.0,
    ]
}

/// A flat 16 by 16 grid: rows below 8 draw material 0, the rest material 1; a UV seam at column 4 (its vertices
/// doubled); columns up to 7 skinned to joint A, from 8 to joint B; shape `Wave` lifts columns from 12.
fn grid() -> Avatar {
    let side = CELLS + 1;
    let node = |name: &str, parent, x: f32| Node {
        name: name.into(),
        parent,
        rest: Transform {
            translation: [x, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        active: true,
    };
    let mut m = Mesh {
        name: "Grid".into(),
        ..Mesh::default()
    };
    let mut skin = Skin {
        joints: vec![1, 2],
        inverse_bind: vec![translation([-0.4, 0.0, 0.0]), translation([-1.2, 0.0, 0.0])],
        ..Skin::default()
    };
    let mut push = |m: &mut Mesh, x: u32, y: u32, u: f32| {
        m.positions.push([x as f32 * 0.1, y as f32 * 0.1, 0.0]);
        m.normals.push([0.0, 0.0, 1.0]);
        m.uv0.push([u, y as f32 / CELLS as f32]);
        skin.vertex_joints.push([u16::from(x >= 8), 0, 0, 0]);
        skin.vertex_weights.push([1.0, 0.0, 0.0, 0.0]);
    };
    for y in 0..side {
        for x in 0..side {
            push(&mut m, x, y, x as f32 / CELLS as f32);
        }
    }
    // The seam: column 4 again, with other UVs, used by the cells to its right.
    for y in 0..side {
        push(&mut m, 4, y, 0.75);
    }
    m.skin = Some(skin);
    let at = |x: u32, y: u32, right_of: u32| {
        if x == 4 && right_of >= 4 {
            side * side + y
        } else {
            y * side + x
        }
    };
    for (material, rows) in [(0, 0..8), (1, 8..CELLS)] {
        let first_index = m.indices.len() as u32;
        for y in rows {
            for x in 0..CELLS {
                let [a, b, c, d] = [at(x, y, x), at(x + 1, y, x), at(x, y + 1, x), at(x + 1, y + 1, x)];
                m.indices.extend([a, b, c, c, b, d]);
            }
        }
        m.submeshes.push(Submesh {
            first_index,
            index_count: m.indices.len() as u32 - first_index,
            material,
        });
    }
    m.blend_shapes.push(BlendShape {
        name: "Wave".into(),
        default_weight: 1.0,
        deltas: (0..side * side)
            .filter(|v| v % side >= 12)
            .map(|vertex| ShapeDelta {
                vertex,
                position: [0.0, 0.0, 0.3],
                normal: [0.0; 3],
            })
            .collect(),
    });
    let material = Material {
        name: String::new(),
        model: MaterialModel::Unlit(Unlit {
            color: [1.0; 4],
            color_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        render_queue: 2000,
    };
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![
                node("Root", None, 0.0),
                node("A", Some(0), 0.4),
                node("B", Some(0), 1.2),
            ],
        },
        meshes: vec![m],
        materials: vec![material.clone(), material],
        ..Avatar::default()
    }
}

#[test]
fn grid_keeps_borders_seams_and_splits() {
    let mut a = grid();
    valid(&a);
    let before = a.clone();
    let l = lod(&mut a);
    valid(&a);
    eprintln!("grid: {l:?}");
    let m = &a.meshes[0];
    assert_eq!(m.lods.len(), LEVELS.len());
    chain(&a, &l, "grid");
    let skin = m.skin.as_ref().unwrap();
    let moved: HashSet<u32> = m.blend_shapes[0].deltas.iter().map(|d| d.vertex).collect();
    for (k, level) in m.lods.iter().enumerate() {
        for &t in level.indices.as_chunks::<3>().0 {
            let xs = t.map(|v| m.positions[v as usize][0]);
            let span = |lo: f32, hi: f32| xs.iter().all(|&x| x > lo - 1e-4 && x < hi + 1e-4);
            // A triangle mixing joints, or moved and unmoved vertices, stays in the one column of cells between.
            let joints = t.map(|v| skin.vertex_joints[v as usize][0]);
            if joints.iter().any(|&j| j != joints[0]) {
                assert!(span(0.7, 0.8), "level {}: {xs:?} mixes joints", k + 1);
            }
            let lifted = t.map(|v| moved.contains(&v));
            if lifted.iter().any(|&s| s != lifted[0]) {
                assert!(span(1.1, 1.2), "level {}: {xs:?} mixes shape motion", k + 1);
            }
        }
    }
    same_look(&before, &a, "grid");
    skins_alike(&a, "grid");
    let mut again = before.clone();
    lod(&mut again);
    assert!(again == a, "not deterministic");
}

#[test]
fn corpus_avatars_keep_their_look() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let mut a = civbb_corpus::generate(&spec);
        prune(&mut a);
        merge(&mut a);
        check(a, &spec.name);
    }
}

/// Builds every local avatar in `corpus/local/glb/` and `corpus/local/vrm/` (gitignored) as `civbb build` does up to
/// the chain and checks it. Skips what is absent.
#[test]
fn local_avatars_keep_their_look() {
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
            merge(&mut a);
            check(a, &f.file_name().unwrap().to_string_lossy());
        }
    }
}

/// Builds the chain on a pruned, merged avatar and checks it.
fn check(mut a: Avatar, name: &str) {
    let before = a.clone();
    let l = lod(&mut a);
    valid(&a);
    eprintln!("{name}: {l:?}");
    chain(&a, &l, name);
    same_look(&before, &a, name);
    skins_alike(&a, name);
}

/// Errors within their caps, triangles falling by at least [`KEEP`] per level, and per submesh, with vertices welded
/// across UV seams: every level still draws each edge the full mesh has open (material boundaries, open edges) and
/// opens no other, except where three or more triangles shared it (seams and closed surfaces stay closed; ADR 0031).
fn chain(a: &Avatar, l: &Lods, name: &str) {
    for (k, (_, cap)) in LEVELS.iter().enumerate() {
        assert!(
            l.error[k] <= cap * 1.0001,
            "{name}: level {} error {}",
            k + 1,
            l.error[k]
        );
        assert!(
            l.triangles[k + 1] <= l.triangles[k],
            "{name}: triangles {:?}",
            l.triangles
        );
    }
    for (mi, m) in a.meshes.iter().enumerate() {
        let full = edges(m, &m.indices, &m.submeshes);
        let mut above = m.indices.len();
        for (k, level) in m.lods.iter().enumerate() {
            assert!(
                level.indices.len() as f32 <= KEEP * above as f32,
                "{name}: mesh {mi} level {}",
                k + 1
            );
            for (f, now) in full.iter().zip(edges(m, &level.indices, &level.submeshes)) {
                let opened = now.open.difference(&f.open).filter(|e| !f.shared.contains(e)).count();
                assert!(
                    f.open.is_subset(&now.all),
                    "{name}: mesh {mi} level {} lost a border edge",
                    k + 1
                );
                assert!(opened == 0, "{name}: mesh {mi} level {} opened {opened} edges", k + 1);
            }
            above = level.indices.len();
        }
    }
}

/// A submesh's edges between vertices welded by position.
struct Edges {
    /// Used by one triangle.
    open: HashSet<(u32, u32)>,
    /// Used by three or more.
    shared: HashSet<(u32, u32)>,
    all: HashSet<(u32, u32)>,
}

fn edges(m: &Mesh, indices: &[u32], submeshes: &[Submesh]) -> Vec<Edges> {
    let mut first: HashMap<[u32; 3], u32> = HashMap::new();
    let weld: Vec<u32> = (0..m.positions.len() as u32)
        .map(|v| *first.entry(m.positions[v as usize].map(f32::to_bits)).or_insert(v))
        .collect();
    submeshes
        .iter()
        .map(|s| {
            let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            for t in indices[range].chunks(3) {
                for (p, q) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let (p, q) = (weld[p as usize], weld[q as usize]);
                    *uses.entry((p.min(q), p.max(q))).or_default() += 1;
                }
            }
            let with = |f: fn(u32) -> bool| uses.iter().filter(|&(_, &n)| f(n)).map(|(&e, _)| e).collect();
            Edges {
                open: with(|n| n == 1),
                shared: with(|n| n > 2),
                all: uses.keys().copied().collect(),
            }
        })
        .collect()
}

/// Steps the full avatar and one per level through the same states: each level's vertices land exactly where the
/// full mesh's same vertices do.
fn skins_alike(a: &Avatar, name: &str) {
    let levels: Vec<Avatar> = (1..=LEVELS.len())
        .map(|k| Avatar {
            meshes: a.meshes.iter().map(|m| m.level(k)).collect(),
            ..a.clone()
        })
        .collect();
    levels.iter().for_each(valid);
    let all: Vec<&Avatar> = std::iter::once(a).chain(&levels).collect();
    states(&all, |state, islands| {
        let full = placed(a, &islands[0]);
        for (k, level) in levels.iter().enumerate() {
            for (mi, (f, p)) in full.iter().zip(placed(level, &islands[k + 1])).enumerate() {
                assert!(
                    f[..p.len()] == p[..],
                    "{name}: state {state} level {} mesh {mi} differs",
                    k + 1
                );
            }
        }
    });
}
