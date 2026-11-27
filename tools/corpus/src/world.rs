//! The test world (ADR 0110): ground, stairs and two ramps up to a platform, and a wall, as rigid meshes, CC0.
//!
//! Metres, +Y up; a player at the origin facing +Z looks at the stairs (x -4 to -2), the walkable 30° ramp (x 0 to
//! 2) and the too steep 60° ramp (x 2 to 4), which all end at the platform's front edge (z 6, 1 m high, to z 10).
//! The wall stands at x 6 to 6.2, z 0 to 8, 2 m high. Behind the player a bench faces it, with one station (ADR 0114).

use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{AlphaMode, Material, MaterialModel, Pbr};
use civbb_format::math::{Transform, Vec3};
use civbb_format::mesh::{Mesh, Submesh};
use civbb_format::{Avatar, Document, Role};

use crate::{cross, normalize, sub};

/// Height of the platform's top.
pub const PLATFORM: f32 = 1.0;
/// Height of each stair step.
pub const STEP: f32 = 0.2;
/// Depth of each stair step.
pub const TREAD: f32 = 0.3;
/// Where the stairs, the ramps and the platform meet.
pub const FRONT: f32 = 6.0;
/// Height of the bench's top.
pub const BENCH: f32 = 0.45;
/// The bench's station: where a seated avatar's origin goes, facing +Z, its hips over the bench.
pub const SEAT: Vec3 = [0.0, 0.0, -3.25];
/// Where a player leaving the bench stands.
pub const EXIT: Vec3 = [0.0, 0.0, -2.4];

/// Flat-shaded triangles of one material.
#[derive(Default)]
struct Part {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    indices: Vec<u32>,
}

impl Part {
    /// A planar polygon, corners counter-clockwise seen from the side it faces.
    fn face(&mut self, corners: &[Vec3]) {
        let n = normalize(cross(sub(corners[1], corners[0]), sub(corners[2], corners[0])));
        let first = self.positions.len() as u32;
        self.positions.extend_from_slice(corners);
        self.normals.extend(corners.iter().map(|_| n));
        for k in 1..corners.len() as u32 - 1 {
            self.indices.extend_from_slice(&[first, first + k, first + k + 1]);
        }
    }

    /// An axis-aligned box from `lo` to `hi` without its bottom.
    fn cuboid(&mut self, lo: Vec3, hi: Vec3) {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        self.face(&[[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]]);
        self.face(&[[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]]);
        self.face(&[[x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [x0, y0, z1]]);
        self.face(&[[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]]);
        self.face(&[[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]]);
    }

    /// A wedge over x `x0` to `x1` rising from the ground at z `z0` to `height` at z `z1`.
    fn ramp(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, height: f32) {
        self.face(&[[x0, 0.0, z0], [x0, height, z1], [x1, height, z1], [x1, 0.0, z0]]);
        self.face(&[[x0, 0.0, z0], [x0, 0.0, z1], [x0, height, z1]]);
        self.face(&[[x1, 0.0, z0], [x1, height, z1], [x1, 0.0, z1]]);
    }

    fn mesh(self, name: &str, material: u32) -> Mesh {
        Mesh {
            name: name.into(),
            node: 0,
            submeshes: vec![Submesh {
                first_index: 0,
                index_count: self.indices.len() as u32,
                material,
            }],
            positions: self.positions,
            normals: self.normals,
            indices: self.indices,
            ..Mesh::default()
        }
    }
}

fn pbr(name: &str, color: Vec3) -> Material {
    Material {
        name: name.into(),
        model: MaterialModel::Pbr(Pbr {
            base_color: [color[0], color[1], color[2], 1.0],
            base_color_tex: None,
            metallic: 0.0,
            roughness: 0.9,
            metallic_roughness_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            occlusion_tex: None,
            emissive: [0.0; 3],
            emissive_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        order: 0,
    }
}

/// Run of a ramp `PLATFORM` high at `degrees`.
fn run(degrees: f32) -> f32 {
    PLATFORM / degrees.to_radians().tan()
}

/// The test world. Same world on every platform.
pub fn world() -> Document {
    let mut ground = Part::default();
    ground.face(&[
        [-15.0, 0.0, -15.0],
        [-15.0, 0.0, 15.0],
        [15.0, 0.0, 15.0],
        [15.0, 0.0, -15.0],
    ]);
    let mut stone = Part::default();
    stone.cuboid([-4.0, 0.0, FRONT], [4.0, PLATFORM, 10.0]);
    let steps = (PLATFORM / STEP).round() as u32;
    for k in 1..steps {
        let depth = TREAD * (steps - k) as f32;
        stone.cuboid([-4.0, 0.0, FRONT - depth], [-2.0, STEP * k as f32, FRONT]);
    }
    let mut ramps = Part::default();
    ramps.ramp(0.0, 2.0, FRONT - run(30.0), FRONT, PLATFORM);
    let mut steep = Part::default();
    steep.ramp(2.0, 4.0, FRONT - run(60.0), FRONT, PLATFORM);
    let mut wall = Part::default();
    wall.cuboid([6.0, 0.0, 0.0], [6.2, 2.0, 8.0]);
    let mut bench = Part::default();
    bench.cuboid([-1.0, 0.0, -3.5], [1.0, BENCH, -3.05]);
    Document {
        name: "Test world".into(),
        hierarchy: Hierarchy {
            nodes: vec![Node {
                name: "Test world".into(),
                parent: None,
                rest: Transform::IDENTITY,
                active: true,
            }],
        },
        meshes: vec![
            ground.mesh("Ground", 0),
            stone.mesh("Stairs and platform", 1),
            ramps.mesh("Ramp", 2),
            steep.mesh("Steep ramp", 3),
            wall.mesh("Wall", 4),
            bench.mesh("Bench", 5),
        ],
        materials: vec![
            pbr("Ground", [0.35, 0.37, 0.35]),
            pbr("Stone", [0.7, 0.65, 0.55]),
            pbr("Ramp", [0.3, 0.45, 0.7]),
            pbr("Steep", [0.7, 0.3, 0.25]),
            pbr("Wall", [0.35, 0.6, 0.35]),
            pbr("Bench", [0.5, 0.33, 0.2]),
        ],
        // Until Phase 2 adds a world role (ADR 0110).
        role: Role::Avatar(Avatar::default()),
        ..Document::default()
    }
}

#[cfg(test)]
mod tests {
    use civbb_format::{Caps, validate};

    use super::*;

    #[test]
    fn validates_and_faces_outwards() {
        let w = world();
        validate(&w, &Caps::default(), |_| true).unwrap_or_else(|e| panic!("{e:?}"));
        // Every top face points up and the ground covers the origin.
        let ground = &w.meshes[0];
        assert!(ground.normals.iter().all(|n| n[1] > 0.99));
        let ramp = &w.meshes[2];
        assert!((ramp.normals[0][1] - 30f32.to_radians().cos()).abs() < 1e-5);
        assert!(ramp.normals[0][2] < 0.0);
    }
}
