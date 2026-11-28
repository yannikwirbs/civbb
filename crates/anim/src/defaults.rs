//! Built-in locomotion (ADR 0108): substitutes for the SDK's empty proxy clips and default Base, Gesture and Action
//! layers, generated in muscle space and merged into a loaded avatar before it compiles.

mod poses;

pub use poses::{Hand, Kind, Poser, Style};

use civbb_format::Document;
use civbb_format::animator::{
    Action, Animator, BlendTree, BlendTreeKind, BodyPart, ChildMotion, ClipIndex, Condition, ConditionMode,
    InterruptionSource, Layer, LayerBlending, LayerGroup, Mask, Motion, State, StateMachine, Transition,
    TransitionDest, Unwritten,
};
use civbb_format::state::{Entry, EntryIndex, EntryKind, Platform, Range, SyncMode};

use crate::action::at_rest;

/// The content entry the default Action layer's floor poses play on (ADR 0119): 0 none, 1 sitting, 2 lying.
pub const POSE: &str = "civbb/pose";

/// What the generator makes for an SDK proxy clip called `name` (trailing digits and spaces ignored unless they follow
/// `_`: creators duplicate clips as `proxy_stand_still3`); `None` for names it does not cover (emotes, #151).
pub fn proxy(name: &str) -> Option<Kind> {
    use Kind::{Gait, Hands};
    use Style::*;
    let name = name.strip_prefix("proxy_")?;
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ');
    // `_45` and `_135` belong to the name.
    let name = if trimmed.ends_with('_') { name } else { trimmed };
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let gait = |style, forward, right| Some(Gait { style, forward, right });
    let (style, rest) = if let Some(r) = name.strip_prefix("run_") {
        (Run, r)
    } else if let Some(r) = name.strip_prefix("sprint_") {
        (Sprint, r)
    } else if let Some(r) = name.strip_prefix("crouch_walk_") {
        (Crouch, r)
    } else if let Some(r) = name.strip_prefix("low_crawl_") {
        (Crawl, r)
    } else if let Some(r) = name.strip_prefix("walk_") {
        (Walk, r)
    } else {
        (Walk, name)
    };
    match rest {
        "forward" => return gait(style, 1.0, 0.0),
        "backward" => return gait(style, -1.0, 0.0),
        "right" | "strafe_right" => return gait(style, 0.0, 1.0),
        "right_45" | "strafe_right_45" => return gait(style, d, d),
        "right_135" | "strafe_right_135" => return gait(style, -d, d),
        "still" if style == Crawl => return gait(Crawl, 0.0, 0.0),
        _ => {}
    }
    Some(match name {
        "stand_still" | "idle" => Kind::Idle,
        "crouch_still" => Gait {
            style: Crouch,
            forward: 0.0,
            right: 0.0,
        },
        "fall_short" | "fall_long" => Kind::Fall,
        "land_quick" => Kind::Land { deep: false },
        "landing" => Kind::Land { deep: true },
        "afk" => Kind::Afk,
        "sit" => Kind::Sit,
        "hands_idle" => Hands(Hand::Idle),
        "hands_fist" => Hands(Hand::Fist),
        "hands_open" => Hands(Hand::Open),
        "hands_point" => Hands(Hand::Point),
        "hands_peace" => Hands(Hand::Peace),
        "hands_rock" => Hands(Hand::Rock),
        "hands_gun" => Hands(Hand::Gun),
        "hands_thumbs_up" => Hands(Hand::ThumbsUp),
        _ => return None,
    })
}

/// Merges the built-in defaults into a validated humanoid avatar with muscle frames: fills empty proxy clips it
/// covers, adds a Base layer, a Gesture layer per hand and an Action layer to each of those groups that has none,
/// with the platform entries they read. Returns whether it changed anything; other avatars stay as they are.
pub fn apply(doc: &mut Document) -> bool {
    let Some(poser) = Poser::new(doc) else {
        return false;
    };
    let mut changed = false;
    for clip in &mut doc.clips {
        if let Some(kind) = proxy(&clip.name).filter(|_| clip.curves.is_empty()) {
            *clip = poser.clip(&clip.name, kind);
            changed = true;
        }
    }
    let animator = doc.animator.take().unwrap_or_default();
    let mut m = Merge {
        doc,
        poser: &poser,
        animator,
    };
    if !m.has_layers("Base") {
        let layer = m.base();
        m.insert(0, layer);
        changed = true;
    }
    if !m.has_layers("Gesture") {
        let layers = [m.gesture(true), m.gesture(false)];
        let at = m.after(&["Base", "Additive"]);
        for (k, layer) in layers.into_iter().enumerate() {
            m.insert(at + k, layer);
        }
        changed = true;
    }
    if !m.has_layers("Action") {
        let layer = m.action();
        let at = m.after(&["Base", "Additive", "Gesture"]);
        m.insert(at, layer);
        changed = true;
    }
    let Merge { doc, animator, .. } = m;
    doc.animator = Some(animator);
    changed
}

/// The desktop player's speeds the blend trees place their gaits at, in m/s (ADR 0106): walk, run, strafe.
const WALK: f32 = 2.0;
const RUN: f32 = 4.0;
/// Postures by `Upright` (ADR 0107 sets 1, 0.55 and 0.25; the VR player its head's height, ADR 0125): crouching
/// below this, ...
pub const CROUCH_BELOW: f32 = 0.7;
/// ... prone below this.
pub const PRONE_BELOW: f32 = 0.4;

struct Merge<'a> {
    doc: &'a mut Document,
    poser: &'a Poser,
    animator: Animator,
}

impl Merge<'_> {
    fn has_layers(&self, group: &str) -> bool {
        let a = &self.animator;
        (a.layers.iter()).any(|l| a.groups[l.group as usize].name == group)
    }

    /// The group called `name`, added with `weight` if missing.
    fn group(&mut self, name: &str, weight: f32) -> u32 {
        let groups = &mut self.animator.groups;
        groups.iter().position(|g| g.name == name).unwrap_or_else(|| {
            groups.push(LayerGroup {
                name: name.into(),
                weight,
            });
            groups.len() - 1
        }) as u32
    }

    /// The index after the last layer in one of `groups`, 0 without one.
    fn after(&self, groups: &[&str]) -> usize {
        let a = &self.animator;
        (a.layers.iter())
            .rposition(|l| groups.contains(&a.groups[l.group as usize].name.as_str()))
            .map_or(0, |i| i + 1)
    }

    /// Inserts `layer` at `at`, moving layer weight actions that point at or past it.
    fn insert(&mut self, at: usize, layer: Layer) {
        for l in &mut self.animator.layers {
            for s in &mut l.state_machine.states {
                for a in &mut s.actions {
                    if let Action::LayerWeight { layer, .. } = a
                        && *layer as usize >= at
                    {
                        *layer += 1;
                    }
                }
            }
        }
        self.animator.layers.insert(at, layer);
    }

    /// The entry of a platform value, added at its rest value if missing.
    fn entry(&mut self, p: Platform) -> EntryIndex {
        let state = &mut self.doc.state;
        state.find(p.name()).unwrap_or_else(|| {
            let rest = at_rest(p).unwrap_or(0.0);
            state.entries.push(Entry::new(p.name(), p.kind(), rest));
            (state.entries.len() - 1) as EntryIndex
        })
    }

    /// A generated clip, added to the document.
    fn clip(&mut self, name: &str, kind: Kind) -> Motion {
        self.doc.clips.push(self.poser.clip(name, kind));
        Motion::Clip {
            clip: (self.doc.clips.len() - 1) as ClipIndex,
        }
    }

    /// A 2D blend tree of a style's gaits around its still pose, at `speed` m/s (`run` adds runs at [`RUN`]).
    fn moving(&mut self, style: Style, speed: f32, run: bool, x: EntryIndex, y: EntryIndex) -> Motion {
        let d = std::f32::consts::FRAC_1_SQRT_2;
        let name = |what: &str| format!("civbb/{style:?}/{what}").to_lowercase();
        let gait = |m: &mut Self, what: &str, forward: f32, right: f32| {
            m.clip(&name(what), Kind::Gait { style, forward, right })
        };
        let still = gait(self, "still", 0.0, 0.0);
        let forward = gait(self, "forward", 1.0, 0.0);
        let backward = gait(self, "backward", -1.0, 0.0);
        let right = gait(self, "right", 0.0, 1.0);
        let right_45 = gait(self, "right_45", d, d);
        let right_135 = gait(self, "right_135", -d, d);
        let s = speed;
        let mut children = vec![
            child(still, [0.0, 0.0], false),
            child(forward, [0.0, s], false),
            child(backward, [0.0, -s], false),
            child(right.clone(), [s, 0.0], false),
            child(right, [-s, 0.0], true),
            child(right_45.clone(), [s * d, s * d], false),
            child(right_45, [-s * d, s * d], true),
            child(right_135.clone(), [s * d, -s * d], false),
            child(right_135, [-s * d, -s * d], true),
        ];
        if run {
            let run_forward = self.clip(
                "civbb/run/forward",
                Kind::Gait {
                    style: Style::Run,
                    forward: 1.0,
                    right: 0.0,
                },
            );
            let run_backward = self.clip(
                "civbb/run/backward",
                Kind::Gait {
                    style: Style::Run,
                    forward: -1.0,
                    right: 0.0,
                },
            );
            children.push(child(run_forward, [0.0, RUN], false));
            children.push(child(run_backward, [0.0, -RUN], false));
        }
        Motion::BlendTree(Box::new(BlendTree {
            kind: BlendTreeKind::FreeformDirectional2D { x, y },
            children,
            normalize: false,
        }))
    }

    /// Standing, crouching, prone and falling on velocity, `Upright` and `Grounded`.
    fn base(&mut self) -> Layer {
        let group = self.group("Base", 1.0);
        let (x, y) = (self.entry(Platform::VelocityX), self.entry(Platform::VelocityZ));
        let up = self.entry(Platform::Upright);
        let grounded = self.entry(Platform::Grounded);
        let standing = self.moving(Style::Walk, WALK, true, x, y);
        let crouching = self.moving(Style::Crouch, WALK / 2.0, false, x, y);
        let prone = self.moving(Style::Crawl, WALK / 4.0, false, x, y);
        let falling = self.clip("civbb/fall", Kind::Fall);
        let (stand, crouch, lie, fall) = (0, 1, 2, 3);
        let cond = |param, mode, threshold| Condition { param, mode, threshold };
        let above = |t| cond(up, ConditionMode::Greater, t);
        let below = |t| cond(up, ConditionMode::Less, t);
        let airborne = cond(grounded, ConditionMode::IfNot, 0.0);
        let landed = cond(grounded, ConditionMode::If, 0.0);
        let states = vec![
            state(
                "Standing",
                standing,
                vec![
                    to(fall, vec![airborne]),
                    to(lie, vec![below(PRONE_BELOW)]),
                    to(crouch, vec![below(CROUCH_BELOW)]),
                ],
            ),
            state(
                "Crouching",
                crouching,
                vec![
                    to(fall, vec![airborne]),
                    to(lie, vec![below(PRONE_BELOW)]),
                    to(stand, vec![above(CROUCH_BELOW)]),
                ],
            ),
            state(
                "Prone",
                prone,
                vec![
                    to(stand, vec![above(CROUCH_BELOW)]),
                    to(crouch, vec![above(PRONE_BELOW)]),
                ],
            ),
            state("Falling", falling, vec![to(stand, vec![landed])]),
        ];
        layer("civbb/locomotion", group, states)
    }

    /// One hand's gestures (`GestureLeft` or `GestureRight` 0 to 7) on its fingers; the fist closes with the
    /// gesture's weight.
    fn gesture(&mut self, left: bool) -> Layer {
        let group = self.group("Gesture", 1.0);
        let (gesture, weight, part, side) = if left {
            let e = (
                self.entry(Platform::GestureLeft),
                self.entry(Platform::GestureLeftWeight),
            );
            (e.0, e.1, BodyPart::LeftFingers, "left")
        } else {
            let e = (
                self.entry(Platform::GestureRight),
                self.entry(Platform::GestureRightWeight),
            );
            (e.0, e.1, BodyPart::RightFingers, "right")
        };
        use Hand::*;
        let mut states = Vec::new();
        let mut any = Vec::new();
        for (n, hand) in [Idle, Fist, Open, Point, Peace, Rock, Gun, ThumbsUp]
            .into_iter()
            .enumerate()
        {
            let name = format!("{hand:?}");
            let mut motion = self.clip(&format!("civbb/hands/{name}").to_lowercase(), Kind::Hands(hand));
            if hand == Fist {
                let idle = states.first().map(|s: &State| s.motion.clone().expect("a clip"));
                let children = vec![
                    ChildMotion {
                        threshold: 0.0,
                        ..child(idle.expect("idle comes first"), [0.0; 2], false)
                    },
                    ChildMotion {
                        threshold: 1.0,
                        ..child(motion, [0.0; 2], false)
                    },
                ];
                motion = Motion::BlendTree(Box::new(BlendTree {
                    kind: BlendTreeKind::Simple1D { param: weight },
                    children,
                    normalize: false,
                }));
            }
            states.push(state(&name, motion, Vec::new()));
            let mut t = to(
                n as u32,
                vec![Condition {
                    param: gesture,
                    mode: ConditionMode::Equals,
                    threshold: n as f32,
                }],
            );
            t.duration = 0.1;
            any.push(t);
        }
        let mut layer = layer(&format!("civbb/hand_{side}"), group, states);
        layer.mask = Some(Mask {
            body_parts: vec![part],
            transforms: None,
        });
        layer.state_machine.any_state_transitions = any;
        layer
    }

    /// AFK, sitting and the floor poses, raising the Action group while they play.
    fn action(&mut self) -> Layer {
        let group = self.group("Action", 0.0);
        let afk = self.entry(Platform::Afk);
        let seated = self.entry(Platform::Seated);
        let pose = self.pose_entry();
        let weight = |goal| Action::GroupWeight {
            group,
            goal,
            duration: 0.25,
        };
        let cond = |param, mode| Condition {
            param,
            mode,
            threshold: 0.0,
        };
        let afk_clip = self.clip("civbb/afk", Kind::Afk);
        let sit_clip = self.clip("civbb/sit", Kind::Sit);
        let floor_clip = self.clip("civbb/floor_sit", Kind::FloorSit);
        let lie_clip = self.clip("civbb/lie", Kind::Lie);
        let posed = |value: f32, mode| Condition {
            param: pose,
            mode,
            threshold: value,
        };
        let mut idle = state(
            "Idle",
            afk_clip.clone(),
            vec![
                to(1, vec![cond(afk, ConditionMode::If)]),
                to(2, vec![cond(seated, ConditionMode::If)]),
                to(3, vec![posed(1.0, ConditionMode::Equals)]),
                to(4, vec![posed(2.0, ConditionMode::Equals)]),
            ],
        );
        idle.motion = None;
        idle.actions = vec![weight(0.0)];
        let mut away = state("AFK", afk_clip, vec![to(0, vec![cond(afk, ConditionMode::IfNot)])]);
        away.actions = vec![weight(1.0)];
        let mut sit = state(
            "Sitting",
            sit_clip,
            vec![to(0, vec![cond(seated, ConditionMode::IfNot)])],
        );
        sit.actions = vec![weight(1.0)];
        let mut floor = state(
            "Sitting on the floor",
            floor_clip,
            vec![to(0, vec![posed(1.0, ConditionMode::NotEqual)])],
        );
        floor.actions = vec![weight(1.0)];
        let mut lie = state(
            "Lying",
            lie_clip,
            vec![to(0, vec![posed(2.0, ConditionMode::NotEqual)])],
        );
        lie.actions = vec![weight(1.0)];
        layer("civbb/action", group, vec![idle, away, sit, floor, lie])
    }

    /// The [`POSE`] entry, added if missing: an int 0 to 2 the owner syncs and the player writes.
    fn pose_entry(&mut self) -> EntryIndex {
        let state = &mut self.doc.state;
        state.find(POSE).unwrap_or_else(|| {
            state.entries.push(Entry {
                range: Some(Range::Int { min: 0, max: 2 }),
                sync: SyncMode::Owner,
                external: true,
                ..Entry::new(POSE, EntryKind::Int, 0.0)
            });
            (state.entries.len() - 1) as EntryIndex
        })
    }
}

fn child(motion: Motion, position: [f32; 2], mirror: bool) -> ChildMotion {
    ChildMotion {
        motion,
        threshold: 0.0,
        position,
        direct_param: None,
        time_scale: 1.0,
        cycle_offset: 0.0,
        mirror,
    }
}

fn state(name: &str, motion: Motion, transitions: Vec<Transition>) -> State {
    State {
        name: name.into(),
        motion: Some(motion),
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        unwritten: Unwritten::Hold,
        mirror: false,
        mirror_param: None,
        transitions,
        actions: Vec::new(),
    }
}

/// A 0.25 s transition to state `dest` when every condition holds.
fn to(dest: u32, conditions: Vec<Condition>) -> Transition {
    Transition {
        dest: TransitionDest::State { state: dest },
        conditions,
        has_exit_time: false,
        exit_time: 0.0,
        fixed_duration: true,
        duration: 0.25,
        offset: 0.0,
        interruption: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: false,
    }
}

fn layer(name: &str, group: u32, states: Vec<State>) -> Layer {
    Layer {
        name: name.into(),
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
