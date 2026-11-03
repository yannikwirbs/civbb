//! Vector and quaternion helpers on the format's array types, and world placements of a node hierarchy.

use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::{Quat, Transform, Vec3};

/// The identity rotation.
pub const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

/// `a + b`.
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a - b`.
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a * s`.
pub fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|c| c * s)
}

/// Dot product.
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Length.
pub fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// Unit vector; zero stays zero.
pub fn normalize(a: Vec3) -> Vec3 {
    let l = length(a);
    if l > 1e-12 { scale(a, 1.0 / l) } else { [0.0; 3] }
}

/// Unit vector, or `None` when `a` is (nearly) zero or not finite.
pub fn unit(a: Vec3) -> Option<Vec3> {
    let l = length(a);
    (l > 1e-6 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

/// Cross product.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Hamilton product `a * b`.
pub fn mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Inverse of a unit quaternion.
pub fn conj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Unit quaternion; a zero or non-finite one becomes the identity, as Unity's `Quaternion.Normalize` does.
pub fn qnormalize(q: Quat) -> Quat {
    let len = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if len > f32::EPSILON && len.is_finite() {
        q.map(|c| c / len)
    } else {
        IDENTITY
    }
}

/// `v` rotated by unit quaternion `q`.
pub fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = [q[0], q[1], q[2]];
    let t = scale(cross(u, v), 2.0);
    add(add(v, scale(t, q[3])), cross(u, t))
}

/// The shortest rotation taking unit `a` to unit `b`.
pub fn from_to(a: Vec3, b: Vec3) -> Quat {
    let d = dot(a, b);
    if d < -0.999_999 {
        // Opposite: half a turn about any axis perpendicular to `a`.
        let side = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let axis = unit(cross(a, side)).unwrap_or([0.0, 0.0, 1.0]);
        return [axis[0], axis[1], axis[2], 0.0];
    }
    let c = cross(a, b);
    qnormalize([c[0], c[1], c[2], 1.0 + d])
}

/// Normalised lerp on the shorter arc.
pub fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let b = if a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>() < 0.0 {
        b.map(|x| -x)
    } else {
        b
    };
    qnormalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t))
}

/// Spherical lerp on the shorter arc.
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let d: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let (b, d) = if d < 0.0 { (b.map(|x| -x), -d) } else { (b, d) };
    if d > 0.9995 {
        return nlerp(a, b, t);
    }
    let angle = d.acos();
    let (wa, wb) = (((1.0 - t) * angle).sin(), (t * angle).sin());
    qnormalize(std::array::from_fn(|k| a[k] * wa + b[k] * wb))
}

/// `[x, y, z]` degrees to a rotation about Z, then X, then Y: `q = qy * qx * qz`, Unity's Euler order (ADR 0007).
pub fn euler_to_quat(deg: Vec3) -> Quat {
    let [(sx, cx), (sy, cy), (sz, cz)] = deg.map(|d| (d.to_radians() * 0.5).sin_cos());
    // qy * qx, then * qz.
    let (x, y, z, w) = (cy * sx, sy * cx, -sy * sx, cy * cx);
    [x * cz + y * sz, y * cz - x * sz, w * sz + z * cz, w * cz - z * sz]
}

/// The inverse of [`euler_to_quat`] for a unit quaternion: `[x, y, z]` degrees, x in -90..=90.
pub fn quat_to_euler(q: Quat) -> Vec3 {
    let [x, y, z, w] = q;
    // Rotation matrix entries of R = Ry * Rx * Rz.
    let r12 = 2.0 * (y * z - w * x);
    let r02 = 2.0 * (x * z + w * y);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let r10 = 2.0 * (x * y + w * z);
    let r11 = 1.0 - 2.0 * (x * x + z * z);
    let ax = (-r12).clamp(-1.0, 1.0).asin();
    let (ay, az) = if r12.abs() < 0.999_999 {
        (r02.atan2(r22), r10.atan2(r11))
    } else {
        // Gimbal lock: Y and Z rotate about the same axis; put it all on Y.
        let r00 = 1.0 - 2.0 * (y * y + z * z);
        let r20 = 2.0 * (x * z - w * y);
        ((-r20).atan2(r00), 0.0)
    };
    [ax, ay, az].map(f32::to_degrees)
}

/// `v` turned by `angle` radians about unit `axis`.
pub fn turn(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (s, c) = (angle * 0.5).sin_cos();
    rotate([axis[0] * s, axis[1] * s, axis[2] * s, c], v)
}

/// A world placement with uniform scale; non-uniform node scale is approximated by its X component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Position.
    pub pos: Vec3,
    /// Rotation.
    pub rot: Quat,
    /// Uniform scale.
    pub scale: f32,
}

impl Pose {
    /// The identity placement.
    pub const IDENTITY: Self = Self {
        pos: [0.0; 3],
        rot: IDENTITY,
        scale: 1.0,
    };

    /// This placement followed by a child's local transform.
    pub fn then(&self, local: &Transform) -> Self {
        Self {
            pos: add(self.pos, rotate(self.rot, scale(local.translation, self.scale))),
            rot: mul(self.rot, local.rotation),
            scale: self.scale * local.scale[0],
        }
    }

    /// A point given in this placement's space, in world space.
    pub fn point(&self, local: Vec3) -> Vec3 {
        add(self.pos, rotate(self.rot, scale(local, self.scale)))
    }
}

/// Every node's world placement from its local transform, parents first; `origin` places the root's parent.
pub fn world_poses(parents: &[Option<NodeIndex>], origin: Pose, locals: &[Transform]) -> Vec<Pose> {
    let mut world = Vec::with_capacity(locals.len());
    world_poses_into(parents, origin, locals, &mut world);
    world
}

/// [`world_poses`] into a reused buffer.
pub fn world_poses_into(parents: &[Option<NodeIndex>], origin: Pose, locals: &[Transform], world: &mut Vec<Pose>) {
    world.clear();
    for (i, l) in locals.iter().enumerate() {
        let parent = parents[i].map_or(origin, |p| world[p as usize]);
        world.push(parent.then(l));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn euler_round_trips_and_matches_the_axis_product() {
        let deg = [30.0, -50.0, 70.0];
        let q = euler_to_quat(deg);
        let axis = |v: Vec3, d: f32| {
            let (s, c) = (d.to_radians() * 0.5).sin_cos();
            [v[0] * s, v[1] * s, v[2] * s, c]
        };
        let product = mul(
            mul(axis([0.0, 1.0, 0.0], deg[1]), axis([1.0, 0.0, 0.0], deg[0])),
            axis([0.0, 0.0, 1.0], deg[2]),
        );
        assert!(close(&q, &product));
        assert!(close(&quat_to_euler(q), &deg));
    }

    #[test]
    fn from_to_turns_a_onto_b_including_opposites() {
        for (a, b) in [([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), ([0.0, 0.0, 1.0], [0.0, 0.0, -1.0])] {
            assert!(close(&rotate(from_to(a, b), a), &b));
        }
    }

    #[test]
    fn qnormalize_follows_unity_for_degenerate_input() {
        assert_eq!(qnormalize([0.0; 4]), IDENTITY);
        assert_eq!(qnormalize([f32::NAN, 0.0, 0.0, 1.0]), IDENTITY);
        assert!(close(&qnormalize([0.0, 0.0, 0.0, 2.0]), &IDENTITY));
    }

    #[test]
    fn slerp_halfway_is_half_the_angle() {
        let q = slerp(IDENTITY, euler_to_quat([0.0, 90.0, 0.0]), 0.5);
        assert!(close(&q, &euler_to_quat([0.0, 45.0, 0.0])));
    }

    #[test]
    fn world_poses_compose_parents_first() {
        let parents = [None, Some(0)];
        let t = Transform {
            translation: [0.0, 1.0, 0.0],
            rotation: euler_to_quat([0.0, 0.0, 90.0]),
            scale: [2.0; 3],
        };
        let w = world_poses(&parents, Pose::IDENTITY, &[t, t]);
        // The child's offset is turned by 90 degrees about Z and doubled: (0, 1) becomes (-2, 0).
        assert!(close(&w[1].pos, &[-2.0, 1.0, 0.0]));
        assert_eq!(w[1].scale, 4.0);
    }
}
