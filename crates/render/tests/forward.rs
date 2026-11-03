//! The forward pass without a device (ADR 0023): pipeline keys, material records, draw batching into runs, and the
//! reverse-Z projection.

use civbb_format::material::{AlphaMode, Material, MaterialModel, Pbr, TextureSlot, Toon, Unlit};
use civbb_render::material::{MATERIAL_WORDS, NONE, PipelineKey, Shader, pack};
use civbb_render::{Batches, Draw, MaterialId, View, batch};

fn unlit(alpha: AlphaMode, double_sided: bool) -> Material {
    Material {
        name: "m".into(),
        model: MaterialModel::Unlit(Unlit {
            color: [0.5, 0.25, 1.0, 0.75],
            color_tex: None,
        }),
        alpha,
        double_sided,
        render_queue: 2000,
    }
}

#[test]
fn every_key_has_its_own_index() {
    let keys: Vec<PipelineKey> = PipelineKey::all().collect();
    assert_eq!(keys.len(), PipelineKey::COUNT);
    for (i, k) in keys.iter().enumerate() {
        assert_eq!(k.index(), i);
    }
    let m = unlit(AlphaMode::Blend, true);
    assert_eq!(
        PipelineKey::of(&m),
        PipelineKey {
            shader: Shader::Unlit,
            blend: true,
            double_sided: true
        }
    );
    // Cutoff is a shader branch, not a pipeline.
    assert!(!PipelineKey::of(&unlit(AlphaMode::Mask { cutoff: 0.5 }, false)).blend);
}

fn f(words: &[u32]) -> Vec<f32> {
    words.iter().map(|&w| f32::from_bits(w)).collect()
}

/// Slot `i` of a record: scale-offset, then texture index and UV set.
fn slot(r: &[u32; MATERIAL_WORDS], i: usize) -> (Vec<f32>, [u32; 2]) {
    let s = &r[24 + i * 8..][..8];
    (f(&s[..4]), [s[4], s[5]])
}

#[test]
fn material_records() {
    let tex = |texture, uv_set| {
        Some(TextureSlot {
            texture,
            uv_set,
            scale_offset: [2.0, 3.0, 0.25, 0.5],
        })
    };
    let pbr = Material {
        name: "pbr".into(),
        model: MaterialModel::Pbr(Pbr {
            base_color: [1.0, 0.5, 0.25, 1.0],
            base_color_tex: tex(1, 0),
            metallic: 0.75,
            roughness: 0.5,
            metallic_roughness_tex: tex(0, 1),
            normal_tex: tex(7, 0),
            normal_scale: 0.8,
            occlusion_tex: None,
            emissive: [0.1, 0.2, 0.3],
            emissive_tex: None,
        }),
        alpha: AlphaMode::Mask { cutoff: 0.4 },
        double_sided: false,
        render_queue: 2450,
    };
    // Texture 7 does not exist: its slot gets none.
    let r = pack(&pbr, &[10, 11]);
    assert_eq!(f(&r[..4]), [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(f(&r[4..8]), [0.1, 0.2, 0.3, 0.0]);
    assert_eq!(f(&r[16..20]), [0.75, 0.5, 0.8, 0.4]);
    assert_eq!(slot(&r, 0), (vec![2.0, 3.0, 0.25, 0.5], [11, 0]));
    assert_eq!(slot(&r, 1).1, [NONE, 0]);
    assert_eq!(slot(&r, 2), (vec![2.0, 3.0, 0.25, 0.5], [10, 1]));
    assert_eq!(slot(&r, 3), (vec![1.0, 1.0, 0.0, 0.0], [NONE, 0]));

    let toon = Material {
        name: "toon".into(),
        model: MaterialModel::Toon(Toon {
            base_color: [1.0; 4],
            base_color_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            shade_color: [0.2, 0.3, 0.4],
            shade_tex: tex(0, 0),
            shade_shift: -0.25,
            shade_toony: 0.9,
            rim_color: [0.5, 0.5, 0.5],
            rim_power: 3.0,
            matcap_tex: tex(1, 0),
            matcap_strength: 0.5,
            emissive: [0.0; 3],
            emissive_tex: None,
            outline_width: 0.0,
            outline_color: [0.0; 3],
            outline_width_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: true,
        render_queue: 2000,
    };
    let r = pack(&toon, &[10, 11]);
    assert_eq!(f(&r[8..12]), [0.2, 0.3, 0.4, 0.0]);
    assert_eq!(f(&r[12..16]), [0.5, 0.5, 0.5, 0.0]);
    assert_eq!(f(&r[16..20])[3], 0.0, "opaque: no cutoff");
    assert_eq!(f(&r[20..24]), [-0.25, 0.9, 3.0, 0.5]);
    assert_eq!(slot(&r, 2).1, [10, 0]);
    assert_eq!(slot(&r, 5).1, [11, 0]);

    let r = pack(&unlit(AlphaMode::Opaque, false), &[]);
    assert_eq!(f(&r[..4]), [0.5, 0.25, 1.0, 0.75]);
    assert!((0..6).all(|i| slot(&r, i).1[0] == NONE));
}

fn draw(first_index: u32, material: &Material, index: u32, distance: f32) -> Draw {
    Draw {
        first_index,
        index_count: 3,
        skinned: 100 + first_index,
        attrs: 200 + first_index,
        material: MaterialId {
            index,
            key: PipelineKey::of(material),
            queue: material.render_queue,
        },
        distance,
    }
}

#[test]
fn opaque_runs_by_pipeline_then_blended_back_to_front() {
    let opaque = unlit(AlphaMode::Opaque, false);
    let two_sided = unlit(AlphaMode::Opaque, true);
    let blend = unlit(AlphaMode::Blend, false);
    let mut late_blend = unlit(AlphaMode::Blend, true);
    late_blend.render_queue = 3100;
    let mut draws = vec![
        draw(0, &blend, 2, 1.0),
        draw(3, &two_sided, 1, 5.0),
        draw(6, &opaque, 0, 5.0),
        draw(9, &late_blend, 3, 9.0),
        draw(12, &blend, 2, 4.0),
        draw(15, &opaque, 0, 1.0),
        draw(18, &blend, 2, 2.0),
    ];
    let mut out = Batches::default();
    batch(&mut draws, &mut out);
    let order: Vec<u32> = draws.iter().map(|d| d.first_index).collect();
    // Opaque by key, insertion order kept; blended by queue, then far to near.
    assert_eq!(order, [6, 15, 3, 12, 18, 0, 9]);
    let runs: Vec<(PipelineKey, u32, u32)> = out.runs.iter().map(|r| (r.key, r.first, r.count)).collect();
    let key = PipelineKey::of;
    assert_eq!(
        runs,
        [
            (key(&opaque), 0, 2),
            (key(&two_sided), 2, 1),
            (key(&blend), 3, 3),
            (key(&late_blend), 6, 1)
        ]
    );
    assert_eq!(out.len(), 7);
    // The draw finds its record through the instance index.
    assert_eq!(&out.commands[..5], [3, 1, 6, 0, 0]);
    assert_eq!(&out.commands[5..10], [3, 1, 15, 0, 1]);
    assert_eq!(&out.records[..4], [0, 106, 206, 0]);

    batch(&mut [], &mut out);
    assert!(out.is_empty() && out.runs.is_empty() && out.commands.is_empty());
}

fn clip(m: &[f32; 16], p: [f32; 3]) -> [f32; 4] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

#[test]
fn look_at_is_reverse_z_with_y_down() {
    let v = View::look_at([0.0, 1.0, 3.0], [0.0, 1.0, 0.0], std::f32::consts::FRAC_PI_2, 2.0, 0.1);
    let ndc = |p| {
        let c = clip(&v.view_proj, p);
        [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
    };
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
    // The target is in the middle, at depth near / distance.
    assert!(close(ndc([0.0, 1.0, 0.0]), [0.0, 0.0, 0.1 / 3.0]));
    // Up is up on screen (negative Y in Vulkan), right is right; 90 degrees vertically, aspect 2.
    assert!(close(ndc([0.0, 4.0, 0.0]), [0.0, -1.0, 0.1 / 3.0]));
    assert!(close(ndc([6.0, 1.0, 0.0]), [1.0, 0.0, 0.1 / 3.0]));
    // The near plane is depth 1; depth falls towards 0 far away.
    assert!(close(ndc([0.0, 1.0, 2.9]), [0.0, 0.0, 1.0]));
    assert!(ndc([0.0, 1.0, -1000.0])[2] < 1e-4);
}
