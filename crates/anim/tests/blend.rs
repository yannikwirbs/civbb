//! Blend tree weights, hand-derived (ADR 0009): 1D, 2D simple directional, freeform cartesian and polar gradient
//! bands, direct; plus nesting, time sync and length inside a state.

use civbb_anim::motion::{gradient_bands, length, simple_1d, simple_directional, weights};
use civbb_anim::{Controller, Instance};
use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::Transform;

fn close(a: &[f32], b: &[f32]) {
    assert!(
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4),
        "{a:?} != {b:?}"
    );
}

#[test]
fn one_d_is_linear_between_neighbours_and_clamped_outside() {
    let th = [3.0, 0.0, 1.0]; // Unsorted on purpose.
    close(&simple_1d(&th, -1.0), &[0.0, 1.0, 0.0]);
    close(&simple_1d(&th, 0.25), &[0.0, 0.75, 0.25]);
    close(&simple_1d(&th, 2.0), &[0.5, 0.0, 0.5]);
    close(&simple_1d(&th, 1.0), &[0.0, 0.0, 1.0]);
    close(&simple_1d(&th, 5.0), &[1.0, 0.0, 0.0]);
}

#[test]
fn cartesian_gradient_bands() {
    let two = [[0.0, 0.0], [1.0, 0.0]];
    close(&gradient_bands(&two, [0.25, 0.0], false), &[0.75, 0.25]);
    // Beyond the second point its band is above 1 and the first's below 0 (clamped).
    close(&gradient_bands(&two, [2.0, 0.0], false), &[0.0, 1.0]);
    // Every influence is 0.5 at the triangle's hypotenuse midpoint; normalised to thirds.
    let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    close(&gradient_bands(&tri, [0.5, 0.5], false), &[1.0 / 3.0; 3]);
    // Exact at the sample points.
    close(&gradient_bands(&tri, [0.0, 1.0], false), &[0.0, 0.0, 1.0]);
}

#[test]
fn polar_gradient_bands() {
    let pts = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    // Half speed forward: centre and forward share it; the side sample's angle band is fully crossed.
    close(&gradient_bands(&pts, [0.5, 0.0], true), &[0.5, 0.5, 0.0]);
    // Full speed at 45 degrees: halfway between the two directions, nothing at the centre.
    let d = std::f32::consts::FRAC_1_SQRT_2;
    close(&gradient_bands(&pts, [d, d], true), &[0.0, 0.5, 0.5]);
    // Speed changes along one direction blend radially: walk (1) and run (2) forward.
    let line = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
    // Centre to walk: 1 - |p| / |walk|; walk to run: magnitude term (1.5 - 1) / 1.5 over (2 - 1) / 1.5.
    close(&gradient_bands(&line, [1.5, 0.0], true), &[0.0, 0.5, 0.5]);
}

#[test]
fn simple_directional_uses_the_two_neighbouring_directions_and_the_centre() {
    let pts = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];
    close(&simple_directional(&pts, [0.25, 0.5]), &[0.25, 0.25, 0.5, 0.0, 0.0]);
    // Outside the triangle the two directions are normalised.
    close(&simple_directional(&pts, [1.0, 1.0]), &[0.0, 0.5, 0.5, 0.0, 0.0]);
    // Wrapping around the -180/180 seam: between left and down.
    close(&simple_directional(&pts, [-0.5, -0.25]), &[0.25, 0.0, 0.0, 0.5, 0.25]);
    close(&simple_directional(&pts, [0.0, 0.0]), &[1.0, 0.0, 0.0, 0.0, 0.0]);
}

fn child(motion: Motion) -> ChildMotion {
    ChildMotion {
        motion,
        threshold: 0.0,
        position: [0.0, 0.0],
        direct_param: None,
        time_scale: 1.0,
        cycle_offset: 0.0,
    }
}

#[test]
fn direct_weights_come_from_parameters_and_may_be_normalised() {
    let mut tree = BlendTree {
        kind: BlendTreeKind::Direct,
        children: (0..3)
            .map(|i| ChildMotion {
                direct_param: Some(i),
                ..child(Motion::Clip(0))
            })
            .collect(),
        normalize: false,
    };
    tree.children[2].direct_param = None;
    let params = [0.2, 0.6, 9.0];
    close(&weights(&tree, &params), &[0.2, 0.6, 0.0]);
    tree.normalize = true;
    close(&weights(&tree, &params), &[0.25, 0.75, 0.0]);
}

/// Node `A`; clip 0 holds A.x at 0 (length 1 s), clip 1 ramps A.x 0 -> 10 (length 3 s), clip 2 holds A.y at 4.
fn avatar(motion: Motion) -> Avatar {
    let key = |time, value, slope| Key {
        time,
        value,
        in_tangent: slope,
        out_tangent: slope,
    };
    let curve = |property, keys| Curve {
        binding: Binding {
            path: "A".into(),
            property,
        },
        keys,
    };
    let clip = |length, curves| Clip {
        name: String::new(),
        length,
        looping: true,
        curves,
    };
    let node = |name: &str, parent| Node {
        name: name.into(),
        parent,
        rest: Transform::IDENTITY,
        active: true,
    };
    let param = |name: &str| Parameter {
        name: name.into(),
        kind: ParamKind::Float,
        default: 0.0,
    };
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![node("Root", None), node("A", Some(0))],
        },
        clips: vec![
            clip(1.0, vec![curve(Property::Position(0), vec![key(0.0, 0.0, 0.0)])]),
            clip(
                3.0,
                vec![curve(
                    Property::Position(0),
                    vec![key(0.0, 0.0, 10.0 / 3.0), key(3.0, 10.0, 10.0 / 3.0)],
                )],
            ),
            clip(1.0, vec![curve(Property::Position(1), vec![key(0.0, 4.0, 0.0)])]),
        ],
        animator: Some(Animator {
            parameters: vec![param("p"), param("q")],
            groups: vec![LayerGroup {
                name: "g".into(),
                weight: 1.0,
            }],
            layers: vec![Layer {
                name: "L".into(),
                group: 0,
                weight: 1.0,
                blending: LayerBlending::Override,
                mask: None,
                state_machine: StateMachine {
                    states: vec![State {
                        name: "S".into(),
                        motion: Some(motion),
                        speed: 1.0,
                        speed_param: None,
                        cycle_offset: 0.0,
                        cycle_offset_param: None,
                        time_param: None,
                        write_defaults: true,
                        transitions: Vec::new(),
                        behaviours: Vec::new(),
                    }],
                    default_state: 0,
                    entry_transitions: Vec::new(),
                    any_state_transitions: Vec::new(),
                },
            }],
        }),
        ..Avatar::default()
    }
}

fn tree_1d() -> Motion {
    let mut ramp = child(Motion::Clip(1));
    ramp.threshold = 1.0;
    Motion::BlendTree(Box::new(BlendTree {
        kind: BlendTreeKind::Simple1D { param: 0 },
        children: vec![child(Motion::Clip(0)), ramp],
        normalize: false,
    }))
}

#[test]
fn blend_tree_states_sync_children_and_average_lengths() {
    let a = avatar(tree_1d());
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true).unwrap();
    let c = Controller::new(&a).unwrap();
    let mut i = Instance::new(&c);
    i.params[0] = 0.5;
    // Length is the weighted average: 0.5 * 1 + 0.5 * 3 = 2 s.
    assert_eq!(
        length(
            c.animator.layers[0].state_machine.states[0].motion.as_ref(),
            &c.bound.clips,
            &i.params
        ),
        2.0
    );
    i.update(&c, 0.0);
    i.update(&c, 0.5);
    // Normalised time 0.25: the ramp is at 0.75 s = 2.5, blended half with 0.
    assert!((i.pose()[0] - 1.25).abs() < 1e-4, "{}", i.pose()[0]);
}

#[test]
fn nested_and_direct_trees_multiply_weights_and_leave_the_rest_at_default() {
    // Direct: child 0 is the 1D tree at weight q, child 1 holds A.y = 4 at weight p.
    let mut inner = child(tree_1d());
    inner.direct_param = Some(1);
    let mut y = child(Motion::Clip(2));
    y.direct_param = Some(0);
    let a = avatar(Motion::BlendTree(Box::new(BlendTree {
        kind: BlendTreeKind::Direct,
        children: vec![inner, y],
        normalize: false,
    })));
    let c = Controller::new(&a).unwrap();
    let mut i = Instance::new(&c);
    i.params = vec![0.5, 0.0];
    i.update(&c, 0.0);
    // The 1D tree has weight 0, so it writes nothing; A.y is half of 4 with the remainder at its default 0.
    assert_eq!(c.bound.bindings.targets.len(), 2);
    let ys = c
        .bound
        .bindings
        .targets
        .iter()
        .position(|t| *t == civbb_anim::Target::Position(1, 1))
        .unwrap();
    assert!((i.pose()[ys] - 2.0).abs() < 1e-5);
}
