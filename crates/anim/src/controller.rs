//! A compiled animator controller and one running instance of it: parameters, layers, the evaluated pose.

use std::collections::{HashMap, HashSet};

use civbb_format::Document;
use civbb_format::animator::{Action, Animator, BodyPart, PoseSource, State, Unwritten};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Transform};
use civbb_format::state::{EntryIndex, Platform, StateTable};

use crate::action::{PoseSources, Rng, at_rest, coerce, drive, part_index};
use crate::bind::{Bound, Slot, Target, bind};
use crate::constraint::{Constraints, Scratch};
use crate::machine::{Ctx, LayerRun};
use crate::mix::Mixer;
use crate::muscle::{self, COUNT, MUSCLES};

/// An avatar's animator with its clips bound to slots and its masks resolved. Shared by every instance.
#[derive(Clone, Debug)]
pub struct Controller {
    /// The animator.
    pub animator: Animator,
    /// The state table its parameters are entries of.
    pub state: StateTable,
    /// Bound clips and slots.
    pub bound: Bound,
    /// Per layer, which slots its mask lets it write; `None` writes all.
    pub(crate) masks: Vec<Option<Vec<bool>>>,
    /// Per slot, 0 or the quaternion component plus 1 (1 marks a rotation's first slot).
    pub(crate) rot: Vec<u8>,
    /// Per slot, where a mirrored motion writes its value and with which sign: the counterpart's slot for muscles
    /// that have one (ADR 0060), else itself.
    pub(crate) mirror: Vec<(Slot, f32)>,
    /// Per layer, whether it is the first of its group, whose weight Unity fixes at 1.
    first_in_group: Vec<bool>,
    /// Some state has Write Defaults off: frames start from the last pose instead of defaults (ADR 0010).
    pub wd_off: bool,
    /// Slots of humanoid muscles. Write Defaults off never holds them: Unity writes the whole human pose each frame,
    /// muscles nothing animates at their default (probed in the SDK host project), so frames start them there.
    muscles: Vec<usize>,
    /// Per entry, the value an instance starts at, in the entry's type: `Entry::default` (for lowered Unity content
    /// the expression parameter's default where there is one, as VRChat loads those, #85).
    pub start: Vec<f32>,
}

impl Controller {
    /// Compiles a validated avatar's animator; `None` if it has none.
    pub fn new(avatar: &Document) -> Option<Self> {
        let animator = avatar.animator.clone()?;
        let bound = bind(avatar);
        let node_of = |t: &Target| match *t {
            Target::Position(n, _) | Target::Rotation(n, _) | Target::Scale(n, _) | Target::Active(n) => Some(n),
            Target::BlendShape(m, _)
            | Target::RendererEnabled(m)
            | Target::Material(m, _, _, _)
            | Target::MaterialSlot(m, _) => Some(avatar.meshes[m as usize].node),
            Target::Constraint(i, _) => Some(avatar.constraints[i as usize].target),
            Target::Parameter(_) | Target::Muscle(_) | Target::Body(_) => None,
        };
        // Muscles follow the mask's body parts; other targets its nodes, if it lists any.
        let masks = animator
            .layers
            .iter()
            .map(|l| {
                let mask = l.mask.as_ref()?;
                let allowed: Option<HashSet<NodeIndex>> = mask.transforms.as_ref().map(|t| t.iter().copied().collect());
                let targets = &bound.bindings.targets;
                Some(
                    targets
                        .iter()
                        .map(|t| match (t, &allowed) {
                            (Target::Muscle(m), _) => {
                                mask.body_parts.contains(&BodyPart::of(MUSCLES[*m as usize].bone))
                            }
                            // The body pose follows the mask's Root part (probed in Unity, #52).
                            (Target::Body(_), _) => mask.body_parts.contains(&BodyPart::Root),
                            (t, Some(allowed)) => node_of(t).is_none_or(|n| allowed.contains(&n)),
                            (_, None) => true,
                        })
                        .collect(),
                )
            })
            .collect();
        let rot = bound
            .bindings
            .targets
            .iter()
            .map(|t| match t {
                Target::Rotation(_, k) => k + 1,
                Target::Body(i) if *i >= 3 => i - 2,
                _ => 0,
            })
            .collect();
        let targets = &bound.bindings.targets;
        let slot_of: HashMap<Target, Slot> = (targets.iter().enumerate()).map(|(s, t)| (*t, s as Slot)).collect();
        let mirror = (targets.iter().enumerate())
            .map(|(s, t)| match *t {
                Target::Muscle(m) => {
                    let (other, sign) = muscle::mirror(m);
                    slot_of
                        .get(&Target::Muscle(other))
                        .map_or((s as Slot, 1.0), |&o| (o, sign))
                }
                _ => (s as Slot, 1.0),
            })
            .collect();
        let mut seen = HashSet::new();
        let first_in_group = animator.layers.iter().map(|l| seen.insert(l.group)).collect();
        let wd_off = animator
            .layers
            .iter()
            .flat_map(|l| &l.state_machine.states)
            .any(|s| !writes_defaults(s));
        let muscles = (0..bound.bindings.targets.len())
            .filter(|&s| matches!(bound.bindings.targets[s], Target::Muscle(_)))
            .collect();
        let state = avatar.state.clone();
        let start = state.entries.iter().map(|e| coerce(e.kind, e.default)).collect();
        Some(Self {
            animator,
            state,
            bound,
            masks,
            rot,
            mirror,
            first_in_group,
            wd_off,
            muscles,
            start,
        })
    }

    /// The entry called `name`.
    pub fn param(&self, name: &str) -> Option<EntryIndex> {
        self.state.find(name)
    }
}

/// One avatar's running animator.
pub struct Instance {
    /// Parameter values; bools and triggers are 0 or 1.
    pub params: Vec<f32>,
    /// Layer weights (the first layer of each group is always 1).
    pub layer_weights: Vec<f32>,
    /// Group weights.
    pub group_weights: Vec<f32>,
    /// Where each body part takes its pose from, set by pose source actions.
    pub pose_sources: PoseSources,
    /// Locomotion disabled by a locomotion action.
    pub locomotion_disabled: bool,
    /// The avatar's owner runs this instance; owner-only actions run only then.
    local: bool,
    blends: Vec<Blend>,
    rng: Rng,
    runs: Vec<Option<LayerRun>>,
    pose: Vec<f32>,
    written: Vec<bool>,
    mixer: Mixer,
}

/// A layer or group weight moving linearly to a goal (layer and group weight actions).
struct Blend {
    group: bool,
    index: u32,
    from: f32,
    goal: f32,
    duration: f32,
    elapsed: f32,
}

impl Instance {
    /// A fresh instance at [`Controller::start`], platform values at [`at_rest`]; the first [`Instance::update`]
    /// enters the start states. `local` is true for the owner's own avatar; it also sets `platform/is_local`.
    pub fn new(c: &Controller, local: bool) -> Self {
        let a = &c.animator;
        let n = c.bound.bindings.len();
        let mut i = Self {
            params: c.start.clone(),
            layer_weights: a.layers.iter().map(|l| l.weight).collect(),
            group_weights: a.groups.iter().map(|g| g.weight).collect(),
            pose_sources: [PoseSource::Tracking; 10],
            locomotion_disabled: false,
            local,
            blends: Vec::new(),
            rng: Rng(0x9e37_79b9_7f4a_7c15),
            runs: vec![None; a.layers.len()],
            pose: c.bound.bindings.defaults.clone(),
            written: vec![false; n],
            mixer: Mixer::new(n),
        };
        for p in Platform::ALL {
            if let Some(v) = at_rest(p) {
                i.set_platform(c, p, v);
            }
        }
        i.set_platform(c, Platform::IsLocal, f32::from(u8::from(local)));
        i
    }

    /// Sets a platform value if the state table has its entry, converted to the entry's type.
    pub fn set_platform(&mut self, c: &Controller, platform: Platform, value: f32) {
        if let Some(p) = c.param(platform.name()) {
            self.params[p as usize] = coerce(c.state.entries[p as usize].kind, value);
        }
    }

    /// Slot values after the last update.
    pub fn pose(&self) -> &[f32] {
        &self.pose
    }

    /// Local rotations of the humanoid's framed bones from the muscle slots, parents first; muscles no clip binds
    /// are 0, as in Unity's default human pose (#86). Empty when no clip animates muscles, so the bones keep their
    /// rotation slots. Where it is not empty it takes precedence over rotation slots of the same bones (ADR 0014).
    pub fn human_pose(&self, c: &Controller) -> Vec<(NodeIndex, Quat)> {
        let Some(rig) = &c.bound.rig else {
            return Vec::new();
        };
        let mut muscles = [0.0; COUNT];
        let mut any = false;
        for (s, t) in c.bound.bindings.targets.iter().enumerate() {
            if let Target::Muscle(m) = t {
                muscles[*m as usize] = self.pose[s];
                any = true;
            }
        }
        if any { rig.rotations(&muscles) } else { Vec::new() }
    }

    /// Writes the pose's node targets into `locals` (every node's local transform) and `active` (every node's own
    /// active flag; a value of 0.5 or more is active), then the [`Instance::human_pose`] rotations over them. Nodes
    /// and components no slot targets keep what they hold.
    pub fn write_nodes(&self, c: &Controller, locals: &mut [Transform], active: &mut [bool]) {
        for (s, t) in c.bound.bindings.targets.iter().enumerate() {
            let v = self.pose[s];
            match *t {
                Target::Position(n, k) => locals[n as usize].translation[k as usize] = v,
                Target::Rotation(n, k) => locals[n as usize].rotation[k as usize] = v,
                Target::Scale(n, k) => locals[n as usize].scale[k as usize] = v,
                Target::Active(n) => active[n as usize] = v >= 0.5,
                _ => {}
            }
        }
        for (n, q) in self.human_pose(c) {
            locals[n as usize].rotation = q;
        }
        // A body pose some layer wrote this update places the hips (ADR 0063); otherwise they keep their transform.
        if let Some(body) = &c.bound.body {
            let mut pose = body.rest;
            let mut any = false;
            for (s, t) in c.bound.bindings.targets.iter().enumerate() {
                if let Target::Body(i) = *t {
                    pose[i as usize] = self.pose[s];
                    any |= self.written[s];
                }
            }
            if any {
                body.place(pose, locals);
            }
        }
    }

    /// Writes the pose's constraint targets into the constraint pass's per-island values (ADR 0040).
    pub fn write_constraints(&self, c: &Controller, constraints: &Constraints, scratch: &mut Scratch) {
        for (s, t) in c.bound.bindings.targets.iter().enumerate() {
            if let Target::Constraint(i, field) = *t {
                constraints.set(scratch, i, field, self.pose[s]);
            }
        }
    }

    /// Layer `i`'s state machine; `None` before the first update.
    pub fn layer(&self, i: usize) -> Option<&LayerRun> {
        self.runs[i].as_ref()
    }

    /// Advances every layer by `dt` seconds, takes transitions, runs entered states' actions and evaluates the
    /// pose, starting from defaults or, with any Write Defaults off state, from the last pose. Animated parameters
    /// take their new values at the end, so conditions see them next frame.
    pub fn update(&mut self, c: &Controller, dt: f32) {
        self.blend_weights(dt);
        if !c.wd_off {
            self.pose.copy_from_slice(&c.bound.bindings.defaults);
        } else {
            for &s in &c.muscles {
                self.pose[s] = c.bound.bindings.defaults[s];
            }
        }
        self.written.fill(false);
        for (li, layer) in c.animator.layers.iter().enumerate() {
            let ctx = Ctx {
                sm: &layer.state_machine,
                clips: &c.bound.clips,
                decls: &c.state.entries,
            };
            // The first update enters the start state and then advances like any other (recorded, #54).
            let started = self.runs[li].is_none().then(|| {
                let run = LayerRun::start(&ctx, &mut self.params);
                let s = run.current;
                self.runs[li] = Some(run);
                s
            });
            if let Some(s) = started {
                self.behave(c, &layer.state_machine.states[s as usize].actions);
            }
            let run = self.runs[li].as_mut().expect("started above");
            // An interrupted cross-fade continues from the pose last output, before this frame's advance
            // (recorded in Unity, interruption_source).
            let snapshot = run.active.is_some().then(|| {
                self.mixer.layer(c, li, run, &self.params, &self.pose);
                self.mixer.snapshot()
            });
            run.advance(&ctx, &self.params, dt);
            let entered = run
                .select(&ctx, &self.params)
                .map(|t| run.fire(&ctx, &mut self.params, t, snapshot));
            if let Some(s) = entered {
                self.behave(c, &layer.state_machine.states[s as usize].actions);
            }
            let lw = if c.first_in_group[li] {
                1.0
            } else {
                self.layer_weights[li]
            };
            let w = (lw * self.group_weights[layer.group as usize]).clamp(0.0, 1.0);
            if w > 0.0 {
                let run = self.runs[li].as_ref().expect("started above");
                self.mixer.layer(c, li, run, &self.params, &self.pose);
                self.mixer.apply(c, li, w, &mut self.pose, &mut self.written);
            }
        }
        for (s, t) in c.bound.bindings.targets.iter().enumerate() {
            if let Target::Parameter(p) = t
                && self.written[s]
            {
                self.params[*p as usize] = self.pose[s];
            }
        }
    }

    /// Runs a state's actions on entering it.
    fn behave(&mut self, c: &Controller, actions: &[Action]) {
        for b in actions {
            match b {
                Action::Entries { owner_only, ops } => {
                    if self.local || !owner_only {
                        drive(ops, &c.state.entries, &mut self.params, &mut self.rng);
                    }
                }
                Action::PoseSource(parts) => {
                    for &(part, mode) in parts {
                        self.pose_sources[part_index(part)] = mode;
                    }
                }
                &Action::LayerWeight { layer, goal, duration } => self.start_blend(false, layer, goal, duration),
                &Action::GroupWeight { group, goal, duration } => self.start_blend(true, group, goal, duration),
                Action::Locomotion { disabled } => self.locomotion_disabled = *disabled,
            }
        }
    }

    fn start_blend(&mut self, group: bool, index: u32, goal: f32, duration: f32) {
        let weights = if group {
            &self.group_weights
        } else {
            &self.layer_weights
        };
        let from = weights[index as usize];
        self.blends.retain(|b| (b.group, b.index) != (group, index));
        self.blends.push(Blend {
            group,
            index,
            from,
            goal,
            duration,
            elapsed: 0.0,
        });
        self.blend_weights(0.0);
    }

    /// Moves blending weights `dt` seconds on; finished blends end at their goal.
    fn blend_weights(&mut self, dt: f32) {
        for b in &mut self.blends {
            b.elapsed += dt;
            let t = if b.duration > 0.0 {
                (b.elapsed / b.duration).min(1.0)
            } else {
                1.0
            };
            let w = if b.group {
                &mut self.group_weights
            } else {
                &mut self.layer_weights
            };
            w[b.index as usize] = b.from + t * (b.goal - b.from);
        }
        self.blends.retain(|b| b.elapsed < b.duration);
    }
}

/// Whether a state writes rest values: `unwritten` is `Rest` and it has a motion. States without a motion write
/// nothing, so they hold (recorded in Unity 2022.3 and 6000.3, #54).
pub(crate) fn writes_defaults(s: &State) -> bool {
    s.unwritten == Unwritten::Rest && s.motion.is_some()
}
