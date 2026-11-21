//! A humanoid clip's root-motion settings against Unity (#52, ADR 0063): one clip whose body pose moves and turns,
//! played with `applyRootMotion` off under every combination of the bake, keep-original, feet and loop-pose flags,
//! with and without orientation and level offsets (`tests/unity/root_motion.json`,
//! `tools/unity-truth/Assets/Editor/RootMotion.cs`). The body pose Unity reads back must match the sampled clip's.

use std::path::Path;

use civbb_anim::body::RootMotion;
use civbb_anim::clip::{BodyCurves, BoundClip};
use civbb_format::animator::{Key, RootSettings};
use civbb_format::math::{Quat, Vec3};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    times: Vec<f32>,
    curves: Vec<RecordedCurve>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct RecordedCurve {
    /// Time, value, in tangent, out tangent.
    keys: Vec<[f32; 4]>,
}

#[derive(Deserialize)]
struct Case {
    settings: Settings,
    samples: Vec<Sample>,
}

#[derive(Deserialize)]
struct Settings {
    loop_time: bool,
    loop_blend: bool,
    cycle_offset: f32,
    bake_orientation: bool,
    bake_position_y: bool,
    bake_position_xz: bool,
    original_orientation: bool,
    original_position_y: bool,
    original_position_xz: bool,
    height_from_feet: bool,
    orientation_offset_y: f32,
    level: f32,
}

#[derive(Deserialize)]
struct Sample {
    /// `GetHumanPose`'s body position and rotation.
    body: (Vec3, Quat),
}

/// Unity's left-handed axes to ours: mirror X (ADR 0006); RootT x and RootQ y and z negate (ADR 0039).
const MIRROR: [f32; 7] = [-1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0];

#[test]
fn root_motion_settings_match_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/root_motion.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(r.curves.len(), 7);
    let axes: [Vec<Key>; 7] = std::array::from_fn(|i| {
        let m = MIRROR[i];
        (r.curves[i].keys.iter())
            .map(|k| Key {
                time: k[0],
                value: m * k[1],
                in_tangent: m * k[2],
                out_tangent: m * k[3],
            })
            .collect()
    });
    let (mut checked, mut worst, mut feet) = (0, 0.0f32, 0);
    for (ci, c) in r.cases.iter().enumerate() {
        let s = &c.settings;
        // The clip's own cycle offset is not part of the format (no corpus clip sets it, ADR 0063).
        if s.cycle_offset != 0.0 {
            continue;
        }
        feet += usize::from(s.height_from_feet);
        let settings = RootSettings {
            bake_orientation: s.bake_orientation,
            bake_position_y: s.bake_position_y,
            bake_position_xz: s.bake_position_xz,
            original_orientation: s.original_orientation,
            original_position_y: s.original_position_y,
            original_position_xz: s.original_position_xz,
            loop_pose: s.loop_blend,
            // Degrees about Unity's +Y are degrees about our -Y.
            orientation_offset: -s.orientation_offset_y,
            level: s.level,
        };
        let mut body = BodyCurves {
            slot: 0,
            axes: axes.clone(),
            root: None,
        };
        body.root = Some(RootMotion::new(settings, body.eval(0.0), body.eval(1.0), s.loop_time));
        let clip = BoundClip {
            length: 1.0,
            looping: s.loop_time,
            body: Some(body),
            ..BoundClip::default()
        };
        for (&t, sample) in r.times.iter().zip(&c.samples) {
            let time = if s.loop_time { t.fract() } else { t.min(1.0) };
            let mut out = [0.0; 7];
            clip.sample(time, &mut out);
            let (p, q) = sample.body;
            let want: [f32; 7] = std::array::from_fn(|i| MIRROR[i] * [p[0], p[1], p[2], q[0], q[1], q[2], q[3]][i]);
            let sign = if (3..7).map(|i| out[i] * want[i]).sum::<f32>() < 0.0 {
                -1.0
            } else {
                1.0
            };
            let err = (0..7)
                .map(|i| {
                    (if i < 3 {
                        out[i] - want[i]
                    } else {
                        sign * out[i] - want[i]
                    })
                    .abs()
                })
                .fold(0.0, f32::max);
            assert!(err < 2e-5, "case {ci} at {t}: {out:?}, Unity {want:?}");
            worst = worst.max(err);
            checked += 1;
        }
    }
    // Feet-based height changed nothing in these clips (they have no foot goal curves), so it is not modelled.
    assert!(feet > 0 && checked > 2500, "{checked} samples checked");
    eprintln!("worst component error {worst}");
}
