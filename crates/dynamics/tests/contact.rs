//! Contact conformance, hand-derived from ADR 0017: receiver types, tags, self and others, permits, local-only,
//! capsules, scale and clamps, inactive nodes, OnEnter speed, and the grid against brute force.

use civbb_dynamics::contact::{Participant, segment_distance, step};
use civbb_dynamics::math::{Pose, euler_to_quat};
use civbb_dynamics::{ContactRig, ContactState};
use civbb_format::Avatar;
use civbb_format::dynamics::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Transform, Vec3};

fn sphere(radius: f32) -> Shape {
    Shape::Sphere { radius }
}

fn vol(shape: Shape) -> ContactVolume {
    ContactVolume {
        node: 0,
        shape,
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

fn sender(shape: Shape, tags: &[&str]) -> ContactSender {
    ContactSender {
        volume: vol(shape),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        local_only: false,
    }
}

fn receiver(shape: Shape, kind: ReceiverKind) -> ContactReceiver {
    ContactReceiver {
        volume: vol(shape),
        tags: vec!["Hand".into()],
        parameter: "P".into(),
        kind,
        allow_self: true,
        allow_others: true,
        local_only: false,
        min_velocity: 0.0,
    }
}

/// An avatar of one root node carrying `senders` and `receivers`.
fn avatar(senders: Vec<ContactSender>, receivers: Vec<ContactReceiver>) -> Avatar {
    Avatar {
        hierarchy: Hierarchy {
            nodes: vec![Node {
                name: "Root".into(),
                parent: None,
                rest: Transform::IDENTITY,
                active: true,
            }],
        },
        contacts: Contacts { senders, receivers },
        ..Avatar::default()
    }
}

/// Avatars on a client, each with one root node placed by the test.
struct Scene {
    rigs: Vec<ContactRig>,
    states: Vec<ContactState>,
    poses: Vec<Pose>,
    active: Vec<bool>,
    local: Vec<bool>,
}

impl Scene {
    fn new(avatars: &[Avatar]) -> Self {
        let rigs: Vec<ContactRig> = avatars.iter().map(ContactRig::new).collect();
        Self {
            states: rigs.iter().map(ContactState::new).collect(),
            poses: vec![Pose::IDENTITY; rigs.len()],
            active: vec![true; rigs.len()],
            local: vec![false; rigs.len()],
            rigs,
        }
    }

    fn at(&mut self, avatar: usize, pos: Vec3) -> &mut Self {
        self.poses[avatar].pos = pos;
        self
    }

    fn run_with(&mut self, permits: impl Fn(u64, u64) -> bool, dt: f32) -> Vec<Vec<f32>> {
        let mut ps: Vec<Participant> = self
            .rigs
            .iter()
            .zip(&mut self.states)
            .enumerate()
            .map(|(i, (rig, state))| Participant {
                id: 100 + i as u64,
                rig,
                state,
                world: std::slice::from_ref(&self.poses[i]),
                active: std::slice::from_ref(&self.active[i]),
                local: self.local[i],
            })
            .collect();
        step(&mut ps, permits, dt);
        self.states.iter().map(|s| s.values().to_vec()).collect()
    }

    fn run(&mut self) -> Vec<Vec<f32>> {
        self.run_with(|_, _| true, 1.0 / 60.0)
    }

    /// Avatar 0's first receiver after one frame.
    fn value(&mut self) -> f32 {
        self.run()[0][0]
    }
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

/// Avatar 0: one receiver at the origin. Avatar 1: one `Hand` sender of radius `rs` at `x` on the X axis.
fn pair(r: ContactReceiver, rs: f32, x: f32) -> Scene {
    let mut s = Scene::new(&[
        avatar(vec![], vec![r]),
        avatar(vec![sender(sphere(rs), &["Hand"])], vec![]),
    ]);
    s.at(1, [x, 0.0, 0.0]);
    s
}

#[test]
fn constant_is_one_while_overlapping() {
    let r = || receiver(sphere(0.15), ReceiverKind::Constant);
    // Radii sum to 0.2.
    close(pair(r(), 0.05, 0.19).value(), 1.0);
    close(pair(r(), 0.05, 0.21).value(), 0.0);
    // Touching counts (sums exact in binary).
    close(
        pair(receiver(sphere(0.25), ReceiverKind::Constant), 0.25, 0.5).value(),
        1.0,
    );
    // Stays 1 on later frames, drops when the sender leaves.
    let mut s = pair(r(), 0.05, 0.1);
    close(s.value(), 1.0);
    close(s.value(), 1.0);
    close(s.at(1, [0.5, 0.0, 0.0]).value(), 0.0);
}

#[test]
fn proximity_measures_the_senders_closest_point_against_the_receiver_radius() {
    let r = || receiver(sphere(0.2), ReceiverKind::Proximity);
    // Sender surface 0.1 from the centre of a 0.2 receiver: 1 - 0.1 / 0.2.
    close(pair(r(), 0.05, 0.15).value(), 0.5);
    // Surface at or past the centre.
    close(pair(r(), 0.05, 0.05).value(), 1.0);
    close(pair(r(), 0.05, 0.0).value(), 1.0);
    // Just touching the edge, and outside.
    close(pair(r(), 0.05, 0.25).value(), 0.0);
    close(pair(r(), 0.05, 0.3).value(), 0.0);
}

#[test]
fn proximity_takes_the_closest_sender() {
    let mut s = Scene::new(&[
        avatar(vec![], vec![receiver(sphere(0.2), ReceiverKind::Proximity)]),
        avatar(vec![sender(sphere(0.05), &["Hand"])], vec![]),
        avatar(vec![sender(sphere(0.05), &["Hand"])], vec![]),
    ]);
    s.at(1, [0.15, 0.0, 0.0]).at(2, [0.0, -0.1, 0.0]);
    // 0.5 and 1 - 0.05 / 0.2.
    close(s.value(), 0.75);
}

#[test]
fn capsules_are_segments_with_a_radius() {
    // Height 1 with radius 0.1: core from y = -0.4 to 0.4.
    let cap = Shape::Capsule {
        radius: 0.1,
        height: 1.0,
    };
    let r = || receiver(cap, ReceiverKind::Constant);
    let at = |p: Vec3| {
        let mut s = pair(r(), 0.05, 0.0);
        s.at(1, p).value()
    };
    close(at([0.149, 0.3, 0.0]), 1.0);
    close(at([0.151, 0.3, 0.0]), 0.0);
    close(at([0.0, 0.549, 0.0]), 1.0);
    close(at([0.0, 0.551, 0.0]), 0.0);
    // The volume rotation turns the core onto X.
    let mut turned = r();
    turned.volume.rotation = euler_to_quat([0.0, 0.0, 90.0]);
    close(pair(turned.clone(), 0.05, 0.549).value(), 1.0);
    let mut s = pair(turned, 0.05, 0.0);
    close(s.at(1, [0.0, 0.3, 0.0]).value(), 0.0);
    // Proximity against a capsule measures to its core segment.
    let mut s = pair(receiver(cap, ReceiverKind::Proximity), 0.05, 0.0);
    close(s.at(1, [0.1, 0.4, 0.0]).value(), 0.5);
}

#[test]
fn tags_need_one_case_sensitive_match_among_the_first_sixteen() {
    let with = |tags: &[&str], shared: &[&str]| {
        let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
        r.tags = shared.iter().map(|t| t.to_string()).collect();
        let mut s = Scene::new(&[avatar(vec![], vec![r]), avatar(vec![sender(sphere(0.1), tags)], vec![])]);
        s.value()
    };
    close(with(&["Hand"], &["Hand"]), 1.0);
    close(with(&["hand"], &["Hand"]), 0.0);
    close(with(&["Foot", "Head"], &["Torso", "Head"]), 1.0);
    close(with(&["Foot"], &["Head"]), 0.0);
    close(with(&[], &["Head"]), 0.0);
    let many: Vec<String> = (0..17).map(|i| format!("T{i:02}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    close(with(&many, &["T15"]), 1.0);
    close(with(&many, &["T16"]), 0.0);
}

#[test]
fn allow_self_and_allow_others_filter_by_avatar() {
    let one = |allow_self: bool| {
        let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
        r.allow_self = allow_self;
        Scene::new(&[avatar(vec![sender(sphere(0.1), &["Hand"])], vec![r])]).value()
    };
    close(one(true), 1.0);
    close(one(false), 0.0);

    let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
    r.allow_others = false;
    close(pair(r, 0.1, 0.0).value(), 0.0);
}

#[test]
fn permits_gate_other_avatars_only() {
    let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
    r.allow_others = true;
    let mut s = Scene::new(&[
        avatar(vec![sender(sphere(0.1), &["Hand"])], vec![r.clone()]),
        avatar(vec![sender(sphere(0.1), &["Hand"])], vec![r]),
    ]);
    // Avatar 101 accepts nobody: its own sender still counts; avatar 100 accepts 101.
    let asked = std::cell::RefCell::new(Vec::new());
    let v = s.run_with(
        |rec, snd| {
            asked.borrow_mut().push((rec, snd));
            rec == 100
        },
        1.0 / 60.0,
    );
    assert_eq!(v, [[1.0], [1.0]]);
    let mut asked = asked.into_inner();
    asked.sort();
    assert_eq!(asked, [(100, 101), (101, 100)]);

    // Without its own sender, avatar 101 now sees nothing.
    let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
    r.allow_self = false;
    let mut s = Scene::new(&[
        avatar(vec![sender(sphere(0.1), &["Hand"])], vec![]),
        avatar(vec![], vec![r]),
    ]);
    assert_eq!(s.run_with(|rec, _| rec != 101, 1.0 / 60.0)[1], [0.0]);
}

#[test]
fn local_only_contacts_exist_only_on_the_owners_client() {
    // A local-only receiver on a remote avatar is not evaluated; on the local avatar it is.
    let mut r = receiver(sphere(0.1), ReceiverKind::Constant);
    r.local_only = true;
    let mut s = pair(r.clone(), 0.1, 0.0);
    close(s.value(), 0.0);
    let mut s = pair(r, 0.1, 0.0);
    s.local[0] = true;
    close(s.value(), 1.0);

    // A local-only sender on a remote avatar does not exist; on the local avatar it reaches remote receivers too.
    let mut snd = sender(sphere(0.1), &["Hand"]);
    snd.local_only = true;
    let r = receiver(sphere(0.1), ReceiverKind::Constant);
    let mut s = Scene::new(&[avatar(vec![], vec![r.clone()]), avatar(vec![snd.clone()], vec![])]);
    close(s.value(), 0.0);
    let mut s = Scene::new(&[avatar(vec![], vec![r]), avatar(vec![snd], vec![])]);
    s.local[1] = true;
    close(s.value(), 1.0);
}

#[test]
fn inactive_nodes_have_no_contacts() {
    let mut s = pair(receiver(sphere(0.1), ReceiverKind::Constant), 0.1, 0.0);
    s.active[1] = false;
    close(s.value(), 0.0);
    s.active[1] = true;
    close(s.value(), 1.0);
    s.active[0] = false;
    close(s.value(), 0.0);
}

#[test]
fn scale_grows_volumes_up_to_the_caps() {
    // Scale 2: receiver radius 0.2, sender 0.2 (sender avatar unscaled radius 0.1, scaled too).
    let mut s = pair(receiver(sphere(0.1), ReceiverKind::Constant), 0.1, 0.39);
    s.poses[0].scale = 2.0;
    close(s.value(), 0.0);
    s.poses[1].scale = 2.0;
    close(s.value(), 1.0);

    // Radius 2 at scale 2 clamps to 3: a 0.5 sender touches at 3.5, not 3.6.
    let mut s = pair(receiver(sphere(2.0), ReceiverKind::Constant), 0.5, 3.5);
    s.poses[0].scale = 2.0;
    close(s.value(), 1.0);
    let mut s = pair(receiver(sphere(2.0), ReceiverKind::Constant), 0.5, 3.6);
    s.poses[0].scale = 2.0;
    close(s.value(), 0.0);

    // Height 8 clamps to 6: radius 0.1, core to y = 2.9; a 0.1 sender reaches it up to 3.1.
    let cap = Shape::Capsule {
        radius: 0.1,
        height: 8.0,
    };
    let mut s = pair(receiver(cap, ReceiverKind::Constant), 0.1, 0.0);
    close(s.at(1, [0.0, 3.09, 0.0]).value(), 1.0);
    let mut s = pair(receiver(cap, ReceiverKind::Constant), 0.1, 0.0);
    close(s.at(1, [0.0, 3.11, 0.0]).value(), 0.0);
}

#[test]
fn on_enter_fires_once_per_entry() {
    let mut s = pair(receiver(sphere(0.1), ReceiverKind::OnEnter), 0.05, 1.0);
    close(s.value(), 0.0);
    close(s.at(1, [0.1, 0.0, 0.0]).value(), 1.0);
    close(s.value(), 0.0);
    close(s.at(1, [0.05, 0.0, 0.0]).value(), 0.0);
    close(s.at(1, [1.0, 0.0, 0.0]).value(), 0.0);
    close(s.at(1, [0.0, 0.0, 0.0]).value(), 1.0);

    // Senders enter on their own: a second one entering while the first stays fires again.
    let mut s = Scene::new(&[
        avatar(vec![], vec![receiver(sphere(0.1), ReceiverKind::OnEnter)]),
        avatar(vec![sender(sphere(0.05), &["Hand"])], vec![]),
        avatar(vec![sender(sphere(0.05), &["Hand"])], vec![]),
    ]);
    s.at(2, [1.0, 0.0, 0.0]);
    close(s.value(), 1.0);
    close(s.value(), 0.0);
    close(s.at(2, [0.0, 0.1, 0.0]).value(), 1.0);

    // Overlapping on the first frame counts as entering.
    close(
        pair(receiver(sphere(0.1), ReceiverKind::OnEnter), 0.05, 0.0).value(),
        1.0,
    );
}

#[test]
fn on_enter_needs_the_relative_speed() {
    let r = |min: f32| {
        let mut r = receiver(sphere(0.1), ReceiverKind::OnEnter);
        r.min_velocity = min;
        r
    };
    let dt = 0.1;
    // The sender moves 0.4 m in 0.1 s: 4 m/s.
    let enter = |min: f32| {
        let mut s = pair(r(min), 0.05, 0.5);
        s.run_with(|_, _| true, dt);
        s.at(1, [0.1, 0.0, 0.0]).run_with(|_, _| true, dt)[0][0]
    };
    close(enter(3.0), 1.0);
    close(enter(5.0), 0.0);

    // A slow entry is used up: staying inside does not fire later.
    let mut s = pair(r(5.0), 0.05, 0.5);
    s.run_with(|_, _| true, dt);
    s.at(1, [0.1, 0.0, 0.0]).run_with(|_, _| true, dt);
    close(s.at(1, [0.0, 0.0, 0.0]).run_with(|_, _| true, dt)[0][0], 0.0);

    // The receiver moving onto a still sender counts the same.
    let mut s = pair(r(3.0), 0.05, 0.0);
    s.at(0, [-0.5, 0.0, 0.0]).run_with(|_, _| true, dt);
    close(s.at(0, [-0.1, 0.0, 0.0]).run_with(|_, _| true, dt)[0][0], 1.0);

    // Both moving +X fast (10 and 8 m/s), closing at 2 m/s: too slow for 3.
    let mut s = pair(r(3.0), 0.05, 0.3);
    s.run_with(|_, _| true, dt);
    s.at(0, [1.0, 0.0, 0.0]).at(1, [1.1, 0.0, 0.0]);
    close(s.run_with(|_, _| true, dt)[0][0], 0.0);

    // No speed on the first frame.
    close(pair(r(0.1), 0.05, 0.0).value(), 0.0);
}

#[test]
fn the_grid_finds_pairs_across_cell_boundaries() {
    for (r, x) in [(0.99, 1.01), (-0.01, 0.01), (-1.0, -0.99), (2.98, 3.01), (0.5, 0.5)] {
        let mut s = pair(receiver(sphere(0.02), ReceiverKind::Constant), 0.02, x);
        s.at(0, [r, r, r]).at(1, [x, r, r]);
        close(s.value(), 1.0);
    }
}

#[test]
fn the_grid_matches_brute_force() {
    // 40 avatars, each with a sender and a constant receiver of LCG-picked radii and places in an 8 m cube.
    let mut seed: u32 = 12345;
    let mut rand = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1 << 24) as f32
    };
    let n = 40;
    let mut avatars = Vec::new();
    let mut place = Vec::new();
    for _ in 0..n {
        let rs = 0.05 + rand() * 1.5;
        let rr = 0.05 + rand() * 1.5;
        let mut r = receiver(sphere(rr), ReceiverKind::Constant);
        r.allow_self = false;
        avatars.push(avatar(vec![sender(sphere(rs), &["Hand"])], vec![r]));
        place.push(([rand() * 8.0 - 4.0, rand() * 8.0 - 4.0, rand() * 8.0 - 4.0], rs, rr));
    }
    let mut s = Scene::new(&avatars);
    for (i, (p, _, _)) in place.iter().enumerate() {
        s.at(i, *p);
    }
    let got = s.run();
    let mut hits = 0;
    for (i, (pi, _, rr)) in place.iter().enumerate() {
        let want = place.iter().enumerate().any(|(j, (pj, rs, _))| {
            let d = (0..3).map(|k| (pi[k] - pj[k]).powi(2)).sum::<f32>().sqrt();
            j != i && d <= rr + rs
        });
        hits += usize::from(want);
        assert_eq!(got[i][0], f32::from(u8::from(want)), "avatar {i}");
    }
    // The layout is neither empty nor saturated.
    assert!(hits > 5 && hits < n - 5, "{hits} hits");
}

#[test]
fn segment_distance_handles_parallel_crossing_and_degenerate_segments() {
    let d = segment_distance;
    close(d([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]), 1.0);
    close(d([0.0; 3], [1.0, 0.0, 0.0], [0.5, -1.0, 0.5], [0.5, 1.0, 0.5]), 0.5);
    close(d([0.0; 3], [1.0, 0.0, 0.0], [3.0, 0.0, 0.0], [4.0, 0.0, 0.0]), 2.0);
    close(d([0.0; 3], [0.0; 3], [3.0, 4.0, 0.0], [3.0, 4.0, 0.0]), 5.0);
    close(d([0.0; 3], [0.0; 3], [-1.0, 1.0, 0.0], [1.0, 1.0, 0.0]), 1.0);
    close(
        d([2.0, 1.0, 0.0], [2.0, 3.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0]),
        2.0f32.sqrt(),
    );
}
