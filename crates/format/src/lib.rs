//! CIVBB content format: the document model and signed, content-addressed runtime packages (design doc §9).

pub mod animator;
pub mod avatar;
pub mod blob;
pub mod cbor;
pub mod constraint;
pub mod cost;
pub mod document;
pub mod dynamics;
pub mod expressions;
pub mod face;
pub mod hierarchy;
pub mod material;
pub mod math;
pub mod mesh;
pub mod meta;
pub mod package;
pub mod state;
pub mod validate;

pub use avatar::Avatar;
pub use blob::BlobId;
pub use document::{Document, Role};
pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use package::{Package, ReadLimits};
pub use validate::{Caps, validate};

/// Version of the runtime package's data model. Bump on any change to the types in this crate; readers refuse
/// packages of any other version, which are rebuilt from their kept source (ADR 0087).
pub const FORMAT_VERSION: u32 = 35;

#[cfg(test)]
pub(crate) mod testutil {
    use crate::animator::*;
    use crate::constraint::*;
    use crate::dynamics::*;
    use crate::expressions::*;
    use crate::face::*;
    use crate::hierarchy::{HumanBone, Humanoid, MuscleFrame, Node};
    use crate::material::*;
    use crate::math::Transform;
    use crate::mesh::*;
    use crate::state::{Entry, EntryKind, Range, StateTable, SyncMode};
    use crate::*;

    /// A tiny skinned, morphing, textured avatar touching every type.
    pub fn sample_avatar() -> Document {
        let node = |name: &str, parent| Node {
            name: name.into(),
            parent,
            rest: Transform::IDENTITY,
            active: true,
        };
        Document {
            name: "sample".into(),
            hierarchy: hierarchy::Hierarchy {
                nodes: vec![node("Armature", None), node("Hips", Some(0))],
            },
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
                    shade_color: [0.8, 0.7, 0.7],
                    shade_tex: None,
                    shade_shift: 0.0,
                    shade_toony: 0.9,
                    light_clamp: Some(LightClamp { min: 0.0, max: 1.0 }),
                    normal: None,
                    emission: None,
                    matcaps: Vec::new(),
                    rim: Some(Rim {
                        color: [0.0; 3],
                        power: 2.0,
                        border: 0.5,
                        blur: 1.0,
                        shadow_mask: 0.0,
                        main_strength: 0.0,
                    }),
                    rim_shade: None,
                    shade: None,
                    color_adjust: None,
                    outline: Some(Outline {
                        width: 0.001,
                        color: [0.0; 3],
                        width_tex: None,
                        tex: None,
                    }),
                }),
                alpha: AlphaMode::Opaque,
                double_sided: false,
                order: 0,
            }],
            textures: vec![Texture {
                name: "Skin_D".into(),
                format: ImageFormat::Png,
                color_space: ColorSpace::Srgb,
                blob: BlobId([0; 32]),
            }],
            state: StateTable {
                entries: vec![
                    Entry {
                        name: "Toggle".into(),
                        kind: EntryKind::Bool,
                        default: 0.0,
                        range: None,
                        rest: None,
                        sync: SyncMode::Owner,
                        saved: true,
                        external: true,
                    },
                    Entry {
                        name: "Boop".into(),
                        kind: EntryKind::Float,
                        default: 0.0,
                        range: Some(Range::Float {
                            min: 0.0,
                            max: 1.0,
                            bits: 8,
                        }),
                        rest: None,
                        sync: SyncMode::None,
                        saved: false,
                        external: false,
                    },
                ],
            },
            animator: Some(sample_animator()),
            clips: vec![Clip {
                name: "Toggle".into(),
                length: 1.0 / 60.0,
                looping: false,
                root: None,
                curves: vec![Curve {
                    target: Target::Mesh {
                        mesh: 0,
                        property: MeshProperty::Weight(0),
                    },
                    keys: vec![Key {
                        time: 0.0,
                        value: 1.0,
                        in_tangent: 0.0,
                        out_tangent: 0.0,
                    }],
                }],
            }],
            dynamics: Dynamics {
                chains: vec![Chain {
                    root: 1,
                    ignore: vec![],
                    endpoint: [0.0, 0.1, 0.0],
                    multi_child: MultiChild::Ignore,
                    version: Version::V1_1,
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
                    outputs: ChainOutputs {
                        angle: Some(1),
                        ..ChainOutputs::default()
                    },
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
                        component: None,
                        shape: ContactShape::Sphere { radius: 0.1 },
                        position: [0.0; 3],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                    },
                    tags: vec!["Hand".into()],
                    parameter: Some(1),
                    kind: ReceiverKind::OnEnter,
                    allow_self: false,
                    allow_others: true,
                    local_only: true,
                    min_velocity: 0.05,
                    face: false,
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
                options: Default::default(),
            }],
            role: Role::Avatar(Avatar {
                humanoid: Some(Humanoid {
                    bones: vec![(HumanBone::Hips, 1)],
                    human_scale: 1.0,
                    center_of_mass: None,
                    muscle_frames: vec![MuscleFrame {
                        bone: HumanBone::Hips,
                        pre: [0.0, 0.0, 0.0, 1.0],
                        post: [0.0, 0.0, 0.0, 1.0],
                        sign: [1.0; 3],
                        min: [0.0; 3],
                        max: [0.0; 3],
                        twist: 1.0,
                    }],
                }),
                viewpoint: [0.0, 1.5, 0.1],
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
                expressions: Some(Expressions {
                    menu: Menu {
                        controls: vec![Control {
                            name: "Toggle".into(),
                            icon: None,
                            kind: ControlKind::Toggle,
                            parameter: Some(0),
                            value: 1.0,
                            sub_parameters: vec![],
                        }],
                    },
                }),
                meta: Some(meta::Meta {
                    authors: vec!["CIVBB".into()],
                    ..meta::Meta::default()
                }),
            }),
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
            unwritten: Unwritten::Hold,
            mirror: false,
            mirror_param: None,
            transitions: vec![],
            actions: vec![],
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
                    mirror: false,
                    cycle_offset: 0.0,
                }],
                normalize: false,
            }))),
        );
        on.actions.push(Action::Entries {
            owner_only: true,
            ops: vec![EntryOp::Copy {
                src: 0,
                dst: 0,
                remap: Some(([0.0, 1.0], [1.0, 0.0])),
            }],
        });
        Animator {
            groups: vec![LayerGroup {
                name: "FX".into(),
                weight: 1.0,
            }],
            layers: vec![Layer {
                name: "Toggle".into(),
                group: 0,
                weight: 1.0,
                blending: LayerBlending::Override,
                mask: Some(Mask {
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
        let back: Document = cbor::from_slice(&cbor::to_vec(&a), usize::MAX).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn decode_rejects_oversized_input() {
        let bytes = cbor::to_vec(&sample_avatar());
        assert!(cbor::from_slice::<Document>(&bytes, bytes.len() - 1).is_err());
    }
}
