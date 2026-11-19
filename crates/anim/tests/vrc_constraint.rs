//! VRChat constraint options (ADR 0040, 0046): every case recorded from the VRChat SDK's own constraint runtime
//! (`tests/unity/vrc_constraints.json`, `tools/unity-truth/Assets/Editor/VrcConstraints.cs`), replayed frame by frame
//! on one island.

use civbb_anim::Constraints;
use civbb_anim::constraint::Scratch;
use civbb_format::Avatar;
use civbb_format::constraint::{Constraint, ConstraintField};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Quat, Transform};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    unity: String,
    stable: bool,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    nodes: Vec<Node>,
    constraints: Vec<Constraint>,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
struct Frame {
    set: Vec<(u32, ConstraintField, f32)>,
    input: Vec<Transform>,
    out: Vec<Transform>,
}

fn same_rotation(a: Quat, b: Quat, eps: f32) -> bool {
    let close = |s: f32| a.iter().zip(&b).all(|(x, y)| (x - s * y).abs() < eps);
    close(1.0) || close(-1.0)
}

fn close(a: &Transform, b: &Transform, scale_eps: f32) -> bool {
    let t = a
        .translation
        .iter()
        .zip(&b.translation)
        .all(|(x, y)| (x - y).abs() < 2e-4);
    let s = (a.scale.iter().zip(&b.scale)).all(|(x, y)| (x - y).abs() <= scale_eps * y.abs().max(1.0));
    t && s && same_rotation(a.rotation, b.rotation, 2e-4)
}

#[test]
fn vrchat_sdk_recordings_match() {
    let r: Recording = serde_json::from_str(include_str!("unity/vrc_constraints.json")).unwrap();
    assert!(r.unity.starts_with("2022.3"), "{}", r.unity);
    assert!(r.stable, "the recording skipped or repeated a frame");
    let mut failures = Vec::new();
    for case in &r.cases {
        let avatar = Avatar {
            hierarchy: Hierarchy {
                nodes: case.nodes.clone(),
            },
            constraints: case.constraints.clone(),
            ..Avatar::default()
        };
        civbb_format::validate(&avatar, &Default::default(), |_| true).unwrap();
        let constraints = Constraints::new(&avatar);
        let mut scratch = Scratch::default();
        // Unity blends scale with an approximate `pow`, up to 1% above the exact geometric mean.
        let scale_eps = if case.name.contains("scale") { 0.01 } else { 1e-4 };
        for (f, frame) in case.frames.iter().enumerate() {
            for &(k, field, value) in &frame.set {
                constraints.set(&mut scratch, k, field, value);
            }
            let mut locals = frame.input.clone();
            constraints.evaluate(&mut locals, &mut scratch);
            for (i, (got, want)) in locals.iter().zip(&frame.out).enumerate() {
                if !close(got, want, scale_eps) {
                    failures.push(format!("{} frame {f} node {i}: got {got:?}, want {want:?}", case.name));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
