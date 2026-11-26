//! Material curves (ADRs 0025, 0099): native fields read and written as stored, the sRGB transfer, per-renderer slots
//! defaulting from the renderer's first material, unbound reports.

use civbb_anim::material::{self, to_linear, to_srgb};
use civbb_anim::{Target, bind};
use civbb_format::Document;
use civbb_format::animator::{Clip, Curve, Key, MeshProperty, Target as CurveTarget};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{AlphaMode, Material, MaterialField as F, MaterialModel, Pbr, TextureSlot, Unlit};
use civbb_format::math::Transform;
use civbb_format::mesh::{Mesh, Submesh};

fn unlit() -> Material {
    Material {
        name: "Unlit".into(),
        model: MaterialModel::Unlit(Unlit {
            color: [0.25, 0.5, 1.0, 0.8],
            color_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        order: 0,
    }
}

fn pbr() -> Material {
    Material {
        name: "Pbr".into(),
        model: MaterialModel::Pbr(Pbr {
            base_color: [1.0, 0.0, 0.0, 1.0],
            base_color_tex: Some(TextureSlot {
                texture: 0,
                uv_set: 0,
                scale_offset: [2.0, 0.5, 0.25, 0.125],
            }),
            metallic: 0.75,
            roughness: 0.25,
            metallic_roughness_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            occlusion_tex: None,
            emissive: [0.0; 3],
            emissive_tex: None,
        }),
        alpha: AlphaMode::Mask { cutoff: 0.5 },
        double_sided: false,
        order: 0,
    }
}

/// A material curve on the `Body` renderer (mesh 0).
fn curve(field: F, component: u8, srgb: bool, value: f32) -> Curve {
    Curve {
        target: CurveTarget::Mesh {
            mesh: 0,
            property: MeshProperty::Material { field, component, srgb },
        },
        keys: vec![Key {
            time: 0.0,
            value,
            in_tangent: 0.0,
            out_tangent: 0.0,
        }],
    }
}

/// Root, `Body` (a renderer with an Unlit then a PBR submesh), `Hat` (no renderer).
fn avatar(curves: Vec<Curve>) -> Document {
    let node = |name: &str, parent| Node {
        name: String::from(name),
        parent,
        rest: Transform::IDENTITY,
        active: true,
    };
    let sub = |material| Submesh {
        first_index: 0,
        index_count: 0,
        material,
    };
    Document {
        hierarchy: Hierarchy {
            nodes: vec![node("Root", None), node("Body", Some(0)), node("Hat", Some(0))],
        },
        meshes: vec![Mesh {
            name: "Body".into(),
            node: 1,
            submeshes: vec![sub(0), sub(1)],
            ..Mesh::default()
        }],
        materials: vec![unlit(), pbr()],
        clips: vec![Clip {
            name: "c".into(),
            length: 1.0,
            looping: false,
            root: None,
            curves,
        }],
        ..Document::default()
    }
}

fn close(a: &[f32], b: &[f32]) {
    assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5), "{a:?} != {b:?}");
}

#[test]
fn fields_read_and_write_as_stored() {
    let mut m = pbr();
    close(&material::get(&m, F::BaseColor).unwrap(), &[1.0, 0.0, 0.0, 1.0]);
    assert!(material::set(&mut m, F::BaseColor, 1, 0.5));
    close(&material::get(&m, F::BaseColor).unwrap(), &[1.0, 0.5, 0.0, 1.0]);
    close(&material::get(&m, F::Roughness).unwrap()[..1], &[0.25]);
    close(
        &material::get(&m, F::BaseColorTransform).unwrap(),
        &[2.0, 0.5, 0.25, 0.125],
    );
    material::set(&mut m, F::BaseColorTransform, 3, 0.5);
    let MaterialModel::Pbr(p) = &m.model else {
        unreachable!()
    };
    close(&p.base_color_tex.unwrap().scale_offset, &[2.0, 0.5, 0.25, 0.5]);
    // Cutoff lives on masked materials only; fields a model lacks are absent.
    close(&material::get(&m, F::Cutoff).unwrap()[..1], &[0.5]);
    assert_eq!(material::get(&unlit(), F::Cutoff), None);
    assert_eq!(material::get(&unlit(), F::BaseColorTransform), None, "no texture");
    assert!(!material::set(&mut unlit(), F::ShadeColor, 0, 1.0));
    // A component past an RGB colour's three is accepted and ignored.
    let before = m.clone();
    assert!(material::set(&mut m, F::Emission, 3, 0.5));
    assert_eq!(m, before);
}

#[test]
fn srgb_curves_decode_as_unity_does() {
    close(&[to_linear(0.5)], &[0.214_041_14]);
    close(&[to_srgb(to_linear(0.7))], &[0.7]);
    // HDR: Unity's `Mathf.GammaToLinearSpace` raises values above 1 to 2.2 (measured: 32 gives 2048).
    close(&[to_linear(32.0) / 2048.0, to_srgb(to_linear(5.0))], &[1.0, 5.0]);
    // An animated colour reaches the shader converted so, `[HDR]` or not; alpha unconverted (rendered in Unity
    // 2022.3, linear project, #54: 0.2 gave 0.033105, an `[HDR]` 5 gave 34.493, alpha 0.5 stayed 0.5).
    assert!((to_linear(0.2) - 0.033_105).abs() < 1e-6 && (to_linear(5.0) - 34.493).abs() < 1e-3);
}

#[test]
fn curves_bind_per_renderer_with_the_first_materials_default() {
    let b = bind(&avatar(vec![
        curve(F::BaseColor, 1, true, 1.0),
        curve(F::BaseColor, 3, true, 1.0),
        curve(F::Metallic, 0, false, 0.0),
        curve(F::BaseColorTransform, 3, false, 0.0),
    ]));
    assert!(b.unbound.is_empty(), "{:?}", b.unbound);
    assert_eq!(
        b.bindings.targets,
        [
            Target::Material(0, F::BaseColor, 1, true),
            Target::Material(0, F::BaseColor, 3, true),
            Target::Material(0, F::Metallic, 0, false),
            Target::Material(0, F::BaseColorTransform, 3, false),
        ]
    );
    // Defaults come from the renderer's first material (the Unlit one), sRGB-encoded where the curve is, alpha
    // linear; the PBR-only fields default to 0 because that material lacks them, as Unity reads material 0 (ADR 0067).
    close(&b.bindings.defaults, &[to_srgb(0.5), 0.8, 0.0, 0.0]);
}

#[test]
fn curves_without_a_target_are_reported() {
    let b = bind(&avatar(vec![curve(F::ShadeColor, 0, true, 1.0)]));
    let reasons: Vec<_> = b.unbound.iter().map(|u| u.reason).collect();
    assert_eq!(reasons, ["no material on the renderer has the field"]);
    assert!(b.bindings.is_empty());
}
