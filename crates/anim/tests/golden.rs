//! Golden-file conformance: runs every `tests/golden/*.json` case frame by frame and compares the pose.
//!
//! Cases are hand-derived for now, each citing its source (ADR 0002, 0010); Unity recordings replace them once
//! the exporter exists. A case is a small controller (nodes, parameters, clips, layers) plus frames: parameter
//! changes, then expected values (`"<node>.x|y|z|active"`) and current states (`"<layer>": "<state>"`).

use std::collections::BTreeMap;
use std::path::Path;

use civbb_anim::{Controller, Instance, Target};
use civbb_format::Avatar;
use civbb_format::animator::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::Transform;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    source: String,
    nodes: Vec<NodeSpec>,
    params: Vec<Parameter>,
    clips: BTreeMap<String, ClipSpec>,
    layers: Vec<LayerSpec>,
    dt: f32,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSpec {
    name: String,
    #[serde(default = "yes")]
    active: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipSpec {
    length: f32,
    #[serde(default)]
    looping: bool,
    /// `[path.property, [[time, value], ...]]`, linear keys.
    curves: Vec<(String, Vec<(f32, f32)>)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerSpec {
    name: String,
    #[serde(default = "one")]
    weight: f32,
    #[serde(default)]
    additive: bool,
    states: Vec<StateSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateSpec {
    name: String,
    motion: Option<MotionSpec>,
    wd: bool,
    #[serde(default)]
    transitions: Vec<TransitionSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum MotionSpec {
    Clip(String),
    /// Parameter and `[threshold, clip]` children.
    Simple1d(String, Vec<(f32, String)>),
    /// `[parameter, clip]` children.
    Direct(Vec<(String, String)>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionSpec {
    to: String,
    #[serde(default, rename = "if")]
    if_: Vec<String>,
    #[serde(default)]
    if_not: Vec<String>,
    #[serde(default)]
    duration: f32,
    exit_time: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    #[serde(default)]
    set: BTreeMap<String, f32>,
    expect: BTreeMap<String, Value>,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

impl Case {
    fn param(&self, name: &str) -> u32 {
        self.params
            .iter()
            .position(|p| p.name == name)
            .unwrap_or_else(|| panic!("no parameter {name}")) as u32
    }

    fn clip(&self, name: &str) -> u32 {
        self.clips
            .keys()
            .position(|k| k == name)
            .unwrap_or_else(|| panic!("no clip {name}")) as u32
    }

    fn motion(&self, m: &MotionSpec) -> Motion {
        let child = |motion, threshold, direct_param| ChildMotion {
            motion,
            threshold,
            position: [0.0; 2],
            direct_param,
            time_scale: 1.0,
            cycle_offset: 0.0,
        };
        let tree = |kind, children| {
            Motion::BlendTree(Box::new(BlendTree {
                kind,
                children,
                normalize: false,
            }))
        };
        match m {
            MotionSpec::Clip(c) => Motion::Clip(self.clip(c)),
            MotionSpec::Simple1d(p, children) => tree(
                BlendTreeKind::Simple1D { param: self.param(p) },
                children
                    .iter()
                    .map(|(t, c)| child(Motion::Clip(self.clip(c)), *t, None))
                    .collect(),
            ),
            MotionSpec::Direct(children) => tree(
                BlendTreeKind::Direct,
                children
                    .iter()
                    .map(|(p, c)| child(Motion::Clip(self.clip(c)), 0.0, Some(self.param(p))))
                    .collect(),
            ),
        }
    }

    fn avatar(&self) -> Avatar {
        let mut nodes = vec![Node {
            name: "Root".into(),
            parent: None,
            rest: Transform::IDENTITY,
            active: true,
        }];
        nodes.extend(self.nodes.iter().map(|n| Node {
            name: n.name.clone(),
            parent: Some(0),
            rest: Transform::IDENTITY,
            active: n.active,
        }));
        let clips = self
            .clips
            .iter()
            .map(|(name, c)| Clip {
                name: name.clone(),
                length: c.length,
                looping: c.looping,
                curves: c.curves.iter().map(|(target, keys)| curve(target, keys)).collect(),
            })
            .collect();
        let layers = self
            .layers
            .iter()
            .map(|l| {
                let index = |name: &str| {
                    l.states
                        .iter()
                        .position(|s| s.name == name)
                        .unwrap_or_else(|| panic!("no state {name}")) as u32
                };
                let states = l
                    .states
                    .iter()
                    .map(|s| State {
                        name: s.name.clone(),
                        motion: s.motion.as_ref().map(|m| self.motion(m)),
                        speed: 1.0,
                        speed_param: None,
                        cycle_offset: 0.0,
                        cycle_offset_param: None,
                        time_param: None,
                        write_defaults: s.wd,
                        transitions: s.transitions.iter().map(|t| self.transition(t, index(&t.to))).collect(),
                        behaviours: Vec::new(),
                    })
                    .collect();
                Layer {
                    name: l.name.clone(),
                    group: 0,
                    weight: l.weight,
                    blending: if l.additive {
                        LayerBlending::Additive
                    } else {
                        LayerBlending::Override
                    },
                    mask: None,
                    state_machine: StateMachine {
                        states,
                        default_state: 0,
                        entry_transitions: Vec::new(),
                        any_state_transitions: Vec::new(),
                    },
                }
            })
            .collect();
        Avatar {
            hierarchy: Hierarchy { nodes },
            animator: Some(Animator {
                parameters: self.params.clone(),
                groups: vec![LayerGroup {
                    name: "All".into(),
                    weight: 1.0,
                }],
                layers,
            }),
            clips,
            ..Avatar::default()
        }
    }

    fn transition(&self, t: &TransitionSpec, dest: u32) -> Transition {
        let mut conditions = Vec::new();
        for (names, mode) in [(&t.if_, ConditionMode::If), (&t.if_not, ConditionMode::IfNot)] {
            conditions.extend(names.iter().map(|n| Condition {
                param: self.param(n),
                mode,
                threshold: 0.0,
            }));
        }
        Transition {
            dest: TransitionDest::State(dest),
            conditions,
            has_exit_time: t.exit_time.is_some(),
            exit_time: t.exit_time.unwrap_or(0.0),
            fixed_duration: true,
            duration: t.duration,
            offset: 0.0,
            interruption: InterruptionSource::None,
            ordered_interruption: true,
            can_transition_to_self: false,
        }
    }
}

fn curve(target: &str, keys: &[(f32, f32)]) -> Curve {
    let (path, prop) = target
        .rsplit_once('.')
        .unwrap_or_else(|| panic!("bad curve target {target}"));
    let property = match prop {
        "x" => Property::Position(0),
        "y" => Property::Position(1),
        "z" => Property::Position(2),
        "active" => Property::Active,
        _ => panic!("unknown property {prop}"),
    };
    let slope = |i: usize, j: usize| (keys[j].1 - keys[i].1) / (keys[j].0 - keys[i].0);
    Curve {
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
    }
}

/// The current value of `"<node>.<property>"`: its slot, or the node's rest value when nothing animates it.
fn value(c: &Controller, a: &Avatar, i: &Instance, key: &str) -> f32 {
    let (path, prop) = key.rsplit_once('.').unwrap_or_else(|| panic!("bad key {key}"));
    let node = a
        .hierarchy
        .nodes
        .iter()
        .position(|n| n.name == path)
        .unwrap_or_else(|| panic!("no node {path}")) as u32;
    let target = match prop {
        "x" => Target::Position(node, 0),
        "y" => Target::Position(node, 1),
        "z" => Target::Position(node, 2),
        "active" => Target::Active(node),
        _ => panic!("unknown property {prop}"),
    };
    match c.bound.bindings.targets.iter().position(|t| *t == target) {
        Some(s) => i.pose()[s],
        None => match target {
            Target::Active(n) => f32::from(u8::from(a.hierarchy.nodes[n as usize].active)),
            _ => 0.0,
        },
    }
}

fn run(path: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let case: Case =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(!case.source.is_empty(), "{name}: cite a source");
    let a = case.avatar();
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true)
        .unwrap_or_else(|e| panic!("{name}: invalid case: {e:?}"));
    let c = Controller::new(&a).unwrap();
    assert!(c.bound.unbound.is_empty(), "{name}: {:?}", c.bound.unbound);
    let mut inst = Instance::new(&c, true);
    for (f, frame) in case.frames.iter().enumerate() {
        for (p, v) in &frame.set {
            inst.params[case.param(p) as usize] = *v;
        }
        inst.update(&c, case.dt);
        for (key, want) in &frame.expect {
            if let Some(state) = want.as_str() {
                let li = case
                    .layers
                    .iter()
                    .position(|l| l.name == *key)
                    .unwrap_or_else(|| panic!("no layer {key}"));
                let got = &case.layers[li].states[inst.layer(li).unwrap().current as usize].name;
                assert_eq!(got, state, "{name}: frame {f}: layer {key}");
            } else {
                let want = want.as_f64().unwrap_or_else(|| panic!("{name}: bad expectation {key}")) as f32;
                let got = value(&c, &a, &inst, key);
                assert!(
                    (got - want).abs() < 1e-4,
                    "{name}: frame {f}: {key} = {got}, expected {want}"
                );
            }
        }
    }
}

#[test]
fn golden_cases() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    assert!(paths.len() >= 6, "golden cases missing");
    for p in &paths {
        run(p);
    }
}
