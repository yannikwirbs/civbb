//! Validation of clips and animator controllers: curves and keys, layers, states, transitions, blend trees and
//! behaviours.

use crate::animator::{
    Animator, Behaviour, BlendTreeKind, Clip, DriverOp, Motion, Property, Transition, TransitionDest,
};

use super::V;

impl V {
    pub(super) fn clips(&mut self, clips: &[Clip]) {
        let c = self.caps;
        self.cap(clips.len(), c.clips, "clips");
        let (mut curves, mut keys) = (0, 0);
        for (ci, clip) in clips.iter().enumerate() {
            let p = || format!("clips[{ci}]");
            self.string(&clip.name, p);
            self.ensure(clip.length.is_finite() && clip.length >= 0.0, p, "invalid length");
            if let Some(r) = &clip.root {
                self.finite(&[r.orientation_offset, r.level], || format!("clips[{ci}].root"));
            }
            curves += clip.curves.len();
            for (ki, curve) in clip.curves.iter().enumerate() {
                let cp = || format!("clips[{ci}].curves[{ki}]");
                keys += curve.keys.len();
                self.string(&curve.binding.path, cp);
                let component_ok = match &curve.binding.property {
                    Property::Position(i) | Property::EulerRotation(i) | Property::Scale(i) => *i < 3,
                    Property::Rotation(i) => *i < 4,
                    Property::RootMotion(i) => *i < 7,
                    Property::Muscle(s) | Property::BlendShape(s) | Property::Material(s) | Property::Parameter(s) => {
                        self.string(s, cp);
                        true
                    }
                    Property::Component { component, property } => {
                        self.string(component, cp);
                        self.string(property, cp);
                        true
                    }
                    Property::Active
                    | Property::RendererEnabled
                    | Property::MaterialSlot(_)
                    | Property::Constraint { .. } => true,
                };
                self.ensure(component_ok, cp, "component index out of range");
                // Tangents may be infinite (stepped keys); times and values may not.
                self.ensure(
                    curve.keys.iter().all(|k| k.time.is_finite() && k.value.is_finite()),
                    cp,
                    "non-finite key time or value",
                );
                self.ensure(
                    curve.keys.windows(2).all(|w| w[0].time < w[1].time),
                    cp,
                    "keys not in ascending time",
                );
            }
        }
        self.cap(curves, c.curves, "total curves");
        self.cap(keys, c.keys, "total keys");
    }

    /// `params` is the number of state entries.
    pub(super) fn animator(&mut self, a: &Animator, clips: usize, params: usize) {
        let c = self.caps;
        self.cap(a.layers.len(), c.layers, "animator.layers");
        let (mut states, mut transitions) = (0, 0);
        for (li, layer) in a.layers.iter().enumerate() {
            let p = || format!("animator.layers[{li}]");
            self.string(&layer.name, p);
            self.index(layer.group as usize, a.groups.len(), p, "group");
            if let Some(mask) = &layer.mask {
                for t in mask.transforms.iter().flatten() {
                    self.string(t, p);
                }
            }
            let sm = &layer.state_machine;
            let n = sm.states.len();
            states += n;
            self.ensure(n > 0, p, "state machine has no states");
            self.index(sm.default_state as usize, n, p, "default state");
            let all = sm.entry_transitions.iter().chain(&sm.any_state_transitions);
            let all = all.chain(sm.states.iter().flat_map(|s| &s.transitions));
            for t in all {
                transitions += 1;
                self.transition(t, n, params, p);
            }
            for (si, s) in sm.states.iter().enumerate() {
                let sp = || format!("animator.layers[{li}].states[{si}]");
                self.string(&s.name, sp);
                for o in [s.speed_param, s.cycle_offset_param, s.time_param, s.mirror_param]
                    .into_iter()
                    .flatten()
                {
                    self.index(o as usize, params, sp, "parameter");
                }
                if let Some(m) = &s.motion {
                    self.motion(m, 0, params, clips, &sp);
                }
                for b in &s.behaviours {
                    match b {
                        Behaviour::ParameterDriver { ops, .. } => {
                            for op in ops {
                                let (a, b) = match *op {
                                    DriverOp::Set { param, .. }
                                    | DriverOp::Add { param, .. }
                                    | DriverOp::Random { param, .. } => (param, param),
                                    DriverOp::Copy { src, dst, .. } => (src, dst),
                                };
                                self.index(a as usize, params, sp, "driver parameter");
                                self.written(b, sp, "driver parameter");
                            }
                        }
                        Behaviour::LayerWeight { layer, .. } => {
                            self.index(*layer as usize, a.layers.len(), sp, "layer")
                        }
                        Behaviour::GroupWeight { group, .. } => {
                            self.index(*group as usize, a.groups.len(), sp, "group")
                        }
                        Behaviour::TrackingControl(_) | Behaviour::Locomotion { .. } => {}
                    }
                }
            }
        }
        self.cap(states, c.states, "total animator states");
        self.cap(transitions, c.transitions, "total animator transitions");
    }

    fn transition(&mut self, t: &Transition, states: usize, params: usize, path: impl Fn() -> String) {
        if let TransitionDest::State(s) = t.dest {
            self.index(s as usize, states, &path, "transition destination");
        }
        for cond in &t.conditions {
            self.index(cond.param as usize, params, &path, "condition parameter");
        }
        self.finite(&[t.exit_time, t.duration, t.offset], path);
    }

    fn motion(&mut self, m: &Motion, depth: usize, params: usize, clips: usize, path: &dyn Fn() -> String) {
        let cap = self.caps.blend_tree_depth;
        match m {
            Motion::Clip(c) => self.index(*c as usize, clips, path, "clip"),
            Motion::BlendTree(_) if depth >= cap => self.fail(path, format!("blend trees nested deeper than {cap}")),
            Motion::BlendTree(tree) => {
                let axes = match tree.kind {
                    BlendTreeKind::Simple1D { param } => vec![param],
                    BlendTreeKind::SimpleDirectional2D { x, y }
                    | BlendTreeKind::FreeformDirectional2D { x, y }
                    | BlendTreeKind::FreeformCartesian2D { x, y } => vec![x, y],
                    BlendTreeKind::Direct => vec![],
                };
                for a in axes {
                    self.index(a as usize, params, path, "blend parameter");
                }
                for child in &tree.children {
                    if let Some(d) = child.direct_param {
                        self.index(d as usize, params, path, "direct parameter");
                    }
                    self.motion(&child.motion, depth + 1, params, clips, path);
                }
            }
        }
    }
}
