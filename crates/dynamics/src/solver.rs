//! Steps an avatar's chains against the animated pose and writes the simulated rotations back into the local pose:
//! the VRChat SDK's PhysBone model as fitted to its recordings (ADR 0049), plus grab and pose.

use civbb_format::dynamics::{MultiChild, Shape};
use civbb_format::math::{Quat, Transform, Vec3};

use crate::rig::{LimitKind, NO_PARENT, Rig};
use civbb_core::math::{
    IDENTITY, Pose, add, conj, cross, dot, from_to, length, mul, nlerp, normalize, qnormalize, rotate, scale, sub,
    unit, world_poses,
};

/// The fixed simulation step, seconds. Frame time accumulates; leftover time is simulated for the output only.
pub const STEP: f32 = 1.0 / 60.0;
/// Longer frames (hitches) are simulated as this long.
pub const MAX_FRAME: f32 = 0.1;
/// Simplified integration keeps this fraction of `spring` as velocity.
const SPRING_KEEP: f32 = 0.99;
const DOWN: Vec3 = [0.0, -1.0, 0.0];

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

/// A collider placed in the world for this step.
#[derive(Clone, Copy, Debug)]
enum Placed {
    /// A segment from `a` to `b` (a point for spheres) with a radius.
    Capsule { a: Vec3, b: Vec3, radius: f32 },
    /// A plane through `point` with unit `normal`.
    Plane { point: Vec3, normal: Vec3 },
}

/// Per-particle simulation state.
#[derive(Clone, Debug, Default)]
struct Particles {
    pos: Vec<Vec3>,
    /// Position before the last step (advanced integration's velocity).
    prev: Vec<Vec3>,
    /// Displacement of the last step before the length constraint (simplified integration's velocity).
    vel: Vec<Vec3>,
    /// Offset from the parent after the last step, before immobile moved anything.
    off: Vec<Vec3>,
    /// Animated (or posed) offset from the parent at the last step, world space.
    rest: Vec<Vec3>,
}

impl Particles {
    fn new(n: usize) -> Self {
        let v = vec![[0.0; 3]; n];
        Self {
            pos: v.clone(),
            prev: v.clone(),
            vel: v.clone(),
            off: v.clone(),
            rest: v,
        }
    }

    fn copy_range(&mut self, from: &Self, r: std::ops::Range<usize>) {
        for (a, b) in [
            (&mut self.pos, &from.pos),
            (&mut self.prev, &from.prev),
            (&mut self.vel, &from.vel),
            (&mut self.off, &from.off),
            (&mut self.rest, &from.rest),
        ] {
            a[r.clone()].copy_from_slice(&b[r.clone()]);
        }
    }
}

/// One avatar's simulation state, for the [`Rig`] it was made for.
#[derive(Clone, Debug)]
pub struct State {
    /// After the last full step.
    p: Particles,
    /// At the frame's time: the last full step plus a partial one for the leftover time.
    out: Particles,
    /// Per particle: rotation of its bone from the animated to the simulated direction (scratch).
    turn: Vec<Quat>,
    /// Per chain: the grabbed particle and where it is held.
    grab: Vec<Option<(u32, Vec3)>>,
    /// Per chain: posed rest vectors per particle, in the chain frame's space.
    posed: Vec<Option<Vec<Vec3>>>,
    /// Per chain: the frame's placement at the last full step.
    frame: Vec<Option<Pose>>,
    /// The animated world pose at the last frame; `None` before the first.
    last: Option<Vec<Pose>>,
    /// Seconds since the last full step.
    time: f32,
}

impl State {
    /// A state that starts at the animated pose on the first [`State::step`].
    pub fn new(rig: &Rig) -> Self {
        let chains = rig.chains.len();
        Self {
            p: Particles::new(rig.len()),
            out: Particles::new(rig.len()),
            turn: vec![IDENTITY; rig.len()],
            grab: vec![None; chains],
            posed: vec![None; chains],
            frame: vec![None; chains],
            last: None,
            time: 0.0,
        }
    }

    /// Simulates `dt` seconds. `locals` is the animated local pose of every node (constraints and IK applied) and is
    /// overwritten with the simulated rotations, and translations where bones stretch; `origin` places the avatar
    /// root's parent in the world.
    pub fn step(&mut self, rig: &Rig, origin: Pose, locals: &mut [Transform], dt: f32) {
        if rig.chains.is_empty() {
            return;
        }
        let mut world = world_poses(&rig.parents, origin, locals);
        let start = match self.last.take() {
            Some(start) => start,
            None => {
                for c in 0..rig.chains.len() {
                    self.start(rig, c, origin, &world);
                }
                world.clone()
            }
        };
        let dt = dt.clamp(0.0, MAX_FRAME);
        // Full steps at their time within the frame, against the animated pose interpolated to that time.
        let total = self.time + dt;
        let steps = ((total + 1e-6) / STEP).floor() as u32;
        let mut between = world.clone();
        for j in 1..=steps {
            let t = if dt > 0.0 {
                ((j as f32 * STEP - self.time) / dt).clamp(0.0, 1.0)
            } else {
                1.0
            };
            for (b, (s, w)) in between.iter_mut().zip(start.iter().zip(&world)) {
                *b = Pose {
                    pos: add(s.pos, scale(sub(w.pos, s.pos), t)),
                    rot: nlerp(s.rot, w.rot, t),
                    scale: s.scale + (w.scale - s.scale) * t,
                };
            }
            let placed = place(rig, &between);
            for c in 0..rig.chains.len() {
                let frame = frame_of(rig, c, origin, &between);
                let last = self.frame[c].replace(frame);
                let ctx = Ctx {
                    world: &between,
                    placed: &placed,
                    frame,
                    last,
                    posed: self.posed[c].as_deref(),
                    grab: self.grab[c],
                    r: 1.0,
                };
                advance(rig, c, &ctx, &mut self.p, &mut self.turn);
            }
        }
        self.time = (total - steps as f32 * STEP).max(0.0);
        // The output: the leftover time as a partial step that is not kept.
        let r = self.time / STEP;
        let placed = place(rig, &world);
        for c in 0..rig.chains.len() {
            let ch = &rig.chains[c];
            let range = ch.first as usize..(ch.first + ch.count) as usize;
            self.out.copy_range(&self.p, range);
            if r > 1e-4 {
                let ctx = Ctx {
                    world: &world,
                    placed: &placed,
                    frame: frame_of(rig, c, origin, &world),
                    last: self.frame[c],
                    posed: self.posed[c].as_deref(),
                    grab: self.grab[c],
                    r,
                };
                advance(rig, c, &ctx, &mut self.out, &mut self.turn);
            }
        }
        let animated = world.clone();
        for c in 0..rig.chains.len() {
            self.write(rig, c, origin, &mut world, locals);
        }
        self.last = Some(animated);
    }

    /// Puts chain `c` at the animated pose, at rest.
    fn start(&mut self, rig: &Rig, c: usize, origin: Pose, world: &[Pose]) {
        let ch = &rig.chains[c];
        for k in ch.first as usize..(ch.first + ch.count) as usize {
            let a = anim(rig, world, k);
            let off = match rig.parent[k] {
                NO_PARENT => [0.0; 3],
                p => sub(a, anim(rig, world, p as usize)),
            };
            for ps in [&mut self.p, &mut self.out] {
                ps.pos[k] = a;
                ps.prev[k] = a;
                ps.vel[k] = [0.0; 3];
                ps.off[k] = off;
                ps.rest[k] = off;
            }
        }
        self.frame[c] = Some(frame_of(rig, c, origin, world));
    }

    /// Turns each chain node so its rest directions point at its simulated children, parents first.
    fn write(&self, rig: &Rig, c: usize, origin: Pose, world: &mut [Pose], locals: &mut [Transform]) {
        let ch = &rig.chains[c];
        let first = ch.first as usize;
        let range = first..first + ch.count as usize;
        let pos = &self.out.pos;
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
                let swing = from_to(rest_dir, normalize(sub(pos[q], base.pos)));
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
                    let now = length(sub(pos[q], base.pos));
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
            let pos = &self.p.pos;
            let shape = range
                .map(|k| match rig.parent[k] {
                    NO_PARENT => [0.0; 3],
                    p => scale(rotate(conj(frame.rot), sub(pos[k], pos[p as usize])), 1.0 / frame.scale),
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
        let (pos, rest) = (&self.out.pos, &self.out.rest);
        let mut out = ChainParams {
            is_grabbed: self.grab[chain].is_some(),
            is_posed: self.posed[chain].is_some(),
            ..ChainParams::default()
        };
        for k in range.clone().filter(|&k| !rig.pinned[k]) {
            let rest = length(rest[k]);
            let now = length(sub(pos[k], pos[rig.parent[k] as usize]));
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
            let now = normalize(sub(pos[k], pos[rig.parent[k] as usize]));
            let cos = dot(normalize(rest[k]), now).clamp(-1.0, 1.0);
            out.angle = cos.acos().to_degrees() / 180.0;
        }
        out
    }
}

/// What one step of one chain reads.
struct Ctx<'a> {
    /// The animated world pose at the step's time.
    world: &'a [Pose],
    placed: &'a [(Placed, bool)],
    /// The chain frame now and at the last full step.
    frame: Pose,
    last: Option<Pose>,
    posed: Option<&'a [Vec3]>,
    grab: Option<(u32, Vec3)>,
    /// The step's length as a fraction of [`STEP`] (below 1 for the partial output step).
    r: f32,
}

/// The node whose motion `immobile` cancels, placed: the chain root's parent, or the avatar origin.
fn frame_of(rig: &Rig, c: usize, origin: Pose, world: &[Pose]) -> Pose {
    rig.chains[c].frame.map_or(origin, |f| world[f as usize])
}

/// Particle `k`'s animated position.
fn anim(rig: &Rig, world: &[Pose], k: usize) -> Vec3 {
    match rig.node[k] {
        Some(n) => world[n as usize].pos,
        None => {
            let leaf = rig.node[rig.parent[k] as usize].expect("tips hang off nodes");
            world[leaf as usize].point(rig.endpoint[k])
        }
    }
}

/// Every collider placed in the world.
fn place(rig: &Rig, world: &[Pose]) -> Vec<(Placed, bool)> {
    rig.colliders
        .iter()
        .map(|col| {
            let w = world[col.node as usize];
            let center = w.point(col.position);
            let up = rotate(mul(w.rot, col.rotation), [0.0, 1.0, 0.0]);
            let shape = match col.shape {
                Shape::Sphere { radius } => Placed::Capsule {
                    a: center,
                    b: center,
                    radius: radius.max(0.0) * w.scale,
                },
                Shape::Capsule { radius, height } => {
                    let radius = radius.max(0.0) * w.scale;
                    let half = (height * 0.5 * w.scale - radius).max(0.0);
                    Placed::Capsule {
                        a: add(center, scale(up, -half)),
                        b: add(center, scale(up, half)),
                        radius,
                    }
                }
                Shape::Plane => Placed::Plane {
                    point: center,
                    normal: up,
                },
            };
            (shape, col.inside)
        })
        .collect()
}

/// One step of chain `c`, parents first (ADR 0049).
fn advance(rig: &Rig, c: usize, ctx: &Ctx, ps: &mut Particles, turn: &mut [Quat]) {
    let ch = &rig.chains[c];
    let first = ch.first as usize;
    let range = first..first + ch.count as usize;
    let world = ctx.world;
    let (frame, r) = (ctx.frame, ctx.r);
    // Each particle's animated (or posed) offset from its parent.
    let base = |k: usize| -> Vec3 {
        match (ctx.posed, rig.parent[k]) {
            (_, NO_PARENT) => [0.0; 3],
            (Some(posed), _) => rotate(frame.rot, scale(posed[k - first], frame.scale)),
            (None, p) => sub(anim(rig, world, k), anim(rig, world, p as usize)),
        }
    };
    // Immobile carries particles along with the frame's motion since the last step, so it adds no velocity.
    if let Some(last) = ctx.last {
        for k in range.clone() {
            let im = rig.immobile[k];
            if im > 0.0 && !rig.pinned[k] {
                for x in [&mut ps.pos[k], &mut ps.prev[k]] {
                    let moved = if ch.immobile_world {
                        add(*x, sub(frame.pos, last.pos))
                    } else {
                        frame.point(scale(rotate(conj(last.rot), sub(*x, last.pos)), 1.0 / last.scale))
                    };
                    *x = add(*x, scale(sub(moved, *x), im));
                }
            }
        }
    }
    for k in range.clone() {
        if rig.pinned[k] {
            let a = anim(rig, world, k);
            ps.pos[k] = a;
            ps.prev[k] = a;
            ps.vel[k] = [0.0; 3];
            turn[k] = IDENTITY;
            continue;
        }
        let p = rig.parent[k] as usize;
        let parent = ps.pos[p];
        let b = base(k);
        let len = length(b);
        // The rest offset follows the parent bone's simulated rotation.
        let off = rotate(turn[p], b);
        let dir = normalize(off);
        let mut g = rig.gravity[k];
        if g != 0.0 && rig.gravity_falloff[k] > 0.0 {
            let at_rest = dot(normalize(ps.off[k]), normalize(b)).max(0.0);
            g *= 1.0 - rig.gravity_falloff[k] * at_rest;
        }
        let rest_dir = if g != 0.0 && !ch.version_1_0 {
            // Version 1.1: gravity bends the rest direction towards down.
            unit(add(scale(dir, 1.0 - g.abs()), scale(DOWN, g))).unwrap_or(dir)
        } else {
            dir
        };
        let target = add(parent, scale(rest_dir, len));
        let pos = ps.pos[k];
        let pull = rig.pull[k] * r;
        let mut x = if ch.advanced {
            let mut x = add(pos, scale(sub(pos, ps.prev[k]), rig.spring[k] * r));
            x = add(x, scale(sub(target, x), pull));
            // Stiffness keeps the bone's last offset from its parent.
            let stiff = add(parent, ps.off[k]);
            add(x, scale(sub(stiff, x), rig.stiffness[k]))
        } else {
            let keep = rig.spring[k] * SPRING_KEEP * r;
            add(
                pos,
                add(scale(ps.vel[k], keep), scale(sub(target, pos), pull * (1.0 - keep))),
            )
        };
        let mut vel = sub(x, pos);
        if g != 0.0 && ch.version_1_0 {
            // Version 1.0: gravity is a force of `gravity` bone lengths per step.
            x = add(x, scale(DOWN, g * len));
        }
        if ch.limit != LimitKind::None {
            let reach = sub(x, parent);
            let (axis, across) = limit_axes(rig, world, turn[p], ch.limit_rotation, k, p);
            let d = limit(ch.limit, axis, across, rig.limit_a[k], rig.limit_b[k], normalize(reach));
            x = add(parent, scale(d, length(reach)));
        }
        x = add(parent, scale(normalize(sub(x, parent)), len));
        // Colliders push the bone (the segment from its parent) out, then the bone keeps its length.
        for &ci in &ch.colliders {
            let (shape, inside) = ctx.placed[ci as usize];
            let pushed = collide(shape, inside, rig.radius[k], parent, x);
            if pushed != x {
                if let Some(n) = unit(sub(pushed, x)) {
                    // The push stops motion into the collider.
                    let into = dot(vel, n);
                    if into < 0.0 {
                        vel = sub(vel, scale(n, into));
                    }
                }
                x = add(parent, scale(normalize(sub(pushed, parent)), len));
            }
        }
        ps.prev[k] = pos;
        ps.pos[k] = x;
        ps.vel[k] = vel;
        ps.rest[k] = b;
        turn[k] = from_to(dir_or(b), normalize(sub(x, parent)));
    }
    if let Some((g, target)) = ctx.grab {
        // The path from the first free ancestor down to the grabbed particle points straight at the hand.
        let mut path = Vec::new();
        let mut k = g as usize;
        while !rig.pinned[k] {
            path.push(k);
            k = rig.parent[k] as usize;
        }
        for &k in path.iter().rev() {
            let p = rig.parent[k] as usize;
            let len = length(base(k));
            let to = sub(target, ps.pos[p]);
            let reach = if k == g as usize {
                length(to).clamp(len * (1.0 - rig.max_squish[k]), len * (1.0 + rig.max_stretch[k]))
            } else {
                len
            };
            ps.pos[k] = add(ps.pos[p], scale(normalize(to), reach));
            ps.prev[k] = ps.pos[k];
            ps.vel[k] = [0.0; 3];
        }
    }
    for k in range {
        if rig.parent[k] != NO_PARENT {
            ps.off[k] = sub(ps.pos[k], ps.pos[rig.parent[k] as usize]);
        }
    }
}

fn dir_or(v: Vec3) -> Vec3 {
    unit(v).unwrap_or([0.0, 1.0, 0.0])
}

/// Particle `k`'s limit frame (its Y and X): the parent node's animated rotation turned with the parent bone
/// (`turn`), swung in the parent's space so Y lies along the child's local direction, then by the limit rotation.
fn limit_axes(rig: &Rig, world: &[Pose], turn: Quat, limit_rotation: Quat, k: usize, p: usize) -> (Vec3, Vec3) {
    let node = rig.node[p].expect("limits apply below nodes");
    let rot = world[node as usize].rot;
    let local = match rig.node[k] {
        Some(n) => rotate(conj(rot), sub(world[n as usize].pos, world[node as usize].pos)),
        None => rig.endpoint[k],
    };
    let frame = mul(
        mul(mul(turn, rot), from_to([0.0, 1.0, 0.0], dir_or(local))),
        limit_rotation,
    );
    (rotate(frame, [0.0, 1.0, 0.0]), rotate(frame, [1.0, 0.0, 0.0]))
}

/// Direction `d` brought within a limit around `axis` (the limit frame's Y); `across` is the frame's X, which the
/// hinge and pitch turn about (yaw turns about the third axis). Angles in radians.
fn limit(kind: LimitKind, axis: Vec3, across: Vec3, a: f32, b: f32, d: Vec3) -> Vec3 {
    let mut x = sub(across, scale(axis, dot(across, axis)));
    if length(x) < 1e-6 {
        x = cross(
            axis,
            if axis[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            },
        );
    }
    let x = normalize(x);
    let z = cross(axis, x);
    let (along, px, pz) = (dot(d, axis), dot(d, x), dot(d, z));
    match kind {
        LimitKind::Hinge => {
            // Turning about X keeps the direction in the plane of the axis and Z.
            let angle = pz.atan2(along).clamp(-a, a);
            add(scale(axis, angle.cos()), scale(z, angle.sin()))
        }
        LimitKind::Polar => {
            // Yaw (about Z, travel along X) as a latitude, pitch (about X, travel along Z) as a longitude, each
            // clamped on its own.
            let yaw = px.clamp(-1.0, 1.0).asin().clamp(-b, b);
            let pitch = pz.atan2(along).clamp(-a, a);
            add(
                scale(add(scale(axis, pitch.cos()), scale(z, pitch.sin())), yaw.cos()),
                scale(x, yaw.sin()),
            )
        }
        _ => {
            let theta = along.clamp(-1.0, 1.0).acos();
            if theta <= a {
                return d;
            }
            let side = unit(add(scale(x, px), scale(z, pz))).unwrap_or(x);
            add(scale(axis, a.cos()), scale(side, a.sin()))
        }
    }
}

/// Bone end `x` (parent at `a`, bone radius `r`) pushed so the bone leaves a collider, or stays inside one: by the
/// depth at the bone's closest point, along that point's normal.
fn collide(shape: Placed, inside: bool, r: f32, a: Vec3, x: Vec3) -> Vec3 {
    match shape {
        Placed::Capsule { a: p, b: q, radius } => {
            if inside {
                // The bone point farthest from the core is an end: the tip.
                let (c, _) = closest_on(p, q, x);
                let d = sub(x, c);
                let l = length(d);
                let room = (radius - r).max(0.0);
                return if l > room { add(c, scale(dir_or(d), room)) } else { x };
            }
            let (on_bone, on_core) = closest_between(a, x, p, q);
            let d = sub(on_bone, on_core);
            let l = length(d);
            if l >= radius + r {
                return x;
            }
            add(x, scale(dir_or(d), radius + r - l))
        }
        Placed::Plane { point, normal } => {
            let (sa, sx) = (dot(sub(a, point), normal), dot(sub(x, point), normal));
            if inside {
                let m = sa.max(sx);
                if m > -r { sub(x, scale(normal, m + r)) } else { x }
            } else {
                let m = sa.min(sx);
                if m < r { add(x, scale(normal, r - m)) } else { x }
            }
        }
    }
}

/// The point of segment `p`-`q` closest to `x`, and its parameter.
fn closest_on(p: Vec3, q: Vec3, x: Vec3) -> (Vec3, f32) {
    let pq = sub(q, p);
    let n = dot(pq, pq);
    let t = if n > 1e-12 {
        (dot(sub(x, p), pq) / n).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (add(p, scale(pq, t)), t)
}

/// The closest points of segments `p1`-`q1` and `p2`-`q2` (Ericson, Real-Time Collision Detection 5.1.9).
fn closest_between(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    if e <= 1e-12 {
        return (closest_on(p1, q1, p2).0, p2);
    }
    if a <= 1e-12 {
        return (p1, closest_on(p2, q2, p1).0);
    }
    let (c, b) = (dot(d1, r), dot(d1, d2));
    let den = a * e - b * b;
    let mut s = if den > 1e-12 {
        ((b * f - c * e) / den).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}
