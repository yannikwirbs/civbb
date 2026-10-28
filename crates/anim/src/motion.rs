//! What a state plays: its motion's clips with weights and sample times, its length and whether it loops.
//!
//! Blend tree weights follow ADR 0009: 1D linear between neighbouring thresholds, 2D simple directional by
//! barycentric weights with the centre, freeform cartesian and directional by gradient bands (Johansen 2009,
//! §6.3; polar with α = 2), direct from parameters.

use civbb_format::animator::{BlendTree, BlendTreeKind, ClipIndex, Motion};

use crate::clip::BoundClip;

/// One clip a motion plays, after blend tree weighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    /// Clip.
    pub clip: ClipIndex,
    /// Seconds into the clip.
    pub time: f32,
    /// Weight within the motion.
    pub weight: f32,
}

/// Seconds a motion takes at speed 1: blend trees average their clips' lengths (divided by time scale) by
/// weight. Empty states and zero-length clips count as 1 s (ADR 0008).
pub fn length(motion: Option<&Motion>, clips: &[BoundClip], params: &[f32]) -> f32 {
    let (mut sum, mut weight) = (0.0, 0.0);
    if let Some(m) = motion {
        weighted_length(m, clips, params, 1.0, 1.0, &mut sum, &mut weight);
    }
    if weight > 0.0 && sum > 0.0 { sum / weight } else { 1.0 }
}

fn weighted_length(
    m: &Motion,
    clips: &[BoundClip],
    params: &[f32],
    w: f32,
    scale: f32,
    sum: &mut f32,
    weight: &mut f32,
) {
    match m {
        Motion::Clip(c) => {
            let len = clips[*c as usize].length;
            *sum += w * if len > 0.0 { len } else { 1.0 } / scale;
            *weight += w;
        }
        Motion::BlendTree(tree) => {
            for (child, cw) in tree.children.iter().zip(weights(tree, params)) {
                let s = scale * child.time_scale.abs();
                if cw > 0.0 && s > 0.0 {
                    weighted_length(&child.motion, clips, params, w * cw, s, sum, weight);
                }
            }
        }
    }
}

/// Whether a motion loops (any of its clips does), which repeats exit times below 1 every cycle.
pub fn loops(motion: Option<&Motion>, clips: &[BoundClip]) -> bool {
    match motion {
        Some(Motion::Clip(c)) => clips[*c as usize].looping,
        Some(Motion::BlendTree(t)) => t.children.iter().any(|c| loops(Some(&c.motion), clips)),
        None => false,
    }
}

/// Appends the clips `motion` plays at `normalized` time (cycle offset included) with weights above 0.
/// Blend tree children share the normalised time, shifted by their cycle offsets.
pub fn leaves(motion: &Motion, clips: &[BoundClip], params: &[f32], normalized: f32, out: &mut Vec<Leaf>) {
    collect(motion, clips, params, normalized, 1.0, out);
}

fn collect(m: &Motion, clips: &[BoundClip], params: &[f32], normalized: f32, w: f32, out: &mut Vec<Leaf>) {
    match m {
        Motion::Clip(c) => out.push(Leaf {
            clip: *c,
            time: clip_time(&clips[*c as usize], normalized),
            weight: w,
        }),
        Motion::BlendTree(tree) => {
            for (child, cw) in tree.children.iter().zip(weights(tree, params)) {
                if cw > 0.0 {
                    collect(
                        &child.motion,
                        clips,
                        params,
                        normalized + child.cycle_offset,
                        w * cw,
                        out,
                    );
                }
            }
        }
    }
}

/// Seconds into `clip` at a normalised time: wrapped for looping clips, clamped otherwise.
pub fn clip_time(clip: &BoundClip, normalized: f32) -> f32 {
    let n = if clip.looping {
        normalized - normalized.floor()
    } else {
        normalized.clamp(0.0, 1.0)
    };
    n * clip.length
}

/// Child weights of a blend tree for the current parameters, in child order.
pub fn weights(tree: &BlendTree, params: &[f32]) -> Vec<f32> {
    let p = |i: u32| params[i as usize];
    let points = || tree.children.iter().map(|c| c.position).collect::<Vec<_>>();
    match tree.kind {
        BlendTreeKind::Simple1D { param } => {
            let th: Vec<f32> = tree.children.iter().map(|c| c.threshold).collect();
            simple_1d(&th, p(param))
        }
        BlendTreeKind::SimpleDirectional2D { x, y } => simple_directional(&points(), [p(x), p(y)]),
        BlendTreeKind::FreeformDirectional2D { x, y } => gradient_bands(&points(), [p(x), p(y)], true),
        BlendTreeKind::FreeformCartesian2D { x, y } => gradient_bands(&points(), [p(x), p(y)], false),
        BlendTreeKind::Direct => {
            let w: Vec<f32> = tree.children.iter().map(|c| c.direct_param.map_or(0.0, p)).collect();
            let sum: f32 = w.iter().filter(|w| **w > 0.0).sum();
            if tree.normalize && sum > 0.0 {
                w.iter().map(|w| w / sum).collect()
            } else {
                w
            }
        }
    }
}

/// Linear between the two thresholds around `v`, clamped to the end children. Thresholds need not be sorted.
pub fn simple_1d(thresholds: &[f32], v: f32) -> Vec<f32> {
    let mut w = vec![0.0; thresholds.len()];
    let mut order: Vec<usize> = (0..thresholds.len()).collect();
    order.sort_by(|&a, &b| thresholds[a].total_cmp(&thresholds[b]));
    let (Some(&first), Some(&last)) = (order.first(), order.last()) else {
        return w;
    };
    if v <= thresholds[first] || v.is_nan() {
        w[first] = 1.0;
    } else if v >= thresholds[last] {
        w[last] = 1.0;
    } else {
        let k = order.partition_point(|&i| thresholds[i] <= v);
        let (a, b) = (order[k - 1], order[k]);
        let t = (v - thresholds[a]) / (thresholds[b] - thresholds[a]);
        w[a] = 1.0 - t;
        w[b] = t;
    }
    w
}

/// Barycentric weights of the two directions around `p` and the centre child (at the origin): inside their
/// triangle the centre takes the rest, outside the two directions are normalised (ADR 0009).
pub fn simple_directional(points: &[[f32; 2]], p: [f32; 2]) -> Vec<f32> {
    let mut w = vec![0.0; points.len()];
    let centre = points.iter().position(|q| q[0] == 0.0 && q[1] == 0.0);
    let mut dirs: Vec<(f32, usize)> = (0..points.len())
        .filter(|&i| Some(i) != centre)
        .map(|i| (points[i][1].atan2(points[i][0]), i))
        .collect();
    dirs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at_origin = p[0] == 0.0 && p[1] == 0.0;
    match (centre, dirs.is_empty() || at_origin) {
        (Some(c), true) => {
            w[c] = 1.0;
            return w;
        }
        (None, true) => {
            let n = dirs.len().max(1) as f32;
            for &(_, i) in &dirs {
                w[i] = 1.0 / n;
            }
            return w;
        }
        _ => {}
    }
    // The directions just clockwise and counter-clockwise of p, wrapping around.
    let angle = p[1].atan2(p[0]);
    let k = dirs.partition_point(|d| d.0 <= angle);
    let a = dirs[(k + dirs.len() - 1) % dirs.len()].1;
    let b = dirs[k % dirs.len()].1;
    let (pa, pb) = (points[a], points[b]);
    let det = pa[0] * pb[1] - pa[1] * pb[0];
    let (mut wa, mut wb) = if a == b || det.abs() < 1e-9 {
        // One direction (or two opposite ones): project onto the nearer.
        let len2 = pa[0] * pa[0] + pa[1] * pa[1];
        (((p[0] * pa[0] + p[1] * pa[1]) / len2).max(0.0), 0.0)
    } else {
        ((p[0] * pb[1] - p[1] * pb[0]) / det, (pa[0] * p[1] - pa[1] * p[0]) / det)
    };
    wa = wa.max(0.0);
    wb = wb.max(0.0);
    let sum = wa + wb;
    if sum > 1.0 || (centre.is_none() && sum > 0.0) {
        wa /= sum;
        wb /= sum;
    }
    w[a] += wa;
    w[b] += wb;
    if let Some(c) = centre {
        w[c] = (1.0 - wa - wb).max(0.0);
    }
    w
}

/// Gradient band interpolation (Johansen 2009, eq. 6.6 and 6.7), in cartesian or polar space (eq. 6.14, 6.15).
pub fn gradient_bands(points: &[[f32; 2]], p: [f32; 2], polar: bool) -> Vec<f32> {
    const ALPHA: f32 = 2.0;
    let len = |v: [f32; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    // Signed angle from `from` to `to`; 0 when either is the origin (an origin sample takes its partner's direction).
    let angle = |from: [f32; 2], to: [f32; 2]| {
        let cross = from[0] * to[1] - from[1] * to[0];
        let dot = from[0] * to[0] + from[1] * to[1];
        if len(from) == 0.0 || len(to) == 0.0 {
            0.0
        } else {
            cross.atan2(dot)
        }
    };
    let mut h: Vec<f32> = (0..points.len())
        .map(|i| {
            let pi = points[i];
            let mut best = 1.0f32;
            for (j, &pj) in points.iter().enumerate() {
                if j == i {
                    continue;
                }
                let (ij, ip) = if polar {
                    let mean = (len(pi) + len(pj)) / 2.0;
                    // An origin sample has no direction: it borrows its partner's, so its bands are radial.
                    let dir_i = if len(pi) == 0.0 { pj } else { pi };
                    let ij = [(len(pj) - len(pi)) / mean, angle(dir_i, pj) * ALPHA];
                    let ip = [(len(p) - len(pi)) / mean, angle(dir_i, p) * ALPHA];
                    (ij, ip)
                } else {
                    ([pj[0] - pi[0], pj[1] - pi[1]], [p[0] - pi[0], p[1] - pi[1]])
                };
                let len2 = ij[0] * ij[0] + ij[1] * ij[1];
                if len2 > 0.0 {
                    best = best.min(1.0 - (ip[0] * ij[0] + ip[1] * ij[1]) / len2);
                }
            }
            best.max(0.0)
        })
        .collect();
    let sum: f32 = h.iter().sum();
    if sum > 0.0 {
        for v in &mut h {
            *v /= sum;
        }
    }
    h
}
