//! Compiler stage 5 toggle collapsing: layers that only switch between two constant clips on one bool collapse
//! into one layer per group playing them all in a direct blend tree, with the same pose every frame (design doc §7,
//! ADR 0028).

use std::collections::{HashMap, HashSet};

use civbb_anim::Target;
use civbb_format::Avatar;
use civbb_format::animator::{
    Animator, Behaviour, BlendTree, BlendTreeKind, Clip, ClipIndex, ConditionMode, DriverOp, InterruptionSource, Layer,
    LayerBlending, Motion, ParamIndex, ParamKind, Parameter, State, StateMachine, Transition, TransitionDest,
};

/// A layer that only switches between two constant clips on one bool parameter.
struct Toggle {
    layer: usize,
    param: ParamIndex,
    off: ClipIndex,
    on: ClipIndex,
    /// The default state's clip.
    start: ClipIndex,
}

/// Collapses toggle layers of a validated avatar's animator in place; returns how many layers went.
pub fn collapse(a: &mut Avatar) -> usize {
    let Some(an) = &a.animator else { return 0 };
    let bound = civbb_anim::bind(a);
    let n = an.layers.len();
    let layer_slots: Vec<Vec<u32>> = an
        .layers
        .iter()
        .map(|l| {
            let mut clips = Vec::new();
            for s in &l.state_machine.states {
                s.motion.iter().for_each(|m| motion_clips(m, &mut clips));
            }
            let mut slots: Vec<u32> = clips
                .iter()
                .flat_map(|&c| bound.clips[c as usize].slots.iter().copied())
                .collect();
            slots.sort_unstable();
            slots.dedup();
            slots
        })
        .collect();
    let mut writers = vec![0; bound.bindings.len()];
    for &s in layer_slots.iter().flatten() {
        writers[s as usize] += 1;
    }
    let animated_params: HashSet<ParamIndex> = layer_slots
        .iter()
        .flatten()
        .filter_map(|&s| match bound.bindings.targets[s as usize] {
            Target::Parameter(p) => Some(p),
            _ => None,
        })
        .collect();
    let behaviours = |li: usize| an.layers[li].state_machine.states.iter().flat_map(|s| &s.behaviours);
    let weighted: HashSet<u32> = (0..n)
        .flat_map(behaviours)
        .filter_map(|b| match b {
            Behaviour::LayerWeight { layer, .. } => Some(*layer),
            _ => None,
        })
        .collect();
    // Parameters a driver in each layer sets; they change mid-update, so layer order decides who sees them.
    let driven: Vec<HashSet<ParamIndex>> = (0..n)
        .map(|li| {
            behaviours(li)
                .filter_map(|b| match b {
                    Behaviour::ParameterDriver { ops, .. } => Some(ops),
                    _ => None,
                })
                .flatten()
                .map(|op| match *op {
                    DriverOp::Set { param, .. } | DriverOp::Add { param, .. } | DriverOp::Random { param, .. } => param,
                    DriverOp::Copy { dst, .. } => dst,
                })
                .collect()
        })
        .collect();

    // Each toggle must write slots no other layer writes (so summing them equals layering them), on a bool no clip
    // animates (clips may set fractions the blend tree would blend), in a layer no behaviour reweights.
    let mut groups: Vec<(u32, Vec<Toggle>)> = Vec::new();
    for t in (0..n).filter_map(|li| toggle(an, &a.clips, li)) {
        if !layer_slots[t.layer].iter().all(|&s| writers[s as usize] == 1)
            || animated_params.contains(&t.param)
            || weighted.contains(&(t.layer as u32))
        {
            continue;
        }
        let g = an.layers[t.layer].group;
        match groups.iter_mut().find(|(group, _)| *group == g) {
            // The collapsed layer takes the first toggle's place: a later toggle joins only if no layer between
            // them drives its parameter.
            Some((_, ts)) => {
                if !(ts[0].layer..t.layer).any(|l| driven[l].contains(&t.param)) {
                    ts.push(t);
                }
            }
            None => groups.push((g, vec![t])),
        }
    }
    groups.retain(|(_, ts)| ts.len() > 1);
    if groups.is_empty() {
        return 0;
    }

    let wd_off = any_wd_off(an);
    let an = a.animator.as_mut().expect("checked above");
    let mut name = String::from("civbb/One");
    while an.parameters.iter().any(|p| p.name == name) {
        name.push('_');
    }
    let one = an.parameters.len() as ParamIndex;
    an.parameters.push(Parameter {
        name,
        kind: ParamKind::Float,
        default: 1.0,
    });
    let mut replace: HashMap<usize, Layer> = HashMap::new();
    let mut removed = HashSet::new();
    for (group, ts) in &groups {
        removed.extend(ts.iter().map(|t| t.layer));
        replace.insert(ts[0].layer, layer(*group, ts, one));
    }
    let old = std::mem::take(&mut an.layers);
    let mut map = vec![0u32; old.len()];
    for (i, l) in old.into_iter().enumerate() {
        map[i] = an.layers.len() as u32;
        if let Some(l) = replace.remove(&i) {
            an.layers.push(l);
        } else if !removed.contains(&i) {
            an.layers.push(l);
        }
    }
    for b in an
        .layers
        .iter_mut()
        .flat_map(|l| &mut l.state_machine.states)
        .flat_map(|s| &mut s.behaviours)
    {
        if let Behaviour::LayerWeight { layer, .. } = b {
            *layer = map[*layer as usize];
        }
    }
    // One Write Defaults off state makes frames start from the last pose (ADR 0010); keep that if only toggles had
    // one, with a state nothing enters.
    if wd_off && !any_wd_off(an) {
        let l = &mut an.layers[map[groups[0].1[0].layer] as usize];
        l.state_machine.states.push(State {
            write_defaults: false,
            ..state("Hold", None, Vec::new())
        });
    }
    removed.len() - groups.len()
}

/// The toggle a layer is, if it is one: an override layer at weight 1 without a mask, two states each playing a
/// constant clip with one zero-duration transition to the other on the same bool (`If` from off, `IfNot` from on),
/// no exit times, entry or any-state transitions, and no behaviours.
fn toggle(an: &Animator, clips: &[Clip], li: usize) -> Option<Toggle> {
    let l = &an.layers[li];
    let sm = &l.state_machine;
    if l.blending != LayerBlending::Override
        || l.mask.is_some()
        || l.weight != 1.0
        || sm.states.len() != 2
        || !sm.entry_transitions.is_empty()
        || !sm.any_state_transitions.is_empty()
    {
        return None;
    }
    let mut arms = [None; 2];
    for (s, st) in sm.states.iter().enumerate() {
        let (Some(Motion::Clip(clip)), [t]) = (&st.motion, st.transitions.as_slice()) else {
            return None;
        };
        let [c] = t.conditions.as_slice() else { return None };
        if !st.behaviours.is_empty()
            || t.dest != TransitionDest::State(1 - s as u32)
            || t.has_exit_time
            || t.duration != 0.0
            || !constant(&clips[*clip as usize])
        {
            return None;
        }
        arms[s] = Some((*clip, c.param, c.mode));
    }
    let [Some((c0, p0, m0)), Some((c1, p1, m1))] = arms else {
        return None;
    };
    if p0 != p1 || an.parameters[p0 as usize].kind != ParamKind::Bool {
        return None;
    }
    let (off, on) = match (m0, m1) {
        (ConditionMode::If, ConditionMode::IfNot) => (c0, c1),
        (ConditionMode::IfNot, ConditionMode::If) => (c1, c0),
        _ => return None,
    };
    Some(Toggle {
        layer: li,
        param: p0,
        off,
        on,
        start: if sm.default_state == 0 { c0 } else { c1 },
    })
}

/// Whether every curve of a clip has the same value at any time: one key, or equal keys with stepped segments.
/// Hermite segments between equal keys can differ from the key by rounding, and the collapsed layer samples its
/// clips at other times than the toggle's states did.
fn constant(clip: &Clip) -> bool {
    clip.curves.iter().all(|c| {
        c.keys
            .windows(2)
            .all(|k| k[0].value == k[1].value && !(k[0].out_tangent.is_finite() && k[1].in_tangent.is_finite()))
    })
}

/// The collapsed layer. `Start` plays each toggle's default clip for the first frame, as each toggle layer started
/// in its default state whatever the parameter; from the second frame `Toggles` plays each toggle's clip for its
/// bool through a 1D tree. Every child weighs 1 by the constant parameter `one`, and Write Defaults is on so each
/// slot gets exactly its one clip's value (ADR 0010).
fn layer(group: u32, ts: &[Toggle], one: ParamIndex) -> Layer {
    let child = |motion, threshold, direct_param| civbb_format::animator::ChildMotion {
        motion,
        threshold,
        position: [0.0; 2],
        direct_param,
        time_scale: 1.0,
        cycle_offset: 0.0,
    };
    let direct = |children| {
        Some(Motion::BlendTree(Box::new(BlendTree {
            kind: BlendTreeKind::Direct,
            children,
            normalize: false,
        })))
    };
    let starts = ts
        .iter()
        .map(|t| child(Motion::Clip(t.start), 0.0, Some(one)))
        .collect();
    let toggles = ts
        .iter()
        .map(|t| {
            let tree = BlendTree {
                kind: BlendTreeKind::Simple1D { param: t.param },
                children: vec![
                    child(Motion::Clip(t.off), 0.0, None),
                    child(Motion::Clip(t.on), 1.0, None),
                ],
                normalize: false,
            };
            child(Motion::BlendTree(Box::new(tree)), 0.0, Some(one))
        })
        .collect();
    let next = Transition {
        dest: TransitionDest::State(1),
        conditions: Vec::new(),
        has_exit_time: true,
        exit_time: 0.0,
        fixed_duration: true,
        duration: 0.0,
        offset: 0.0,
        interruption: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: false,
    };
    Layer {
        name: "Toggles".into(),
        group,
        weight: 1.0,
        blending: LayerBlending::Override,
        mask: None,
        state_machine: StateMachine {
            states: vec![
                state("Start", direct(starts), vec![next]),
                state("Toggles", direct(toggles), Vec::new()),
            ],
            default_state: 0,
            entry_transitions: Vec::new(),
            any_state_transitions: Vec::new(),
        },
    }
}

fn state(name: &str, motion: Option<Motion>, transitions: Vec<Transition>) -> State {
    State {
        name: name.into(),
        motion,
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        write_defaults: true,
        transitions,
        behaviours: Vec::new(),
    }
}

fn any_wd_off(an: &Animator) -> bool {
    an.layers
        .iter()
        .flat_map(|l| &l.state_machine.states)
        .any(|s| !s.write_defaults)
}

/// Appends every clip a motion plays.
fn motion_clips(m: &Motion, out: &mut Vec<ClipIndex>) {
    match m {
        Motion::Clip(c) => out.push(*c),
        Motion::BlendTree(t) => t.children.iter().for_each(|c| motion_clips(&c.motion, out)),
    }
}
