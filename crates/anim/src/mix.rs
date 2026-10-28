//! Layer pose mixing: weighted sources, the default fill, override and additive layers, masks (ADR 0008).

use civbb_format::animator::{LayerBlending, State};
use civbb_format::math::Quat;

use crate::bind::Slot;
use crate::clip::normalize;
use crate::controller::Controller;
use crate::machine::{From, LayerRun, Playing};
use crate::motion::{self, Leaf};

const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

/// Scratch buffers for evaluating one layer at a time.
pub(crate) struct Mixer {
    /// Weighted sum of (value - fill) per slot; additive layers: of (value - first-frame value).
    acc: Vec<f32>,
    /// Layer values (override) or deltas (additive) of the written slots.
    val: Vec<f32>,
    mark: Vec<bool>,
    written: Vec<Slot>,
    sample: Vec<f32>,
    base: Vec<f32>,
    leaves: Vec<Leaf>,
}

impl Mixer {
    pub(crate) fn new(slots: usize) -> Self {
        Self {
            acc: vec![0.0; slots],
            val: vec![0.0; slots],
            mark: vec![false; slots],
            written: Vec::new(),
            sample: vec![0.0; slots],
            base: vec![0.0; slots],
            leaves: Vec::new(),
        }
    }

    /// Evaluates layer `li` into `val` for every slot a source with weight above 0 animates.
    pub(crate) fn layer(&mut self, c: &Controller, li: usize, run: &LayerRun, params: &[f32]) {
        for &s in &self.written {
            self.acc[s as usize] = 0.0;
            self.mark[s as usize] = false;
        }
        self.written.clear();
        let additive = c.animator.layers[li].blending == LayerBlending::Additive;
        let defaults = &c.bound.bindings.defaults;
        let progress = run.active.map_or(0.0, |a| a.progress.clamp(0.0, 1.0));
        // Sum of source weights that fall back to the fill value (here the default; #16 adds Write Defaults off).
        let mut fill = 0.0;
        let mut sources: [Option<(f32, Src)>; 2] = [None, None];
        sources[0] = Some((
            1.0 - progress,
            match &run.from {
                From::State(p) => Src::State(*p),
                From::Snapshot(_) => Src::Snapshot,
            },
        ));
        if let Some(a) = &run.active {
            sources[1] = Some((progress, Src::State(a.to)));
        }
        for (ws, src) in sources.into_iter().flatten() {
            if ws <= 0.0 {
                continue;
            }
            match src {
                Src::Snapshot => {
                    let From::Snapshot(snap) = &run.from else {
                        unreachable!()
                    };
                    fill += ws;
                    for &(s, v) in snap {
                        let su = s as usize;
                        let reference = match (additive, c.rot[su]) {
                            (false, _) => defaults[su],
                            (true, 0) => 0.0,
                            (true, k) => IDENTITY[k as usize - 1],
                        };
                        self.add(s, ws * (v - reference));
                    }
                }
                Src::State(p) => {
                    let state = &c.animator.layers[li].state_machine.states[p.state as usize];
                    self.leaves.clear();
                    if let Some(m) = &state.motion {
                        motion::leaves(m, &c.bound.clips, normalized(state, &p, params), &mut self.leaves);
                    }
                    let total: f32 = self.leaves.iter().map(|l| l.weight).sum();
                    fill += if self.leaves.is_empty() { ws } else { ws * total };
                    for li in 0..self.leaves.len() {
                        let leaf = self.leaves[li];
                        let w = ws * leaf.weight;
                        if w > 0.0 {
                            self.leaf(c, leaf, w, additive);
                        }
                    }
                }
            }
        }
        self.written.sort_unstable();
        let mut i = 0;
        while i < self.written.len() {
            let s = self.written[i] as usize;
            if c.rot[s] == 1 {
                let q: [f32; 4] = std::array::from_fn(|k| {
                    let base = if additive { IDENTITY[k] } else { fill * defaults[s + k] };
                    base + self.acc[s + k]
                });
                self.val[s..s + 4].copy_from_slice(&normalize(q));
                i += 4;
            } else {
                self.val[s] = if additive { 0.0 } else { fill * defaults[s] } + self.acc[s];
                i += 1;
            }
        }
    }

    /// Adds one clip at weight `w`: its values relative to the fill, or relative to its first frame (additive).
    fn leaf(&mut self, c: &Controller, leaf: Leaf, w: f32, additive: bool) {
        let clip = &c.bound.clips[leaf.clip as usize];
        let defaults = &c.bound.bindings.defaults;
        clip.sample(leaf.time, &mut self.sample);
        if additive {
            clip.sample(0.0, &mut self.base);
        }
        let mut i = 0;
        while i < clip.slots.len() {
            let s = clip.slots[i];
            let su = s as usize;
            if c.rot[su] == 1 {
                let q = quat(&self.sample, su);
                let d = if additive {
                    align(mul(conj(quat(&self.base, su)), q), IDENTITY)
                } else {
                    align(q, quat(defaults, su))
                };
                let reference = if additive { IDENTITY } else { quat(defaults, su) };
                for k in 0..4 {
                    self.add(s + k as Slot, w * (d[k] - reference[k]));
                }
                i += 4;
            } else {
                let reference = if additive { self.base[su] } else { defaults[su] };
                self.add(s, w * (self.sample[su] - reference));
                i += 1;
            }
        }
    }

    fn add(&mut self, s: Slot, v: f32) {
        let su = s as usize;
        if !self.mark[su] {
            self.mark[su] = true;
            self.written.push(s);
        }
        self.acc[su] += v;
    }

    /// The layer's values, for an interruption snapshot.
    pub(crate) fn snapshot(&self) -> Vec<(Slot, f32)> {
        self.written.iter().map(|&s| (s, self.val[s as usize])).collect()
    }

    /// Blends the evaluated layer over `pose` at `weight`, within its mask; marks written slots.
    pub(crate) fn apply(&self, c: &Controller, li: usize, weight: f32, pose: &mut [f32], written: &mut [bool]) {
        let additive = c.animator.layers[li].blending == LayerBlending::Additive;
        let mask = c.masks[li].as_deref();
        let mut i = 0;
        while i < self.written.len() {
            let su = self.written[i] as usize;
            let rot = c.rot[su] == 1;
            let n = if rot { 4 } else { 1 };
            i += n;
            if mask.is_some_and(|m| !m[su]) {
                continue;
            }
            written[su..su + n].fill(true);
            if rot {
                let cur = quat(pose, su);
                let q = if additive {
                    let d = normalize(lerp(IDENTITY, quat(&self.val, su), weight));
                    mul(cur, d)
                } else {
                    lerp(cur, align(quat(&self.val, su), cur), weight)
                };
                pose[su..su + 4].copy_from_slice(&normalize(q));
            } else if additive {
                pose[su] += weight * self.val[su];
            } else {
                pose[su] += weight * (self.val[su] - pose[su]);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Src {
    State(Playing),
    Snapshot,
}

/// Normalised sample time of a playing state, cycle offset included.
fn normalized(state: &State, p: &Playing, params: &[f32]) -> f32 {
    p.time
        + state
            .cycle_offset_param
            .map_or(state.cycle_offset, |o| params[o as usize])
}

fn quat(buf: &[f32], s: usize) -> Quat {
    [buf[s], buf[s + 1], buf[s + 2], buf[s + 3]]
}

/// `q` or `-q`, whichever is in `reference`'s hemisphere, so weighted sums do not cancel.
fn align(q: Quat, reference: Quat) -> Quat {
    let dot: f32 = q.iter().zip(reference).map(|(a, b)| a * b).sum();
    if dot < 0.0 { q.map(|c| -c) } else { q }
}

fn lerp(a: Quat, b: Quat, t: f32) -> Quat {
    std::array::from_fn(|k| a[k] + t * (b[k] - a[k]))
}

fn conj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

fn mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}
