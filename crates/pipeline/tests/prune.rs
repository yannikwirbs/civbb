//! Pruning (ADR 0026): unused skin joints, unreferenced nodes and undriven blendshapes go; everything referenced,
//! its ancestors, chain descendants, animated, face-bound, face-tracking and non-zero shapes stay, with every index
//! remapped, the avatar valid and its clips binding as before.

use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::constraint::{Constraint, ConstraintKind, ConstraintSource};
use civbb_format::face::{Face, ShapeBind};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Mat4, Transform};
use civbb_format::mesh::{BlendShape, Mesh, Skin};
use civbb_format::{Avatar, Caps, validate};
use civbb_pipeline::{Pruned, prune};

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn valid(a: &Avatar) {
    if let Err(e) = validate(a, &Caps::default(), |_| true) {
        panic!("{e:?}");
    }
}

fn names(a: &Avatar) -> Vec<&str> {
    a.hierarchy.nodes.iter().map(|n| n.name.as_str()).collect()
}

/// Root; Body (mesh skinned to Hips and Spare, only Hips weighted); Hips; Spare (unused joint); Lone (nothing);
/// Group/Target (a constraint targets it); Toggle (a clip animates it); Tail/Tip (a chain on Tail); Group/Gone.
fn avatar() -> Avatar {
    let node = |name: &str, parent: Option<u32>| Node {
        name: name.into(),
        parent,
        rest: Transform::IDENTITY,
        active: true,
    };
    let shape = |name: &str, default_weight: f32| BlendShape {
        name: name.into(),
        default_weight,
        deltas: Vec::new(),
    };
    let mut chain = civbb_corpus::generate(&civbb_corpus::bloated()).dynamics.chains[0].clone();
    chain.root = 8;
    chain.ignore.clear();
    chain.colliders.clear();
    let mut a = Avatar {
        hierarchy: Hierarchy {
            nodes: vec![
                node("Root", None),      // 0
                node("Body", Some(0)),   // 1
                node("Hips", Some(0)),   // 2
                node("Spare", Some(2)),  // 3
                node("Lone", Some(0)),   // 4
                node("Group", Some(0)),  // 5
                node("Target", Some(5)), // 6
                node("Toggle", Some(0)), // 7
                node("Tail", Some(2)),   // 8
                node("Tip", Some(8)),    // 9
                node("Gone", Some(5)),   // 10
            ],
        },
        meshes: vec![Mesh {
            name: "Body".into(),
            node: 1,
            positions: vec![[0.0; 3]; 2],
            normals: vec![[0.0, 1.0, 0.0]; 2],
            skin: Some(Skin {
                joints: vec![3, 2],
                inverse_bind: vec![IDENTITY; 2],
                vertex_joints: vec![[1, 0, 0, 0], [1, 1, 0, 0]],
                vertex_weights: vec![[1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0]],
            }),
            blend_shapes: vec![
                shape("Unused", 0.0),
                shape("Smile", 0.0),
                shape("Gone", 0.0),
                shape("Rest", 0.5),
                shape("jawopen", 0.0),
                shape("Blink", 0.0),
            ],
            ..Mesh::default()
        }],
        face: Some(Face {
            blink: vec![ShapeBind {
                mesh: 0,
                shape: 5,
                weight: 1.0,
            }],
            ..Face::default()
        }),
        clips: vec![Clip {
            name: "c".into(),
            length: 1.0,
            looping: false,
            curves: [
                ("Toggle", Property::Active),
                ("Body", Property::BlendShape("Smile".into())),
            ]
            .into_iter()
            .map(|(path, property)| Curve {
                binding: Binding {
                    path: path.into(),
                    property,
                },
                keys: vec![Key {
                    time: 0.0,
                    value: 1.0,
                    in_tangent: 0.0,
                    out_tangent: 0.0,
                }],
            })
            .collect(),
        }],
        constraints: vec![Constraint {
            kind: ConstraintKind::Position { offset: [0.0; 3] },
            target: 6,
            sources: vec![ConstraintSource {
                node: 7,
                weight: 1.0,
                offset: Transform::IDENTITY,
            }],
            weight: 1.0,
            active: true,
            axes: [true; 3],
            rest: Transform::IDENTITY,
            options: Default::default(),
        }],
        ..Avatar::default()
    };
    a.dynamics.chains.push(chain);
    a
}

#[test]
fn prunes_what_nothing_references_and_remaps_the_rest() {
    let mut a = avatar();
    valid(&a);
    let pruned = prune(&mut a);
    valid(&a);
    assert_eq!(
        pruned,
        Pruned {
            joints: 1,
            nodes: 3,
            shapes: 2
        }
    );
    // Spare lost its only reference (a joint without weight); Lone and Gone had none. Tip is simulated by the chain.
    assert_eq!(
        names(&a),
        ["Root", "Body", "Hips", "Group", "Target", "Toggle", "Tail", "Tip"]
    );
    let parents: Vec<_> = a.hierarchy.nodes.iter().map(|n| n.parent).collect();
    assert_eq!(
        parents,
        [None, Some(0), Some(0), Some(0), Some(3), Some(0), Some(2), Some(6)]
    );
    let skin = a.meshes[0].skin.as_ref().unwrap();
    assert_eq!(skin.joints, [2]);
    assert_eq!(skin.vertex_joints, [[0, 0, 0, 0], [0, 0, 0, 0]]);
    assert_eq!((a.constraints[0].target, a.constraints[0].sources[0].node), (4, 5));
    assert_eq!(a.dynamics.chains[0].root, 6);
    // Animated, resting at a weight, face tracking (any case) and face-bound shapes stay.
    let shapes: Vec<_> = a.meshes[0].blend_shapes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(shapes, ["Smile", "Rest", "jawopen", "Blink"]);
    assert_eq!(a.face.as_ref().unwrap().blink[0].shape, 3);

    // A second pass finds nothing.
    assert_eq!(prune(&mut a), Pruned::default());
}

#[test]
fn corpus_avatars_stay_valid_and_bind_the_same() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let mut a = civbb_corpus::generate(&spec);
        let before = civbb_anim::bind(&a);
        let pruned = prune(&mut a);
        valid(&a);
        let after = civbb_anim::bind(&a);
        assert_eq!(after.unbound, before.unbound, "{}", spec.name);
        assert_eq!(after.bindings.defaults, before.bindings.defaults, "{}", spec.name);
        eprintln!("{}: {pruned:?}", spec.name);
    }
}
