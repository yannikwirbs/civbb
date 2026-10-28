//! CIVBB content format: the avatar data model and signed, content-addressed runtime packages (design doc §9).

pub mod animator;
pub mod avatar;
pub mod blob;
pub mod cbor;
pub mod constraint;
pub mod cost;
pub mod dynamics;
pub mod expressions;
pub mod face;
pub mod hierarchy;
pub mod material;
pub mod math;
pub mod mesh;
pub mod meta;
pub mod package;
pub mod validate;

pub use avatar::Avatar;
pub use blob::BlobId;
pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use package::{Package, ReadLimits};
pub use validate::{Caps, validate};

/// Version of the data model. Bump on any incompatible change to the types in this crate.
pub const FORMAT_VERSION: u32 = 3;

#[cfg(test)]
pub(crate) mod testutil {
    use crate::animator::*;
    use crate::constraint::*;
    use crate::dynamics::*;
    use crate::expressions::*;
    use crate::face::*;
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
            face: Some(Face {
                visemes: vec![(
                    Viseme::Aa,
                    vec![ShapeBind {
                        mesh: 0,
                        shape: 0,
                        weight: 1.0,
                    }],
                )],
                blink: vec![],
                eyes: Some(EyeLook {
                    driver: EyeDriver::Bones {
                        left: Some(1),
                        right: None,
                    },
                    horizontal_inner: RangeMap {
                        input_max: 90.0,
                        output_scale: 10.0,
                    },
                    horizontal_outer: RangeMap {
                        input_max: 90.0,
                        output_scale: 10.0,
                    },
                    vertical_down: RangeMap {
                        input_max: 90.0,
                        output_scale: 10.0,
                    },
                    vertical_up: RangeMap {
                        input_max: 90.0,
                        output_scale: 10.0,
                    },
                }),
            }),
            meta: Some(meta::Meta {
                authors: vec!["CIVBB".into()],
                ..meta::Meta::default()
            }),
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
            animator: Some(sample_animator()),
            clips: vec![Clip {
                name: "Toggle".into(),
                length: 1.0 / 60.0,
                looping: false,
                curves: vec![Curve {
                    binding: Binding {
                        path: "Body".into(),
                        property: Property::BlendShape("vrc.v_aa".into()),
                    },
                    keys: vec![Key {
                        time: 0.0,
                        value: 100.0,
                        in_tangent: 0.0,
                        out_tangent: 0.0,
                    }],
                }],
            }],
            expressions: Some(Expressions {
                parameters: vec![ExpressionParam {
                    name: "Toggle".into(),
                    kind: ExpressionParamKind::Bool,
                    default: 0.0,
                    saved: true,
                    synced: true,
                }],
                menu: Menu {
                    controls: vec![Control {
                        name: "Toggle".into(),
                        icon: None,
                        kind: ControlKind::Toggle,
                        parameter: Some("Toggle".into()),
                        value: 1.0,
                        sub_parameters: vec![],
                    }],
                },
            }),
            dynamics: Dynamics {
                chains: vec![Chain {
                    root: 1,
                    ignore: vec![],
                    endpoint: [0.0, 0.1, 0.0],
                    multi_child: MultiChild::Ignore,
                    integration: Integration::Simplified,
                    pull: Graded::constant(0.2),
                    spring: Graded {
                        value: 0.2,
                        curve: vec![[0.0, 1.0], [1.0, 0.5]],
                    },
                    stiffness: Graded::constant(0.2),
                    gravity: Graded::constant(0.0),
                    gravity_falloff: Graded::constant(0.0),
                    immobile: Graded::constant(0.0),
                    immobile_type: ImmobileType::AllMotion,
                    limit: Limit::Angle {
                        max_angle: Graded::constant(45.0),
                        rotation: [0.0; 3],
                    },
                    radius: Graded::constant(0.02),
                    colliders: vec![0],
                    max_stretch: Graded::constant(0.0),
                    max_squish: Graded::constant(0.0),
                    allow_grab: true,
                    allow_posing: true,
                    parameter: Some("Tail".into()),
                }],
                colliders: vec![Collider {
                    node: 1,
                    shape: Shape::Capsule {
                        radius: 0.1,
                        height: 0.4,
                    },
                    position: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    inside: false,
                }],
            },
            contacts: Contacts {
                senders: vec![],
                receivers: vec![ContactReceiver {
                    volume: ContactVolume {
                        node: 1,
                        shape: Shape::Sphere { radius: 0.1 },
                        position: [0.0; 3],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                    },
                    tags: vec!["Hand".into()],
                    parameter: "Boop".into(),
                    kind: ReceiverKind::OnEnter,
                    allow_self: false,
                    allow_others: true,
                    local_only: true,
                    min_velocity: 0.05,
                }],
            },
            constraints: vec![Constraint {
                kind: ConstraintKind::Rotation { offset: [0.0; 3] },
                target: 1,
                sources: vec![ConstraintSource {
                    node: 0,
                    weight: 1.0,
                    offset: Transform::IDENTITY,
                }],
                weight: 1.0,
                active: true,
                axes: [true; 3],
                rest: Transform::IDENTITY,
            }],
        }
    }

    fn sample_animator() -> Animator {
        let transition = Transition {
            dest: TransitionDest::State(1),
            conditions: vec![Condition {
                param: 0,
                mode: ConditionMode::If,
                threshold: 0.0,
            }],
            has_exit_time: false,
            exit_time: 0.0,
            fixed_duration: true,
            duration: 0.0,
            offset: 0.0,
            interruption: InterruptionSource::None,
            ordered_interruption: true,
            can_transition_to_self: false,
        };
        let state = |name: &str, motion| State {
            name: name.into(),
            motion,
            speed: 1.0,
            speed_param: None,
            cycle_offset: 0.0,
            cycle_offset_param: None,
            time_param: None,
            write_defaults: false,
            transitions: vec![],
            behaviours: vec![],
        };
        let mut off = state("Off", None);
        off.transitions.push(transition);
        let mut on = state(
            "On",
            Some(Motion::BlendTree(Box::new(BlendTree {
                kind: BlendTreeKind::Direct,
                children: vec![ChildMotion {
                    motion: Motion::Clip(0),
                    threshold: 0.0,
                    position: [0.0; 2],
                    direct_param: Some(0),
                    time_scale: 1.0,
                    cycle_offset: 0.0,
                }],
                normalize: false,
            }))),
        );
        on.behaviours.push(Behaviour::ParameterDriver {
            local_only: true,
            ops: vec![DriverOp::Copy {
                src: 0,
                dst: 0,
                remap: Some(([0.0, 1.0], [1.0, 0.0])),
            }],
        });
        Animator {
            parameters: vec![Parameter {
                name: "Toggle".into(),
                kind: ParamKind::Bool,
                default: 0.0,
            }],
            groups: vec![LayerGroup {
                name: "FX".into(),
                weight: 1.0,
            }],
            layers: vec![Layer {
                name: "Toggle".into(),
                group: 0,
                weight: 1.0,
                blending: LayerBlending::Override,
                mask: Some(AvatarMask {
                    body_parts: vec![BodyPart::Head],
                    transforms: None,
                }),
                state_machine: StateMachine {
                    states: vec![off, on],
                    default_state: 0,
                    entry_transitions: vec![],
                    any_state_transitions: vec![],
                },
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
