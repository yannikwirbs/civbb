//! Constraints: ordered by dependency once when compiled, then evaluated in one pass over the local pose (ADR 0018).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use civbb_format::Avatar;
use civbb_format::constraint::{Constraint, ConstraintKind, WorldUp};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Transform, Vec3};

use crate::clip::{euler_to_quat, normalize, quat_to_euler};
use crate::mix::{conj, mul};

/// An avatar's constraints in evaluation order: every constraint after those that move what it reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constraints {
    parents: Vec<Option<NodeIndex>>,
    /// The constraints, sorted.
    pub list: Vec<Constraint>,
}

impl Constraints {
    /// Sorts a validated avatar's constraints. A constraint runs after every constraint whose target is (or is an
    /// ancestor of) a node it reads: its sources, its up node, its target. Ties and cycles keep list order.
    pub fn new(avatar: &Avatar) -> Self {
        let parents: Vec<Option<NodeIndex>> = avatar.hierarchy.nodes.iter().map(|n| n.parent).collect();
        let list = &avatar.constraints;
        let mut by_target: HashMap<NodeIndex, Vec<usize>> = HashMap::new();
        for (i, c) in list.iter().enumerate() {
            by_target.entry(c.target).or_default().push(i);
        }
        let mut after: Vec<Vec<usize>> = vec![Vec::new(); list.len()];
        let mut waits = vec![0usize; list.len()];
        for (b, c) in list.iter().enumerate() {
            let mut before = Vec::new();
            for (read, own) in reads(c) {
                let mut n = Some(read);
                while let Some(node) = n {
                    for &a in by_target.get(&node).into_iter().flatten() {
                        // A constraint on the same target runs after the earlier ones only.
                        if a != b && !(own && node == c.target && a > b) {
                            before.push(a);
                        }
                    }
                    n = parents[node as usize];
                }
            }
            before.sort_unstable();
            before.dedup();
            waits[b] = before.len();
            for a in before {
                after[a].push(b);
            }
        }

        let mut ready: BinaryHeap<Reverse<usize>> = (0..list.len()).filter(|&i| waits[i] == 0).map(Reverse).collect();
        let mut done = vec![false; list.len()];
        let mut order = Vec::with_capacity(list.len());
        while order.len() < list.len() {
            // A cycle leaves nothing ready: take the earliest remaining constraint.
            let i = match ready.pop() {
                Some(Reverse(i)) if !done[i] => i,
                Some(_) => continue,
                None => (0..list.len()).find(|&i| !done[i]).expect("constraints remain"),
            };
            done[i] = true;
            order.push(i);
            for &b in &after[i] {
                waits[b] = waits[b].saturating_sub(1);
                if waits[b] == 0 && !done[b] {
                    ready.push(Reverse(b));
                }
            }
        }
        Self {
            parents,
            list: order.into_iter().map(|i| list[i].clone()).collect(),
        }
    }

    /// Applies every constraint in order to `locals`, the animated local pose of every node.
    pub fn evaluate(&self, locals: &mut [Transform]) {
        if self.list.is_empty() {
            return;
        }
        let mut world: Vec<Place> = Vec::with_capacity(locals.len());
        for (i, l) in locals.iter().enumerate() {
            let parent = self.parents[i].map_or(Place::IDENTITY, |p| world[p as usize]);
            world.push(parent.then(l));
        }
        let mut moved = vec![false; locals.len()];
        for c in &self.list {
            let t = c.target as usize;
            let parent = self.parents[t].map_or(Place::IDENTITY, |p| world[p as usize]);
            let Some(new) = solve(c, &world, &parent, &locals[t]) else {
                continue;
            };
            locals[t] = new;
            // Re-place the target's subtree (children come after their parents).
            moved.fill(false);
            moved[t] = true;
            world[t] = parent.then(&new);
            for i in t + 1..locals.len() {
                if let Some(p) = self.parents[i].filter(|&p| moved[p as usize]) {
                    moved[i] = true;
                    world[i] = world[p as usize].then(&locals[i]);
                }
            }
        }
    }
}

/// The nodes a constraint reads in world space, each flagged when it is the constraint's own target.
fn reads(c: &Constraint) -> impl Iterator<Item = (NodeIndex, bool)> + '_ {
    let up = match c.kind {
        ConstraintKind::Aim {
            world_up: WorldUp::ObjectUp(n) | WorldUp::ObjectRotationUp(n, _),
            ..
        } => Some(n),
        ConstraintKind::LookAt { world_up_node, .. } => world_up_node,
        _ => None,
    };
    let sources = c.sources.iter().map(|s| (s.node, false));
    sources.chain(up.map(|n| (n, false))).chain([(c.target, true)])
}

/// A world placement with per-axis scale, composed like Unity's `lossyScale` (no shear).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    pos: Vec3,
    rot: Quat,
    scale: Vec3,
}

impl Place {
    const IDENTITY: Self = Self {
        pos: [0.0; 3],
        rot: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };

    fn then(&self, l: &Transform) -> Self {
        Self {
            pos: add(self.pos, rotate(self.rot, mul3(l.translation, self.scale))),
            rot: mul(self.rot, l.rotation),
            scale: mul3(self.scale, l.scale),
        }
    }

    /// A world point in this placement's local space; axes with zero scale keep `fallback`.
    fn local_point(&self, p: Vec3, fallback: Vec3) -> Vec3 {
        div3(rotate(conj(self.rot), sub(p, self.pos)), self.scale, fallback)
    }
}

/// The constrained local transform, or `None` to leave the node as animated.
fn solve(c: &Constraint, world: &[Place], parent: &Place, cur: &Transform) -> Option<Transform> {
    if !c.active || c.sources.is_empty() {
        return None;
    }
    let total: f32 = c.sources.iter().map(|s| s.weight.max(0.0)).sum();
    let mut out = *cur;
    if total <= 0.0 {
        // No source weight: the at-rest values.
        match c.kind {
            ConstraintKind::Position { .. } => out.translation = pick(c.axes, c.rest.translation, cur.translation),
            ConstraintKind::Scale { .. } => out.scale = pick(c.axes, c.rest.scale, cur.scale),
            ConstraintKind::Parent { rotation_axes } => {
                out.translation = pick(c.axes, c.rest.translation, cur.translation);
                out.rotation = pick_rotation(rotation_axes, c.rest.rotation, cur.rotation);
            }
            _ => out.rotation = pick_rotation(c.axes, c.rest.rotation, cur.rotation),
        }
        return Some(out);
    }
    let w = c.weight.clamp(0.0, 1.0);
    let place = |n: NodeIndex| world[n as usize];
    let weighted = || c.sources.iter().map(|s| (place(s.node), s.weight.max(0.0)));
    let average = |points: &mut dyn Iterator<Item = (Vec3, f32)>| {
        let sum = points.fold([0.0; 3], |acc, (p, sw)| add(acc, scale(p, sw)));
        scale(sum, 1.0 / total)
    };
    let local_rotation = |q: Quat, offset: Vec3| mul(mul(conj(parent.rot), q), euler_to_quat(offset));
    // The rotation a constraint gives, weighted from rest, with unconstrained axes left as animated.
    let rotated = |q: Quat, axes: [bool; 3]| pick_rotation(axes, nlerp(c.rest.rotation, q, w), cur.rotation);
    let me = parent.then(cur).pos;

    match c.kind {
        ConstraintKind::Position { offset } => {
            let p = average(&mut weighted().map(|(s, sw)| (s.pos, sw)));
            let local = add(parent.local_point(p, cur.translation), offset);
            out.translation = pick(c.axes, lerp(c.rest.translation, local, w), cur.translation);
        }
        ConstraintKind::Rotation { offset } => {
            let q = blend(&mut weighted().map(|(s, sw)| (s.rot, sw)), total);
            out.rotation = rotated(local_rotation(q, offset), c.axes);
        }
        ConstraintKind::Scale { offset } => {
            let log = weighted().fold([0.0; 3], |acc, (s, sw)| {
                add(acc, scale(s.scale.map(|x| x.max(f32::MIN_POSITIVE).ln()), sw))
            });
            let world_scale = scale(log, 1.0 / total).map(f32::exp);
            let local = mul3(div3(world_scale, parent.scale, cur.scale), offset);
            out.scale = pick(c.axes, lerp(c.rest.scale, local, w), cur.scale);
        }
        ConstraintKind::Parent { rotation_axes } => {
            let held = || {
                c.sources.iter().map(|s| {
                    let p = place(s.node);
                    let pos = add(p.pos, rotate(p.rot, s.offset.translation));
                    (pos, mul(p.rot, s.offset.rotation), s.weight.max(0.0))
                })
            };
            let p = average(&mut held().map(|(pos, _, sw)| (pos, sw)));
            let q = blend(&mut held().map(|(_, q, sw)| (q, sw)), total);
            let local = parent.local_point(p, cur.translation);
            out.translation = pick(c.axes, lerp(c.rest.translation, local, w), cur.translation);
            out.rotation = rotated(local_rotation(q, [0.0; 3]), rotation_axes);
        }
        ConstraintKind::Aim {
            offset,
            aim_axis,
            up_axis,
            world_up,
        } => {
            let goal = average(&mut weighted().map(|(s, sw)| (s.pos, sw)));
            let up = match world_up {
                WorldUp::SceneUp => Some([0.0, 1.0, 0.0]),
                WorldUp::Vector(v) => Some(v),
                WorldUp::ObjectUp(n) => Some(sub(place(n).pos, me)),
                WorldUp::ObjectRotationUp(n, v) => Some(rotate(place(n).rot, v)),
                WorldUp::None => None,
            };
            let q = aim(aim_axis, up_axis, sub(goal, me), up)?;
            out.rotation = rotated(local_rotation(q, offset), c.axes);
        }
        ConstraintKind::LookAt {
            offset,
            roll,
            world_up_node,
        } => {
            let goal = average(&mut weighted().map(|(s, sw)| (s.pos, sw)));
            let up = world_up_node.map_or([0.0, 1.0, 0.0], |n| sub(place(n).pos, me));
            let q = aim([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], sub(goal, me), Some(up))?;
            let q = mul(q, euler_to_quat([0.0, 0.0, -roll]));
            out.rotation = rotated(local_rotation(q, offset), [true; 3]);
        }
    }
    Some(out)
}

/// Unity's source rotation blend: from the first source with weight, nlerp towards each later one by its share of
/// the total weight.
fn blend(sources: &mut dyn Iterator<Item = (Quat, f32)>, total: f32) -> Quat {
    let mut acc: Option<Quat> = None;
    for (q, w) in sources.filter(|&(_, w)| w > 0.0) {
        acc = Some(acc.map_or(q, |a| nlerp(a, q, w / total)));
    }
    acc.unwrap_or([0.0, 0.0, 0.0, 1.0])
}

/// The world rotation turning local `axis` onto `dir` with local `up_axis` as close to `up` as possible; without
/// `up`, the shortest turn from the identity. `None` when `dir` is zero.
fn aim(axis: Vec3, up_axis: Vec3, dir: Vec3, up: Option<Vec3>) -> Option<Quat> {
    let dir = unit(dir)?;
    let axis = unit(axis)?;
    let framed = up.and_then(|u| Some(mul(frame(dir, u)?, conj(frame(axis, up_axis)?))));
    Some(framed.unwrap_or_else(|| from_to(axis, dir)))
}

/// The rotation whose X, Y and Z columns are `f`, `u` made perpendicular to `f`, and `f × u`; `None` when they are
/// parallel.
fn frame(f: Vec3, u: Vec3) -> Option<Quat> {
    let u = unit(sub(u, scale(f, dot(u, f))))?;
    let z = cross(f, u);
    let (m00, m10, m20) = (f[0], f[1], f[2]);
    let (m01, m11, m21) = (u[0], u[1], u[2]);
    let (m02, m12, m22) = (z[0], z[1], z[2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    Some(normalize(q))
}

/// The shortest rotation taking unit `a` to unit `b`.
fn from_to(a: Vec3, b: Vec3) -> Quat {
    let d = dot(a, b);
    if d < -0.999_999 {
        let side = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let axis = unit(cross(a, side)).unwrap_or([0.0, 0.0, 1.0]);
        return [axis[0], axis[1], axis[2], 0.0];
    }
    let c = cross(a, b);
    normalize([c[0], c[1], c[2], 1.0 + d])
}

/// Normalised lerp on the shorter arc.
fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let b = if a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>() < 0.0 {
        b.map(|x| -x)
    } else {
        b
    };
    normalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t))
}

/// `v` on constrained axes, `cur` on the others.
fn pick(axes: [bool; 3], v: Vec3, cur: Vec3) -> Vec3 {
    std::array::from_fn(|k| if axes[k] { v[k] } else { cur[k] })
}

/// `q`'s Euler angles on constrained axes, `cur`'s on the others.
fn pick_rotation(axes: [bool; 3], q: Quat, cur: Quat) -> Quat {
    if axes == [true; 3] {
        return q;
    }
    euler_to_quat(pick(axes, quat_to_euler(q), quat_to_euler(cur)))
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|k| a[k] + b[k])
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|k| a[k] - b[k])
}

fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|x| x * s)
}

fn mul3(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|k| a[k] * b[k])
}

/// `a / b` per axis; zero divisors keep `fallback`.
fn div3(a: Vec3, b: Vec3, fallback: Vec3) -> Vec3 {
    std::array::from_fn(|k| if b[k] != 0.0 { a[k] / b[k] } else { fallback[k] })
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: Vec3) -> Option<Vec3> {
    let l = dot(a, a).sqrt();
    (l > 1e-6 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = [q[0], q[1], q[2]];
    let t = scale(cross(u, v), 2.0);
    add(add(v, scale(t, q[3])), cross(u, t))
}
