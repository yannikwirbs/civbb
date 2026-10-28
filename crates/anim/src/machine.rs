//! Mecanim layer state machines: entry, any-state and exit transitions, exit time, interruption, triggers (ADR 0008).

use civbb_format::animator::{
    Condition, ConditionMode, InterruptionSource, ParamKind, Parameter, State, StateIndex, StateMachine, Transition,
    TransitionDest,
};

use crate::bind::Slot;
use crate::clip::BoundClip;
use crate::motion;

/// A state being played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing {
    /// State.
    pub state: StateIndex,
    /// Normalised time, without the cycle offset; keeps growing past 1.
    pub time: f32,
    prev: f32,
    /// Not yet checked for exit time since it was entered; that first check includes its start time.
    entered: bool,
    inclusive: bool,
}

/// A transition, by where it is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionRef {
    /// Any-state transition.
    Any(u32),
    /// Outgoing transition of a state.
    State(StateIndex, u32),
}

/// A running cross-fade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active {
    /// Destination.
    pub to: Playing,
    /// 0 to 1.
    pub progress: f32,
    /// Seconds.
    duration: f32,
    /// The transition.
    pub which: TransitionRef,
}

/// Where a layer blends from.
#[derive(Clone, Debug, PartialEq)]
pub enum From {
    /// A playing state.
    State(Playing),
    /// The static pose captured when a transition was interrupted: slots and values before layer weighting.
    Snapshot(Vec<(Slot, f32)>),
}

/// One layer's state machine at run time.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRun {
    /// Current state, or the snapshot an interrupting transition blends from.
    pub from: From,
    /// The state whose transitions count as the current state's (the source after an interruption).
    pub current: StateIndex,
    /// Cross-fade in progress.
    pub active: Option<Active>,
}

/// What a layer needs to run: its state machine, the clips and the parameters.
pub struct Ctx<'a> {
    /// State machine.
    pub sm: &'a StateMachine,
    /// All clips.
    pub clips: &'a [BoundClip],
    /// Parameter declarations.
    pub decls: &'a [Parameter],
}

impl Ctx<'_> {
    fn state(&self, s: StateIndex) -> &State {
        &self.sm.states[s as usize]
    }

    fn transition(&self, t: TransitionRef) -> &Transition {
        match t {
            TransitionRef::Any(i) => &self.sm.any_state_transitions[i as usize],
            TransitionRef::State(s, i) => &self.state(s).transitions[i as usize],
        }
    }

    fn length(&self, s: StateIndex) -> f32 {
        motion::length(self.state(s).motion.as_ref(), self.clips)
    }

    fn enter(&self, s: StateIndex, offset: f32, params: &[f32]) -> Playing {
        let time = self.state(s).time_param.map_or(offset, |p| params[p as usize]);
        Playing {
            state: s,
            time,
            prev: time,
            entered: true,
            inclusive: false,
        }
    }

    fn advance(&self, p: &mut Playing, params: &[f32], dt: f32) {
        let s = self.state(p.state);
        p.prev = p.time;
        p.inclusive = p.entered;
        p.entered = false;
        p.time = match s.time_param {
            Some(tp) => params[tp as usize],
            None => {
                let speed = s.speed * s.speed_param.map_or(1.0, |sp| params[sp as usize]);
                p.time + dt * speed / self.length(p.state)
            }
        };
    }

    /// The first entry transition whose conditions hold, else the default state. Consumes its triggers.
    fn entry(&self, params: &mut [f32]) -> StateIndex {
        for t in &self.sm.entry_transitions {
            if let TransitionDest::State(s) = t.dest
                && t.conditions.iter().all(|c| holds(c, params))
            {
                self.consume(t, params);
                return s;
            }
        }
        self.sm.default_state
    }

    fn consume(&self, t: &Transition, params: &mut [f32]) {
        for c in &t.conditions {
            if self.decls[c.param as usize].kind == ParamKind::Trigger {
                params[c.param as usize] = 0.0;
            }
        }
    }
}

fn holds(c: &Condition, params: &[f32]) -> bool {
    let v = params[c.param as usize];
    match c.mode {
        ConditionMode::If => v != 0.0,
        ConditionMode::IfNot => v == 0.0,
        ConditionMode::Greater => v > c.threshold,
        ConditionMode::Less => v < c.threshold,
        ConditionMode::Equals => v == c.threshold,
        ConditionMode::NotEqual => v != c.threshold,
    }
}

/// Whether the exit time `e` was reached during the last advance of `p`: on the first frame that reaches it,
/// every cycle for `e < 1` in looping states, once otherwise.
fn exit_reached(p: &Playing, e: f32, looping: bool) -> bool {
    let (prev, cur) = (p.prev, p.time);
    let mut point = if looping && e < 1.0 {
        (prev - e).ceil().max(0.0) + e
    } else {
        e
    };
    if !p.inclusive && point <= prev {
        if !(looping && e < 1.0) {
            return false;
        }
        point += 1.0;
    }
    point >= prev && point <= cur
}

/// Which playing state a candidate transition reads its exit time from.
#[derive(Clone, Copy, PartialEq)]
enum Clock {
    From,
    To,
}

impl LayerRun {
    /// Starts the layer in its entry state.
    pub fn start(ctx: &Ctx, params: &mut [f32]) -> Self {
        let s = ctx.entry(params);
        LayerRun {
            from: From::State(ctx.enter(s, 0.0, params)),
            current: s,
            active: None,
        }
    }

    /// The state the layer is in or heading to.
    pub fn target(&self) -> StateIndex {
        self.active.map_or(self.current, |a| a.to.state)
    }

    /// Advances time and the cross-fade; a finished cross-fade makes its destination current.
    pub fn advance(&mut self, ctx: &Ctx, params: &[f32], dt: f32) {
        if let From::State(p) = &mut self.from {
            ctx.advance(p, params, dt);
        }
        if let Some(a) = &mut self.active {
            ctx.advance(&mut a.to, params, dt);
            a.progress += dt / a.duration;
            if a.progress >= 1.0 {
                self.from = From::State(a.to);
                self.current = a.to.state;
                self.active = None;
            }
        }
    }

    /// The transition to take this frame, if any. Any-state transitions come first; during a cross-fade the
    /// interruption source adds the source's and/or destination's transitions.
    pub fn select(&self, ctx: &Ctx, params: &[f32]) -> Option<TransitionRef> {
        let any = (0..ctx.sm.any_state_transitions.len() as u32).map(|i| (TransitionRef::Any(i), Clock::From));
        let of = |s: StateIndex, clock| {
            (0..ctx.state(s).transitions.len() as u32).map(move |i| (TransitionRef::State(s, i), clock))
        };
        let Some(a) = &self.active else {
            return any
                .chain(of(self.current, Clock::From))
                .find(|&(t, c)| self.valid(ctx, params, t, c))
                .map(|(t, _)| t);
        };
        let tr = ctx.transition(a.which);
        let (src, dst) = (of(self.current, Clock::From), of(a.to.state, Clock::To));
        let queue: Vec<(TransitionRef, Clock)> = match tr.interruption {
            InterruptionSource::None => any.collect(),
            InterruptionSource::Source => any.chain(src).collect(),
            InterruptionSource::Destination => any.chain(dst).collect(),
            InterruptionSource::SourceThenDestination => any.chain(src).chain(dst).collect(),
            InterruptionSource::DestinationThenSource => any.chain(dst).chain(src).collect(),
        };
        for (t, clock) in queue {
            let valid = self.valid(ctx, params, t, clock);
            if t == a.which {
                // Only an any-state transition may interrupt itself.
                if matches!(t, TransitionRef::Any(_)) && valid {
                    return Some(t);
                }
                if tr.ordered_interruption {
                    return None;
                }
            } else if valid {
                return Some(t);
            }
        }
        None
    }

    fn valid(&self, ctx: &Ctx, params: &[f32], t: TransitionRef, clock: Clock) -> bool {
        let tr = ctx.transition(t);
        if tr.conditions.is_empty() && !tr.has_exit_time {
            // Unity ignores transitions with neither.
            return false;
        }
        if matches!(t, TransitionRef::Any(_))
            && !tr.can_transition_to_self
            && tr.dest == TransitionDest::State(self.target())
        {
            return false;
        }
        if tr.has_exit_time {
            let playing = match (clock, &self.from, &self.active) {
                (Clock::To, _, Some(a)) => a.to,
                (Clock::From, From::State(p), _) => *p,
                (Clock::From, From::Snapshot(_), Some(a)) if matches!(t, TransitionRef::Any(_)) => a.to,
                _ => return false,
            };
            let looping = motion::loops(ctx.state(playing.state).motion.as_ref(), ctx.clips);
            if !exit_reached(&playing, tr.exit_time, looping) {
                return false;
            }
        }
        tr.conditions.iter().all(|c| holds(c, params))
    }

    /// Takes transition `t`. `snapshot` is the layer's current pose, required when a cross-fade is interrupted.
    /// Returns the entered state.
    pub fn fire(
        &mut self,
        ctx: &Ctx,
        params: &mut [f32],
        t: TransitionRef,
        snapshot: Option<Vec<(Slot, f32)>>,
    ) -> StateIndex {
        let tr = ctx.transition(t);
        ctx.consume(tr, params);
        let dest = match tr.dest {
            TransitionDest::State(s) => s,
            TransitionDest::Exit => ctx.entry(params),
        };
        let owner = match t {
            TransitionRef::State(s, _) => s,
            TransitionRef::Any(_) => self.target(),
        };
        let duration = if tr.fixed_duration {
            tr.duration
        } else {
            tr.duration * ctx.length(owner)
        };
        if let Some(old) = self.active.take() {
            self.from = From::Snapshot(snapshot.unwrap_or_default());
            self.current = match t {
                TransitionRef::State(s, _) => s,
                TransitionRef::Any(_) => old.to.state,
            };
        }
        let to = ctx.enter(dest, tr.offset, params);
        if duration > 0.0 {
            self.active = Some(Active {
                to,
                progress: 0.0,
                duration,
                which: t,
            });
        } else {
            self.from = From::State(to);
            self.current = dest;
        }
        dest
    }
}
