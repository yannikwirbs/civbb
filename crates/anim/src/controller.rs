//! A compiled animator controller and one running instance of it: parameters, layers, the evaluated pose.

use std::collections::HashSet;

use civbb_format::Avatar;
use civbb_format::animator::{Animator, ParamIndex};

use crate::bind::{Bound, Target, bind, node_paths};
use crate::machine::{Ctx, LayerRun};
use crate::mix::Mixer;

/// An avatar's animator with its clips bound to slots and its masks resolved. Shared by every instance.
#[derive(Clone, Debug)]
pub struct Controller {
    /// The animator.
    pub animator: Animator,
    /// Bound clips and slots.
    pub bound: Bound,
    /// Per layer, which slots its mask lets it write; `None` writes all.
    pub(crate) masks: Vec<Option<Vec<bool>>>,
    /// Per slot, 0 or the quaternion component plus 1 (1 marks a rotation's first slot).
    pub(crate) rot: Vec<u8>,
    /// Per layer, whether it is the first of its group, whose weight Unity fixes at 1.
    first_in_group: Vec<bool>,
    /// Some state has Write Defaults off: frames start from the last pose instead of defaults (ADR 0010).
    pub wd_off: bool,
}

impl Controller {
    /// Compiles a validated avatar's animator; `None` if it has none.
    pub fn new(avatar: &Avatar) -> Option<Self> {
        let animator = avatar.animator.clone()?;
        let bound = bind(avatar);
        let paths = node_paths(&avatar.hierarchy);
        let node_of = |t: &Target| match *t {
            Target::Position(n, _) | Target::Rotation(n, _) | Target::Scale(n, _) | Target::Active(n) => Some(n),
            Target::BlendShape(m, _) | Target::RendererEnabled(m) => Some(avatar.meshes[m as usize].node),
            Target::Parameter(_) => None,
        };
        let masks = animator
            .layers
            .iter()
            .map(|l| {
                let allowed: HashSet<&str> = l
                    .mask
                    .as_ref()?
                    .transforms
                    .as_ref()?
                    .iter()
                    .map(String::as_str)
                    .collect();
                let targets = &bound.bindings.targets;
                Some(
                    targets
                        .iter()
                        .map(|t| node_of(t).is_none_or(|n| allowed.contains(paths[n as usize].as_str())))
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
                _ => 0,
            })
            .collect();
        let mut seen = HashSet::new();
        let first_in_group = animator.layers.iter().map(|l| seen.insert(l.group)).collect();
        let wd_off = animator
            .layers
            .iter()
            .flat_map(|l| &l.state_machine.states)
            .any(|s| !s.write_defaults);
        Some(Self {
            animator,
            bound,
            masks,
            rot,
            first_in_group,
            wd_off,
        })
    }

    /// The parameter called `name`.
    pub fn param(&self, name: &str) -> Option<ParamIndex> {
        self.animator
            .parameters
            .iter()
            .position(|p| p.name == name)
            .map(|i| i as ParamIndex)
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
    runs: Vec<Option<LayerRun>>,
    pose: Vec<f32>,
    written: Vec<bool>,
    mixer: Mixer,
}

impl Instance {
    /// A fresh instance at the parameters' defaults; the first [`Instance::update`] enters the start states.
    pub fn new(c: &Controller) -> Self {
        let a = &c.animator;
        let n = c.bound.bindings.len();
        Self {
            params: a.parameters.iter().map(|p| p.default).collect(),
            layer_weights: a.layers.iter().map(|l| l.weight).collect(),
            group_weights: a.groups.iter().map(|g| g.weight).collect(),
            runs: vec![None; a.layers.len()],
            pose: c.bound.bindings.defaults.clone(),
            written: vec![false; n],
            mixer: Mixer::new(n),
        }
    }

    /// Slot values after the last update.
    pub fn pose(&self) -> &[f32] {
        &self.pose
    }

    /// Layer `i`'s state machine; `None` before the first update.
    pub fn layer(&self, i: usize) -> Option<&LayerRun> {
        self.runs[i].as_ref()
    }

    /// Advances every layer by `dt` seconds, takes transitions and evaluates the pose, starting from defaults or,
    /// with any Write Defaults off state, from the last pose. Animated parameters take their new values at the
    /// end, so conditions see them next frame.
    pub fn update(&mut self, c: &Controller, dt: f32) {
        if !c.wd_off {
            self.pose.copy_from_slice(&c.bound.bindings.defaults);
        }
        self.written.fill(false);
        for (li, layer) in c.animator.layers.iter().enumerate() {
            let ctx = Ctx {
                sm: &layer.state_machine,
                clips: &c.bound.clips,
                decls: &c.animator.parameters,
            };
            let run = match &mut self.runs[li] {
                Some(run) => {
                    run.advance(&ctx, &self.params, dt);
                    if let Some(t) = run.select(&ctx, &self.params) {
                        let snapshot = run.active.is_some().then(|| {
                            self.mixer.layer(c, li, run, &self.params, &self.pose);
                            self.mixer.snapshot()
                        });
                        run.fire(&ctx, &mut self.params, t, snapshot);
                    }
                    run
                }
                slot @ None => slot.insert(LayerRun::start(&ctx, &mut self.params)),
            };
            let lw = if c.first_in_group[li] {
                1.0
            } else {
                self.layer_weights[li]
            };
            let w = (lw * self.group_weights[layer.group as usize]).clamp(0.0, 1.0);
            if w > 0.0 {
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
}
