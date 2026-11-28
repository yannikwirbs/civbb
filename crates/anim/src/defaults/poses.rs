//! Generated built-in clips (ADR 0108): key poses and a procedural gait in muscle space, each key's body height
//! solved on the avatar's skeleton so the lowest bone stays on the floor.

use std::f32::consts::TAU;

use civbb_core::math::{Pose, world_poses};
use civbb_format::Document;
use civbb_format::animator::{Clip, Curve, HumanoidProperty, Key, Target};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Transform};

use crate::body::BodyRig;
use crate::muscle::{COUNT, MUSCLES, Rig, find, property};

/// A clip the generator makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Standing still, breathing.
    Idle,
    /// A gait cycle: `forward` and `right` the direction of travel (unit length or zero).
    Gait {
        /// How it moves.
        style: Style,
        /// Forward share of the direction, -1 to 1.
        forward: f32,
        /// Rightward share of the direction, 0 to 1 (mirror for left).
        right: f32,
    },
    /// In the air.
    Fall,
    /// Absorbing a landing; `deep` for a long fall.
    Land {
        /// A long fall's deeper squat.
        deep: bool,
    },
    /// Away from the keyboard: slumped, head down.
    Afk,
    /// Seated on a chair.
    Sit,
    /// Sitting on the floor, legs out in front, leaning back a little (ADR 0119).
    FloorSit,
    /// Lying on the back (ADR 0119).
    Lie,
    /// A hand gesture on both hands.
    Hands(Hand),
}

/// The gaits: posture, cycle time and swing sizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    /// Upright walking.
    Walk,
    /// Upright running.
    Run,
    /// Upright sprinting.
    Sprint,
    /// Crouched walking.
    Crouch,
    /// Lying prone, crawling.
    Crawl,
}

/// Hand gestures, in `GestureLeft`/`GestureRight` order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hand {
    /// Relaxed.
    Idle,
    /// Fist.
    Fist,
    /// Open, fingers spread.
    Open,
    /// Index finger pointing.
    Point,
    /// Index and middle fingers up.
    Peace,
    /// Index and little fingers up.
    Rock,
    /// Index finger and thumb out.
    Gun,
    /// Thumb up.
    ThumbsUp,
}

/// One key pose: muscle values and the body's forward pitch in radians.
#[derive(Clone, Copy)]
struct P {
    m: [f32; COUNT],
    pitch: f32,
}

impl P {
    fn set(&mut self, name: &str, v: f32) {
        self.m[muscle(name)] = v;
    }

    /// Both sides' muscle `name` (without its `Left `/`Right ` prefix).
    fn both(&mut self, name: &str, v: f32) {
        self.set(&format!("Left {name}"), v);
        self.set(&format!("Right {name}"), v);
    }
}

fn muscle(name: &str) -> usize {
    find(name).unwrap_or_else(|| panic!("no muscle {name}")) as usize
}

/// Upper leg front-back for a thigh swung `deg` forward of standing: standing is 0.6 (30 degrees on the muscle's
/// 50-degree positive side), forward runs into the 90-degree negative side.
fn thigh(deg: f32) -> f32 {
    let a = 30.0 - deg;
    if a >= 0.0 { a / 50.0 } else { a / 90.0 }
}

/// Lower leg or forearm stretch for a joint bent by `deg`: 1 is straight, each unit 80 degrees.
fn bend(deg: f32) -> f32 {
    1.0 - deg / 80.0
}

/// Muscles the body clips write: everything but eyes, jaw and fingers.
fn body_muscles() -> impl Iterator<Item = usize> {
    (0..COUNT).filter(|&i| {
        let n = MUSCLES[i].name;
        !(n.contains("Eye") || n.starts_with("Jaw") || n.contains("Hand."))
    })
}

/// Muscles the hand clips write: the fingers.
fn finger_muscles() -> impl Iterator<Item = usize> {
    (0..COUNT).filter(|&i| MUSCLES[i].name.contains("Hand."))
}

/// Standing with straight legs and the arms down. Signs on the rig (`tests/defaults.rs`): front-back of the spine,
/// chest, neck, head and arms is positive backwards, nods positive up, upper leg in-out positive outwards.
fn stand() -> P {
    let mut p = P {
        m: [0.0; COUNT],
        pitch: 0.0,
    };
    p.both("Upper Leg Front-Back", thigh(0.0));
    p.both("Lower Leg Stretch", bend(0.0));
    p.both("Arm Down-Up", -0.5);
    p.both("Arm Front-Back", 0.2);
    p.both("Forearm Stretch", bend(12.0));
    p
}

impl Style {
    /// Cycle seconds, thigh swing and knee lift in degrees, arm swing in muscle units, body pitch in degrees.
    fn sizes(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Style::Walk => (0.9, 22.0, 40.0, 0.2, 2.0),
            Style::Run => (0.62, 35.0, 80.0, 0.45, 8.0),
            Style::Sprint => (0.54, 42.0, 95.0, 0.55, 12.0),
            Style::Crouch => (1.0, 15.0, 25.0, 0.1, 18.0),
            Style::Crawl => (1.4, 10.0, 30.0, 0.35, 90.0),
        }
    }

    /// The posture the gait swings about.
    fn base(self) -> P {
        let mut p = stand();
        match self {
            Style::Walk => {}
            Style::Run | Style::Sprint => p.both("Forearm Stretch", bend(85.0)),
            Style::Crouch => {
                p.both("Upper Leg Front-Back", thigh(85.0));
                p.both("Lower Leg Stretch", bend(110.0));
                p.both("Arm Front-Back", -0.2);
                p.both("Forearm Stretch", bend(45.0));
            }
            Style::Crawl => {
                p.both("Arm Down-Up", 0.2);
                p.both("Arm Front-Back", -0.7);
                p.both("Forearm Stretch", bend(95.0));
                p.set("Head Nod Down-Up", 0.6);
            }
        }
        p
    }
}

/// The pose of `kind` at normalised time `t` (0 to 1).
fn pose(kind: Kind, t: f32) -> P {
    let s = (TAU * t).sin();
    match kind {
        Kind::Idle => {
            let mut p = stand();
            p.set("Chest Front-Back", 0.03 * s);
            p.both("Arm Down-Up", -0.5 + 0.015 * s);
            p
        }
        Kind::Gait { style, forward, right } => gait(style, forward, right, t),
        Kind::Fall => {
            let mut p = stand();
            p.set("Left Upper Leg Front-Back", thigh(30.0));
            p.set("Right Upper Leg Front-Back", thigh(10.0));
            p.both("Lower Leg Stretch", bend(35.0));
            p.both("Arm Down-Up", 0.1 + 0.03 * s);
            p.both("Forearm Stretch", bend(30.0));
            p
        }
        Kind::Land { deep } => {
            // Down into the squat and back up.
            let d = (std::f32::consts::PI * t).sin() * if deep { 1.0 } else { 0.5 };
            let mut p = stand();
            p.both("Upper Leg Front-Back", thigh(70.0 * d));
            p.both("Lower Leg Stretch", bend(100.0 * d));
            p.pitch = (20.0 * d).to_radians();
            p
        }
        Kind::Afk => {
            let mut p = stand();
            p.set("Head Nod Down-Up", -0.7);
            p.set("Neck Nod Down-Up", -0.3);
            p.set("Spine Front-Back", -0.3);
            p.set("Chest Front-Back", -0.3);
            p.both("Arm Down-Up", -0.65);
            p.both("Arm Front-Back", 0.1);
            p.both("Lower Leg Stretch", bend(10.0));
            p.pitch = 6f32.to_radians();
            p
        }
        Kind::Sit => {
            let mut p = stand();
            p.both("Upper Leg Front-Back", thigh(90.0));
            p.both("Lower Leg Stretch", bend(90.0));
            p.both("Arm Front-Back", -0.1);
            p.both("Forearm Stretch", bend(60.0));
            p
        }
        Kind::FloorSit => {
            let mut p = stand();
            p.both("Upper Leg Front-Back", thigh(90.0));
            p.both("Lower Leg Stretch", bend(5.0));
            p.set("Spine Front-Back", 0.15);
            p.both("Arm Front-Back", -0.2);
            p.both("Forearm Stretch", bend(30.0));
            p
        }
        Kind::Lie => {
            let mut p = stand();
            p.both("Lower Leg Stretch", bend(10.0));
            p.set("Head Nod Down-Up", -0.3);
            p.pitch = (-90f32).to_radians();
            p
        }
        Kind::Hands(h) => hands(h),
    }
}

/// One gait pose at normalised time `t`: the left leg swings forward over the first half of the cycle.
fn gait(style: Style, forward: f32, right: f32, t: f32) -> P {
    let (_, swing, lift, arm, pitch) = style.sizes();
    let mut p = style.base();
    let (base_thigh, base_knee) = if style == Style::Crouch {
        (85.0, 110.0)
    } else {
        (0.0, 0.0)
    };
    let moving = forward != 0.0 || right != 0.0;
    p.pitch = (pitch
        * if style == Style::Crawl || !moving {
            1.0
        } else {
            forward.max(0.0)
        })
    .to_radians();
    if style == Style::Crouch && !moving {
        p.pitch = 10f32.to_radians();
    }
    if !moving {
        return p;
    }
    for (side, phase) in [("Left", 0.0), ("Right", 0.5)] {
        let a = TAU * (t + phase);
        // Thigh forward over the swing; the knee lifts while the foot travels forward (cos > 0 going forward).
        let fwd = swing * forward * a.sin();
        let travel = a.cos() * forward.signum();
        let knee = base_knee + lift * travel.max(0.0).powi(2) * forward.abs().max(right) + 5.0;
        p.set(&format!("{side} Upper Leg Front-Back"), thigh(base_thigh + fwd));
        p.set(&format!("{side} Lower Leg Stretch"), bend(knee));
        // Sideways: the legs step out in turn, the right leg further.
        let out = if side == "Right" { 0.35 } else { 0.2 };
        p.set(&format!("{side} Upper Leg In-Out"), out * right * a.sin().max(0.0));
        // Arms swing against the legs (front-back is positive backwards).
        let other = if side == "Left" { "Right" } else { "Left" };
        let base_arm = p.m[muscle(&format!("{other} Arm Front-Back"))];
        p.set(&format!("{other} Arm Front-Back"), base_arm - arm * forward * a.sin());
    }
    p
}

fn hands(h: Hand) -> P {
    let mut p = P {
        m: [0.0; COUNT],
        pitch: 0.0,
    };
    // Stretch per finger (thumb, index, middle, ring, little) and the thumb's spread.
    let (fingers, thumb_spread) = match h {
        Hand::Idle => ([0.2, 0.3, 0.2, 0.1, 0.0], 0.0),
        Hand::Fist => ([-0.8, -1.0, -1.0, -1.0, -1.0], 0.3),
        Hand::Open => ([1.0, 1.0, 1.0, 1.0, 1.0], 0.5),
        Hand::Point => ([-0.8, 1.0, -1.0, -1.0, -1.0], 0.3),
        Hand::Peace => ([-0.8, 1.0, 1.0, -1.0, -1.0], 0.3),
        Hand::Rock => ([-0.8, 1.0, -1.0, -1.0, 1.0], 0.3),
        Hand::Gun => ([1.0, 1.0, -1.0, -1.0, -1.0], 0.6),
        Hand::ThumbsUp => ([1.0, -1.0, -1.0, -1.0, -1.0], 0.7),
    };
    for side in ["LeftHand", "RightHand"] {
        for (f, v) in ["Thumb", "Index", "Middle", "Ring", "Little"].into_iter().zip(fingers) {
            for j in 1..=3 {
                p.set(&format!("{side}.{f}.{j} Stretched"), v);
            }
        }
        p.set(&format!("{side}.Thumb.Spread"), thumb_spread);
        let spread = if h == Hand::Open { 0.5 } else { 0.0 };
        for f in ["Index", "Middle", "Ring", "Little"] {
            p.set(&format!("{side}.{f}.Spread"), spread);
        }
    }
    p
}

impl Kind {
    /// Seconds per cycle or for the whole clip, and whether it loops.
    fn timing(self) -> (f32, bool) {
        match self {
            Kind::Idle => (4.0, true),
            Kind::Gait { style, .. } => (style.sizes().0, true),
            Kind::Fall => (1.0, true),
            Kind::Land { deep } => (if deep { 0.6 } else { 0.35 }, false),
            Kind::Afk | Kind::Sit | Kind::FloorSit | Kind::Lie | Kind::Hands(_) => (1.0, true),
        }
    }

    /// Whether its pose changes over time.
    fn moves(self) -> bool {
        match self {
            Kind::Gait { forward, right, .. } => forward != 0.0 || right != 0.0,
            Kind::Idle | Kind::Fall | Kind::Land { .. } => true,
            Kind::Afk | Kind::Sit | Kind::FloorSit | Kind::Lie | Kind::Hands(_) => false,
        }
    }
}

/// Builds the clips for one avatar: its muscle rig and body rig, and the floor its rest pose stands on.
pub struct Poser {
    rig: Rig,
    body: BodyRig,
    parents: Vec<Option<NodeIndex>>,
    rest: Vec<Transform>,
    /// Every human bone's node.
    bones: Vec<NodeIndex>,
    /// The lowest human bone's height in the rest pose.
    floor: f32,
}

impl Poser {
    /// The poser of a validated humanoid with muscle frames; `None` otherwise.
    pub fn new(doc: &Document) -> Option<Self> {
        let h = doc.avatar()?.humanoid.as_ref()?;
        let nodes = &doc.hierarchy.nodes;
        let rig = Rig::new(h, |n| nodes[n as usize].rest.rotation)?;
        let body = BodyRig::new(&doc.hierarchy, h)?;
        let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
        let rest: Vec<Transform> = nodes.iter().map(|n| n.rest).collect();
        let bones: Vec<NodeIndex> = h.bones.iter().map(|b| b.node).collect();
        let world = world_poses(&parents, Pose::IDENTITY, &rest);
        let floor = bones
            .iter()
            .map(|&n| world[n as usize].pos[1])
            .fold(f32::INFINITY, f32::min);
        Some(Self {
            rig,
            body,
            parents,
            rest,
            bones,
            floor,
        })
    }

    /// The body position height (over the human scale) that puts the lowest human bone of the pose with muscles
    /// `m` and body rotation `q` at the rest pose's floor.
    pub fn height(&self, m: &[f32; COUNT], q: Quat) -> f32 {
        let mut locals = self.rest.clone();
        for (n, r) in self.rig.rotations(m) {
            locals[n as usize].rotation = r;
        }
        self.body.place([0.0, 0.0, 0.0, q[0], q[1], q[2], q[3]], &mut locals);
        let world = world_poses(&self.parents, Pose::IDENTITY, &locals);
        let low = (self.bones.iter())
            .map(|&n| world[n as usize].pos[1])
            .fold(f32::INFINITY, f32::min);
        (self.floor - low) / self.body.human_scale()
    }

    /// The rest pose's body pose.
    pub fn rest_body(&self) -> [f32; 7] {
        self.body.rest
    }

    /// The clip of `kind`, called `name`.
    pub fn clip(&self, name: &str, kind: Kind) -> Clip {
        let (length, looping) = kind.timing();
        let n = if kind.moves() { 16 } else { 1 };
        let poses: Vec<P> = (0..n).map(|i| pose(kind, i as f32 / n as f32)).collect();
        let hands = matches!(kind, Kind::Hands(_));
        let mut curves = Vec::new();
        let written: Vec<usize> = if hands {
            finger_muscles().collect()
        } else {
            body_muscles().collect()
        };
        for m in written {
            let values: Vec<f32> = poses.iter().map(|p| p.m[m]).collect();
            curves.push(curve(
                Target::Humanoid {
                    property: property(m as u8),
                },
                &values,
                length,
                looping,
            ));
        }
        if !hands {
            let rest = self.body.rest;
            let mut body: [Vec<f32>; 7] = Default::default();
            for p in &poses {
                let (s, c) = (p.pitch * 0.5).sin_cos();
                let q = [s, 0.0, 0.0, c];
                let y = self.height(&p.m, q);
                for (k, v) in [rest[0], y, rest[2], q[0], q[1], q[2], q[3]].into_iter().enumerate() {
                    body[k].push(v);
                }
            }
            for (k, values) in body.iter().enumerate() {
                let target = Target::Humanoid {
                    property: HumanoidProperty::Body { component: k as u8 },
                };
                curves.push(curve(target, values, length, looping));
            }
        }
        Clip {
            name: name.to_string(),
            length,
            looping,
            root: None,
            curves,
        }
    }
}

/// A curve through `values` spaced evenly over `length` seconds, with Catmull-Rom tangents; a looping curve ends
/// on its first value and its tangents wrap.
fn curve(target: Target, values: &[f32], length: f32, looping: bool) -> Curve {
    let n = values.len();
    if n == 1 {
        let k = |time| Key {
            time,
            value: values[0],
            in_tangent: 0.0,
            out_tangent: 0.0,
        };
        return Curve {
            target,
            keys: vec![k(0.0), k(length)],
        };
    }
    let dt = length / n as f32;
    let at = |i: isize| {
        let i = if looping {
            i.rem_euclid(n as isize)
        } else {
            i.clamp(0, n as isize - 1)
        };
        values[i as usize]
    };
    let mut keys: Vec<Key> = (0..=n as isize)
        .map(|i| {
            let tangent = (at(i + 1) - at(i - 1)) / (2.0 * dt);
            Key {
                time: i as f32 * dt,
                value: at(i),
                in_tangent: tangent,
                out_tangent: tangent,
            }
        })
        .collect();
    if !looping {
        // The last key holds the start pose: landings end standing.
        let last = keys.last_mut().expect("n > 1 keys");
        last.value = values[0];
        last.in_tangent = 0.0;
        last.out_tangent = 0.0;
    }
    Curve { target, keys }
}
