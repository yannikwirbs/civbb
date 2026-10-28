//! Built-in parameters and state behaviours (ADR 0011): names and types, parameter drivers, local-only drivers,
//! layer and group weight control, tracking and locomotion control.

use civbb_anim::{Builtin, Controller, Instance};
use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::Transform;

fn param(name: &str, kind: ParamKind) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        default: 0.0,
    }
}

fn state(behaviours: Vec<Behaviour>, transitions: Vec<Transition>) -> State {
    State {
        name: String::new(),
        motion: None,
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        write_defaults: true,
        transitions,
        behaviours,
    }
}

fn layer(states: Vec<State>, group: u32) -> Layer {
    Layer {
        name: String::new(),
        group,
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

fn controller(parameters: Vec<Parameter>, layers: Vec<Layer>) -> Controller {
    let a = Avatar {
        hierarchy: Hierarchy {
            nodes: vec![Node {
                name: "Root".into(),
                parent: None,
                rest: Transform::IDENTITY,
                active: true,
            }],
        },
        animator: Some(Animator {
            parameters,
            groups: (0..2)
                .map(|i| LayerGroup {
                    name: format!("g{i}"),
                    weight: 1.0,
                })
                .collect(),
            layers,
        }),
        ..Avatar::default()
    };
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true).unwrap();
    Controller::new(&a).unwrap()
}

#[test]
fn builtins_have_legacy_names_and_convert_to_the_declared_type() {
    let names: Vec<&str> = Builtin::ALL.iter().map(|b| b.name()).collect();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len());
    assert!(names.contains(&"AFK") && names.contains(&"VRMode") && names.contains(&"GestureLeftWeight"));
    assert_eq!(Builtin::GestureLeft.kind(), ParamKind::Int);
    assert_eq!(Builtin::Upright.kind(), ParamKind::Float);
    assert_eq!(Builtin::Seated.kind(), ParamKind::Bool);

    let params = vec![
        param("IsLocal", ParamKind::Bool),
        param("GestureLeft", ParamKind::Int),
        param("VelocityX", ParamKind::Float),
        // Declared with another type than the engine's: converted to the declared one.
        param("GestureRightWeight", ParamKind::Bool),
    ];
    let c = controller(params, vec![layer(vec![state(vec![], vec![])], 0)]);
    assert_eq!(Instance::new(&c, false).params[0], 0.0);
    let mut i = Instance::new(&c, true);
    assert_eq!(i.params[0], 1.0);
    i.set_builtin(&c, Builtin::GestureLeft, 3.4);
    i.set_builtin(&c, Builtin::VelocityX, 1.5);
    i.set_builtin(&c, Builtin::GestureRightWeight, 0.3);
    i.set_builtin(&c, Builtin::Viseme, 7.0); // Not declared: ignored.
    assert_eq!(i.params, [1.0, 3.0, 1.5, 1.0]);
}

fn driver(local_only: bool, ops: Vec<DriverOp>) -> Behaviour {
    Behaviour::ParameterDriver { local_only, ops }
}

#[test]
fn parameter_drivers_run_on_enter_in_order() {
    let params = vec![
        param("f", ParamKind::Float),
        param("i", ParamKind::Int),
        param("b", ParamKind::Bool),
        param("r", ParamKind::Float),
        param("n", ParamKind::Int),
        param("local", ParamKind::Float),
    ];
    let ops = vec![
        DriverOp::Set { param: 0, value: 2.0 },
        DriverOp::Add { param: 0, value: 0.5 },
        // 2.5 in 0..5 maps to 5 in 0..10.
        DriverOp::Copy {
            src: 0,
            dst: 1,
            remap: Some(([0.0, 5.0], [0.0, 10.0])),
        },
        DriverOp::Random {
            param: 2,
            min: 0.0,
            max: 0.0,
            chance: 1.0,
        },
        DriverOp::Random {
            param: 3,
            min: 3.0,
            max: 3.0,
            chance: 0.0,
        },
        DriverOp::Random {
            param: 4,
            min: 2.0,
            max: 4.0,
            chance: 0.0,
        },
    ];
    let behaviours = vec![
        driver(false, ops),
        driver(true, vec![DriverOp::Set { param: 5, value: 1.0 }]),
    ];
    let c = controller(params, vec![layer(vec![state(behaviours, vec![])], 0)]);
    for local in [true, false] {
        let mut i = Instance::new(&c, local);
        i.update(&c, 0.1);
        assert_eq!(i.params[..4], [2.5, 5.0, 1.0, 3.0]);
        assert!([2.0, 3.0, 4.0].contains(&i.params[4]), "{}", i.params[4]);
        assert_eq!(i.params[5], if local { 1.0 } else { 0.0 }, "local-only driver");
        // Entering once runs once.
        i.update(&c, 0.1);
        assert_eq!(i.params[0], 2.5);
    }
}

#[test]
fn random_ints_cover_their_inclusive_range() {
    let params = vec![param("n", ParamKind::Int), param("go", ParamKind::Bool)];
    let reenter = Transition {
        dest: TransitionDest::State(0),
        conditions: vec![Condition {
            param: 1,
            mode: ConditionMode::IfNot,
            threshold: 0.0,
        }],
        has_exit_time: false,
        exit_time: 0.0,
        fixed_duration: true,
        duration: 0.0,
        offset: 0.0,
        interruption: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: true,
    };
    let random = DriverOp::Random {
        param: 0,
        min: 0.0,
        max: 2.0,
        chance: 0.0,
    };
    let s = state(vec![driver(false, vec![random])], vec![]);
    let mut l = layer(vec![s], 0);
    l.state_machine.any_state_transitions = vec![reenter];
    let c = controller(params, vec![l]);
    let mut i = Instance::new(&c, true);
    let mut seen = [0; 3];
    for _ in 0..300 {
        i.update(&c, 0.1);
        seen[i.params[0] as usize] += 1;
    }
    assert!(seen.iter().all(|&n| n > 50), "{seen:?}");
}

#[test]
fn layer_and_group_weight_control_blend_to_their_goals() {
    let go = Transition {
        dest: TransitionDest::State(1),
        conditions: vec![Condition {
            param: 0,
            mode: ConditionMode::If,
            threshold: 0.0,
        }],
        has_exit_time: false,
        exit_time: 0.0,
        fixed_duration: true,
        duration: 0.0,
        offset: 0.0,
        interruption: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: false,
    };
    let control = vec![
        Behaviour::LayerWeight {
            layer: 1,
            goal: 0.0,
            duration: 1.0,
        },
        Behaviour::GroupWeight {
            group: 1,
            goal: 0.25,
            duration: 0.0,
        },
        Behaviour::TrackingControl(vec![(TrackingPart::LeftHand, TrackingMode::Animation)]),
        Behaviour::Locomotion { disabled: true },
    ];
    let base = layer(vec![state(vec![], vec![go]), state(control, vec![])], 0);
    let other = layer(vec![state(vec![], vec![])], 0);
    let c = controller(vec![param("go", ParamKind::Bool)], vec![base, other]);
    let mut i = Instance::new(&c, true);
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert_eq!((i.layer_weights[1], i.group_weights[1]), (1.0, 0.25));
    assert_eq!(i.tracking[TrackingPart::LeftHand as usize], TrackingMode::Animation);
    assert_eq!(i.tracking[TrackingPart::Head as usize], TrackingMode::Tracking);
    assert!(i.locomotion_disabled);
    i.update(&c, 0.5);
    assert!((i.layer_weights[1] - 0.5).abs() < 1e-6);
    i.update(&c, 0.6);
    assert_eq!(i.layer_weights[1], 0.0);
}
