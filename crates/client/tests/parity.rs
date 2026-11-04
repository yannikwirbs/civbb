//! FLIP parity scoring without a device (ADR 0033): FLIP of an image against itself is 0, a known perturbation
//! matches NVIDIA's reference evaluator, scores cover only the avatar's pixels, the traffic light's bounds, and the
//! rig's views.

use civbb_client::parity::{self, HEIGHT, PIXELS_PER_DEGREE, THRESHOLDS, WIDTH};
use civbb_format::cost::{Light, ViewScore};

const W: u32 = 64;
const H: u32 = 48;

/// A 64x48 RGB8 test pattern: red, green and blue ramps at different rates.
fn pattern() -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..H {
        for x in 0..W {
            out.extend([(x * 4 % 256) as u8, (y * 5 % 256) as u8, ((x + y) * 3 % 256) as u8]);
        }
    }
    out
}

#[test]
fn an_image_against_itself_is_zero() {
    let p = pattern();
    assert!(parity::flip(&p, &p, W, H).iter().all(|&e| e == 0.0));
}

/// The pattern with red inverted in the 16x16 square at (20, 10). Expected values from NVIDIA's reference,
/// `flip-evaluator` 1.7 (Python bindings of the C++ FLIP): `evaluate(ref / 255, test / 255, "LDR")` at its default
/// 67.02 pixels per degree gives a mean of 0.03645635, and these pixels.
#[test]
fn a_known_perturbation_matches_the_reference() {
    assert!((PIXELS_PER_DEGREE - 67.020_645).abs() < 1e-4);
    let reference = pattern();
    let mut test = reference.clone();
    for y in 10..26 {
        for x in 20..36 {
            let i = 3 * (y * W + x) as usize;
            test[i] = 255 - test[i];
        }
    }
    let error = parity::flip(&reference, &test, W, H);
    let mean = error.iter().map(|&e| f64::from(e)).sum::<f64>() / error.len() as f64;
    assert!((mean - 0.036_456_35).abs() < 1e-6, "mean {mean}");
    for ((x, y), want) in [
        ((28, 18), 0.287_300),
        ((20, 10), 0.478_535),
        ((5, 5), 0.0),
        ((40, 30), 0.001_049_28),
    ] {
        let got = error[(y * W + x) as usize];
        assert!((got - want).abs() < 1e-5, "({x}, {y}): {got}, reference {want}");
    }
}

/// BGRA8 texels of the rig's size, all background, as the headless target stores it.
fn background() -> Vec<u8> {
    let rig = parity::rig(&civbb_corpus::generate(&civbb_corpus::lean()));
    let encode = |c: f32| {
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    let [r, g, b] = rig.background.map(encode);
    [b, g, r, 255].repeat((WIDTH * HEIGHT) as usize)
}

#[test]
fn scores_cover_only_the_avatars_pixels() {
    let empty = background();
    let (s, _) = parity::compare(&empty, &empty, WIDTH, HEIGHT);
    assert_eq!(s, ViewScore::default());

    // A 10x10 white square in the reference only: 100 pixels covered, all badly wrong.
    let mut square = empty.clone();
    for y in 300..310 {
        for x in 300..310 {
            let i = 4 * (y * WIDTH + x) as usize;
            square[i..i + 3].copy_from_slice(&[255, 255, 255]);
        }
    }
    let (s, error) = parity::compare(&square, &empty, WIDTH, HEIGHT);
    assert_eq!(s.pixels, 100);
    assert!(s.mean > 0.5 && s.p99 > 0.5 && s.max <= 1.0, "{s:?}");
    // FLIP spreads error past the square's edge; those pixels are not the avatar's and do not count.
    assert!(error[(299 * WIDTH + 305) as usize] > 0.0);
    assert_eq!(parity::light(&s, &THRESHOLDS), Light::Red);

    let (s, _) = parity::compare(&square, &square, WIDTH, HEIGHT);
    assert_eq!((s.pixels, s.mean, s.max), (100, 0.0, 0.0));
}

#[test]
fn score_takes_mean_p99_and_max() {
    let mut errors: Vec<f32> = (0..=100).rev().map(|i| i as f32 / 100.0).collect();
    let s = parity::score(&mut errors);
    assert!((s.mean - 0.5).abs() < 1e-6);
    assert_eq!((s.p99, s.max, s.pixels), (0.99, 1.0, 101));
    assert_eq!(parity::score(&mut []), ViewScore::default());
}

#[test]
fn the_light_is_green_within_both_bounds_and_red_past_either() {
    let t = THRESHOLDS;
    let at = |mean, p99| {
        parity::light(
            &ViewScore {
                mean,
                p99,
                max: p99,
                pixels: 1,
            },
            &t,
        )
    };
    assert_eq!(at(t.green_mean, t.green_p99), Light::Green);
    assert_eq!(at(t.green_mean * 1.01, 0.0), Light::Yellow);
    assert_eq!(at(0.0, t.green_p99 * 1.01), Light::Yellow);
    assert_eq!(at(t.red_mean, t.red_p99), Light::Yellow);
    assert_eq!(at(t.red_mean * 1.01, 0.0), Light::Red);
    assert_eq!(at(0.0, t.red_p99 * 1.01), Light::Red);
}

#[test]
fn the_rig_circles_the_bounds_centre() {
    let rig = parity::rig(&civbb_corpus::generate(&civbb_corpus::bloated()));
    assert_eq!((rig.width, rig.height), (WIDTH, HEIGHT));
    let names: Vec<&str> = rig.views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["front", "left", "back", "right"]);
    let t = rig.views[0].target;
    for v in &rig.views {
        assert_eq!(v.target, t);
        assert_eq!(v.eye[1], t[1], "{}: level with the centre", v.name);
    }
    let (front, left, back, right) = (rig.views[0].eye, rig.views[1].eye, rig.views[2].eye, rig.views[3].eye);
    // The avatar faces +Z, so its left is +X.
    assert!(front[2] > t[2] && back[2] < t[2] && left[0] > t[0] && right[0] < t[0]);
}

#[test]
fn the_rig_frames_what_shows_at_rest_in_world_space() {
    // Moving the root node moves the rest pose and its bounds; bind-pose mesh positions would not move (#69).
    let avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let (lo, hi) = parity::rest_bounds(&avatar);
    assert!(hi[1] - lo[1] > 1.0, "a standing avatar: {lo:?} {hi:?}");
    let mut moved = avatar.clone();
    let root = moved.hierarchy.nodes.iter().position(|n| n.parent.is_none()).unwrap();
    moved.hierarchy.nodes[root].rest.translation[0] += 2.0;
    let (mlo, mhi) = parity::rest_bounds(&moved);
    assert!((mlo[0] - lo[0] - 2.0).abs() < 1e-4 && (mhi[0] - hi[0] - 2.0).abs() < 1e-4);
    let shift = parity::rig(&moved).views[0].target[0] - parity::rig(&avatar).views[0].target[0];
    assert!((shift - 2.0).abs() < 1e-4, "the rig follows: {shift}");
}
