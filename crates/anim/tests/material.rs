//! Material curves (ADR 0025): Unity property names to native fields, Unity's units (sRGB colours, smoothness, V-up
//! offsets), per-renderer slots defaulting from the renderer's first material with the field, unbound reports.

use civbb_anim::material::{self, Field, to_linear, to_srgb};
use civbb_anim::{Target, bind};
use civbb_format::Avatar;
use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{AlphaMode, Material, MaterialModel, Pbr, TextureSlot, Unlit};
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
        render_queue: 2000,
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
        render_queue: 2450,
    }
}

fn curve(path: &str, name: &str, value: f32) -> Curve {
    Curve {
        binding: Binding {
            path: path.into(),
            property: Property::Material(name.into()),
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
fn avatar(curves: Vec<Curve>) -> Avatar {
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
    Avatar {
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
            curves,
        }],
        ..Avatar::default()
    }
}

fn close(a: &[f32], b: &[f32]) {
    assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5), "{a:?} != {b:?}");
}

#[test]
fn names_parse_to_fields_and_components() {
    assert_eq!(material::parse("_Color.r"), Some((Field::BaseColor, 0)));
    assert_eq!(material::parse("_BaseColor.a"), Some((Field::BaseColor, 3)));
    assert_eq!(material::parse("_EmissionColor.b"), Some((Field::Emission, 2)));
    assert_eq!(material::parse("_MainTex_ST.w"), Some((Field::MainTexSt, 3)));
    assert_eq!(material::parse("_Glossiness"), Some((Field::Smoothness, 0)));
    assert_eq!(material::parse("_LineColor.g"), Some((Field::OutlineColor, 1)));
    // Vectors need a component, scalars must not have one, unknown names and suffixes bind to nothing.
    for bad in ["_Color", "_Metallic.x", "_Color.q", "_Nope.r", "", "."] {
        assert_eq!(material::parse(bad), None, "{bad}");
    }
}

#[test]
fn values_are_in_unity_units() {
    let mut m = pbr();
    // Linear colours read sRGB-encoded; alpha stays linear.
    close(&material::get(&m, Field::BaseColor).unwrap(), &[1.0, 0.0, 0.0, 1.0]);
    assert!(material::set(&mut m, Field::BaseColor, 1, 0.5));
    let MaterialModel::Pbr(p) = &m.model else {
        unreachable!()
    };
    close(&p.base_color, &[1.0, to_linear(0.5), 0.0, 1.0]);
    close(&[to_linear(0.5)], &[0.214_041_14]);
    close(&[to_srgb(to_linear(0.7))], &[0.7]);
    // Smoothness is one minus roughness.
    close(&material::get(&m, Field::Smoothness).unwrap()[..1], &[0.75]);
    material::set(&mut m, Field::Smoothness, 0, 0.1);
    let MaterialModel::Pbr(p) = &m.model else {
        unreachable!()
    };
    close(&[p.roughness], &[0.9]);
    // Unity's V offset is from the bottom: ours is 1 - scale - offset, and stays put when the V scale changes.
    close(&material::get(&m, Field::MainTexSt).unwrap(), &[2.0, 0.5, 0.25, 0.375]);
    material::set(&mut m, Field::MainTexSt, 1, 0.25);
    close(&material::get(&m, Field::MainTexSt).unwrap(), &[2.0, 0.25, 0.25, 0.375]);
    material::set(&mut m, Field::MainTexSt, 3, 0.5);
    let MaterialModel::Pbr(p) = &m.model else {
        unreachable!()
    };
    close(&p.base_color_tex.unwrap().scale_offset, &[2.0, 0.25, 0.25, 0.25]);
    // Cutoff lives on masked materials only; fields a model lacks are absent.
    close(&material::get(&m, Field::Cutoff).unwrap()[..1], &[0.5]);
    assert_eq!(material::get(&unlit(), Field::Cutoff), None);
    assert_eq!(material::get(&unlit(), Field::MainTexSt), None, "no texture");
    assert!(!material::set(&mut unlit(), Field::ShadeColor, 0, 1.0));
    // Alpha on an RGB colour is accepted and ignored.
    let before = m.clone();
    assert!(material::set(&mut m, Field::Emission, 3, 0.5));
    assert_eq!(m, before);
}

#[test]
fn curves_bind_per_renderer_with_the_first_materials_default() {
    let b = bind(&avatar(vec![
        curve("Body", "_Color.g", 1.0),
        curve("Body", "_Metallic", 0.0),
        curve("Body", "_MainTex_ST.w", 0.0),
    ]));
    assert!(b.unbound.is_empty(), "{:?}", b.unbound);
    assert_eq!(
        b.bindings.targets,
        [
            Target::Material(0, Field::BaseColor, 1),
            Target::Material(0, Field::Metallic, 0),
            Target::Material(0, Field::MainTexSt, 3),
        ]
    );
    // `_Color` from the Unlit (first) submesh, sRGB-encoded; the others only the PBR one has.
    close(&b.bindings.defaults, &[to_srgb(0.5), 0.75, 0.375]);
}

#[test]
fn curves_without_a_target_are_reported() {
    let cases = [
        (curve("Body", "_Nope.r", 1.0), "unknown material property"),
        (curve("Hat", "_Color.r", 1.0), "no mesh at path"),
        (
            curve("Body", "_ShadeColor.r", 1.0),
            "no material on the renderer has the property",
        ),
    ];
    for (c, reason) in cases {
        let b = bind(&avatar(vec![c]));
        let reasons: Vec<_> = b.unbound.iter().map(|u| u.reason).collect();
        assert_eq!(reasons, [reason]);
        assert!(b.bindings.is_empty());
    }
}
