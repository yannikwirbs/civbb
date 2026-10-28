//! Unity `AnimationCurve` key semantics, hand-derived from the Hermite basis (ADR 0007): clamp, tangent scaling,
//! constant segments, overshoot, segment choice at key times.

use civbb_anim::curve::eval;
use civbb_format::animator::Key;

const INF: f32 = f32::INFINITY;

fn key(time: f32, value: f32, in_tangent: f32, out_tangent: f32) -> Key {
    Key {
        time,
        value,
        in_tangent,
        out_tangent,
    }
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

#[test]
fn linear_tangents_interpolate_linearly() {
    let k = [key(0.0, 0.0, 1.0, 1.0), key(1.0, 1.0, 1.0, 1.0)];
    for t in [0.1, 0.25, 0.5, 0.9] {
        close(eval(&k, t), t);
    }
}

#[test]
fn flat_tangents_ease_in_out() {
    // h01(s) = -2s^3 + 3s^2.
    let k = [key(0.0, 0.0, 0.0, 0.0), key(1.0, 1.0, 0.0, 0.0)];
    close(eval(&k, 0.25), 0.15625);
    close(eval(&k, 0.5), 0.5);
    close(eval(&k, 0.75), 0.84375);
}

#[test]
fn tangents_scale_with_segment_length() {
    // Slope 2 over a 2 s segment is a straight line: tangents are per second, not per segment.
    let k = [key(0.0, 0.0, 2.0, 2.0), key(2.0, 4.0, 2.0, 2.0)];
    close(eval(&k, 0.5), 1.0);
    close(eval(&k, 1.5), 3.0);
}

#[test]
fn hermite_overshoots_without_clamping() {
    // h10(0.5) * 4 + h11(0.5) * -4 = 0.125 * 4 + -0.125 * -4.
    let k = [key(0.0, 0.0, 0.0, 4.0), key(1.0, 0.0, -4.0, 0.0)];
    close(eval(&k, 0.5), 1.0);
}

#[test]
fn clamps_outside_the_keys() {
    let k = [key(1.0, 5.0, 1.0, 1.0), key(2.0, 6.0, 1.0, 1.0)];
    assert_eq!(eval(&k, -10.0), 5.0);
    assert_eq!(eval(&k, 1.0), 5.0);
    assert_eq!(eval(&k, 2.0), 6.0);
    assert_eq!(eval(&k, 100.0), 6.0);
    assert_eq!(eval(&k, f32::NAN), 5.0);
}

#[test]
fn single_key_is_constant_and_empty_is_zero() {
    let k = [key(0.5, 3.0, 9.0, 9.0)];
    for t in [-1.0, 0.5, 2.0] {
        assert_eq!(eval(&k, t), 3.0);
    }
    assert_eq!(eval(&[], 1.0), 0.0);
}

#[test]
fn a_non_finite_tangent_on_either_side_makes_the_segment_constant() {
    for (out0, in1) in [(INF, 1.0), (1.0, INF), (-INF, 1.0), (1.0, -INF), (f32::NAN, 1.0)] {
        let k = [key(0.0, 1.0, 0.0, out0), key(1.0, 2.0, in1, 0.0)];
        assert_eq!(eval(&k, 0.0), 1.0);
        assert_eq!(eval(&k, 0.999), 1.0, "out {out0} in {in1}");
        assert_eq!(eval(&k, 1.0), 2.0);
    }
}

#[test]
fn constant_segment_does_not_affect_neighbours() {
    // Segment 0..1 stepped, 1..2 linear.
    let k = [
        key(0.0, 0.0, 0.0, INF),
        key(1.0, 1.0, INF, 1.0),
        key(2.0, 2.0, 1.0, 0.0),
    ];
    assert_eq!(eval(&k, 0.5), 0.0);
    close(eval(&k, 1.5), 1.5);
}

#[test]
fn key_times_pick_the_segment_starting_there() {
    let k: Vec<Key> = (0..100).map(|i| key(i as f32, (i * i) as f32, 0.0, 0.0)).collect();
    for i in 0..100 {
        assert_eq!(eval(&k, i as f32), (i * i) as f32);
    }
    // Midpoint of segment 41..42 with flat tangents is the average.
    close(eval(&k, 41.5), (41.0 * 41.0 + 42.0 * 42.0) / 2.0);
}
