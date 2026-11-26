//! Contacts (ADR 0017, 0047): every case recorded from the VRChat SDK's own contact runtime
//! (`tests/unity/vrc_contacts.json`, `tools/unity-truth/Assets/Editor/VrcContacts.cs`), replayed frame by frame: a
//! sender moving through a receiver, sphere, capsule and box shapes, the receiver types, face proximity and the
//! component's node apart from the root transform.

use civbb_core::math::Pose;
use civbb_dynamics::contact::{Participant, step};
use civbb_dynamics::{ContactRig, ContactState};
use civbb_format::Avatar;
use civbb_format::dynamics::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Quat, Transform, Vec3};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    unity: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    kind: ReceiverKind,
    min_velocity: f32,
    face: bool,
    sender_shape: Shape,
    receiver_shape: Shape,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
struct Shape {
    #[serde(rename = "type")]
    kind: String,
    radius: f32,
    height: f32,
    size: Vec3,
    position: Vec3,
    rotation: Quat,
}

#[derive(Deserialize)]
struct Frame {
    sender: WorldPose,
    receiver: WorldPose,
    component: bool,
    receiver_active: bool,
    dt: f32,
    value: f32,
}

#[derive(Deserialize)]
struct WorldPose {
    position: Vec3,
    rotation: Quat,
    scale: f32,
}

impl WorldPose {
    fn pose(&self) -> Pose {
        Pose {
            pos: self.position,
            rot: self.rotation,
            scale: self.scale,
        }
    }
}

/// Node 0 carries the sender, node 1 places the receiver and node 2 is the receiver component's node.
fn volume(node: u32, component: Option<u32>, s: &Shape) -> ContactVolume {
    let shape = match s.kind.as_str() {
        "Sphere" => ContactShape::Sphere { radius: s.radius },
        "Capsule" => ContactShape::Capsule {
            radius: s.radius,
            height: s.height,
        },
        _ => ContactShape::Box { size: s.size },
    };
    ContactVolume {
        node,
        component,
        shape,
        position: s.position,
        rotation: s.rotation,
    }
}

fn avatar(case: &Case) -> Avatar {
    let tags = vec![case.name.clone()];
    // One root, as validation wants; the test gives world poses directly.
    let node = |name: &str| Node {
        name: name.into(),
        parent: (name != "S").then_some(0),
        rest: Transform::IDENTITY,
        active: true,
    };
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![node("S"), node("R"), node("C")],
        },
        contacts: Contacts {
            senders: vec![ContactSender {
                volume: volume(0, None, &case.sender_shape),
                tags: tags.clone(),
                local_only: false,
            }],
            receivers: vec![ContactReceiver {
                volume: volume(1, Some(2), &case.receiver_shape),
                tags,
                parameter: None,
                kind: case.kind,
                allow_self: true,
                allow_others: true,
                local_only: false,
                min_velocity: case.min_velocity,
                face: case.face,
            }],
        },
        ..Avatar::default()
    }
}

#[test]
fn vrchat_sdk_recordings_match() {
    let r: Recording = serde_json::from_str(include_str!("unity/vrc_contacts.json")).unwrap();
    assert!(r.unity.starts_with("2022.3"), "{}", r.unity);
    let mut failures = Vec::new();
    for case in &r.cases {
        let a = avatar(case);
        civbb_format::validate(&a, &Default::default(), |_| true).unwrap();
        let rig = ContactRig::new(&a);
        let mut state = ContactState::new(&rig);
        // The SDK registers contacts during its first frame, so its frame 1 is civbb's first.
        for (f, frame) in case.frames.iter().enumerate().skip(1) {
            let world = [frame.sender.pose(), frame.receiver.pose(), Pose::IDENTITY];
            let active = [true, frame.receiver_active, frame.component];
            let mut p = [Participant {
                id: 1,
                rig: &rig,
                state: &mut state,
                world: &world,
                active: &active,
                local: true,
            }];
            step(&mut p, |_, _| true, frame.dt);
            let got = state.values()[0];
            if (got - frame.value).abs() > 1e-4 {
                failures.push(format!("{} frame {f}: got {got}, want {}", case.name, frame.value));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
