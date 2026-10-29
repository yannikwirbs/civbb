//! Constraints (ADR 0018): every case recorded from Unity's constraints (`tests/unity/constraints.json`), and
//! hand-derived dependency order, ties, cycles, inactive constraints and subtree updates.

use civbb_anim::Constraints;
use civbb_format::Avatar;
use civbb_format::constraint::{Constraint, ConstraintKind, ConstraintSource};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Quat, Transform, Vec3};
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
    out: Vec<Transform>,
}

fn same_rotation(a: Quat, b: Quat, eps: f32) -> bool {
    let close = |s: f32| a.iter().zip(&b).all(|(x, y)| (x - s * y).abs() < eps);
    close(1.0) || close(-1.0)
}

#[test]
fn unity_recordings_match() {
    let r: Recording = serde_json::from_str(include_str!("unity/constraints.json")).unwrap();
    assert!(r.unity.starts_with("6000."), "{}", r.unity);
    assert!(r.stable, "the recording changed between frames");
    assert!(r.cases.len() >= 40);
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
        let mut locals: Vec<Transform> = case.nodes.iter().map(|n| n.rest).collect();
        Constraints::new(&avatar).evaluate(&mut locals);
        // Unity blends scale with an approximate `pow`, up to 1% above the exact geometric mean.
        let scale_eps = if case.name.starts_with("scale") { 0.01 } else { 1e-4 };
        for (i, (got, want)) in locals.iter().zip(&case.out).enumerate() {
            let t = got
                .translation
                .iter()
                .zip(&want.translation)
                .all(|(a, b)| (a - b).abs() < 2e-4);
            let s = got
                .scale
                .iter()
                .zip(&want.scale)
                .all(|(a, b)| (a - b).abs() <= scale_eps * b.abs().max(1.0));
            if !t || !s || !same_rotation(got.rotation, want.rotation, 2e-4) {
                failures.push(format!("{} node {i}: got {got:?}, want {want:?}", case.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn node(name: &str, parent: Option<u32>, t: Vec3) -> Node {
    Node {
        name: name.into(),
        parent,
        rest: Transform {
            translation: t,
            ..Transform::IDENTITY
        },
        active: true,
    }
}

fn position(target: u32, source: u32) -> Constraint {
    Constraint {
        kind: ConstraintKind::Position { offset: [0.0; 3] },
        target,
        sources: vec![ConstraintSource {
            node: source,
            weight: 1.0,
            offset: Transform::IDENTITY,
        }],
        weight: 1.0,
        active: true,
        axes: [true; 3],
        rest: Transform::IDENTITY,
    }
}

/// Root with children A (1, 0, 0), B (0, 2, 0), C (0, 0, 3), and D under C at (0, 1, 0).
fn avatar(constraints: Vec<Constraint>) -> Avatar {
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![
                node("Root", None, [0.0; 3]),
                node("A", Some(0), [1.0, 0.0, 0.0]),
                node("B", Some(0), [0.0, 2.0, 0.0]),
                node("C", Some(0), [0.0, 0.0, 3.0]),
                node("D", Some(3), [0.0, 1.0, 0.0]),
            ],
        },
        constraints,
        ..Avatar::default()
    }
}

fn run(a: &Avatar) -> Vec<Vec3> {
    let mut locals: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
    Constraints::new(a).evaluate(&mut locals);
    locals.iter().map(|l| l.translation).collect()
}

fn targets(a: &Avatar) -> Vec<u32> {
    Constraints::new(a).list.iter().map(|c| c.target).collect()
}

#[test]
fn constraints_run_after_what_they_read() {
    // B follows A, which follows C: listed reader first, they still settle in one pass.
    let a = avatar(vec![position(2, 1), position(1, 3)]);
    assert_eq!(targets(&a), [1, 2]);
    assert_eq!(run(&a)[2], [0.0, 0.0, 3.0]);

    // A follows D, whose parent C follows B: C's constraint moves D, so it runs first.
    let a = avatar(vec![position(1, 4), position(3, 2)]);
    assert_eq!(targets(&a), [3, 1]);
    assert_eq!(run(&a)[1], [0.0, 3.0, 0.0]);

    // A constraint on D runs after one on its parent C, since D's place depends on C.
    let a = avatar(vec![position(4, 1), position(3, 2)]);
    assert_eq!(targets(&a), [3, 4]);
    // D at world A (1, 0, 0) with C at (0, 2, 0): local (1, -2, 0).
    assert_eq!(run(&a)[4], [1.0, -2.0, 0.0]);
}

#[test]
fn independent_constraints_and_shared_targets_keep_list_order() {
    let a = avatar(vec![position(2, 3), position(1, 3)]);
    assert_eq!(targets(&a), [2, 1]);
    // Two constraints on A: the later one wins.
    let a = avatar(vec![position(1, 2), position(1, 3)]);
    assert_eq!(targets(&a), [1, 1]);
    assert_eq!(Constraints::new(&a).list[0].sources[0].node, 2);
    assert_eq!(run(&a)[1], [0.0, 0.0, 3.0]);
}

#[test]
fn cycles_fall_back_to_list_order() {
    // A follows B and B follows A: no order satisfies both; the first listed runs first.
    let a = avatar(vec![position(2, 1), position(1, 2)]);
    assert_eq!(targets(&a), [2, 1]);
    let p = run(&a);
    assert_eq!((p[1], p[2]), ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
}

#[test]
fn inactive_and_sourceless_constraints_leave_the_pose() {
    let mut off = position(1, 2);
    off.active = false;
    let mut empty = position(1, 2);
    empty.sources.clear();
    for c in [off, empty] {
        assert_eq!(run(&avatar(vec![c]))[1], [1.0, 0.0, 0.0]);
    }
}
