//! Compile-time binding and clip sampling: slot sharing, defaults, unbound reports, rotation grouping, Euler order.

use civbb_anim::clip::{euler_to_quat, quat_to_euler};
use civbb_anim::{Target, bind};
use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Quat, Transform};
use civbb_format::mesh::{BlendShape, Mesh};

const ARM_REST: Quat = [0.0, 0.0, 0.382_683_43, 0.923_879_5]; // 45 degrees about Z.

fn node(name: &str, parent: Option<u32>, active: bool) -> Node {
    Node {
        name: name.into(),
        parent,
        rest: Transform::IDENTITY,
        active,
    }
}

fn curve(path: &str, property: Property, keys: &[(f32, f32)]) -> Curve {
    // Linear keys.
    let slope = |a: (f32, f32), b: (f32, f32)| (b.1 - a.1) / (b.0 - a.0);
    let keys = (0..keys.len())
        .map(|i| Key {
            time: keys[i].0,
            value: keys[i].1,
            in_tangent: if i > 0 { slope(keys[i - 1], keys[i]) } else { 0.0 },
            out_tangent: if i + 1 < keys.len() {
                slope(keys[i], keys[i + 1])
            } else {
                0.0
            },
        })
        .collect();
    Curve {
        binding: Binding {
            path: path.into(),
            property,
        },
        keys,
    }
}

fn clip(curves: Vec<Curve>) -> Clip {
    Clip {
        name: "c".into(),
        length: 1.0,
        looping: false,
        curves,
    }
}

/// Root, `Body` (mesh with `Smile` at rest 0.3), `Body/Arm` (rest 45 degrees about Z), `Hat` (inactive).
fn avatar(clips: Vec<Clip>) -> Avatar {
    let mut arm = node("Arm", Some(1), true);
    arm.rest.rotation = ARM_REST;
    arm.rest.translation = [0.0, 0.5, 0.0];
    let param = |name: &str, kind| Parameter {
        name: name.into(),
        kind,
        default: 0.25,
    };
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![
                node("Root", None, true),
                node("Body", Some(0), true),
                arm,
                node("Hat", Some(0), false),
            ],
        },
        meshes: vec![Mesh {
            name: "Body".into(),
            node: 1,
            blend_shapes: vec![BlendShape {
                name: "Smile".into(),
                default_weight: 0.3,
                deltas: Vec::new(),
            }],
            ..Mesh::default()
        }],
        animator: Some(Animator {
            parameters: vec![param("Fl", ParamKind::Float), param("B", ParamKind::Bool)],
            ..Animator::default()
        }),
        clips,
        ..Avatar::default()
    }
}

fn close(a: &[f32], b: &[f32]) {
    assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5), "{a:?} != {b:?}");
}

#[test]
fn binds_targets_with_defaults_and_shares_slots_across_clips() {
    let a = avatar(vec![
        clip(vec![
            curve("Body/Arm", Property::Position(1), &[(0.0, 1.0), (1.0, 2.0)]),
            curve("Body", Property::BlendShape("Smile".into()), &[(0.0, 100.0)]),
            curve("Hat", Property::Active, &[(0.0, 1.0)]),
            curve("Body", Property::RendererEnabled, &[(0.0, 0.0)]),
            curve("", Property::Parameter("Fl".into()), &[(0.0, 0.75)]),
        ]),
        clip(vec![curve("Body/Arm", Property::Position(1), &[(0.0, 3.0)])]),
    ]);
    let b = bind(&a);
    assert!(b.unbound.is_empty(), "{:?}", b.unbound);
    let t = &b.bindings.targets;
    assert_eq!(
        t,
        &[
            Target::Position(2, 1),
            Target::BlendShape(0, 0),
            Target::Active(3),
            Target::RendererEnabled(0),
            Target::Parameter(0),
        ]
    );
    // Rest pose, blendshape weight in curve units (0 to 100), inactive flag, renderer on, parameter default.
    close(&b.bindings.defaults, &[0.5, 30.0, 0.0, 1.0, 0.25]);
    assert_eq!(b.clips[1].curves[0].slot, 0);
    assert_eq!(b.clips[0].slots, [0, 1, 2, 3, 4]);
    assert_eq!(b.clips[1].slots, [0]);

    let mut out = vec![-1.0; b.bindings.len()];
    b.clips[0].sample(0.5, &mut out);
    close(&out, &[1.5, 100.0, 1.0, 0.0, 0.75]);
    // A clip writes only its own slots.
    b.clips[1].sample(0.0, &mut out);
    close(&out, &[3.0, 100.0, 1.0, 0.0, 0.75]);
}

#[test]
fn reports_what_does_not_bind() {
    let cases = [
        (curve("Nope", Property::Active, &[(0.0, 1.0)]), "no node at path"),
        (
            curve("Hat", Property::BlendShape("Smile".into()), &[(0.0, 1.0)]),
            "no mesh at path",
        ),
        (
            curve("Hat", Property::RendererEnabled, &[(0.0, 1.0)]),
            "no mesh at path",
        ),
        (
            curve("Body", Property::BlendShape("Frown".into()), &[(0.0, 1.0)]),
            "no such blendshape",
        ),
        (
            curve("", Property::Parameter("Nope".into()), &[(0.0, 1.0)]),
            "no such parameter",
        ),
        (
            curve("", Property::Parameter("B".into()), &[(0.0, 1.0)]),
            "only float parameters can be animated",
        ),
        (curve("Body", Property::Active, &[]), "no keys"),
        (
            curve("", Property::Muscle("Jaw Close".into()), &[(0.0, 1.0)]),
            "no muscle frames (#53)",
        ),
        (
            curve("", Property::RootMotion(0), &[(0.0, 1.0)]),
            "body pose curves not supported yet (#52)",
        ),
        (
            curve("Body", Property::Material("_Color.r".into()), &[(0.0, 1.0)]),
            "material curves not supported yet (#49)",
        ),
    ];
    for (c, reason) in cases {
        let b = bind(&avatar(vec![clip(vec![c.clone()])]));
        assert_eq!(b.unbound.len(), 1, "{c:?}");
        assert_eq!(
            (b.unbound[0].clip, b.unbound[0].curve, b.unbound[0].reason),
            (0, 0, reason)
        );
        assert!(b.bindings.is_empty() && b.clips[0].slots.is_empty());
    }
}

#[test]
fn duplicate_bindings_keep_the_first_curve() {
    let a = avatar(vec![clip(vec![
        curve("Hat", Property::Active, &[(0.0, 1.0)]),
        curve("Hat", Property::Active, &[(0.0, 0.0)]),
        curve("Body/Arm", Property::Rotation(3), &[(0.0, 1.0)]),
        curve("Body/Arm", Property::Rotation(3), &[(0.0, 0.0)]),
    ])]);
    let b = bind(&a);
    let reasons: Vec<_> = b.unbound.iter().map(|u| (u.curve, u.reason)).collect();
    assert_eq!(reasons, [(1, "duplicate binding"), (3, "duplicate binding")]);
    let mut out = vec![0.0; b.bindings.len()];
    b.clips[0].sample(0.0, &mut out);
    assert_eq!(out[0], 1.0);
}

#[test]
fn partial_quaternion_curves_fill_from_rest_and_normalise() {
    // Only w is animated, from 1 to 0: x, y, z hold the rest rotation; the result is renormalised.
    let a = avatar(vec![clip(vec![curve(
        "Body/Arm",
        Property::Rotation(3),
        &[(0.0, 0.0), (1.0, 2.0)],
    )])]);
    let b = bind(&a);
    assert_eq!(
        b.bindings.targets,
        (0..4).map(|i| Target::Rotation(2, i)).collect::<Vec<_>>()
    );
    close(&b.bindings.defaults, &ARM_REST);
    assert_eq!(b.clips[0].slots, [0, 1, 2, 3]);
    let mut out = vec![0.0; 4];
    b.clips[0].sample(0.0, &mut out);
    close(&out, &[0.0, 0.0, 1.0, 0.0]);
    b.clips[0].sample(1.0, &mut out);
    let z = ARM_REST[2];
    let len = (z * z + 4.0f32).sqrt();
    close(&out, &[0.0, 0.0, z / len, 2.0 / len]);
}

#[test]
fn euler_curves_write_the_quaternion_slots() {
    let a = avatar(vec![
        clip(vec![curve(
            "Body/Arm",
            Property::EulerRotation(1),
            &[(0.0, 0.0), (1.0, 180.0)],
        )]),
        clip(vec![curve("Body/Arm", Property::Rotation(0), &[(0.0, 0.0)])]),
    ]);
    let b = bind(&a);
    assert!(b.unbound.is_empty());
    // Both clips share the node's quaternion slots.
    assert_eq!(b.bindings.len(), 4);
    assert_eq!(b.clips[0].slots, [0, 1, 2, 3]);
    let mut out = vec![0.0; 4];
    // Rest is 45 degrees about Z: x and z hold their rest angles, y is animated to 90 degrees at t = 0.5.
    b.clips[0].sample(0.5, &mut out);
    close(&out, &euler_to_quat([0.0, 90.0, 45.0]));
}

#[test]
fn quaternion_curves_win_over_euler_curves_on_one_node() {
    let a = avatar(vec![clip(vec![
        curve("Body/Arm", Property::EulerRotation(0), &[(0.0, 90.0)]),
        curve("Body/Arm", Property::Rotation(3), &[(0.0, 1.0)]),
    ])]);
    let b = bind(&a);
    let reasons: Vec<_> = b.unbound.iter().map(|u| (u.curve, u.reason)).collect();
    assert_eq!(reasons, [(0, "Euler curve on a node with quaternion curves")]);
    assert!(b.clips[0].eulers.is_empty());
}

fn mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn axis(i: usize, deg: f32) -> Quat {
    let (s, c) = (deg.to_radians() / 2.0).sin_cos();
    let mut q = [0.0, 0.0, 0.0, c];
    q[i] = s;
    q
}

#[test]
fn euler_order_is_z_then_x_then_y() {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    close(&euler_to_quat([0.0, 90.0, 0.0]), &[0.0, h, 0.0, h]);
    for e in [[30.0, 40.0, 50.0], [-80.0, 170.0, -10.0], [10.0, -120.0, 95.0]] {
        let expected = mul(mul(axis(1, e[1]), axis(0, e[0])), axis(2, e[2]));
        close(&euler_to_quat(e), &expected);
        let back = quat_to_euler(expected);
        assert!(
            back.iter().zip(e).all(|(a, b)| (a - b).abs() < 1e-3),
            "{back:?} != {e:?}"
        );
    }
    // Gimbal lock at x = 90: Y and Z coincide; the round trip keeps the rotation, not the angles.
    let q = euler_to_quat([90.0, 20.0, 30.0]);
    let back = euler_to_quat(quat_to_euler(q));
    let dot: f32 = q.iter().zip(back).map(|(a, b)| a * b).sum();
    assert!(dot.abs() > 1.0 - 1e-5, "{q:?} vs {back:?}");
}

#[test]
fn corpus_avatars_bind_completely() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let a = civbb_corpus::generate(&spec);
        let b = bind(&a);
        assert!(
            b.unbound.is_empty(),
            "{}: {:?}",
            spec.name,
            &b.unbound[..b.unbound.len().min(5)]
        );
        assert_eq!(b.clips.len(), a.clips.len());
        let mut out = b.bindings.defaults.clone();
        for c in &b.clips {
            c.sample(0.0, &mut out);
        }
    }
}
