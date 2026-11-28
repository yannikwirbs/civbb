//! The character controller (ADR 0110): a vertical capsule moved against static world triangles with gravity,
//! jumps, walkable slopes, steps and sliding along walls; Jolt's `CharacterVirtual` takes its place with Jolt.

use bevy_ecs::prelude::*;
use civbb_core::math::{Pose, add, cross, dot, length, scale, sub, world_poses};
use civbb_format::Document;
use civbb_format::math::Vec3;

/// The capsule's radius in metres.
pub const RADIUS: f32 = 0.25;
/// The standing capsule's height in metres, from the feet (the body's position) to the top.
pub const HEIGHT: f32 = 1.6;
/// The highest step climbed, in metres.
pub const STEP: f32 = 0.3;
/// Upward speed a jump starts with, in m/s.
pub const JUMP: f32 = 3.0;
/// Downward acceleration in m/s².
pub const GRAVITY: f32 = 9.81;
/// Cosine of the steepest walkable slope, 45°.
const WALKABLE: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Distance within which a walkable surface counts as ground.
const SKIN: f32 = 0.01;
/// Resolution passes per substep.
const PASSES: usize = 4;
/// Most substeps per move.
const MAX_SUBSTEPS: f32 = 64.0;

/// The world's static triangles.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct WorldMesh {
    /// Triangles in world space.
    pub triangles: Vec<[Vec3; 3]>,
}

impl WorldMesh {
    /// A flat ground at y 0, 1 km on a side.
    pub fn plane() -> Self {
        let (a, b, c, d) = (
            [-500.0, 0.0, -500.0],
            [-500.0, 0.0, 500.0],
            [500.0, 0.0, 500.0],
            [500.0, 0.0, -500.0],
        );
        Self {
            triangles: vec![[a, b, c], [a, c, d]],
        }
    }

    /// The triangles of `document`'s rigid meshes, placed at `origin` by its rest pose; skinned meshes are left out.
    pub fn from_document(document: &Document, origin: Pose) -> Self {
        let nodes = &document.hierarchy.nodes;
        let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
        let rest: Vec<_> = nodes.iter().map(|n| n.rest).collect();
        let world = world_poses(&parents, origin, &rest);
        let mut triangles = Vec::new();
        for m in document.meshes.iter().filter(|m| m.skin.is_none()) {
            let place = world[m.node as usize];
            for t in m.indices.as_chunks::<3>().0 {
                triangles.push(t.map(|i| place.point(m.positions[i as usize])));
            }
        }
        Self { triangles }
    }

    /// Moves a capsule standing at `pos` by `delta` and pushes it out of the triangles; returns where it ends and what
    /// it touched. Edges up to a step above `floor`, the feet's height before the move, are stood on. The capsule is
    /// `height` high; it, its radius and the step are `size` times that and their constants.
    fn slide(&self, mut pos: Vec3, delta: Vec3, floor: f32, height: f32, size: f32) -> (Vec3, Touch) {
        let (radius, height, step_height) = (RADIUS * size, height * size, STEP * size);
        let n = (length(delta) / (radius * 0.5)).ceil().clamp(1.0, MAX_SUBSTEPS);
        let step = scale(delta, 1.0 / n);
        let mut touch = Touch::default();
        for _ in 0..n as usize {
            pos = add(pos, step);
            for _ in 0..PASSES {
                let mut pushed = false;
                for t in &self.triangles {
                    let (a, b) = (
                        [pos[0], pos[1] + radius, pos[2]],
                        [pos[0], pos[1] + height - radius, pos[2]],
                    );
                    let (on_axis, on_t) = closest_segment_triangle(a, b, t);
                    let away = sub(on_axis, on_t);
                    let d = length(away);
                    if d >= radius + SKIN {
                        continue;
                    }
                    let normal = if d > 1e-6 {
                        scale(away, 1.0 / d)
                    } else {
                        // The axis crosses the triangle: out along the face, to the side the capsule's middle is on.
                        let n = face_normal(t);
                        let mid = scale(add(a, b), 0.5);
                        if dot(n, sub(mid, t[0])) < 0.0 {
                            scale(n, -1.0)
                        } else {
                            n
                        }
                    };
                    let depth = radius - d;
                    // Touching an edge or corner, not a face: a low one below the feet's sphere is a step's edge,
                    // stood on like ground whatever the faces meeting there are.
                    let edge = dot(face_normal(t), normal).abs() < 0.999;
                    let step = edge && normal[1] > 0.05 && on_t[1] <= floor + step_height;
                    if normal[1] >= WALKABLE || step {
                        touch.ground = true;
                        if depth > 0.0 {
                            // Straight up until the sphere rests on the edge or the face.
                            pos[1] += if edge {
                                let h = (on_axis[0] - on_t[0]).hypot(on_axis[2] - on_t[2]);
                                on_t[1] + (radius * radius - h * h).max(0.0).sqrt() - on_axis[1]
                            } else {
                                depth / normal[1]
                            };
                            pushed = true;
                        }
                    } else if depth > 0.0 {
                        touch.blocked |= normal[1] > -WALKABLE;
                        touch.ceiling |= normal[1] < 0.0;
                        pos = add(pos, scale(normal, depth));
                        pushed = true;
                    }
                }
                if !pushed {
                    break;
                }
            }
        }
        (pos, touch)
    }

    /// Whether a capsule `height` high standing at `pos`, `size` times its height and [`RADIUS`], overlaps no
    /// triangle by more than the ground skin: where a body may stand up to that height.
    pub fn fits(&self, pos: Vec3, height: f32, size: f32) -> bool {
        let radius = RADIUS * size;
        let (a, b) = (
            [pos[0], pos[1] + radius, pos[2]],
            [pos[0], pos[1] + height * size - radius, pos[2]],
        );
        self.triangles.iter().all(|t| {
            let (on_axis, on_t) = closest_segment_triangle(a, b, t);
            length(sub(on_axis, on_t)) >= radius - SKIN
        })
    }
}

/// What a move touched.
#[derive(Clone, Copy, Debug, Default)]
struct Touch {
    /// Walkable ground within the skin.
    ground: bool,
    /// Something not walkable, a wall or a steep slope, in the way.
    blocked: bool,
    /// Something above.
    ceiling: bool,
}

/// A character: where it stands, its vertical speed, whether it is on the ground, its height and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// The feet: the capsule's bottom.
    pub pos: Vec3,
    /// Upward speed in m/s.
    pub vertical: f32,
    /// Standing on walkable ground.
    pub grounded: bool,
    /// The capsule's height in metres before `scale`: [`HEIGHT`] standing, lower crouching or prone; at least
    /// twice [`RADIUS`].
    pub height: f32,
    /// The capsule and the step are this many times `height`, [`RADIUS`] and [`STEP`]; the jump reaches this many
    /// times its height (ADR 0114).
    pub scale: f32,
}

impl Body {
    /// Moves by `walk` (m/s, horizontal) over `dt` seconds, jumping first if asked and on the ground, then falling
    /// and landing under gravity.
    pub fn step(&mut self, world: &WorldMesh, walk: Vec3, jump: bool, dt: f32) {
        let was_grounded = self.grounded;
        if self.grounded && jump {
            self.vertical = JUMP * self.scale.sqrt();
        } else if self.grounded {
            self.vertical = 0.0;
        }
        self.vertical -= GRAVITY * dt;
        let across = [walk[0] * dt, 0.0, walk[2] * dt];
        let (mut pos, touch) = world.slide(self.pos, across, self.pos[1], self.height, self.scale);
        if was_grounded && touch.blocked {
            // Step up: lifted, across, set down; kept if it gets further and lands on walkable ground.
            let (up, _) = world.slide(
                self.pos,
                [0.0, STEP * self.scale, 0.0],
                self.pos[1],
                self.height,
                self.scale,
            );
            let (over, _) = world.slide(up, across, self.pos[1], self.height, self.scale);
            let (down, landed) = world.slide(
                over,
                [0.0, self.pos[1] - up[1] - SKIN, 0.0],
                self.pos[1],
                self.height,
                self.scale,
            );
            let further = |p: Vec3| (p[0] - self.pos[0]).hypot(p[2] - self.pos[2]);
            if landed.ground && further(down) > further(pos) + 1e-4 {
                pos = down;
            }
        }
        let (mut pos, touch) = world.slide(
            pos,
            [0.0, self.vertical * dt, 0.0],
            self.pos[1],
            self.height,
            self.scale,
        );
        if touch.ceiling && self.vertical > 0.0 {
            self.vertical = 0.0;
        }
        self.grounded = touch.ground && self.vertical <= 0.0;
        if was_grounded && !jump && !self.grounded {
            // Down stairs and ramps: snapped to ground up to a step below, else falling.
            let (down, landed) = world.slide(
                pos,
                [0.0, -STEP * self.scale, 0.0],
                self.pos[1],
                self.height,
                self.scale,
            );
            if landed.ground {
                (pos, self.grounded) = (down, true);
            }
        }
        if self.grounded {
            self.vertical = 0.0;
        }
        self.pos = pos;
    }
}

fn face_normal(t: &[Vec3; 3]) -> Vec3 {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let l = length(n);
    if l > 0.0 { scale(n, 1.0 / l) } else { [0.0, 1.0, 0.0] }
}

/// The closest points of segment `a`-`b` and triangle `t`: on the segment, then on the triangle.
fn closest_segment_triangle(a: Vec3, b: Vec3, t: &[Vec3; 3]) -> (Vec3, Vec3) {
    // The segment crossing the triangle's plane inside it touches it there.
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let (da, db) = (dot(n, sub(a, t[0])), dot(n, sub(b, t[0])));
    if da * db < 0.0 {
        let p = add(a, scale(sub(b, a), da / (da - db)));
        if length(sub(closest_point_triangle(p, t), p)) < 1e-5 {
            return (p, p);
        }
    }
    let mut best = (a, closest_point_triangle(a, t));
    let mut consider = |pair: (Vec3, Vec3)| {
        if length(sub(pair.0, pair.1)) < length(sub(best.0, best.1)) {
            best = pair;
        }
    };
    consider((b, closest_point_triangle(b, t)));
    for k in 0..3 {
        consider(closest_segment_segment(a, b, t[k], t[(k + 1) % 3]));
    }
    best
}

/// The point of triangle `t` closest to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_point_triangle(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}

/// The closest points of segments `p1`-`q1` and `p2`-`q2` (Ericson 5.1.9).
fn closest_segment_segment(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    const EPS: f32 = 1e-12;
    let (s, t) = if a <= EPS && e <= EPS {
        (0.0, 0.0)
    } else if a <= EPS {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= EPS {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let s = if denom > EPS {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let t = (b * s + f) / e;
            if t < 0.0 {
                ((-c / a).clamp(0.0, 1.0), 0.0)
            } else if t > 1.0 {
                (((b - c) / a).clamp(0.0, 1.0), 1.0)
            } else {
                (s, t)
            }
        }
    };
    (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_points() {
        let t = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        // Above the inside, beyond an edge, beyond a corner.
        assert!(length(sub(closest_point_triangle([0.2, 1.0, 0.2], &t), [0.2, 0.0, 0.2])) < 1e-6);
        assert!(length(sub(closest_point_triangle([0.5, 1.0, -1.0], &t), [0.5, 0.0, 0.0])) < 1e-6);
        assert!(length(sub(closest_point_triangle([2.0, 1.0, -1.0], &t), [1.0, 0.0, 0.0])) < 1e-6);
        // A vertical segment through the triangle touches it; one beside it is closest at an edge.
        let (s, p) = closest_segment_triangle([0.2, -1.0, 0.2], [0.2, 1.0, 0.2], &t);
        assert!(length(sub(s, p)) < 1e-6 && (p[1]).abs() < 1e-6);
        let (s, p) = closest_segment_triangle([0.5, -1.0, -0.5], [0.5, 1.0, -0.5], &t);
        assert!((length(sub(s, p)) - 0.5).abs() < 1e-5, "{s:?} {p:?}");
    }

    /// The plane and a step `height` high from z 1 on.
    fn step_world(height: f32) -> WorldMesh {
        let mut w = WorldMesh::plane();
        let (y, x) = (height, 1.0);
        w.triangles.extend([
            [[-x, y, 1.0], [-x, y, 3.0], [x, y, 3.0]],
            [[-x, y, 1.0], [x, y, 3.0], [x, y, 1.0]],
            [[-x, 0.0, 1.0], [-x, y, 1.0], [x, y, 1.0]],
            [[-x, 0.0, 1.0], [x, y, 1.0], [x, 0.0, 1.0]],
        ]);
        w
    }

    #[test]
    fn steps_above_the_feet_sphere_are_stepped_up_and_higher_ones_block() {
        for (height, climbed) in [(0.28, true), (0.35, false)] {
            let world = step_world(height);
            let mut body = Body {
                pos: [0.0; 3],
                vertical: 0.0,
                grounded: true,
                height: HEIGHT,
                scale: 1.0,
            };
            for _ in 0..60 {
                body.step(&world, [0.0, 0.0, 2.0], false, 1.0 / 60.0);
                assert!(body.grounded, "{height}: {body:?}");
            }
            let on_top = (body.pos[1] - height).abs() < 1e-4 && body.pos[2] > 1.9;
            let blocked = body.pos[1].abs() < 1e-4 && (body.pos[2] - (1.0 - RADIUS)).abs() < 1e-3;
            assert!(if climbed { on_top } else { blocked }, "{height}: {body:?}");
        }
    }

    #[test]
    fn stands_on_the_plane_and_lands_after_a_jump() {
        let world = WorldMesh::plane();
        let mut body = Body {
            pos: [0.0; 3],
            vertical: 0.0,
            grounded: true,
            height: HEIGHT,
            scale: 1.0,
        };
        body.step(&world, [0.0; 3], false, 1.0 / 60.0);
        assert!(body.grounded && body.pos[1].abs() < 1e-5, "{body:?}");
        body.step(&world, [0.0; 3], true, 1.0 / 60.0);
        assert!(!body.grounded && body.pos[1] > 0.0);
        for _ in 0..60 {
            body.step(&world, [0.0; 3], false, 1.0 / 60.0);
        }
        assert!(body.grounded && body.pos[1].abs() < 1e-5, "{body:?}");
    }
}
