//! Unity material curves lowered to native material fields (ADR 0099): property names by the renderer's shader
//! family, V-up texture offsets to the stored V-down ones, and the components Unity writes as 0 because no curve
//! animates them (ADR 0067) baked into the renderer's materials.

use std::collections::{BTreeMap, HashMap};

use civbb_anim::material::{get, set};
use civbb_format::animator::{Clip, Curve, Key, MeshProperty, Target};
use civbb_format::material::{Material, MaterialField};
use civbb_format::mesh::{Mesh, MeshIndex};

use super::super::unity_material::{Family, curve_field};

/// The target of a Unity material curve (`_Color.r`, `_Metallic`) on `mesh`: the field the first of its submesh
/// materials whose family reads the name maps it to. Colours are sRGB-encoded, as Unity animates them.
pub(super) fn target(
    name: &str,
    mesh: MeshIndex,
    meshes: &[Mesh],
    families: &[Option<Family>],
) -> Result<Target, &'static str> {
    let (base, component) = match name.rsplit_once('.') {
        Some((base, suffix)) => {
            let c = match suffix {
                "r" | "x" => 0,
                "g" | "y" => 1,
                "b" | "z" => 2,
                "a" | "w" => 3,
                _ => return Err("unknown material property"),
            };
            (base, Some(c))
        }
        None => (name, None),
    };
    let family = |m: u32| families.get(m as usize).copied().flatten();
    let field = (meshes[mesh as usize].submeshes.iter())
        .find_map(|s| curve_field(family(s.material), base))
        .ok_or("unknown material property")?;
    let component = match (field.components(), component) {
        (1, None) => 0,
        (n, Some(c)) if n > 1 && c < n => c,
        _ => return Err("no such material field component"),
    };
    Ok(Target::Mesh {
        mesh,
        property: MeshProperty::Material {
            field,
            component,
            srgb: field.is_color(),
        },
    })
}

/// A value the importer writes into the materials of a mesh: a component Unity holds at a constant.
pub(in crate::gltf_import) struct Bake {
    mesh: MeshIndex,
    field: MaterialField,
    component: u8,
    value: f32,
}

/// Rewrites the clips' texture offset curves from Unity's V-up offset to the stored one, and lists the components of
/// animated fields that no clip animates, at the value Unity holds them at. Until then component 3 of
/// [`MaterialField::BaseColorTransform`] curves holds Unity's V offset.
pub(super) fn finish(
    clips: &mut [Clip],
    meshes: &[Mesh],
    materials: &[Material],
    dropped: &mut BTreeMap<&'static str, usize>,
) -> Vec<Bake> {
    // Components animated in any clip, per mesh and field, in first-seen order.
    let mut animated: Vec<(MeshIndex, MaterialField, [bool; 4])> = Vec::new();
    for c in clips.iter().flat_map(|c| &c.curves) {
        if let Some((mesh, field, component)) = material(&c.target) {
            match animated.iter_mut().find(|a| a.0 == mesh && a.1 == field) {
                Some(a) => a.2[component as usize] = true,
                None => {
                    let mut a = [false; 4];
                    a[component as usize] = true;
                    animated.push((mesh, field, a));
                }
            }
        }
    }
    let mut bakes = Vec::new();
    for &(mesh, field, mut a) in &animated {
        if field == MaterialField::BaseColorTransform {
            for clip in clips.iter_mut() {
                offset(clip, mesh, a, renderer_default(meshes, materials, mesh, field), dropped);
            }
            // The stored offset follows Unity's V scale and V offset.
            a[3] |= a[1];
        }
        for component in (0..field.components()).filter(|&c| !a[c as usize]) {
            // A stored offset with Unity's V scale and V offset both 0 is 1.
            let value = if field == MaterialField::BaseColorTransform && component == 3 {
                1.0
            } else {
                0.0
            };
            bakes.push(Bake {
                mesh,
                field,
                component,
                value,
            });
        }
    }
    bakes
}

/// Writes `bakes` into the materials of their meshes' submeshes that have the field; a material another mesh also
/// uses is copied first.
pub(in crate::gltf_import) fn bake(bakes: &[Bake], meshes: &mut [Mesh], materials: &mut Vec<Material>) {
    let mut done: Vec<MeshIndex> = Vec::new();
    for b in bakes {
        if done.contains(&b.mesh) {
            continue;
        }
        done.push(b.mesh);
        let mine: Vec<&Bake> = bakes.iter().filter(|x| x.mesh == b.mesh).collect();
        let mut copies: HashMap<u32, u32> = HashMap::new();
        for s in 0..meshes[b.mesh as usize].submeshes.len() {
            let old = meshes[b.mesh as usize].submeshes[s].material;
            let new = match copies.get(&old) {
                Some(&n) => n,
                None => {
                    let mut m = materials[old as usize].clone();
                    for x in &mine {
                        put(&mut m, x);
                    }
                    let shared = (meshes.iter().enumerate())
                        .any(|(i, o)| i != b.mesh as usize && o.submeshes.iter().any(|x| x.material == old));
                    let n = if m == materials[old as usize] {
                        old
                    } else if shared {
                        materials.push(m);
                        (materials.len() - 1) as u32
                    } else {
                        materials[old as usize] = m;
                        old
                    };
                    copies.insert(old, n);
                    n
                }
            };
            meshes[b.mesh as usize].submeshes[s].material = new;
        }
    }
}

/// Writes a baked value. Unity's V offset is kept when its V scale becomes 0: the stored offset grows by the scale.
fn put(m: &mut Material, b: &Bake) {
    if b.field == MaterialField::BaseColorTransform
        && b.component == 1
        && let Some(st) = get(m, b.field)
    {
        set(m, b.field, 3, st[3] + st[1]);
    }
    set(m, b.field, b.component, b.value);
}

fn material(t: &Target) -> Option<(MeshIndex, MaterialField, u8)> {
    match *t {
        Target::Mesh {
            mesh,
            property: MeshProperty::Material { field, component, .. },
        } => Some((mesh, field, component)),
        _ => None,
    }
}

/// The renderer's default for a field as stored: its first material's value, 0 when that material lacks it.
fn renderer_default(meshes: &[Mesh], materials: &[Material], mesh: MeshIndex, field: MaterialField) -> [f32; 4] {
    let first = meshes[mesh as usize].submeshes.first();
    first
        .and_then(|s| get(&materials[s.material as usize], field))
        .unwrap_or([0.0; 4])
}

/// The clip's stored V offset curve, `1 - v_scale - v_offset_up`, from Unity's V scale and V offset curves on
/// `mesh`; components the clip lacks hold their default where another clip animates them, else 0 (`animated`).
fn offset(
    clip: &mut Clip,
    mesh: MeshIndex,
    animated: [bool; 4],
    default: [f32; 4],
    dropped: &mut BTreeMap<&'static str, usize>,
) {
    let find = |clip: &Clip, c: u8| {
        (clip.curves.iter()).position(|x| material(&x.target) == Some((mesh, MaterialField::BaseColorTransform, c)))
    };
    let (y, w) = (find(clip, 1), find(clip, 3));
    if y.is_none() && w.is_none() {
        return;
    }
    let y_default = if animated[1] { default[1] } else { 0.0 };
    let w_default = if animated[3] {
        1.0 - default[1] - default[3]
    } else {
        0.0
    };
    let keys = |i: Option<usize>| i.map(|i| clip.curves[i].keys.as_slice());
    let combined = match (keys(y), keys(w)) {
        (Some(y), Some(w)) if y.len() == w.len() && y.iter().zip(w).all(|(a, b)| a.time == b.time) => {
            y.iter().zip(w).map(|(a, b)| sum(a, b)).collect()
        }
        (Some(y), Some(_)) => {
            *dropped
                .entry("V scale and V offset keyed at different times")
                .or_default() += 1;
            y.iter().map(|k| shift(k, w_default)).collect()
        }
        (Some(y), None) => y.iter().map(|k| shift(k, w_default)).collect(),
        (None, Some(w)) => w.iter().map(|k| shift(k, y_default)).collect(),
        (None, None) => unreachable!("returned above"),
    };
    let negated = |keys: Vec<Key>| -> Vec<Key> {
        (keys.into_iter())
            .map(|k| Key {
                time: k.time,
                value: 1.0 - k.value,
                in_tangent: -k.in_tangent,
                out_tangent: -k.out_tangent,
            })
            .collect()
    };
    let keys = negated(combined);
    match w {
        Some(i) => clip.curves[i].keys = keys,
        None => clip.curves.push(Curve {
            target: Target::Mesh {
                mesh,
                property: MeshProperty::Material {
                    field: MaterialField::BaseColorTransform,
                    component: 3,
                    srgb: false,
                },
            },
            keys,
        }),
    }
}

fn sum(a: &Key, b: &Key) -> Key {
    Key {
        time: a.time,
        value: a.value + b.value,
        in_tangent: a.in_tangent + b.in_tangent,
        out_tangent: a.out_tangent + b.out_tangent,
    }
}

fn shift(k: &Key, by: f32) -> Key {
    Key {
        value: k.value + by,
        ..*k
    }
}
