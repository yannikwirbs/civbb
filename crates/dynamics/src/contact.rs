//! Contact senders and receivers: compiled volumes, a grid broadphase across avatars and receiver values (ADR 0017,
//! 0047).

use std::collections::HashMap;

use civbb_format::Avatar;
use civbb_format::dynamics::{ContactShape, ContactVolume, ReceiverKind};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Vec3};
use civbb_format::state::EntryIndex;

use civbb_core::math::{Pose, add, conj, cross, dot, length, mul, rotate, scale, sub};

/// Largest radius after scaling, metres.
pub const MAX_RADIUS: f32 = 3.0;
/// Largest capsule height and box edge after scaling, metres.
pub const MAX_HEIGHT: f32 = 6.0;
/// Tags per contact that count.
pub const MAX_TAGS: usize = 16;
/// Broadphase cell edge, metres.
pub const CELL: f32 = 1.0;

/// A compiled sender or receiver volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// Node it is placed on.
    pub node: NodeIndex,
    /// Node whose activity enables it (the component's node; `node` unless the format says otherwise).
    pub component: NodeIndex,
    /// Offset from the node.
    pub position: Vec3,
    /// Rotation relative to the node.
    pub rotation: Quat,
    /// Shape before scaling, sizes at least 0.
    pub shape: ContactShape,
    /// The first [`MAX_TAGS`] tags, sorted and deduplicated.
    pub tags: Vec<String>,
    /// Exists only on the avatar owner's client.
    pub local_only: bool,
}

/// A compiled receiver.
#[derive(Clone, Debug, PartialEq)]
pub struct Receiver {
    /// Volume.
    pub volume: Volume,
    /// State entry the value goes to; `None` writes nothing.
    pub parameter: Option<EntryIndex>,
    /// How the value is computed.
    pub kind: ReceiverKind,
    /// Senders on the same avatar count.
    pub allow_self: bool,
    /// Senders on other avatars count.
    pub allow_others: bool,
    /// Minimum relative speed for `OnEnter`, metres per second.
    pub min_velocity: f32,
    /// Box receivers: proximity towards the +Z face.
    pub face: bool,
}

/// An avatar's contacts, compiled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContactRig {
    /// Senders, in format order.
    pub senders: Vec<Volume>,
    /// Receivers, in format order.
    pub receivers: Vec<Receiver>,
}

impl ContactRig {
    /// Compiles a validated avatar's contacts.
    pub fn new(avatar: &Avatar) -> Self {
        let c = &avatar.contacts;
        Self {
            senders: c
                .senders
                .iter()
                .map(|s| volume(&s.volume, &s.tags, s.local_only))
                .collect(),
            receivers: c
                .receivers
                .iter()
                .map(|r| Receiver {
                    volume: volume(&r.volume, &r.tags, r.local_only),
                    parameter: r.parameter,
                    kind: r.kind,
                    allow_self: r.allow_self,
                    allow_others: r.allow_others,
                    min_velocity: r.min_velocity,
                    face: r.face,
                })
                .collect(),
        }
    }
}

fn volume(v: &ContactVolume, tags: &[String], local_only: bool) -> Volume {
    let shape = match v.shape {
        ContactShape::Sphere { radius } => ContactShape::Sphere {
            radius: radius.max(0.0),
        },
        ContactShape::Capsule { radius, height } => ContactShape::Capsule {
            radius: radius.max(0.0),
            height: height.max(0.0),
        },
        ContactShape::Box { size } => ContactShape::Box {
            size: size.map(|s| s.max(0.0)),
        },
    };
    let mut tags: Vec<String> = tags.iter().take(MAX_TAGS).cloned().collect();
    tags.sort_unstable();
    tags.dedup();
    Volume {
        node: v.node,
        component: v.component.unwrap_or(v.node),
        position: v.position,
        rotation: v.rotation,
        shape,
        tags,
        local_only,
    }
}

/// Identifies a sender across frames: the avatar's id and the sender's index.
type SenderId = (u64, u32);

/// One avatar's contact state: last positions, who touches each receiver, and the receivers' values.
#[derive(Clone, Debug, Default)]
pub struct ContactState {
    senders: Vec<Vec3>,
    receivers: Vec<Vec3>,
    touching: Vec<Vec<SenderId>>,
    values: Vec<f32>,
    started: bool,
}

impl ContactState {
    /// A state for `rig`, with every receiver at 0.
    pub fn new(rig: &ContactRig) -> Self {
        Self {
            senders: vec![[0.0; 3]; rig.senders.len()],
            receivers: vec![[0.0; 3]; rig.receivers.len()],
            touching: vec![Vec::new(); rig.receivers.len()],
            values: vec![0.0; rig.receivers.len()],
            started: false,
        }
    }

    /// Each receiver's value from the last [`step`], in [`ContactRig::receivers`] order; the client writes them into
    /// the animator parameters before the next frame's animator step.
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// One avatar taking part in a frame's contact pass.
#[derive(Debug)]
pub struct Participant<'a> {
    /// Stable avatar id, the same every frame.
    pub id: u64,
    /// The avatar's compiled contacts.
    pub rig: &'a ContactRig,
    /// Its state, made by [`ContactState::new`] for `rig`.
    pub state: &'a mut ContactState,
    /// Every node's world pose after dynamics.
    pub world: &'a [Pose],
    /// Whether each node is active, its ancestors included.
    pub active: &'a [bool],
    /// This client owns the avatar, so its local-only contacts exist.
    pub local: bool,
}

/// A volume's shape placed in the world.
#[derive(Clone, Copy, Debug)]
enum Body {
    /// A core segment from `a` to `b` with a radius: spheres and capsules.
    Round { a: Vec3, b: Vec3, radius: f32 },
    /// A box: centre, rotation and half edges.
    Box { centre: Vec3, rot: Quat, half: Vec3 },
}

/// A volume placed in the world, with its centre's velocity.
#[derive(Clone, Copy, Debug)]
struct Placed {
    body: Body,
    velocity: Vec3,
}

impl Placed {
    fn cells(&self) -> impl Iterator<Item = [i32; 3]> {
        let (lo, hi): (Vec3, Vec3) = match self.body {
            Body::Round { a, b, radius } => (
                std::array::from_fn(|k| a[k].min(b[k]) - radius),
                std::array::from_fn(|k| a[k].max(b[k]) + radius),
            ),
            Body::Box { centre, rot, half } => {
                let axes = AXES.map(|e| rotate(rot, e));
                let reach: Vec3 = std::array::from_fn(|k| (0..3).map(|j| axes[j][k].abs() * half[j]).sum());
                (sub(centre, reach), add(centre, reach))
            }
        };
        let lo: [i32; 3] = lo.map(|x| (x / CELL).floor() as i32);
        let hi: [i32; 3] = hi.map(|x| (x / CELL).floor() as i32);
        (lo[0]..=hi[0]).flat_map(move |x| (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| [x, y, z])))
    }
}

const AXES: [Vec3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Places `v` on its node, or `None` when the contact does not exist on this client this frame.
fn place(v: &Volume, p: &Participant, last: Vec3, dt: f32) -> Option<(Placed, Vec3)> {
    if !p.active[v.component as usize] || (v.local_only && !p.local) {
        return None;
    }
    let w = p.world[v.node as usize];
    let centre = w.point(v.position);
    let rot = mul(w.rot, v.rotation);
    let round = |radius: f32, height: f32| {
        let radius = (radius * w.scale).min(MAX_RADIUS);
        let half = ((height * w.scale).min(MAX_HEIGHT) * 0.5 - radius).max(0.0);
        let up = rotate(rot, [0.0, 1.0, 0.0]);
        Body::Round {
            a: add(centre, scale(up, -half)),
            b: add(centre, scale(up, half)),
            radius,
        }
    };
    let body = match v.shape {
        ContactShape::Sphere { radius } => round(radius, 0.0),
        ContactShape::Capsule { radius, height } => round(radius, height),
        ContactShape::Box { size } => Body::Box {
            centre,
            rot,
            half: size.map(|s| (s * w.scale).min(MAX_HEIGHT) * 0.5),
        },
    };
    let velocity = if p.state.started && dt > 0.0 {
        scale(sub(centre, last), 1.0 / dt)
    } else {
        [0.0; 3]
    };
    Some((Placed { body, velocity }, centre))
}

/// Runs one frame's contact pass over every avatar on this client: places all contacts, finds sender and receiver
/// pairs through the grid and updates each receiver's value. `permits(receiver_avatar, sender_avatar)` is the
/// receiving owner's setting for whose contacts may affect their avatar; it is not asked about an avatar's own
/// senders. `dt` is the frame time in seconds.
pub fn step(avatars: &mut [Participant], permits: impl Fn(u64, u64) -> bool, dt: f32) {
    // Senders: (avatar slot, sender index, placement), and the grid over them.
    let mut senders: Vec<(usize, u32, Placed)> = Vec::new();
    let mut receivers: Vec<Vec<Option<Placed>>> = Vec::with_capacity(avatars.len());
    for (slot, p) in avatars.iter_mut().enumerate() {
        let mut placed = Vec::with_capacity(p.rig.receivers.len());
        for (i, s) in p.rig.senders.iter().enumerate() {
            if let Some((s, centre)) = place(s, p, p.state.senders[i], dt) {
                senders.push((slot, i as u32, s));
                p.state.senders[i] = centre;
            }
        }
        for (i, r) in p.rig.receivers.iter().enumerate() {
            let r = place(&r.volume, p, p.state.receivers[i], dt);
            if let Some((_, centre)) = r {
                p.state.receivers[i] = centre;
            }
            placed.push(r.map(|(r, _)| r));
        }
        receivers.push(placed);
    }
    let mut grid: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
    for (k, (_, _, s)) in senders.iter().enumerate() {
        for cell in s.cells() {
            grid.entry(cell).or_default().push(k as u32);
        }
    }

    let ids: Vec<u64> = avatars.iter().map(|p| p.id).collect();
    let rigs: Vec<&ContactRig> = avatars.iter().map(|p| p.rig).collect();
    let mut candidates: Vec<u32> = Vec::new();
    for (slot, p) in avatars.iter_mut().enumerate() {
        for (i, r) in p.rig.receivers.iter().enumerate() {
            let Some(rp) = receivers[slot][i] else {
                p.state.touching[i].clear();
                p.state.values[i] = 0.0;
                continue;
            };
            candidates.clear();
            for cell in rp.cells() {
                candidates.extend(grid.get(&cell).into_iter().flatten());
            }
            candidates.sort_unstable();
            candidates.dedup();

            let mut now: Vec<SenderId> = Vec::new();
            let mut value: f32 = 0.0;
            for &k in &candidates {
                let (s_slot, s_index, sp) = senders[k as usize];
                let allowed = if s_slot == slot {
                    r.allow_self
                } else {
                    r.allow_others && permits(ids[slot], ids[s_slot])
                };
                if !allowed || !shares_tag(&r.volume.tags, &rigs[s_slot].senders[s_index as usize].tags) {
                    continue;
                }
                if gap(&rp.body, &sp.body) > 0.0 {
                    continue;
                }
                let id = (ids[s_slot], s_index);
                now.push(id);
                let entered = !p.state.touching[i].contains(&id);
                value = value.max(match r.kind {
                    ReceiverKind::Constant => 1.0,
                    ReceiverKind::OnEnter => {
                        let fast = length(sub(sp.velocity, rp.velocity)) >= r.min_velocity;
                        f32::from(u8::from(fast && entered))
                    }
                    // A sender counts from its second frame of contact, as in the VRChat SDK (ADR 0047).
                    ReceiverKind::Proximity if entered => 0.0,
                    ReceiverKind::Proximity => proximity(&rp.body, &sp.body, r.face),
                });
            }
            p.state.touching[i] = now;
            p.state.values[i] = value;
        }
        p.state.started = true;
    }
}

/// The distance between two bodies' surfaces, 0 or less when they touch (box pairs: 0 or infinity).
fn gap(r: &Body, s: &Body) -> f32 {
    match (*r, *s) {
        (Body::Round { a, b, radius }, Body::Round { a: c, b: d, radius: rs }) => {
            segment_distance(a, b, c, d) - radius - rs
        }
        (Body::Round { a, b, radius }, Body::Box { centre, rot, half })
        | (Body::Box { centre, rot, half }, Body::Round { a, b, radius }) => {
            let local = |p: Vec3| rotate(conj(rot), sub(p, centre));
            segment_box(local(a), local(b), half) - radius
        }
        (
            Body::Box { centre, rot, half },
            Body::Box {
                centre: c,
                rot: q,
                half: h,
            },
        ) => {
            if boxes_overlap((centre, rot, half), (c, q, h)) {
                0.0
            } else {
                f32::INFINITY
            }
        }
    }
}

/// A receiver's proximity to a touching sender, 0 to 1 (ADR 0017, 0047). Spheres and capsules: 1 minus the distance
/// from the receiver's core to the sender's surface over its radius. Boxes: 1 minus the largest of the sender's
/// closest point to the centre over the half edges, per axis; with `face`, 1 minus the depth of the sender's highest
/// point within the box's X and Y extent below the +Z face, over the box's depth.
fn proximity(r: &Body, s: &Body, face: bool) -> f32 {
    let v = match *r {
        Body::Round { a, b, radius } if radius > 0.0 => {
            1.0 - gap(&Body::Round { a, b, radius: 0.0 }, s).max(0.0) / radius
        }
        Body::Round { .. } => 1.0,
        Body::Box { centre, rot, half } if face => {
            let local = |p: Vec3| rotate(conj(rot), sub(p, centre));
            let top = match *s {
                Body::Round { a, b, radius } => highest(local(a), local(b), radius, half),
                // Box senders: the highest corner, the box's X and Y extent not applied (unverified).
                Body::Box {
                    centre: c,
                    rot: q,
                    half: h,
                } => corners(c, q, h)
                    .map(|p| local(p)[2])
                    .into_iter()
                    .fold(f32::MIN, f32::max),
            };
            if half[2] > 0.0 {
                1.0 - (half[2] - top) / (2.0 * half[2])
            } else {
                1.0
            }
        }
        Body::Box { centre, rot, half } => {
            let q = rotate(conj(rot), sub(closest(s, centre), centre));
            let far = (0..3).map(|k| if half[k] > 0.0 { q[k].abs() / half[k] } else { 0.0 });
            1.0 - far.fold(0.0, f32::max)
        }
    };
    v.clamp(0.0, 1.0)
}

/// The point of `s` closest to `p` (`p` itself when inside).
fn closest(s: &Body, p: Vec3) -> Vec3 {
    match *s {
        Body::Round { a, b, radius } => {
            let ab = sub(b, a);
            let len2 = dot(ab, ab);
            let t = if len2 > 0.0 {
                (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let core = add(a, scale(ab, t));
            let off = sub(p, core);
            let d = length(off);
            if d <= radius {
                p
            } else {
                add(core, scale(off, radius / d))
            }
        }
        Body::Box { centre, rot, half } => {
            let l = rotate(conj(rot), sub(p, centre));
            add(
                centre,
                rotate(rot, std::array::from_fn(|k| l[k].clamp(-half[k], half[k]))),
            )
        }
    }
}

/// Distance from segment `a b` to the box of half edges `half` at the origin, both in the box's frame.
fn segment_box(a: Vec3, b: Vec3, half: Vec3) -> f32 {
    let outside = |t: f32| {
        let p = add(a, scale(sub(b, a), t));
        length(std::array::from_fn(|k| p[k] - p[k].clamp(-half[k], half[k])))
    };
    -maximise(|t| -outside(t))
}

/// The highest Z of a capsule (core `a b`, `radius`) within the X and Y extent of the box of half edges `half`, in
/// the box's frame.
fn highest(a: Vec3, b: Vec3, radius: f32, half: Vec3) -> f32 {
    let top = |t: f32| {
        let p = add(a, scale(sub(b, a), t));
        let side = length([
            p[0] - p[0].clamp(-half[0], half[0]),
            p[1] - p[1].clamp(-half[1], half[1]),
            0.0,
        ]);
        // Concave in `t`; beyond the radius it falls steeply, so the search stays where the capsule reaches.
        p[2] + (radius * radius - side * side).max(0.0).sqrt() - (side - radius).max(0.0) * 1e4
    };
    maximise(top)
}

/// The largest value of `f` on [0, 1] for a concave `f` (golden-section search).
fn maximise(f: impl Fn(f32) -> f32) -> f32 {
    const G: f32 = 0.618_034;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let (m1, m2) = (hi - G * (hi - lo), lo + G * (hi - lo));
        if f(m1) < f(m2) {
            lo = m1;
        } else {
            hi = m2;
        }
    }
    f(0.5 * (lo + hi)).max(f(0.0)).max(f(1.0))
}

/// A box's eight corners.
fn corners(centre: Vec3, rot: Quat, half: Vec3) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        let l: Vec3 = std::array::from_fn(|k| if (i >> k) & 1 == 1 { half[k] } else { -half[k] });
        add(centre, rotate(rot, l))
    })
}

/// Whether two boxes (centre, rotation, half edges) overlap, by separating axes; touching counts.
fn boxes_overlap((c1, r1, h1): (Vec3, Quat, Vec3), (c2, r2, h2): (Vec3, Quat, Vec3)) -> bool {
    let a = AXES.map(|e| rotate(r1, e));
    let b = AXES.map(|e| rotate(r2, e));
    let t = sub(c2, c1);
    let crossed = a.iter().flat_map(|x| b.iter().map(move |y| cross(*x, *y)));
    let mut axes = a.into_iter().chain(b).chain(crossed);
    axes.all(|n| {
        let ra: f32 = (0..3).map(|k| h1[k] * dot(a[k], n).abs()).sum();
        let rb: f32 = (0..3).map(|k| h2[k] * dot(b[k], n).abs()).sum();
        dot(n, n) <= 1e-12 || dot(t, n).abs() <= ra + rb
    })
}

/// Whether two sorted tag lists have a tag in common.
fn shares_tag(a: &[String], b: &[String]) -> bool {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

/// Shortest distance between segments `p1 q1` and `p2 q2` (Ericson, Real-Time Collision Detection, 5.1.9).
pub fn segment_distance(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let eps = 1e-12;
    let (s, t) = if a <= eps && e <= eps {
        (0.0, 0.0)
    } else if a <= eps {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= eps {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s = if denom > eps {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
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
            (s, t)
        }
    };
    length(sub(add(p1, scale(d1, s)), add(p2, scale(d2, t))))
}
