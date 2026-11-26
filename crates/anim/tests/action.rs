//! Platform values (ADR 0096) and state actions (ADR 0011): names and types, entry operations, owner-only actions,
//! layer and group weights, pose sources and locomotion.

use civbb_anim::{Controller, Instance};
use civbb_format::Document;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::Transform;
use civbb_format::state::{Entry, EntryKind, Platform, StateTable};

fn param(name: &str, kind: EntryKind) -> Entry {
    Entry::new(name, kind, 0.0)
}

fn platform(p: Platform, kind: EntryKind) -> Entry {
    param(p.name(), kind)
}

fn state(actions: Vec<Action>, transitions: Vec<Transition>) -> State {
    State {
        name: String::new(),
        motion: None,
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        unwritten: Unwritten::Rest,
        mirror: false,
        mirror_param: None,
        transitions,
        actions,
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

fn controller(entries: Vec<Entry>, layers: Vec<Layer>) -> Controller {
    let a = Document {
        state: StateTable { entries },
        hierarchy: Hierarchy {
            nodes: vec![Node {
                name: "Root".into(),
                parent: None,
                rest: Transform::IDENTITY,
                active: true,
            }],
        },
        animator: Some(Animator {
            groups: (0..2)
                .map(|i| LayerGroup {
                    name: format!("g{i}"),
                    weight: 1.0,
                })
                .collect(),
            layers,
        }),
        ..Document::default()
    };
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true).unwrap();
    Controller::new(&a).unwrap()
}

#[test]
fn platform_values_have_unique_names_and_convert_to_the_declared_type() {
    for names in [
        Platform::ALL.map(Platform::name),
        Platform::ALL.map(Platform::vrchat_name),
    ] {
        let mut unique = names.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
    assert!(Platform::ALL.iter().all(|p| p.name().starts_with("platform/")));
    assert_eq!(Platform::from_vrchat_name("AFK"), Some(Platform::Afk));
    assert_eq!(
        Platform::from_vrchat_name("EyeHeightAsMeters"),
        Some(Platform::EyeHeight)
    );
    assert_eq!(Platform::GestureLeft.kind(), EntryKind::Int);
    assert_eq!(Platform::Upright.kind(), EntryKind::Float);
    assert_eq!(Platform::Seated.kind(), EntryKind::Bool);

    let params = vec![
        platform(Platform::IsLocal, EntryKind::Bool),
        platform(Platform::GestureLeft, EntryKind::Int),
        platform(Platform::VelocityX, EntryKind::Float),
        // Declared with another type than the engine's: converted to the declared one.
        platform(Platform::GestureRightWeight, EntryKind::Bool),
    ];
    let c = controller(params, vec![layer(vec![state(vec![], vec![])], 0)]);
    assert_eq!(Instance::new(&c, false).params[0], 0.0);
    let mut i = Instance::new(&c, true);
    assert_eq!(i.params[0], 1.0);
    i.set_platform(&c, Platform::GestureLeft, 3.4);
    i.set_platform(&c, Platform::VelocityX, 1.5);
    i.set_platform(&c, Platform::GestureRightWeight, 0.3);
    i.set_platform(&c, Platform::Viseme, 7.0); // No entry: ignored.
    assert_eq!(i.params, [1.0, 3.0, 1.5, 1.0]);
}

#[test]
fn instances_start_standing_on_the_ground() {
    // Creators often leave Grounded and Upright at 0; VRChat's locomotion would then start in SmallHop (#86).
    let params = vec![
        platform(Platform::Grounded, EntryKind::Bool),
        platform(Platform::Upright, EntryKind::Float),
        platform(Platform::AvatarVersion, EntryKind::Int),
        platform(Platform::ScaleFactor, EntryKind::Float),
        platform(Platform::VelocityY, EntryKind::Float),
        platform(Platform::Seated, EntryKind::Bool),
        param("user", EntryKind::Float),
    ];
    let c = controller(params, vec![layer(vec![state(vec![], vec![])], 0)]);
    assert_eq!(Instance::new(&c, false).params, [1.0, 1.0, 3.0, 1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn instances_start_at_the_entry_defaults_in_their_types() {
    // The Unity importer puts the expression parameter's default into `default` (#85, `gltf_import` tests).
    let params = vec![
        Entry::new("07_mat", EntryKind::Float, 0.5),
        Entry::new("WingsClosed", EntryKind::Bool, 0.75),
        Entry::new("Outfit", EntryKind::Int, 2.4),
        Entry {
            rest: Some(0.0),
            ..Entry::new("local", EntryKind::Float, 0.25)
        },
        platform(Platform::Grounded, EntryKind::Bool),
    ];
    let c = controller(params, vec![layer(vec![state(vec![], vec![])], 0)]);
    assert_eq!(c.start, [0.5, 1.0, 2.0, 0.25, 0.0]);
    assert_eq!(Instance::new(&c, false).params, [0.5, 1.0, 2.0, 0.25, 1.0]);
}

fn driver(owner_only: bool, ops: Vec<EntryOp>) -> Action {
    Action::Entries { owner_only, ops }
}

#[test]
fn parameter_drivers_run_on_enter_in_order() {
    let params = vec![
        param("f", EntryKind::Float),
        param("i", EntryKind::Int),
        param("b", EntryKind::Bool),
        param("r", EntryKind::Float),
        param("n", EntryKind::Int),
        param("local", EntryKind::Float),
    ];
    let ops = vec![
        EntryOp::Set { param: 0, value: 2.0 },
        EntryOp::Add { param: 0, value: 0.5 },
        // 2.5 in 0..5 maps to 5 in 0..10.
        EntryOp::Copy {
            src: 0,
            dst: 1,
            remap: Some(Remap {
                from: [0.0, 5.0],
                to: [0.0, 10.0],
            }),
        },
        EntryOp::Random {
            param: 2,
            min: 0.0,
            max: 0.0,
            chance: 1.0,
        },
        EntryOp::Random {
            param: 3,
            min: 3.0,
            max: 3.0,
            chance: 0.0,
        },
        EntryOp::Random {
            param: 4,
            min: 2.0,
            max: 4.0,
            chance: 0.0,
        },
    ];
    let actions = vec![
        driver(false, ops),
        driver(true, vec![EntryOp::Set { param: 5, value: 1.0 }]),
    ];
    let c = controller(params, vec![layer(vec![state(actions, vec![])], 0)]);
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
    let params = vec![param("n", EntryKind::Int), param("go", EntryKind::Bool)];
    let reenter = Transition {
        dest: TransitionDest::State { state: 0 },
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
    let random = EntryOp::Random {
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
        dest: TransitionDest::State { state: 1 },
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
        Action::LayerWeight {
            layer: 1,
            goal: 0.0,
            duration: 1.0,
        },
        Action::GroupWeight {
            group: 1,
            goal: 0.25,
            duration: 0.0,
        },
        Action::PoseSource {
            parts: vec![PartSource {
                part: PosePart::LeftHand,
                source: PoseSource::Animation,
            }],
        },
        Action::Locomotion { disabled: true },
    ];
    let base = layer(vec![state(vec![], vec![go]), state(control, vec![])], 0);
    let other = layer(vec![state(vec![], vec![])], 0);
    let c = controller(vec![param("go", EntryKind::Bool)], vec![base, other]);
    let mut i = Instance::new(&c, true);
    i.update(&c, 0.1);
    i.params[0] = 1.0;
    i.update(&c, 0.1);
    assert_eq!((i.layer_weights[1], i.group_weights[1]), (1.0, 0.25));
    assert_eq!(i.pose_sources[PosePart::LeftHand as usize], PoseSource::Animation);
    assert_eq!(i.pose_sources[PosePart::Head as usize], PoseSource::Tracking);
    assert!(i.locomotion_disabled);
    i.update(&c, 0.5);
    assert!((i.layer_weights[1] - 0.5).abs() < 1e-6);
    i.update(&c, 0.6);
    assert_eq!(i.layer_weights[1], 0.0);
}
