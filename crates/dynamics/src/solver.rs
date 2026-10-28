//! Steps an avatar's chains against the animated pose (inertia, gravity, pull, stiffness, lengths, immobile, grab and
//! pose) and writes the simulated rotations back into the local pose (ADR 0015).

use civbb_format::dynamics::MultiChild;
use civbb_format::math::{Transform, Vec3};

use crate::math::{Pose, add, conj, dot, from_to, length, mul, normalize, qnormalize, rotate, scale, sub};
use crate::rig::{NO_PARENT, Rig};

/// The step that per-step parameters (pull, spring) refer to, seconds.
pub const REFERENCE_STEP: f32 = 1.0 / 60.0;
/// Longest substep; longer frames are split.
pub const MAX_STEP: f32 = 1.0 / 60.0;
/// Longer frames (hitches) are simulated as this long.
pub const MAX_FRAME: f32 = 0.1;

/// What a chain reports to the animator under its parameter prefix (`_IsGrabbed`, `_IsPosed`, `_Angle`,
/// `_Stretch`, `_Squish`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChainParams {
    /// A player holds the chain.
    pub is_grabbed: bool,
    /// The chain was posed and not grabbed since.
    pub is_posed: bool,
    /// Angle of the chain's last bone from its rest direction, 0 to 1 for 0 to 180 degrees.
    pub angle: f32,
    /// How close the most stretched bone is to its max stretch, 0 to 1.
    pub stretch: f32,
    /// How close the most squished bone is to its max squish, 0 to 1.
    pub squish: f32,
}

/// One avatar's simulation state, for the [`Rig`] it was made for.
#[derive(Clone, Debug)]
pub struct State {
    pos: Vec<Vec3>,
    prev: Vec<Vec3>,
    /// Each particle's rest vector from its parent at the last step, world space.
    rest: Vec<Vec3>,
    /// Per chain: the grabbed particle and where it is held.
    grab: Vec<Option<(u32, Vec3)>>,
    /// Per chain: posed rest vectors per particle, in the chain frame's space.
    posed: Vec<Option<Vec<Vec3>>>,
    /// Per chain: the frame's placement at the last step; `None` before the first.
    frame: Vec<Option<Pose>>,
}

impl State {
    /// A state that starts at the animated pose on the first [`State::step`].
    pub fn new(rig: &Rig) -> Self {
        let chains = rig.chains.len();
        Self {
            pos: vec![[0.0; 3]; rig.len()],
            prev: vec![[0.0; 3]; rig.len()],
            rest: vec![[0.0; 3]; rig.len()],
            grab: vec![None; chains],
            posed: vec![None; chains],
            frame: vec![None; chains],
        }
    }

    /// Simulates `dt` seconds. `locals` is the animated local pose of every node (constraints and IK applied) and is
    /// overwritten with the simulated rotations, and translations where bones stretch; `origin` places the avatar
    /// root's parent in the world.
    pub fn step(&mut self, rig: &Rig, origin: Pose, locals: &mut [Transform], dt: f32) {
        let mut world: Vec<Pose> = Vec::with_capacity(locals.len());
        for (i, l) in locals.iter().enumerate() {
            let parent = rig.parents[i].map_or(origin, |p| world[p as usize]);
            world.push(parent.then(l));
        }
        let dt = dt.clamp(0.0, MAX_FRAME);
        let steps = (dt / MAX_STEP).ceil().max(1.0);
        let h = dt / steps;
        let r = h / REFERENCE_STEP;
        for (c, ch) in rig.chains.iter().enumerate() {
            let range = ch.first as usize..(ch.first + ch.count) as usize;
            let frame = ch.frame.map_or(origin, |f| world[f as usize]);
            let anim: Vec<Vec3> = range
                .clone()
                .map(|k| match rig.node[k] {
                    Some(n) => world[n as usize].pos,
                    None => {
                        let leaf = rig.node[rig.parent[k] as usize].expect("tips hang off nodes");
                        world[leaf as usize].point(rig.endpoint[k])
                    }
                })
                .collect();
            let a = |k: usize| anim[k - ch.first as usize];
            for k in range.clone() {
                let p = rig.parent[k];
                self.rest[k] = match (&self.posed[c], p) {
                    (_, NO_PARENT) => [0.0; 3],
                    (Some(posed), _) => rotate(frame.rot, scale(posed[k - ch.first as usize], frame.scale)),
                    (None, p) => sub(a(k), a(p as usize)),
                };
            }
            let Some(last) = self.frame[c].replace(frame) else {
                for k in range {
                    self.pos[k] = a(k);
                    self.prev[k] = a(k);
                }
                continue;
            };
            // Immobile carries particles along with the frame's motion, so that motion adds no inertia.
            for k in range.clone() {
                let im = rig.immobile[k];
                if im > 0.0 && !rig.pinned[k] {
                    for x in [&mut self.pos[k], &mut self.prev[k]] {
                        let moved = if ch.immobile_world {
                            add(*x, sub(frame.pos, last.pos))
                        } else {
                            let local = scale(rotate(conj(last.rot), sub(*x, last.pos)), 1.0 / last.scale);
                            frame.point(local)
                        };
                        *x = add(*x, scale(sub(moved, *x), im));
                    }
                }
            }
            if dt > 0.0 {
                for _ in 0..steps as u32 {
                    self.substep(rig, c, &anim, h, r);
                }
            }
            self.write(rig, c, origin, &mut world, locals);
        }
    }

    fn substep(&mut self, rig: &Rig, c: usize, anim: &[Vec3], h: f32, r: f32) {
        let ch = &rig.chains[c];
        let first = ch.first as usize;
        let h2 = h * h;
        for k in first..first + ch.count as usize {
            if rig.pinned[k] {
                self.pos[k] = anim[k - first];
                self.prev[k] = anim[k - first];
                continue;
            }
            let p = rig.parent[k] as usize;
            let rest = self.rest[k];
            let len = length(rest);
            let target = add(self.pos[p], rest);
            // Inertia, damped by spring (or momentum) per reference step.
            let v = scale(sub(self.pos[k], self.prev[k]), rig.spring[k].powf(r));
            self.prev[k] = self.pos[k];
            let mut x = add(self.pos[k], v);
            // Gravity in bone lengths per reference step squared, reduced near the rest direction by the falloff.
            let restness = dot(normalize(rest), normalize(sub(self.pos[k], self.pos[p]))).clamp(0.0, 1.0);
            let g =
                rig.gravity[k] * len / (REFERENCE_STEP * REFERENCE_STEP) * (1.0 - rig.gravity_falloff[k] * restness);
            x[1] -= g * h2;
            // Pull: an XPBD constraint towards the rest target with compliance from `pull`.
            let pull = rig.pull[k];
            if pull > 0.0 {
                let alpha = REFERENCE_STEP * REFERENCE_STEP * (1.0 - pull) / pull;
                x = add(x, scale(sub(target, x), h2 / (h2 + alpha)));
            }
            // Stiffness: how far from the rest target a bone may get, 2 bone lengths at 0, none at 1.
            let stiff = rig.stiffness[k];
            if stiff > 0.0 {
                let off = sub(x, target);
                let max = len * (1.0 - stiff) * 2.0;
                if length(off) > max {
                    x = add(target, scale(normalize(off), max));
                }
            }
            // Motion keeps bone lengths; only grabs stretch and squish.
            let d = sub(x, self.pos[p]);
            if length(d) > 1e-9 {
                x = add(self.pos[p], scale(normalize(d), len));
            }
            self.pos[k] = x;
        }
        if let Some((g, target)) = self.grab[c] {
            // The path from the first free ancestor down to the grabbed particle points straight at the hand.
            let mut path = Vec::new();
            let mut k = g as usize;
            while !rig.pinned[k] {
                path.push(k);
                k = rig.parent[k] as usize;
            }
            for &k in path.iter().rev() {
                let p = rig.parent[k] as usize;
                let len = length(self.rest[k]);
                let to = sub(target, self.pos[p]);
                let reach = if k == g as usize {
                    length(to).clamp(len * (1.0 - rig.max_squish[k]), len * (1.0 + rig.max_stretch[k]))
                } else {
                    len
                };
                self.pos[k] = add(self.pos[p], scale(normalize(to), reach));
                self.prev[k] = self.pos[k];
            }
        }
    }

    /// Turns each chain node so its rest directions point at its simulated children, parents first.
    fn write(&self, rig: &Rig, c: usize, origin: Pose, world: &mut [Pose], locals: &mut [Transform]) {
        let ch = &rig.chains[c];
        let first = ch.first as usize;
        let range = first..first + ch.count as usize;
        for k in range.clone() {
            let Some(n) = rig.node[k] else { continue };
            let n = n as usize;
            let parent = rig.parents[n].map_or(origin, |p| world[p as usize]);
            let base = parent.then(&locals[n]);
            let mut aim = range.clone().filter(|&q| rig.parent[q] == k as u32);
            let aim: Vec<usize> = match (k == first, ch.multi_child) {
                (true, MultiChild::Ignore) => Vec::new(),
                (true, MultiChild::First) => aim.next().into_iter().collect(),
                _ => aim.collect(),
            };
            let mut sum = [0.0; 4];
            for &q in &aim {
                let rest_dir = match rig.node[q] {
                    Some(m) => locals[m as usize].translation,
                    None => rig.endpoint[q],
                };
                let rest_dir = normalize(rotate(base.rot, rest_dir));
                let swing = from_to(rest_dir, normalize(sub(self.pos[q], base.pos)));
                let sign = if sum.iter().zip(swing).map(|(a, b)| a * b).sum::<f32>() < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                sum = std::array::from_fn(|i| sum[i] + sign * swing[i]);
            }
            let rot = if aim.is_empty() {
                base.rot
            } else {
                qnormalize(mul(qnormalize(sum), base.rot))
            };
            world[n] = Pose { rot, ..base };
            locals[n].rotation = mul(conj(parent.rot), rot);
            // Grabbed bones may stretch or squish: scale the child's offset to the simulated length.
            for &q in &aim {
                if let Some(m) = rig.node[q] {
                    let t = &mut locals[m as usize].translation;
                    let rest = length(*t) * base.scale;
                    let now = length(sub(self.pos[q], base.pos));
                    if rest > 1e-9 && (now / rest - 1.0).abs() > 1e-4 {
                        *t = scale(*t, now / rest);
                    }
                }
            }
        }
    }

    /// Holds chain `chain`'s particle `particle` (counted from the chain root, 0) at world position `target`,
    /// moving it there each step until [`State::release`]. Refused (false) if the chain disallows grabbing or the
    /// particle is pinned. Grabbing clears a pose.
    pub fn grab(&mut self, rig: &Rig, chain: usize, particle: u32, target: Vec3) -> bool {
        let ch = &rig.chains[chain];
        if !ch.allow_grab || particle >= ch.count || rig.pinned[(ch.first + particle) as usize] {
            return false;
        }
        self.grab[chain] = Some((ch.first + particle, target));
        self.posed[chain] = None;
        true
    }

    /// Lets go of chain `chain`. With `pose` (and posing allowed) its current shape becomes its rest shape, relative
    /// to the chain's frame, until it is grabbed again or [`State::unpose`]d.
    pub fn release(&mut self, rig: &Rig, chain: usize, pose: bool) {
        self.grab[chain] = None;
        let ch = &rig.chains[chain];
        let Some(frame) = self.frame[chain] else { return };
        if pose && ch.allow_posing {
            let range = ch.first as usize..(ch.first + ch.count) as usize;
            let shape = range
                .map(|k| match rig.parent[k] {
                    NO_PARENT => [0.0; 3],
                    p => scale(
                        rotate(conj(frame.rot), sub(self.pos[k], self.pos[p as usize])),
                        1.0 / frame.scale,
                    ),
                })
                .collect();
            self.posed[chain] = Some(shape);
        }
    }

    /// Returns chain `chain` to its animated rest shape.
    pub fn unpose(&mut self, chain: usize) {
        self.posed[chain] = None;
    }

    /// What chain `chain` reports to the animator.
    pub fn params(&self, rig: &Rig, chain: usize) -> ChainParams {
        let ch = &rig.chains[chain];
        let range = ch.first as usize..(ch.first + ch.count) as usize;
        let mut out = ChainParams {
            is_grabbed: self.grab[chain].is_some(),
            is_posed: self.posed[chain].is_some(),
            ..ChainParams::default()
        };
        for k in range.clone().filter(|&k| !rig.pinned[k]) {
            let rest = length(self.rest[k]);
            let now = length(sub(self.pos[k], self.pos[rig.parent[k] as usize]));
            if rest <= 1e-9 {
                continue;
            }
            let ratio = now / rest;
            if rig.max_stretch[k] > 0.0 {
                out.stretch = out.stretch.max(((ratio - 1.0) / rig.max_stretch[k]).clamp(0.0, 1.0));
            }
            if rig.max_squish[k] > 0.0 {
                out.squish = out.squish.max(((1.0 - ratio) / rig.max_squish[k]).clamp(0.0, 1.0));
            }
        }
        if let Some(k) = range.last().filter(|&k| !rig.pinned[k]) {
            let now = normalize(sub(self.pos[k], self.pos[rig.parent[k] as usize]));
            let cos = dot(normalize(self.rest[k]), now).clamp(-1.0, 1.0);
            out.angle = cos.acos().to_degrees() / 180.0;
        }
        out
    }
}
