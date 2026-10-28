//! Evaluates a curve of Hermite keys with Unity `AnimationCurve` semantics (ADR 0007).

use civbb_format::animator::Key;

/// The curve's value at `t` seconds.
///
/// - Before the first key and after the last the curve holds the end key's value (clamp, as in clips).
/// - Between two keys it is a cubic Hermite with tangents scaled by the segment length.
/// - A segment whose out tangent (left key) or in tangent (right key) is not finite holds the left key's value
///   (Unity "Constant" tangents, glTF `STEP`).
/// - An empty curve is 0, as `AnimationCurve.Evaluate` returns; the binder drops such curves anyway.
pub fn eval(keys: &[Key], t: f32) -> f32 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return 0.0;
    };
    if t <= first.time || t.is_nan() {
        return first.value;
    }
    if t >= last.time {
        return last.value;
    }
    // first.time < t < last.time, so the segment index is in 0..len - 1.
    let i = keys.partition_point(|k| k.time <= t) - 1;
    hermite(&keys[i], &keys[i + 1], t)
}

fn hermite(k0: &Key, k1: &Key, t: f32) -> f32 {
    let dt = k1.time - k0.time;
    let m0 = k0.out_tangent * dt;
    let m1 = k1.in_tangent * dt;
    if !(m0.is_finite() && m1.is_finite()) {
        return k0.value;
    }
    let s = (t - k0.time) / dt;
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    h00 * k0.value + h10 * m0 + h01 * k1.value + h11 * m1
}
