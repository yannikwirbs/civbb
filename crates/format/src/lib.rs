//! CIVBB content format: the avatar data model and signed, content-addressed runtime packages (design doc §9).

pub mod avatar;
pub mod blob;
pub mod cbor;
pub mod hierarchy;
pub mod material;
pub mod math;
pub mod mesh;
pub mod package;

pub use avatar::Avatar;
pub use blob::BlobId;
pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use package::{Package, ReadLimits};

/// Version of the data model. Bump on any incompatible change to the types in this crate.
pub const FORMAT_VERSION: u32 = 1;

#[cfg(test)]
pub(crate) mod testutil {
    use crate::hierarchy::{HumanBone, Humanoid, Node};
    use crate::material::*;
    use crate::math::Transform;
    use crate::mesh::*;
    use crate::*;

    /// A tiny skinned, morphing, textured avatar touching every type.
    pub fn sample_avatar() -> Avatar {
        let node = |name: &str, parent| Node {
            name: name.into(),
            parent,
            rest: Transform::IDENTITY,
            active: true,
        };
        Avatar {
            name: "sample".into(),
            hierarchy: hierarchy::Hierarchy {
                nodes: vec![node("Armature", None), node("Hips", Some(0))],
            },
            humanoid: Some(Humanoid {
                bones: vec![(HumanBone::Hips, 1)],
                viewpoint: [0.0, 1.5, 0.1],
            }),
            meshes: vec![Mesh {
                name: "Body".into(),
                node: 0,
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uv0: vec![[0.0; 2]; 3],
                skin: Some(Skin {
                    joints: vec![1],
                    inverse_bind: vec![[
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                    ]],
                    vertex_joints: vec![[0; 4]; 3],
                    vertex_weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
                }),
                indices: vec![0, 1, 2],
                submeshes: vec![Submesh {
                    first_index: 0,
                    index_count: 3,
                    material: 0,
                }],
                blend_shapes: vec![BlendShape {
                    name: "vrc.v_aa".into(),
                    default_weight: 0.0,
                    deltas: vec![ShapeDelta {
                        vertex: 2,
                        position: [0.0, 0.1, 0.0],
                        normal: [0.0; 3],
                    }],
                }],
                ..Mesh::default()
            }],
            materials: vec![Material {
                name: "Skin".into(),
                model: MaterialModel::Toon(Toon {
                    base_color: [1.0; 4],
                    base_color_tex: Some(TextureSlot {
                        texture: 0,
                        uv_set: 0,
                        scale_offset: [1.0, 1.0, 0.0, 0.0],
                    }),
                    normal_tex: None,
                    normal_scale: 1.0,
                    shade_color: [0.8, 0.7, 0.7],
                    shade_tex: None,
                    shade_shift: 0.0,
                    shade_toony: 0.9,
                    rim_color: [0.0; 3],
                    rim_power: 2.0,
                    matcap_tex: None,
                    matcap_strength: 0.0,
                    emissive: [0.0; 3],
                    emissive_tex: None,
                    outline_width: 0.001,
                    outline_color: [0.0; 3],
                    outline_width_tex: None,
                }),
                alpha: AlphaMode::Opaque,
                double_sided: false,
                render_queue: 2000,
            }],
            textures: vec![Texture {
                name: "Skin_D".into(),
                format: ImageFormat::Png,
                color_space: ColorSpace::Srgb,
                blob: BlobId([0; 32]),
            }],
        }
    }

    #[test]
    fn avatar_cbor_round_trip() {
        let a = sample_avatar();
        let back: Avatar = cbor::from_slice(&cbor::to_vec(&a), usize::MAX).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn decode_rejects_oversized_input() {
        let bytes = cbor::to_vec(&sample_avatar());
        assert!(cbor::from_slice::<Avatar>(&bytes, bytes.len() - 1).is_err());
    }
}
