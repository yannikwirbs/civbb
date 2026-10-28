//! Chain solver behaviour, hand-derived from ADR 0015: rest, gravity against pull, falloff, spring, stiffness, limits,
//! colliders, immobile, frame-rate independence, multi-child roots, grab, pose and the reported parameters.

use civbb_dynamics::math::{Pose, length, normalize, sub};
use civbb_dynamics::rig::eval;
use civbb_dynamics::{Rig, State};
use civbb_format::Avatar;
use civbb_format::dynamics::*;
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::math::{Transform, Vec3};

fn node(name: &str, parent: Option<u32>, t: Vec3) -> Node {
    Node {
        name: name.into(),
        parent,
        rest: Transform {
            translation: t,
            ..Transform::IDENTITY
        },
        active: true,
    }
}

fn g(v: f32) -> Graded {
    Graded::constant(v)
}

/// Pull 0.5, everything else off.
fn chain(root: u32) -> Chain {
    Chain {
        root,
        ignore: Vec::new(),
        endpoint: [0.0; 3],
        multi_child: MultiChild::First,
        integration: Integration::Simplified,
        pull: g(0.5),
        spring: g(0.0),
        stiffness: g(0.0),
        gravity: g(0.0),
        gravity_falloff: g(0.0),
        immobile: g(0.0),
        immobile_type: ImmobileType::AllMotion,
        limit: Limit::None,
        radius: g(0.0),
        colliders: Vec::new(),
        max_stretch: g(0.0),
        max_squish: g(0.0),
        allow_grab: true,
        allow_posing: true,
        parameter: None,
    }
}

/// Root, Base at (0, 1, 0), then `bones` 0.1 m bones along +X, chain rooted at Base.
fn avatar(bones: usize, c: Chain) -> Avatar {
    let mut nodes = vec![node("Root", None, [0.0; 3]), node("Base", Some(0), [0.0, 1.0, 0.0])];
    for i in 0..bones {
        nodes.push(node(&format!("B{i}"), Some(nodes.len() as u32 - 1), [0.1, 0.0, 0.0]));
    }
    Avatar {
        hierarchy: Hierarchy { nodes },
        dynamics: Dynamics {
            chains: vec![Chain { root: 1, ..c }],
            colliders: Vec::new(),
        },
        ..Avatar::default()
    }
}

/// Runs `seconds` at `hz` with the avatar at `origin(t)`; returns the last frame's local pose.
fn run(a: &Avatar, s: &mut State, rig: &Rig, seconds: f32, hz: f32, origin: impl Fn(f32) -> Vec3) -> Vec<Transform> {
    let frames = (seconds * hz).round() as u32;
    let mut locals = Vec::new();
    for f in 0..=frames {
        locals = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
        let o = Pose {
            pos: origin(f as f32 / hz),
            ..Pose::IDENTITY
        };
        s.step(rig, o, &mut locals, if f == 0 { 0.0 } else { 1.0 / hz });
    }
    locals
}

fn world(a: &Avatar, locals: &[Transform]) -> Vec<Pose> {
    let mut w: Vec<Pose> = Vec::new();
    for (i, n) in a.hierarchy.nodes.iter().enumerate() {
        let p = n.parent.map_or(Pose::IDENTITY, |p| w[p as usize]);
        w.push(p.then(&locals[i]));
    }
    w
}

/// Degrees below horizontal of the bone from node `from` to node `to`.
fn droop(a: &Avatar, locals: &[Transform], from: usize, to: usize) -> f32 {
    let w = world(a, locals);
    let d = normalize(sub(w[to].pos, w[from].pos));
    (-d[1]).asin().to_degrees()
}

fn settle(c: Chain, bones: usize, hz: f32) -> f32 {
    let a = avatar(bones, c);
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    let l = run(&a, &mut s, &rig, 5.0, hz, |_| [0.0; 3]);
    droop(&a, &l, 1, 2)
}

#[test]
fn without_forces_the_chain_holds_its_rest_pose() {
    let a = avatar(
        3,
        Chain {
            spring: g(0.8),
            endpoint: [0.1, 0.0, 0.0],
            ..chain(1)
        },
    );
    let rig = Rig::new(&a);
    assert_eq!(rig.len(), 5, "base, three bones, one tip");
    let mut s = State::new(&rig);
    let l = run(&a, &mut s, &rig, 2.0, 60.0, |_| [0.0; 3]);
    for (got, rest) in l.iter().zip(&a.hierarchy.nodes) {
        assert!(
            sub(got.translation, rest.rest.translation)
                .iter()
                .all(|x| x.abs() < 1e-6)
        );
        assert!((got.rotation[3] - 1.0).abs() < 1e-6, "{:?}", got.rotation);
    }
}

#[test]
fn gravity_and_pull_balance_at_tan_gravity_times_one_minus_pull_over_pull() {
    // One bone: at equilibrium the gravity drift per step, g h^2 cos(theta), is undone by the pull's share
    // h^2 / (h^2 + alpha) of the tangential gap len sin(theta) after it; with alpha = ref^2 (1 - pull) / pull and
    // g = gravity len / ref^2 this gives tan(theta) = gravity (1 - pull) / pull for any step h.
    for (gravity, pull) in [(0.25f32, 0.5f32), (0.1, 0.2), (0.5, 0.8)] {
        let expected = (gravity * (1.0 - pull) / pull).atan().to_degrees();
        for hz in [60.0, 144.0] {
            let got = settle(
                Chain {
                    gravity: g(gravity),
                    pull: g(pull),
                    ..chain(1)
                },
                1,
                hz,
            );
            assert!(
                (got - expected).abs() < 0.2,
                "gravity {gravity} pull {pull} at {hz} Hz: {got} != {expected}"
            );
        }
    }
    // Negative gravity lifts; no pull lets the bone hang straight down.
    let up = settle(
        Chain {
            gravity: g(-0.25),
            ..chain(1)
        },
        1,
        60.0,
    );
    assert!((up + 14.036).abs() < 0.2, "{up}");
    let hang = settle(
        Chain {
            gravity: g(0.5),
            pull: g(0.0),
            ..chain(1)
        },
        1,
        60.0,
    );
    assert!((hang - 90.0).abs() < 0.5, "{hang}");
}

#[test]
fn gravity_falloff_removes_gravity_at_the_rest_direction() {
    let sag = |falloff| {
        settle(
            Chain {
                gravity: g(0.5),
                gravity_falloff: g(falloff),
                ..chain(1)
            },
            1,
            60.0,
        )
    };
    let (none, half, full) = (sag(0.0), sag(0.5), sag(1.0));
    assert!(full.abs() < 1e-3, "{full}");
    assert!(none > half && half > 0.0, "{none} {half}");
}

#[test]
fn spring_keeps_momentum_and_overshoots_while_zero_spring_does_not() {
    let peak = |spring| {
        let a = avatar(
            1,
            Chain {
                gravity: g(0.5),
                spring: g(spring),
                ..chain(1)
            },
        );
        let rig = Rig::new(&a);
        let mut s = State::new(&rig);
        let mut peak = 0.0f32;
        for f in 0..300 {
            let mut l: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
            s.step(&rig, Pose::IDENTITY, &mut l, if f == 0 { 0.0 } else { 1.0 / 60.0 });
            peak = peak.max(droop(&a, &l, 1, 2));
        }
        peak
    };
    let equilibrium = (0.5f32).atan().to_degrees();
    assert!(peak(0.0) < equilibrium + 0.01, "{}", peak(0.0));
    assert!(peak(0.9) > equilibrium + 2.0, "{}", peak(0.9));
}

#[test]
fn full_stiffness_holds_the_animated_pose_with_advanced_integration_only() {
    let stiff = |integration| {
        settle(
            Chain {
                gravity: g(1.0),
                pull: g(0.0),
                stiffness: g(1.0),
                integration,
                ..chain(1)
            },
            1,
            60.0,
        )
    };
    assert!(stiff(Integration::Advanced).abs() < 1e-3);
    assert!(stiff(Integration::Simplified) > 80.0, "simplified ignores stiffness");
}

#[test]
fn immobile_cancels_the_avatars_own_motion() {
    // The avatar slides along +Z at 3 m/s; without immobile the bone trails behind (its tip lags in -Z).
    let lag = |immobile| {
        let a = avatar(
            1,
            Chain {
                immobile: g(immobile),
                pull: g(0.2),
                ..chain(1)
            },
        );
        let rig = Rig::new(&a);
        let mut s = State::new(&rig);
        let l = run(&a, &mut s, &rig, 0.5, 60.0, |t| [0.0, 0.0, 3.0 * t]);
        let w = world(&a, &l);
        sub(w[2].pos, w[1].pos)[2]
    };
    assert!(lag(1.0).abs() < 1e-5);
    assert!(lag(0.0) < -0.005, "{}", lag(0.0));
    assert!(lag(0.5) < 0.0 && lag(0.5) > lag(0.0));
}

#[test]
fn frame_rate_does_not_change_the_motion_much() {
    let tip = |hz| {
        let a = avatar(
            3,
            Chain {
                gravity: g(0.3),
                pull: g(0.3),
                spring: g(0.6),
                ..chain(1)
            },
        );
        let rig = Rig::new(&a);
        let mut s = State::new(&rig);
        let l = run(&a, &mut s, &rig, 0.25, hz, |_| [0.0; 3]);
        world(&a, &l)[4].pos
    };
    let (slow, fast) = (tip(60.0), tip(144.0));
    // Mid-swing after a quarter second, within 5 % of the 0.3 m chain.
    assert!(length(sub(slow, fast)) < 0.015, "{slow:?} {fast:?}");
}

#[test]
fn multi_child_roots_ignore_first_or_average() {
    let rooted = |mode| {
        let mut a = avatar(
            1,
            Chain {
                gravity: g(0.5),
                multi_child: mode,
                ..chain(1)
            },
        );
        a.hierarchy.nodes.push(node("Other", Some(1), [0.0, 0.0, 0.1]));
        let rig = Rig::new(&a);
        let mut s = State::new(&rig);
        let l = run(&a, &mut s, &rig, 3.0, 60.0, |_| [0.0; 3]);
        (l[1].rotation, droop(&a, &l, 1, 2), droop(&a, &l, 1, 3))
    };
    let (rot, ..) = rooted(MultiChild::Ignore);
    assert_eq!(rot, [0.0, 0.0, 0.0, 1.0], "the root stays");
    let (_, first, other) = rooted(MultiChild::First);
    assert!(
        (first - 26.565).abs() < 0.3,
        "the root aims at its first child: {first}"
    );
    // The other child (along +Z) lies on the axis the root turns about for the first, so it stays level.
    assert!(other.abs() < 0.01, "{other}");
    let (_, a, b) = rooted(MultiChild::Average);
    // Averaging the two children's swings droops both, each less than alone.
    assert!(a > 1.0 && a < first && b > 1.0 && b < first, "{a} {b}");
}

#[test]
fn grab_moves_the_chain_and_pose_keeps_the_shape() {
    let c = Chain {
        endpoint: [0.1, 0.0, 0.0],
        max_stretch: g(0.5),
        ..chain(1)
    };
    let a = avatar(2, c.clone());
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    run(&a, &mut s, &rig, 0.1, 60.0, |_| [0.0; 3]);
    assert!(!s.grab(&rig, 0, 0, [0.0; 3]), "the root is pinned");
    // Hold the tip (particle 3) straight above the base: the chain stands up.
    assert!(s.grab(&rig, 0, 3, [0.0, 1.3, 0.0]));
    let l = run(&a, &mut s, &rig, 0.2, 60.0, |_| [0.0; 3]);
    assert!((droop(&a, &l, 1, 2) + 90.0).abs() < 0.5);
    let p = s.params(&rig, 0);
    assert!(p.is_grabbed && !p.is_posed);
    assert!(
        (p.angle - 0.5).abs() < 0.01,
        "the tip bone points 90 degrees from rest: {}",
        p.angle
    );

    // Out of reach: the grabbed bone stretches to its max, 1.5 times its length.
    s.grab(&rig, 0, 3, [0.0, 2.0, 0.0]);
    let l = run(&a, &mut s, &rig, 0.2, 60.0, |_| [0.0; 3]);
    assert!((s.params(&rig, 0).stretch - 1.0).abs() < 1e-3);
    let w = world(&a, &l);
    assert!(
        (w[3].pos[1] - 1.2).abs() < 1e-4,
        "the two bones stand straight: {:?}",
        w[3].pos
    );

    // Posed, it keeps the shape; unposed, it falls back to rest.
    s.grab(&rig, 0, 3, [0.0, 1.3, 0.0]);
    run(&a, &mut s, &rig, 0.2, 60.0, |_| [0.0; 3]);
    s.release(&rig, 0, true);
    let l = run(&a, &mut s, &rig, 2.0, 60.0, |_| [0.0; 3]);
    assert!((droop(&a, &l, 1, 2) + 90.0).abs() < 0.5);
    assert!(s.params(&rig, 0).is_posed);
    s.unpose(0);
    let l = run(&a, &mut s, &rig, 3.0, 60.0, |_| [0.0; 3]);
    assert!(droop(&a, &l, 1, 2).abs() < 0.1);

    // Without posing a release lets go; without grabbing nothing holds.
    let mut a = avatar(
        2,
        Chain {
            allow_posing: false,
            ..c.clone()
        },
    );
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    run(&a, &mut s, &rig, 0.1, 60.0, |_| [0.0; 3]);
    s.grab(&rig, 0, 3, [0.0, 1.3, 0.0]);
    run(&a, &mut s, &rig, 0.2, 60.0, |_| [0.0; 3]);
    s.release(&rig, 0, true);
    let l = run(&a, &mut s, &rig, 3.0, 60.0, |_| [0.0; 3]);
    assert!(droop(&a, &l, 1, 2).abs() < 0.1 && !s.params(&rig, 0).is_posed);
    a.dynamics.chains[0].allow_grab = false;
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    assert!(!s.grab(&rig, 0, 3, [0.0; 3]));
}

#[test]
fn graded_values_follow_their_curve_over_depth() {
    let v = Graded {
        value: 2.0,
        curve: vec![[0.0, 1.0], [0.5, 0.5], [1.0, 0.0]],
    };
    for (d, want) in [(0.0, 2.0), (0.25, 1.5), (0.5, 1.0), (0.75, 0.5), (1.0, 0.0), (2.0, 0.0)] {
        assert!((eval(&v, d) - want).abs() < 1e-6, "{d}");
    }
    assert_eq!(eval(&g(0.3), 0.7), 0.3);
    // Out-of-range values from untrusted content are clamped when compiled.
    let rig = Rig::new(&avatar(
        1,
        Chain {
            pull: g(7.0),
            gravity: g(-3.0),
            ..chain(1)
        },
    ));
    assert_eq!((rig.pull[1], rig.gravity[1]), (1.0, -1.0));
}

#[test]
fn the_bloated_corpus_avatar_stays_finite() {
    let a = civbb_corpus::generate(&civbb_corpus::bloated());
    let rig = Rig::new(&a);
    assert_eq!(rig.chains.len(), a.dynamics.chains.len());
    assert!(rig.len() > 4 * rig.chains.len(), "{} particles", rig.len());
    let mut s = State::new(&rig);
    // Walking, then a hitch, then turning on the spot.
    let mut l = Vec::new();
    for f in 0..240 {
        l = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
        let t = f as f32 / 90.0;
        let o = Pose {
            pos: [0.0, 0.0, 1.4 * t],
            rot: [0.0, (t * 0.5).sin(), 0.0, (t * 0.5).cos()],
            scale: 1.0,
        };
        s.step(&rig, o, &mut l, if f == 120 { 0.5 } else { 1.0 / 90.0 });
    }
    assert!(
        l.iter()
            .all(|t| t.rotation.iter().chain(&t.translation).all(|x| x.is_finite()))
    );
    for c in 0..rig.chains.len() {
        let p = s.params(&rig, c);
        assert!([p.angle, p.stretch, p.squish].iter().all(|x| (0.0..=1.0).contains(x)));
    }
}

/// One horizontal bone falling without pull, with a limit or colliders.
fn hang(c: Chain, colliders: Vec<Collider>) -> (f32, f32) {
    let mut a = avatar(1, Chain { pull: g(0.0), ..c });
    a.dynamics.colliders = colliders;
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    let l = run(&a, &mut s, &rig, 3.0, 60.0, |_| [0.0; 3]);
    (droop(&a, &l, 1, 2), s.params(&rig, 0).squish)
}

fn limited(limit: Limit) -> f32 {
    hang(
        Chain {
            gravity: g(1.0),
            limit,
            ..chain(1)
        },
        Vec::new(),
    )
    .0
}

#[test]
fn limits_bound_the_bone_around_its_rest_direction() {
    // The limit frame is the parent's rotation swung so Y runs along the bone (+X here): frame X is world -Y, frame
    // Z is world Z. Turned 90 degrees about the bone, frame X becomes world -Z.
    let turned = [0.0, 90.0, 0.0];
    assert!((limited(Limit::None) - 90.0).abs() < 0.5);
    let angle = Limit::Angle {
        max_angle: g(30.0),
        rotation: [0.0; 3],
    };
    assert!((limited(angle) - 30.0).abs() < 0.1);
    // A hinge turns about frame X: about the vertical, gravity cannot move the bone; about -Z, it droops to the max.
    let hinge = |rotation| Limit::Hinge {
        max_angle: g(30.0),
        rotation,
    };
    assert!(limited(hinge([0.0; 3])).abs() < 0.1);
    assert!((limited(hinge(turned)) - 30.0).abs() < 0.1);
    // Polar: pitch (about frame X) limits travel along frame Z, yaw travel along frame X.
    let polar = |rotation| Limit::Polar {
        max_pitch: g(20.0),
        max_yaw: g(60.0),
        rotation,
    };
    assert!((limited(polar([0.0; 3])) - 60.0).abs() < 0.1);
    assert!((limited(polar(turned)) - 20.0).abs() < 0.1);
    // Rotation tilts the cone: its axis 40 degrees down, 10 wide, so a bone pulled level rests at 30.
    let tilted = Chain {
        gravity: g(0.0),
        pull: g(1.0),
        limit: Limit::Angle {
            max_angle: g(10.0),
            rotation: [0.0, 0.0, -40.0],
        },
        ..chain(1)
    };
    let a = avatar(1, tilted);
    let rig = Rig::new(&a);
    let mut s = State::new(&rig);
    let l = run(&a, &mut s, &rig, 1.0, 60.0, |_| [0.0; 3]);
    assert!((droop(&a, &l, 1, 2) - 30.0).abs() < 0.1);
}

fn collider(shape: Shape, position: Vec3, rotation: [f32; 4], inside: bool) -> Collider {
    Collider {
        node: 0,
        shape,
        position,
        rotation,
        inside,
    }
}

#[test]
fn colliders_push_bones_out_or_keep_them_in() {
    // Gravity 0.2 moves the tip 2 cm per step: a bone that moves its whole length in one step can pass through
    // a collider thinner than that (ADR 0015).
    let with = |radius: f32, col: Collider| {
        hang(
            Chain {
                gravity: g(0.2),
                radius: g(radius),
                colliders: vec![0],
                ..chain(1)
            },
            vec![col],
        )
    };
    const I: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    // A floor at y = 0.95 under the bone (base at y = 1, length 0.1): the tip stops 0.05 down, asin(0.5) = 30
    // degrees; with a 0.02 bone radius 0.03 down, asin(0.3).
    let floor = collider(Shape::Plane, [0.0, 0.95, 0.0], I, false);
    let rest = with(0.0, floor.clone()).0;
    assert!((rest - 30.0).abs() < 0.2, "{rest}");
    assert!((with(0.02, floor).0 - 17.458).abs() < 0.2);
    // A ball of radius 0.06 at (0.1, 0.9): the tip's circle meets it where sin(t + 45) = 1.32 / sqrt(2), t = 23.97.
    let ball = collider(Shape::Sphere { radius: 0.06 }, [0.1, 0.9, 0.0], I, false);
    let rest = with(0.0, ball).0;
    assert!((rest - 23.97).abs() < 1.5, "{rest}");
    // The same circle across a capsule lying along Z through that point.
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let rod = collider(
        Shape::Capsule {
            radius: 0.06,
            height: 0.5,
        },
        [0.1, 0.9, 0.0],
        [h, 0.0, 0.0, h],
        false,
    );
    assert!((with(0.0, rod).0 - 23.97).abs() < 1.5);
    // Inside a ball of radius 0.15 around the base, a bone of radius 0.07 has 0.08 of room: it squishes to 0.8 of
    // its length, 0.4 of its 0.5 max squish.
    let cage = collider(Shape::Sphere { radius: 0.15 }, [0.0, 1.0, 0.0], I, true);
    let (_, squish) = hang(
        Chain {
            gravity: g(0.2),
            radius: g(0.07),
            colliders: vec![0],
            max_squish: g(0.5),
            ..chain(1)
        },
        vec![cage],
    );
    assert!((squish - 0.4).abs() < 1e-3, "{squish}");
}
