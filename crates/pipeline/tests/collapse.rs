//! Toggle collapsing (ADR 0028): the corpus avatars' toggle layers become one direct blend tree layer, layers that
//! are not plain toggles stay, and islands before and after produce the same pose, node activity and parameters
//! every frame under random toggling, parameters set before the first frame and parameter drivers.

use std::sync::Arc;

use civbb_client::{Island, Model, frame};
use civbb_core::math::Pose;
use civbb_format::animator::{
    Behaviour, Condition, ConditionMode, DriverOp, InterruptionSource, Key, Layer, LayerBlending, Motion, ParamKind,
    Parameter, State, StateMachine, Transition, TransitionDest,
};
use civbb_format::{Avatar, Caps, validate};
use civbb_pipeline::collapse;

fn valid(a: &Avatar) {
    if let Err(e) = validate(a, &Caps::default(), |_| true) {
        panic!("{e:?}");
    }
}

fn bloated() -> Avatar {
    civbb_corpus::generate(&civbb_corpus::bloated())
}

/// Collapses a copy, checks it stays valid and behaves the same, and returns how many layers went.
fn collapsed(before: &Avatar) -> (Avatar, usize) {
    let mut a = before.clone();
    let removed = collapse(&mut a);
    valid(&a);
    same_pose(before, &a);
    (a, removed)
}

#[test]
fn corpus_toggle_layers_collapse_into_one() {
    let (a, removed) = collapsed(&civbb_corpus::generate(&civbb_corpus::lean()));
    assert_eq!((removed, a.animator.unwrap().layers.len()), (1, 1));
    let before = bloated();
    let (a, removed) = collapsed(&before);
    assert_eq!(removed, 47);
    let an = a.animator.as_ref().unwrap();
    assert_eq!(an.layers.len(), 1);
    let states: Vec<_> = an.layers[0]
        .state_machine
        .states
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    // Every state was a toggle's, with Write Defaults off: `Hold` keeps frames starting from the last pose.
    assert_eq!(states, ["Start", "Toggles", "Hold"]);
    assert_eq!(an.parameters.last().unwrap().name, "civbb/One");
    let mut again = a.clone();
    assert_eq!(collapse(&mut again), 0);
}

#[test]
fn layers_that_are_not_plain_toggles_stay() {
    // A curve between equal keys with finite tangents can round differently at other times.
    let mut a = bloated();
    let clip = layer_clip(&a, 3, 1);
    a.clips[clip].curves[0].keys = vec![key(0.0, 1.0), key(1.0 / 60.0, 1.0)];
    assert_eq!(collapsed(&a).1, 46);

    // A float parameter's conditions are not a bool's.
    let mut a = bloated();
    let p = toggle_param(&a, 7);
    a.animator.as_mut().unwrap().parameters[p].kind = ParamKind::Float;
    assert_eq!(collapsed(&a).1, 46);

    // Layer 11's on clip also writes layer 10's node: both stay, as summing them is not layering them.
    let mut a = bloated();
    let path = a.clips[layer_clip(&a, 10, 1)].curves[0].binding.path.clone();
    let clip = layer_clip(&a, 11, 1);
    let mut curve = a.clips[clip].curves[0].clone();
    curve.binding.path = path;
    a.clips[clip].curves.push(curve);
    assert_eq!(collapsed(&a).1, 45);

    // A driver between the first toggle and toggle 5 sets its bool whenever the `Drive` bool turns on: toggle 5
    // sees it that frame, the collapsed layer (in the first toggle's place) only next frame, so toggle 5 stays. The
    // driver layer's weight behaviour follows its layer to its new index.
    let mut a = bloated();
    let p = toggle_param(&a, 5) as u32;
    let an = a.animator.as_mut().unwrap();
    let drive = an.parameters.len() as u32;
    an.parameters.push(Parameter {
        name: "Drive".into(),
        kind: ParamKind::Bool,
        default: 0.0,
    });
    let go = |dest, mode| Transition {
        dest: TransitionDest::State(dest),
        conditions: vec![Condition {
            param: drive,
            mode,
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
    let state = |name: &str, transition, behaviours| State {
        name: name.into(),
        motion: None,
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        write_defaults: false,
        transitions: vec![transition],
        behaviours,
    };
    an.layers.insert(
        2,
        Layer {
            name: "Driver".into(),
            group: 0,
            weight: 1.0,
            blending: LayerBlending::Override,
            mask: None,
            state_machine: StateMachine {
                states: vec![
                    state("Idle", go(1, ConditionMode::If), Vec::new()),
                    state(
                        "Set",
                        go(0, ConditionMode::IfNot),
                        vec![
                            Behaviour::ParameterDriver {
                                local_only: false,
                                ops: vec![DriverOp::Set { param: p, value: 1.0 }],
                            },
                            Behaviour::LayerWeight {
                                layer: 2,
                                goal: 1.0,
                                duration: 0.0,
                            },
                        ],
                    ),
                ],
                default_state: 0,
                entry_transitions: Vec::new(),
                any_state_transitions: Vec::new(),
            },
        },
    );
    let (a, removed) = collapsed(&a);
    assert_eq!(removed, 46);
    let names: Vec<_> = a
        .animator
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, ["Toggles", "Driver", "Outfit_5"]);
    let driver = &a.animator.as_ref().unwrap().layers[1].state_machine.states[1];
    assert!(matches!(driver.behaviours[1], Behaviour::LayerWeight { layer: 1, .. }));
}

fn key(time: f32, value: f32) -> Key {
    Key {
        time,
        value,
        in_tangent: 0.0,
        out_tangent: 0.0,
    }
}

/// The clip a corpus toggle layer's state plays.
fn layer_clip(a: &Avatar, layer: usize, state: usize) -> usize {
    match a.animator.as_ref().unwrap().layers[layer].state_machine.states[state].motion {
        Some(Motion::Clip(c)) => c as usize,
        _ => unreachable!("corpus toggles play clips"),
    }
}

fn toggle_param(a: &Avatar, layer: usize) -> usize {
    a.animator.as_ref().unwrap().layers[layer].state_machine.states[0].transitions[0].conditions[0].param as usize
}

/// Steps islands of both avatars through the same frames, some bools set before the first, then random flips and
/// frame times, and compares every slot of the animator's pose, node activity and local transforms, and the
/// parameters `before` declares.
fn same_pose(before: &Avatar, after: &Avatar) {
    let island = |a: &Avatar| Island::new(Arc::new(Model::new(a)), 1, true, Pose::IDENTITY);
    let (mut b, mut a) = (island(before), island(after));
    let bools: Vec<usize> = before
        .animator
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == ParamKind::Bool)
        .map(|(i, _)| i)
        .collect();
    let n = before.animator.as_ref().unwrap().parameters.len();
    let mut rng = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |m: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % m as u64) as usize
    };
    for f in 0..240 {
        let flips = if f == 0 { 5 } else { next(3) };
        let picks: Vec<usize> = (0..flips).map(|_| bools[next(bools.len())]).collect();
        let dt = [0.0, 1.0 / 90.0, 1.0 / 60.0, 1.0 / 30.0][next(4)];
        for island in [&mut b, &mut a] {
            let params = &mut island.animator_mut().unwrap().params;
            for &p in &picks {
                params[p] = 1.0 - params[p];
            }
            frame::step(std::slice::from_mut(island), |_, _| true, dt);
        }
        let (pb, pa) = (
            b.animator_mut().unwrap().pose().to_vec(),
            a.animator_mut().unwrap().pose().to_vec(),
        );
        assert!(pb == pa, "frame {f}: poses differ");
        assert!(b.active() == a.active(), "frame {f}: node activity differs");
        assert!(b.locals() == a.locals(), "frame {f}: local transforms differ");
        let (qb, qa) = (
            b.animator_mut().unwrap().params.clone(),
            a.animator_mut().unwrap().params.clone(),
        );
        assert_eq!(qb[..n], qa[..n], "frame {f}: parameters differ");
    }
}
