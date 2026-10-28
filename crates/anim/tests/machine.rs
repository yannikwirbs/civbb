//! Mecanim state machine conformance, hand-derived from the Unity manual (ADR 0008): entry, exit time, durations,
//! conditions, triggers, any-state, exit, interruption sources, ordered interruption, layers, masks.

use civbb_anim::machine::From;
use civbb_anim::{Controller, Instance};
use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::Transform;

/// Clip `k` holds `A.x` at `k + 1` (so the pose names the state); 1 s long.
fn value_clip(k: usize, looping: bool) -> Clip {
    keyed_clip(&[(0.0, k as f32 + 1.0)], 1.0, looping, "A", Property::Position(0))
}

fn keyed_clip(keys: &[(f32, f32)], length: f32, looping: bool, path: &str, property: Property) -> Clip {
    let slope = |i: usize, j: usize| (keys[j].1 - keys[i].1) / (keys[j].0 - keys[i].0);
    Clip {
        name: String::new(),
        length,
        looping,
        curves: vec![Curve {
            binding: Binding {
                path: path.into(),
                property,
            },
            keys: (0..keys.len())
                .map(|i| Key {
                    time: keys[i].0,
                    value: keys[i].1,
                    in_tangent: if i > 0 { slope(i - 1, i) } else { 0.0 },
                    out_tangent: if i + 1 < keys.len() { slope(i, i + 1) } else { 0.0 },
                })
                .collect(),
        }],
    }
}

fn state(clip: u32) -> State {
    State {
        name: format!("S{clip}"),
        motion: Some(Motion::Clip(clip)),
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        write_defaults: true,
        transitions: Vec::new(),
        behaviours: Vec::new(),
    }
}

fn to(dest: u32, conditions: Vec<Condition>) -> Transition {
    Transition {
        dest: TransitionDest::State(dest),
        conditions,
        has_exit_time: false,
        exit_time: 0.0,
        fixed_duration: true,
        duration: 0.0,
        offset: 0.0,
        interruption: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: false,
    }
}

fn exit_at(dest: u32, exit_time: f32) -> Transition {
    Transition {
        has_exit_time: true,
        exit_time,
        ..to(dest, vec![])
    }
}

fn when(param: u32) -> Condition {
    Condition {
        param,
        mode: ConditionMode::If,
        threshold: 0.0,
    }
}

fn layer(states: Vec<State>) -> Layer {
    Layer {
        name: "L".into(),
        group: 0,
        weight: 1.0,
        blending: LayerBlending::Override,
        mask: None,
        state_machine: StateMachine {
            states,
            default_state: 0,
            entry_transitions: Vec::new(),
            any_state_transitions: Vec::new(),
        },
    }
}

/// Params 0..4 are bools `b0`..`b3`, 4 is trigger `t`, 5 is float `f`.
fn avatar(layers: Vec<Layer>, clips: Vec<Clip>) -> Avatar {
    let node = |name: &str, parent| Node {
        name: name.into(),
        parent,
        rest: Transform::IDENTITY,
        active: true,
    };
    let mut parameters: Vec<Parameter> = (0..4)
        .map(|i| Parameter {
            name: format!("b{i}"),
            kind: ParamKind::Bool,
            default: 0.0,
        })
        .collect();
    parameters.push(Parameter {
        name: "t".into(),
        kind: ParamKind::Trigger,
        default: 0.0,
    });
    parameters.push(Parameter {
        name: "f".into(),
        kind: ParamKind::Float,
        default: 0.0,
    });
    let groups = (0..2)
        .map(|i| LayerGroup {
            name: format!("g{i}"),
            weight: 1.0,
        })
        .collect();
    let a = Avatar {
        hierarchy: Hierarchy {
            nodes: vec![node("Root", None), node("A", Some(0)), node("B", Some(0))],
        },
        animator: Some(Animator {
            parameters,
            groups,
            layers,
        }),
        clips,
        ..Avatar::default()
    };
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true).expect("valid test avatar");
    a
}

/// One layer over clips 0..n (values 1..=n), non-looping unless `looping`.
fn single(states: Vec<State>, looping: bool) -> (Controller, Instance) {
    let n = states.len();
    run(avatar(
        vec![layer(states)],
        (0..n).map(|k| value_clip(k, looping)).collect(),
    ))
}

fn run(a: Avatar) -> (Controller, Instance) {
    let c = Controller::new(&a).expect("animator");
    let i = Instance::new(&c, true);
    (c, i)
}

fn x(i: &Instance) -> f32 {
    i.pose()[0]
}

fn current(i: &Instance) -> u32 {
    i.layer(0).unwrap().current
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

#[test]
fn starts_in_the_default_state_or_the_first_matching_entry() {
    let (c, mut i) = single(vec![state(0), state(1), state(2)], false);
    i.update(&c, 0.1);
    assert_eq!((current(&i), x(&i)), (0, 1.0));

    let mut l = layer(vec![state(0), state(1), state(2)]);
    l.state_machine.entry_transitions = vec![to(1, vec![when(0)]), to(2, vec![when(1)])];
    let (c, mut i) = run(avatar(vec![l], (0..3).map(|k| value_clip(k, false)).collect()));
    i.params[1] = 1.0;
    i.update(&c, 0.1);
    assert_eq!((current(&i), x(&i)), (2, 3.0));
}

#[test]
fn a_zero_duration_transition_switches_in_the_frame_its_condition_holds() {
    let mut s0 = state(0);
    s0.transitions = vec![to(1, vec![when(0)])];
    let (c, mut i) = single(vec![s0, state(1)], false);
    i.update(&c, 0.1);
    i.update(&c, 0.1);
    assert_eq!(x(&i), 1.0);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert_eq!((current(&i), x(&i)), (1, 2.0));
}

#[test]
fn fixed_and_normalised_cross_fades_blend_linearly() {
    // Fixed 0.5 s; normalised 0.5 of a 2 s source = 1 s.
    for (fixed, duration, total) in [(true, 0.5, 0.5), (false, 0.5, 1.0)] {
        let mut s0 = state(0);
        s0.transitions = vec![Transition {
            fixed_duration: fixed,
            duration,
            ..to(1, vec![when(0)])
        }];
        let mut clips = vec![value_clip(0, true), value_clip(1, true)];
        clips[0].length = 2.0;
        let (c, mut i) = run(avatar(vec![layer(vec![s0, state(1)])], clips));
        i.update(&c, 0.1);
        i.params[0] = 1.0;
        i.update(&c, 0.1);
        // Fired this frame at progress 0: still the source.
        assert_eq!(x(&i), 1.0);
        i.update(&c, total / 2.0);
        close(x(&i), 1.5);
        let a = i.layer(0).unwrap().active.unwrap();
        close(a.progress, 0.5);
        i.update(&c, total / 2.0);
        assert!(i.layer(0).unwrap().active.is_none());
        assert_eq!((current(&i), x(&i)), (1, 2.0));
    }
}

#[test]
fn transition_offset_sets_the_destination_start_time() {
    let mut s0 = state(0);
    s0.transitions = vec![Transition {
        offset: 0.25,
        ..to(1, vec![when(0)])
    }];
    let (c, mut i) = single(vec![s0, state(1)], false);
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    let From::State(p) = &i.layer(0).unwrap().from else {
        panic!()
    };
    assert_eq!((p.state, p.time), (1, 0.25));
}

#[test]
fn exit_time_fires_on_the_first_frame_that_reaches_it() {
    let mut s0 = state(0);
    s0.transitions = vec![exit_at(1, 0.75)];
    let (c, mut i) = single(vec![s0, state(1)], false);
    i.update(&c, 0.1); // Enter at 0.
    for _ in 0..7 {
        i.update(&c, 0.1);
    }
    assert_eq!(current(&i), 0, "0.7 < 0.75");
    i.update(&c, 0.1);
    assert_eq!(current(&i), 1, "0.8 >= 0.75");
}

#[test]
fn exit_time_zero_never_fires_on_the_first_frame() {
    let mut s0 = state(0);
    s0.transitions = vec![exit_at(1, 0.0)];
    let (c, mut i) = single(vec![s0, state(1)], false);
    i.update(&c, 0.1);
    assert_eq!(current(&i), 0);
    i.update(&c, 0.1);
    assert_eq!(current(&i), 1);
}

#[test]
fn exit_time_below_one_repeats_every_loop_only_when_looping() {
    for looping in [true, false] {
        let mut s0 = state(0);
        s0.transitions = vec![Transition {
            conditions: vec![when(0)],
            ..exit_at(1, 0.5)
        }];
        let (c, mut i) = single(vec![s0, state(1)], looping);
        i.update(&c, 0.25);
        for _ in 0..3 {
            i.update(&c, 0.25); // Through 0.75: the crossing at 0.5 had the condition false.
        }
        i.params[0] = 1.0;
        for _ in 0..2 {
            i.update(&c, 0.25); // 1.0, 1.25: not yet the next 0.5 point.
        }
        assert_eq!(current(&i), 0);
        i.update(&c, 0.25); // 1.5
        assert_eq!(current(&i), if looping { 1 } else { 0 }, "looping {looping}");
    }
}

#[test]
fn exit_time_above_one_is_checked_once() {
    let mut s0 = state(0);
    s0.transitions = vec![exit_at(1, 1.5)];
    let (c, mut i) = single(vec![s0, state(1)], true);
    i.update(&c, 0.25);
    for _ in 0..5 {
        i.update(&c, 0.25); // Through 1.25, past the 0.5 point.
    }
    assert_eq!(current(&i), 0);
    i.update(&c, 0.25);
    assert_eq!(current(&i), 1);
}

#[test]
fn speed_and_speed_parameter_scale_normalised_time() {
    let mut s0 = state(0);
    s0.speed = 2.0;
    s0.speed_param = Some(5);
    s0.transitions = vec![exit_at(1, 0.75)];
    let (c, mut i) = single(vec![s0, state(1)], false);
    i.params[5] = 2.0;
    i.update(&c, 0.1);
    i.update(&c, 0.1); // 0.4
    assert_eq!(current(&i), 0);
    i.update(&c, 0.1); // 0.8
    assert_eq!(current(&i), 1);
}

#[test]
fn transitions_without_conditions_or_exit_time_are_ignored() {
    let mut s0 = state(0);
    s0.transitions = vec![to(1, vec![])];
    let (c, mut i) = single(vec![s0, state(1)], false);
    for _ in 0..5 {
        i.update(&c, 0.1);
    }
    assert_eq!(current(&i), 0);
}

#[test]
fn triggers_stay_set_until_a_transition_consumes_them() {
    let mut s0 = state(0);
    s0.transitions = vec![to(1, vec![when(4), when(0)])];
    let mut s1 = state(1);
    s1.transitions = vec![to(0, vec![when(1)])];
    let (c, mut i) = single(vec![s0, s1], false);
    i.update(&c, 0.1);
    i.params[4] = 1.0;
    i.update(&c, 0.1);
    i.update(&c, 0.1);
    assert_eq!((current(&i), i.params[4]), (0, 1.0), "b0 false: trigger not consumed");
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert_eq!((current(&i), i.params[4]), (1, 0.0), "consumed by the transition");
}

#[test]
fn any_state_transitions_come_first_and_respect_can_transition_to_self() {
    let mut s0 = state(0);
    s0.transitions = vec![to(1, vec![when(0)])];
    let mut l = layer(vec![s0, state(1), state(2)]);
    l.state_machine.any_state_transitions = vec![to(2, vec![when(0)])];
    let (c, mut i) = run(avatar(vec![l.clone()], (0..3).map(|k| value_clip(k, false)).collect()));
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(current(&i), 2, "any-state beats the state's own transition");
    for _ in 0..3 {
        i.update(&c, 0.1);
    }
    let From::State(p) = &i.layer(0).unwrap().from else {
        panic!()
    };
    close(p.time, 0.3); // Not re-entered.

    l.state_machine.any_state_transitions[0].can_transition_to_self = true;
    let (c, mut i) = run(avatar(vec![l], (0..3).map(|k| value_clip(k, false)).collect()));
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    for _ in 0..3 {
        i.update(&c, 0.1);
    }
    let From::State(p) = &i.layer(0).unwrap().from else {
        panic!()
    };
    assert_eq!((p.state, p.time), (2, 0.0), "re-entered every frame");
}

#[test]
fn exit_runs_the_entry_transitions_again() {
    let mut s0 = state(0);
    s0.transitions = vec![Transition {
        dest: TransitionDest::Exit,
        ..to(0, vec![when(0)])
    }];
    let mut l = layer(vec![s0, state(1), state(2)]);
    l.state_machine.entry_transitions = vec![to(2, vec![when(1)])];
    let (c, mut i) = run(avatar(vec![l], (0..3).map(|k| value_clip(k, false)).collect()));
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.params[1] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(current(&i), 2);
}

/// S0 -> S1 (b1, 1 s, the transition under test) with S0's transitions [S0 -> S2 (b0), S0 -> S1, S0 -> S3 (b2)]
/// and S1's [S1 -> S3 (b3)].
fn interruptible(source: InterruptionSource, ordered: bool) -> (Controller, Instance) {
    let mut s0 = state(0);
    let fade = |dest, cond| Transition {
        duration: 1.0,
        interruption: source,
        ordered_interruption: ordered,
        ..to(dest, vec![when(cond)])
    };
    s0.transitions = vec![fade(2, 0), fade(1, 1), fade(3, 2)];
    let mut s1 = state(1);
    s1.transitions = vec![fade(3, 3)];
    let (c, mut i) = single(vec![s0, s1, state(2), state(3)], false);
    i.update(&c, 0.1);
    i.params[1] = 1.0;
    i.update(&c, 0.1);
    i.update(&c, 0.5); // Halfway: 1 * 0.5 + 2 * 0.5.
    close(x(&i), 1.5);
    i.params[1] = 0.0;
    (c, i)
}

fn interrupted_to(i: &Instance) -> Option<u32> {
    let l = i.layer(0).unwrap();
    matches!(l.from, From::Snapshot(_)).then(|| l.active.unwrap().to.state)
}

#[test]
fn interruption_source_none_lets_nothing_but_any_state_interrupt() {
    let (c, mut i) = interruptible(InterruptionSource::None, true);
    i.params[0] = 1.0;
    i.params[3] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), None);
    assert_eq!(i.layer(0).unwrap().active.unwrap().to.state, 1);
}

#[test]
fn ordered_source_interruption_allows_only_higher_priority_transitions() {
    let (c, mut i) = interruptible(InterruptionSource::Source, true);
    i.params[2] = 1.0; // S0 -> S3 is listed after the running transition.
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), None);
    i.params[0] = 1.0; // S0 -> S2 is listed before it.
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(2));
}

#[test]
fn unordered_source_interruption_allows_any_source_transition() {
    let (c, mut i) = interruptible(InterruptionSource::Source, false);
    i.params[2] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(3));
}

#[test]
fn destination_interruption_uses_the_next_state_transitions() {
    let (c, mut i) = interruptible(InterruptionSource::Destination, true);
    i.params[0] = 1.0; // A source transition: not in the queue.
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), None);
    i.params[3] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(3));
}

#[test]
fn source_then_destination_checks_the_source_first() {
    let (c, mut i) = interruptible(InterruptionSource::SourceThenDestination, true);
    i.params[0] = 1.0;
    i.params[3] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(2));
    let (c, mut i) = interruptible(InterruptionSource::DestinationThenSource, true);
    i.params[0] = 1.0;
    i.params[3] = 1.0;
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(3));
}

#[test]
fn an_interruption_blends_from_a_static_snapshot() {
    let (c, mut i) = interruptible(InterruptionSource::Source, true);
    i.update(&c, 0.1); // Progress 0.6: 1.6.
    close(x(&i), 1.6);
    i.params[0] = 1.0;
    i.update(&c, 0.1); // Progress 0.7 (1.7) is snapshotted; S2 (3) starts at 0.
    close(x(&i), 1.7);
    i.update(&c, 0.25);
    close(x(&i), 0.75 * 1.7 + 0.25 * 3.0);
    i.update(&c, 0.75);
    assert_eq!((current(&i), x(&i)), (2, 3.0));
}

#[test]
fn motion_time_and_cycle_offset_pick_the_sample_time() {
    // A.x goes 0 -> 10 over the clip.
    let clip = keyed_clip(&[(0.0, 0.0), (1.0, 10.0)], 1.0, false, "A", Property::Position(0));
    let mut s = state(0);
    s.time_param = Some(5);
    s.cycle_offset = 0.25;
    let (c, mut i) = run(avatar(vec![layer(vec![s])], vec![clip]));
    i.params[5] = 0.5;
    i.update(&c, 0.1);
    close(x(&i), 7.5);
    i.params[5] = 0.1;
    i.update(&c, 0.1);
    close(x(&i), 3.5);
}

#[test]
fn layers_override_by_weight_and_add_deltas_within_masks() {
    let ramp = keyed_clip(&[(0.0, 10.0), (1.0, 12.0)], 1.0, false, "A", Property::Position(0));
    let clips = vec![value_clip(0, false), value_clip(2, false), ramp];
    let mut base = layer(vec![state(0)]);
    base.weight = 0.0; // Ignored: the first layer of a group is always 1.
    let mut over = layer(vec![state(1)]);
    over.weight = 0.5;
    let mut add = layer(vec![state(2)]);
    add.blending = LayerBlending::Additive;
    add.weight = 0.5;
    let (c, mut i) = run(avatar(vec![base.clone(), over.clone(), add.clone()], clips.clone()));
    i.update(&c, 0.0);
    // 1, then halfway to 3, then + 0.5 * (10 - 10).
    close(x(&i), 2.0);
    i.update(&c, 0.5);
    // Additive delta at t = 0.5 is 11 - 10 = 1.
    close(x(&i), 2.5);

    // Masked to `B` only, the override layer cannot write `A`.
    over.mask = Some(AvatarMask {
        body_parts: Vec::new(),
        transforms: Some(vec!["B".into()]),
    });
    let (c, mut i) = run(avatar(vec![base.clone(), over], clips.clone()));
    i.update(&c, 0.1);
    close(x(&i), 1.0);

    // A group weight of 0 disables the group's layers.
    let mut over = layer(vec![state(1)]);
    over.group = 1;
    let (c, mut i) = run(avatar(vec![base, over], clips));
    i.group_weights[1] = 0.0;
    i.update(&c, 0.1);
    close(x(&i), 1.0);
    i.group_weights[1] = 1.0;
    i.update(&c, 0.1);
    close(x(&i), 3.0);
}

#[test]
fn animated_parameters_take_effect_next_frame() {
    let drive = keyed_clip(&[(0.0, 1.0)], 1.0, false, "", Property::Parameter("f".into()));
    let mut s0 = state(0);
    s0.transitions = vec![to(
        1,
        vec![Condition {
            param: 5,
            mode: ConditionMode::Greater,
            threshold: 0.5,
        }],
    )];
    let (c, mut i) = run(avatar(
        vec![layer(vec![s0, state(1)])],
        vec![drive, value_clip(1, false)],
    ));
    i.update(&c, 0.1);
    assert_eq!((current(&i), i.params[5]), (0, 1.0));
    i.update(&c, 0.1);
    assert_eq!(current(&i), 1);
}

fn rotation_clip(q: [f32; 4]) -> Clip {
    let mut c = keyed_clip(&[(0.0, q[0])], 1.0, false, "A", Property::Rotation(0));
    for (k, &v) in q.iter().enumerate().skip(1) {
        let extra = keyed_clip(&[(0.0, v)], 1.0, false, "A", Property::Rotation(k as u8));
        c.curves.extend(extra.curves);
    }
    c
}

#[test]
fn rotations_cross_fade_in_one_hemisphere() {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    // 90 degrees about Y, stored negated: the same rotation.
    let clips = vec![rotation_clip([0.0, 0.0, 0.0, 1.0]), rotation_clip([0.0, -h, 0.0, -h])];
    let mut s0 = state(0);
    s0.transitions = vec![Transition {
        duration: 1.0,
        ..to(1, vec![when(0)])
    }];
    let (c, mut i) = run(avatar(vec![layer(vec![s0, state(1)])], clips));
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    i.update(&c, 0.5);
    let (s, co) = 22.5f32.to_radians().sin_cos();
    let q = &i.pose()[..4];
    for (a, b) in q.iter().zip([0.0, s, 0.0, co]) {
        close(*a, b);
    }
}

#[test]
fn only_an_any_state_transition_interrupts_itself() {
    let mut l = layer(vec![state(0), state(1)]);
    l.state_machine.any_state_transitions = vec![Transition {
        duration: 1.0,
        can_transition_to_self: true,
        ..to(1, vec![when(0)])
    }];
    let (c, mut i) = run(avatar(vec![l], (0..2).map(|k| value_clip(k, false)).collect()));
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert!(matches!(i.layer(0).unwrap().from, From::State(_)));
    i.update(&c, 0.1);
    assert_eq!(interrupted_to(&i), Some(1));
    close(i.layer(0).unwrap().active.unwrap().progress, 0.0);
}
