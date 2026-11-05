//! Constraints (ADR 0018): every case recorded from Unity's constraints (`tests/unity/constraints.json`), and
//! hand-derived dependency order, ties, cycles, inactive constraints and subtree updates; animated values, Solve In
//! Local Space and Freeze To World per island (ADR 0040).

use civbb_anim::Constraints;
use civbb_anim::constraint::Scratch;
use civbb_format::Avatar;
use civbb_format::constraint::{Constraint, ConstraintField, ConstraintKind, ConstraintSource};
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
        Constraints::new(&avatar).evaluate(&mut locals, &mut Default::default());
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
        options: Default::default(),
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
    Constraints::new(a).evaluate(&mut locals, &mut Default::default());
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

/// `[x, y, z]` degrees about one axis.
fn turn(axis: usize, deg: f32) -> Quat {
    let (s, c) = (deg.to_radians() * 0.5).sin_cos();
    let mut q = [0.0, 0.0, 0.0, c];
    q[axis] = s;
    q
}

fn qmul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;

/// Root with children S (the source) and T (the target, at the origin), rests given; one constraint on T from S.
fn rest_pair(kind: ConstraintKind, source_rest: Quat, target_rest: Quat, weight: f32) -> (Avatar, Vec<Transform>) {
    let mut s = node("S", Some(0), [1.0, 0.0, 0.0]);
    s.rest.rotation = source_rest;
    let mut t = node("T", Some(0), [0.0; 3]);
    t.rest.rotation = target_rest;
    let mut c = position(2, 1);
    c.kind = kind;
    c.weight = weight;
    c.rest.rotation = target_rest;
    let a = Avatar {
        hierarchy: Hierarchy {
            nodes: vec![node("Root", None, [0.0; 3]), s, t],
        },
        constraints: vec![c],
        ..Avatar::default()
    };
    let locals = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
    (a, locals)
}

fn rest_run(a: &Avatar, mut locals: Vec<Transform>, source_rotation: Quat) -> Quat {
    locals[1].rotation = source_rotation;
    Constraints::new(a).evaluate(&mut locals, &mut Default::default());
    locals[2].rotation
}

fn assert_rotation(got: Quat, want: Quat) {
    assert!(same_rotation(got, want, 1e-5), "got {got:?}, want {want:?}");
}

#[test]
fn rest_rotation_copies_the_local_change_onto_the_target_rest() {
    // Source rest Z 30, now Z 30 then X 40: a change of X 40, put on the target's rest Y 90; weight slerps.
    let src = qmul(turn(Z, 30.0), turn(X, 40.0));
    for (w, deg) in [(1.0, 40.0), (0.5, 20.0), (0.0, 0.0)] {
        let (a, l) = rest_pair(ConstraintKind::RestRotation, turn(Z, 30.0), turn(Y, 90.0), w);
        assert_rotation(rest_run(&a, l, src), qmul(turn(Y, 90.0), turn(X, deg)));
    }
}

#[test]
fn rest_roll_keeps_the_twist_about_the_axis_in_the_target_rest_frame() {
    // A change of Z 20 then X 60 twists 60 about X (the swing Z 20 is dropped); weight 0.5 gives 30.
    let roll = ConstraintKind::RestRoll { axis: [1.0, 0.0, 0.0] };
    let change = qmul(turn(Z, 20.0), turn(X, 60.0));
    let (a, l) = rest_pair(roll.clone(), [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 0.5);
    assert_rotation(rest_run(&a, l, change), turn(X, 30.0));
    // Target rest Y 90: its X is the parent's -Z. A change of Z 60 in the parent is X -60 for the target.
    let (a, l) = rest_pair(roll.clone(), [0.0, 0.0, 0.0, 1.0], turn(Y, 90.0), 1.0);
    assert_rotation(rest_run(&a, l, turn(Z, 60.0)), qmul(turn(Y, 90.0), turn(X, -60.0)));
    // ...and a change about the parent's X is pure swing for it.
    let (a, l) = rest_pair(roll, [0.0, 0.0, 0.0, 1.0], turn(Y, 90.0), 1.0);
    assert_rotation(rest_run(&a, l, turn(X, 50.0)), turn(Y, 90.0));
}

#[test]
fn rest_aim_turns_the_axis_at_the_source_by_the_shortest_rotation() {
    // Target at the origin aiming +Y; the source at (1, 0, 0): Z -90, half of it at weight 0.5.
    let aim = ConstraintKind::RestAim { axis: [0.0, 1.0, 0.0] };
    for (w, deg) in [(1.0, -90.0), (0.5, -45.0)] {
        let (a, l) = rest_pair(aim.clone(), [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], w);
        assert_rotation(rest_run(&a, l, [0.0, 0.0, 0.0, 1.0]), turn(Z, deg));
    }
    // Under a parent turned Y 90, the world turn Z -90 is X 90 in the parent's space.
    let (mut a, mut l) = rest_pair(aim, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 1.0);
    a.hierarchy.nodes[0].rest.rotation = turn(Y, 90.0);
    l[0].rotation = turn(Y, 90.0);
    // The source sits at the parent's local (0, 0, 1), world (1, 0, 0).
    l[1].translation = [0.0, 0.0, 1.0];
    assert_rotation(rest_run(&a, l, [0.0, 0.0, 0.0, 1.0]), turn(X, 90.0));
}

/// Runs `a` frame by frame on one island's scratch, with `frame` changing the animated values or the pose first.
fn frames(
    a: &Avatar,
    n: usize,
    mut frame: impl FnMut(usize, &Constraints, &mut Scratch, &mut [Transform]),
) -> Vec<Vec<Vec3>> {
    let c = Constraints::new(a);
    let mut scratch = Scratch::default();
    let mut out = Vec::new();
    for f in 0..n {
        let mut locals: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
        frame(f, &c, &mut scratch, &mut locals);
        c.evaluate(&mut locals, &mut scratch);
        out.push(locals.iter().map(|l| l.translation).collect());
    }
    out
}

#[test]
fn animated_values_are_per_island_and_fall_back_to_the_stored_ones() {
    // A follows B (0, 2, 0) or C (0, 0, 3); rest is the origin, so weight 0.5 lands halfway to the source.
    let mut c = position(1, 2);
    c.sources.push(ConstraintSource {
        node: 3,
        weight: 0.0,
        offset: Transform::IDENTITY,
    });
    let a = avatar(vec![c]);
    let cases: [(&[(ConstraintField, f32)], Vec3); 5] = [
        (&[], [0.0, 2.0, 0.0]),
        (&[(ConstraintField::Weight, 0.5)], [0.0, 1.0, 0.0]),
        (&[(ConstraintField::Active, 0.0)], [1.0, 0.0, 0.0]),
        (&[(ConstraintField::Active, 0.4)], [1.0, 0.0, 0.0]),
        (
            &[
                (ConstraintField::SourceWeight(0), 0.0),
                (ConstraintField::SourceWeight(1), 1.0),
            ],
            [0.0, 0.0, 3.0],
        ),
    ];
    for (set, want) in cases {
        let got = frames(&a, 1, |_, c, s, _| {
            for &(field, v) in set {
                c.set(s, 0, field, v);
            }
        });
        assert_eq!(got[0][1], want, "{set:?}");
    }
    // A source index past the list is ignored.
    let got = frames(&a, 1, |_, c, s, _| c.set(s, 0, ConstraintField::SourceWeight(9), 1.0));
    assert_eq!(got[0][1], [0.0, 2.0, 0.0]);
}

#[test]
fn set_addresses_constraints_in_avatar_order_after_sorting() {
    // Listed reader first, so the sorted order is [1, 0]: avatar constraint 0 (on B) is list entry 1.
    let a = avatar(vec![position(2, 1), position(1, 3)]);
    assert_eq!(targets(&a), [1, 2]);
    let got = frames(&a, 1, |_, c, s, _| c.set(s, 0, ConstraintField::Active, 0.0));
    // B keeps its pose, A still follows C.
    assert_eq!((got[0][1], got[0][2]), ([0.0, 0.0, 3.0], [0.0, 2.0, 0.0]));
}

#[test]
fn solve_in_local_space_copies_the_sources_local_values() {
    // D sits under C (0, 0, 3). In world space it reaches A's world (1, 0, 0), local (1, 0, -3)...
    let world = run(&avatar(vec![position(4, 1)]))[4];
    assert_eq!(world, [1.0, 0.0, -3.0]);
    // ...in local space it takes A's local value as its own.
    let mut c = position(4, 1);
    c.options.local_space = true;
    assert_eq!(run(&avatar(vec![c]))[4], [1.0, 0.0, 0.0]);
}

#[test]
fn freeze_to_world_holds_the_world_placement_until_unfrozen() {
    // D under C follows A. Frozen on frames 1 to 3; C moves from frame 2, A from frame 3 (only frozen D ignores
    // that); unfrozen on frame 4.
    let a = avatar(vec![position(4, 1)]);
    let got = frames(&a, 5, |f, c, s, l| {
        if f >= 2 {
            l[3].translation = [5.0, 0.0, 3.0];
        }
        if f >= 3 {
            l[1].translation = [7.0, 0.0, 0.0];
        }
        c.set(
            s,
            0,
            ConstraintField::Freeze,
            if (1..4).contains(&f) { 1.0 } else { 0.0 },
        );
    });
    let d = |f: usize| got[f][4];
    assert_eq!(d(0), [1.0, 0.0, -3.0]);
    // Frozen at A's world (1, 0, 0) on the frame it freezes, and held there while C and A move.
    assert_eq!(d(1), [1.0, 0.0, -3.0]);
    assert_eq!(d(2), [-4.0, 0.0, -3.0]);
    assert_eq!(d(3), [-4.0, 0.0, -3.0]);
    // Unfrozen: follows A again, at (7, 0, 0) world.
    assert_eq!(d(4), [2.0, 0.0, -3.0]);
}

#[test]
fn an_inactive_constraint_drops_its_frozen_placement() {
    let mut c = position(1, 2);
    c.options.freeze = true;
    let a = avatar(vec![c]);
    // Frozen at B from the start; B moves from frame 1; the constraint is off on frame 2 and on again from frame 3.
    let got = frames(&a, 4, |f, c, s, l| {
        if f >= 1 {
            l[2].translation = [0.0, 4.0, 0.0];
        }
        c.set(s, 0, ConstraintField::Active, if f == 2 { 0.0 } else { 1.0 });
    });
    let a_at = |f: usize| got[f][1];
    assert_eq!(a_at(0), [0.0, 2.0, 0.0]);
    assert_eq!(a_at(1), [0.0, 2.0, 0.0]);
    assert_eq!(a_at(2), [1.0, 0.0, 0.0]);
    // Frozen again where B is now, not where it froze first.
    assert_eq!(a_at(3), [0.0, 4.0, 0.0]);
}
