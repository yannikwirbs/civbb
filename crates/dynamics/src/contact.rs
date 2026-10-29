//! Contact senders and receivers: compiled volumes, a grid broadphase across avatars and receiver values (ADR 0017).

use std::collections::HashMap;

use civbb_format::Avatar;
use civbb_format::dynamics::{ContactVolume, ReceiverKind, Shape};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Vec3};

use crate::math::{Pose, add, dot, length, mul, rotate, scale, sub};

/// Largest radius after scaling, metres.
pub const MAX_RADIUS: f32 = 3.0;
/// Largest capsule height after scaling, metres.
pub const MAX_HEIGHT: f32 = 6.0;
/// Tags per contact that count.
pub const MAX_TAGS: usize = 16;
/// Broadphase cell edge, metres.
pub const CELL: f32 = 1.0;

/// A compiled sender or receiver volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// Node it follows; the contact exists only while the node is active.
    pub node: NodeIndex,
    /// Offset from the node.
    pub position: Vec3,
    /// Rotation relative to the node.
    pub rotation: Quat,
    /// Radius before scaling.
    pub radius: f32,
    /// Capsule height including the caps before scaling; 0 for spheres.
    pub height: f32,
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
    /// Animator parameter the value goes to.
    pub parameter: String,
    /// How the value is computed.
    pub kind: ReceiverKind,
    /// Senders on the same avatar count.
    pub allow_self: bool,
    /// Senders on other avatars count.
    pub allow_others: bool,
    /// Minimum relative speed for `OnEnter`, metres per second.
    pub min_velocity: f32,
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
                    parameter: r.parameter.clone(),
                    kind: r.kind,
                    allow_self: r.allow_self,
                    allow_others: r.allow_others,
                    min_velocity: r.min_velocity,
                })
                .collect(),
        }
    }
}

fn volume(v: &ContactVolume, tags: &[String], local_only: bool) -> Volume {
    let (radius, height) = match v.shape {
        Shape::Sphere { radius } => (radius, 0.0),
        Shape::Capsule { radius, height } => (radius, height),
        // Validation rejects planes; a point never touches anything.
        Shape::Plane => (0.0, 0.0),
    };
    let mut tags: Vec<String> = tags.iter().take(MAX_TAGS).cloned().collect();
    tags.sort_unstable();
    tags.dedup();
    Volume {
        node: v.node,
        position: v.position,
        rotation: v.rotation,
        radius: radius.max(0.0),
        height: height.max(0.0),
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

/// A volume placed in the world: a core segment from `a` to `b` with a radius.
#[derive(Clone, Copy, Debug)]
struct Placed {
    a: Vec3,
    b: Vec3,
    radius: f32,
    velocity: Vec3,
}

impl Placed {
    fn cells(&self) -> impl Iterator<Item = [i32; 3]> {
        let lo: [i32; 3] = std::array::from_fn(|k| ((self.a[k].min(self.b[k]) - self.radius) / CELL).floor() as i32);
        let hi: [i32; 3] = std::array::from_fn(|k| ((self.a[k].max(self.b[k]) + self.radius) / CELL).floor() as i32);
        (lo[0]..=hi[0]).flat_map(move |x| (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| [x, y, z])))
    }
}

/// Places `v` on its node, or `None` when the contact does not exist on this client this frame.
fn place(v: &Volume, p: &Participant, last: Vec3, dt: f32) -> Option<(Placed, Vec3)> {
    if !p.active[v.node as usize] || (v.local_only && !p.local) {
        return None;
    }
    let w = p.world[v.node as usize];
    let centre = w.point(v.position);
    let radius = (v.radius * w.scale).min(MAX_RADIUS);
    let half = ((v.height * w.scale).min(MAX_HEIGHT) * 0.5 - radius).max(0.0);
    let up = rotate(mul(w.rot, v.rotation), [0.0, 1.0, 0.0]);
    let velocity = if p.state.started && dt > 0.0 {
        scale(sub(centre, last), 1.0 / dt)
    } else {
        [0.0; 3]
    };
    let placed = Placed {
        a: add(centre, scale(up, -half)),
        b: add(centre, scale(up, half)),
        radius,
        velocity,
    };
    Some((placed, centre))
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
                let d = segment_distance(rp.a, rp.b, sp.a, sp.b);
                if d > rp.radius + sp.radius {
                    continue;
                }
                let id = (ids[s_slot], s_index);
                now.push(id);
                value = value.max(match r.kind {
                    ReceiverKind::Constant => 1.0,
                    ReceiverKind::OnEnter => {
                        let fast = length(sub(sp.velocity, rp.velocity)) >= r.min_velocity;
                        let entered = !p.state.touching[i].contains(&id);
                        f32::from(u8::from(fast && entered))
                    }
                    ReceiverKind::Proximity if rp.radius > 0.0 => {
                        (1.0 - (d - sp.radius).max(0.0) / rp.radius).clamp(0.0, 1.0)
                    }
                    ReceiverKind::Proximity => 1.0,
                });
            }
            p.state.touching[i] = now;
            p.state.values[i] = value;
        }
        p.state.started = true;
    }
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
