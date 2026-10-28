//! Vector and quaternion helpers for the solver; they move to `civbb-core` with #22.

use civbb_format::math::{Quat, Transform, Vec3};

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

/// Unit quaternion; zero becomes the identity.
pub fn qnormalize(q: Quat) -> Quat {
    let l = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if l > 1e-12 {
        q.map(|c| c / l)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// `v` rotated by unit quaternion `q`.
pub fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = [q[0], q[1], q[2]];
    let t = scale(cross(u, v), 2.0);
    add(add(v, scale(t, q[3])), cross(u, t))
}

/// The shortest rotation taking direction `a` to direction `b` (both unit; zero gives the identity).
pub fn from_to(a: Vec3, b: Vec3) -> Quat {
    let d = dot(a, b);
    if d < -0.999_999 {
        // Opposite: half a turn about any axis perpendicular to `a`.
        let axis = normalize(if a[0].abs() < 0.9 {
            cross(a, [1.0, 0.0, 0.0])
        } else {
            cross(a, [0.0, 1.0, 0.0])
        });
        return [axis[0], axis[1], axis[2], 0.0];
    }
    let c = cross(a, b);
    qnormalize([c[0], c[1], c[2], 1.0 + d])
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
        rot: [0.0, 0.0, 0.0, 1.0],
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
