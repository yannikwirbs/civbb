//! Behaviour as a layered state machine (ADR 0085): layers, state machines, blend trees, transitions, actions, clips.
//!
//! Parameters are entries of the document's state table (`crate::state`).
//!
//! Exporters flatten sub-state machines into one state list per layer and drop muted transitions, so the
//! runtime only sees states, entry transitions, any-state transitions and per-state transitions.

use serde::{Deserialize, Serialize};

use crate::hierarchy::{HumanBone, NodeIndex};
use crate::material::MaterialField;
use crate::mesh::MeshIndex;
use crate::state::EntryIndex;

/// Index into [`crate::Document::clips`].
pub type ClipIndex = u32;
/// Index into [`StateMachine::states`].
pub type StateIndex = u32;

/// An animator: one layer list in evaluation order, each layer tagged with its [`LayerGroup`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Animator {
    /// Layer groups, each weighted as a whole.
    pub groups: Vec<LayerGroup>,
    /// Layers, evaluated in order.
    pub layers: Vec<Layer>,
}

/// Prefix of the state entries the compiler adds (`civbb/One`, ADR 0028); no creator state toggles them.
pub const COMPILER_PARAMETER_PREFIX: &str = "civbb/";

/// A group of layers with a shared weight.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerGroup {
    /// Name, e.g. `FX`.
    pub name: String,
    /// Initial weight, 0 to 1.
    pub weight: f32,
}

/// One animator layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Name.
    pub name: String,
    /// Index into [`Animator::groups`].
    pub group: u32,
    /// Initial weight, 0 to 1. A group's first layer always plays at weight 1.
    pub weight: f32,
    /// How the layer combines with the layers below.
    pub blending: LayerBlending,
    /// Which bones and transforms the layer may write; `None` writes everything.
    pub mask: Option<Mask>,
    /// The layer's states.
    pub state_machine: StateMachine,
}

/// Layer blending modes.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerBlending {
    Override,
    Additive,
}

/// A layer mask: humanoid body parts plus an optional node allowlist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    /// Enabled humanoid body parts.
    pub body_parts: Vec<BodyPart>,
    /// Nodes the layer may animate (curves on their meshes and constraints too); `None` allows all.
    pub transforms: Option<Vec<NodeIndex>>,
}

/// Humanoid body parts of a layer mask.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyPart {
    Root,
    Body,
    Head,
    LeftLeg,
    RightLeg,
    LeftArm,
    RightArm,
    LeftFingers,
    RightFingers,
    LeftFootIk,
    RightFootIk,
    LeftHandIk,
    RightHandIk,
}

/// A flattened state machine.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateMachine {
    /// States.
    pub states: Vec<State>,
    /// State entered when no entry transition matches.
    pub default_state: StateIndex,
    /// Checked on entry (layer start and after an exit), in order; only conditions apply.
    pub entry_transitions: Vec<Transition>,
    /// Checked every update before the current state's transitions.
    pub any_state_transitions: Vec<Transition>,
}

/// A state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Name.
    pub name: String,
    /// What the state plays; `None` is an empty state.
    pub motion: Option<Motion>,
    /// Playback speed multiplier.
    pub speed: f32,
    /// Parameter multiplied into `speed`.
    pub speed_param: Option<EntryIndex>,
    /// Normalised start offset.
    pub cycle_offset: f32,
    /// Parameter replacing `cycle_offset`.
    pub cycle_offset_param: Option<EntryIndex>,
    /// Parameter driving normalised time directly.
    pub time_param: Option<EntryIndex>,
    /// What the state does to targets its layer animates and its motion does not.
    pub unwritten: Unwritten,
    /// Plays humanoid muscles mirrored left to right (ADR 0060).
    #[serde(default)]
    pub mirror: bool,
    /// Bool parameter replacing `mirror`.
    #[serde(default)]
    pub mirror_param: Option<EntryIndex>,
    /// Outgoing transitions, checked in order.
    pub transitions: Vec<Transition>,
    /// Actions run on enter, in order.
    pub actions: Vec<Action>,
}

/// What a state does to targets its layer animates and its motion does not (ADR 0085).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unwritten {
    /// They keep the value last written.
    Hold,
    /// They return to their rest values. A state without a motion writes nothing (ADR 0065).
    Rest,
}

/// What a state or blend tree child plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Motion {
    /// A clip.
    Clip(ClipIndex),
    /// A blend tree.
    BlendTree(Box<BlendTree>),
}

/// A blend tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlendTree {
    /// Blend algorithm and its parameters.
    pub kind: BlendTreeKind,
    /// Children.
    pub children: Vec<ChildMotion>,
    /// Normalise child weights for `Direct` trees.
    pub normalize: bool,
}

/// Blend algorithms.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlendTreeKind {
    Simple1D { param: EntryIndex },
    SimpleDirectional2D { x: EntryIndex, y: EntryIndex },
    FreeformDirectional2D { x: EntryIndex, y: EntryIndex },
    FreeformCartesian2D { x: EntryIndex, y: EntryIndex },
    Direct,
}

/// A child of a blend tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildMotion {
    /// The child's motion.
    pub motion: Motion,
    /// 1D threshold.
    pub threshold: f32,
    /// 2D position.
    pub position: [f32; 2],
    /// Weight parameter for `Direct` trees.
    pub direct_param: Option<EntryIndex>,
    /// Speed multiplier.
    pub time_scale: f32,
    /// Normalised time offset.
    pub cycle_offset: f32,
    /// Mirrors the child's humanoid muscles; combines with its state's and parents' mirrors by exclusive or.
    #[serde(default)]
    pub mirror: bool,
}

/// A transition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// Target.
    pub dest: TransitionDest,
    /// All must hold.
    pub conditions: Vec<Condition>,
    /// Only fire at or after `exit_time`.
    pub has_exit_time: bool,
    /// Normalised source time at which the transition may fire.
    pub exit_time: f32,
    /// `duration` in seconds when true, else normalised to the source state length.
    pub fixed_duration: bool,
    /// Cross-fade duration.
    pub duration: f32,
    /// Normalised start time in the destination.
    pub offset: f32,
    /// Which transitions may interrupt this one.
    pub interruption: InterruptionSource,
    /// Only higher-priority transitions of the source may interrupt.
    pub ordered_interruption: bool,
    /// Any-state transitions only: may re-enter the current state.
    pub can_transition_to_self: bool,
}

/// Transition targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionDest {
    /// A state of the same layer.
    State(StateIndex),
    /// Leave through the layer's exit, then re-run entry transitions.
    Exit,
}

/// One transition condition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    /// Tested parameter.
    pub param: EntryIndex,
    /// Comparison.
    pub mode: ConditionMode,
    /// Comparison value for numeric modes.
    pub threshold: f32,
}

/// Condition comparisons.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionMode {
    If,
    IfNot,
    Greater,
    Less,
    Equals,
    NotEqual,
}

/// Which transitions may interrupt a running transition.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptionSource {
    None,
    Source,
    Destination,
    SourceThenDestination,
    DestinationThenSource,
}

/// What a state does on enter (ADR 0085).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Changes state entries.
    Entries {
        /// Run only for the avatar's owner.
        owner_only: bool,
        /// Operations, in order.
        ops: Vec<EntryOp>,
    },
    /// Sets where body parts take their pose from.
    PoseSource(Vec<(PosePart, PoseSource)>),
    /// Blends a layer's weight.
    LayerWeight {
        /// Target layer.
        layer: u32,
        /// Goal weight.
        goal: f32,
        /// Blend time in seconds.
        duration: f32,
    },
    /// Blends a layer group's weight.
    GroupWeight {
        /// Target group.
        group: u32,
        /// Goal weight.
        goal: f32,
        /// Blend time in seconds.
        duration: f32,
    },
    /// Enables or disables locomotion.
    Locomotion {
        /// Locomotion disabled while true.
        disabled: bool,
    },
}

/// Operations on state entries.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EntryOp {
    Set {
        param: EntryIndex,
        value: f32,
    },
    Add {
        param: EntryIndex,
        value: f32,
    },
    /// Random in `[min, max]`; for bools, true with probability `chance`.
    Random {
        param: EntryIndex,
        min: f32,
        max: f32,
        chance: f32,
    },
    /// Copies `src` to `dst`, optionally remapping `from` range to `to` range.
    Copy {
        src: EntryIndex,
        dst: EntryIndex,
        remap: Option<([f32; 2], [f32; 2])>,
    },
}

/// Body parts with a pose source.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PosePart {
    Head,
    LeftHand,
    RightHand,
    Hip,
    LeftFoot,
    RightFoot,
    LeftFingers,
    RightFingers,
    Eyes,
    Mouth,
}

/// Where a body part takes its pose from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoseSource {
    /// The user's tracking (or the locomotion and IK standing in for it).
    Tracking,
    /// The animator.
    Animation,
}

/// An animation clip.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// Name.
    pub name: String,
    /// Length in seconds.
    pub length: f32,
    /// Loop time.
    pub looping: bool,
    /// How the body pose curves (`HumanoidProperty::Body`) reach the pose; `None` takes them as keyed (ADR 0063).
    #[serde(default)]
    pub root: Option<RootSettings>,
    /// Animated properties.
    pub curves: Vec<Curve>,
}

/// A humanoid clip's root-motion settings (ADR 0063). Avatars run with root motion
/// discarded, so what is not baked into the pose is lost: the body pose plays relative to a root that follows the
/// body (or, keeping the original, follows its change since the clip's start).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RootSettings {
    /// The body's yaw stays in the pose.
    pub bake_orientation: bool,
    /// The body's height stays in the pose.
    pub bake_position_y: bool,
    /// The body's horizontal position stays in the pose.
    pub bake_position_xz: bool,
    /// Rotation based upon the original (the clip's start) rather than the body orientation.
    pub original_orientation: bool,
    /// Height based upon the original rather than the centre of mass.
    pub original_position_y: bool,
    /// Horizontal position based upon the original rather than the centre of mass.
    pub original_position_xz: bool,
    /// Loop pose: the baked part of the pose blends towards the clip's start over each cycle.
    pub loop_pose: bool,
    /// Root rotation offset, degrees about +Y in the format's coordinates.
    pub orientation_offset: f32,
    /// Root height offset, in body position units.
    pub level: f32,
}

/// One animated property.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// What it animates.
    pub target: Target,
    /// Keys in ascending time.
    pub keys: Vec<Key>,
}

/// A Hermite key. Infinite tangents mean a constant (stepped) segment. JSON has no infinity
/// (`serde_json` writes `null`), so a `null` tangent reads as +infinity (ADR 0041).
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub time: f32,
    pub value: f32,
    #[serde(deserialize_with = "tangent")]
    pub in_tangent: f32,
    #[serde(deserialize_with = "tangent")]
    pub out_tangent: f32,
}

fn tangent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    Ok(Option::<f32>::deserialize(d)?.unwrap_or(f32::INFINITY))
}

/// What a curve animates: an element of the document by index and one of its properties (ADR 0089). Values are in
/// each property's units. Importers resolve source paths and names; stages that renumber elements remap targets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    /// A property of a node in [`Hierarchy::nodes`](crate::hierarchy::Hierarchy::nodes).
    Node {
        /// Node index.
        node: NodeIndex,
        /// Animated property.
        property: NodeProperty,
    },
    /// A property of a mesh instance in [`Document::meshes`](crate::Document::meshes).
    Mesh {
        /// Mesh index.
        mesh: MeshIndex,
        /// Animated property.
        property: MeshProperty,
    },
    /// A property of the humanoid.
    Humanoid(HumanoidProperty),
    /// The value of a state entry (a float entry; ADR 0088).
    Entry(EntryIndex),
    /// A field of a constraint in [`Document::constraints`](crate::Document::constraints) (ADR 0040).
    Constraint {
        /// Constraint index.
        constraint: u32,
        /// Animated field.
        field: crate::constraint::ConstraintField,
    },
}

/// Animatable node properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeProperty {
    /// Local translation component (0 = x, 1 = y, 2 = z), metres.
    Translation(u8),
    /// Local rotation quaternion component (0 = x .. 3 = w).
    Rotation(u8),
    /// Local Euler angle component (0 = x, 1 = y, 2 = z) in radians about the node's axes, applied in `order`. A
    /// node's Euler curves in one clip share an order.
    EulerRotation {
        /// Component.
        axis: u8,
        /// The order the three rotations apply in.
        order: EulerOrder,
    },
    /// Local scale component.
    Scale(u8),
    /// Active flag, 0 or 1.
    Active,
}

/// Orders of Euler rotations. One so far; others come with an importer whose source uses them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EulerOrder {
    /// About Z, then X, then Y (`q = qy qx qz`, ADR 0007).
    Zxy,
}

/// Animatable mesh instance properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeshProperty {
    /// Visible flag, 0 or 1.
    Visible,
    /// Morph target (blendshape) weight, by index into the mesh's blend shapes, 0 to 1.
    Weight(u32),
    /// A component of a material field, overridden on every submesh of this mesh instance whose material has the
    /// field; its default is the value in the first submesh's material, 0 when that material lacks the field (ADRs
    /// 0025, 0067, 0099).
    Material {
        /// The field.
        field: MaterialField,
        /// Component, below [`MaterialField::components`].
        component: u8,
        /// Colour keys are sRGB-encoded and decoded when applied (values above 1 by the power 2.2); only for
        /// [`MaterialField::is_color`] fields. Without it colours are linear.
        srgb: bool,
    },
    /// The material of a submesh: key values are indices into [`Document::materials`](crate::Document::materials),
    /// held until the next key.
    MaterialSlot(u8),
}

/// Animatable humanoid properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HumanoidProperty {
    /// A muscle: the rotation of `bone` about one axis of its muscle frame, -1 to 1 over the muscle's range (ADR
    /// 0014). The bones and axes that have muscles are listed in `civbb_anim::muscle::MUSCLES`.
    Muscle {
        /// The bone it turns.
        bone: HumanBone,
        /// Axis of the bone's muscle frame: 0 twist (X), 1 Y, 2 Z.
        axis: u8,
    },
    /// The body pose: position (0..3) or rotation (3..7) component (ADR 0063).
    Body(u8),
}

impl BodyPart {
    /// The mask body part a humanoid bone belongs to.
    pub fn of(bone: HumanBone) -> BodyPart {
        use HumanBone as H;
        match bone {
            H::Hips | H::Spine | H::Chest | H::UpperChest => BodyPart::Body,
            H::Neck | H::Head | H::Jaw | H::LeftEye | H::RightEye => BodyPart::Head,
            H::LeftShoulder | H::LeftUpperArm | H::LeftLowerArm | H::LeftHand => BodyPart::LeftArm,
            H::RightShoulder | H::RightUpperArm | H::RightLowerArm | H::RightHand => BodyPart::RightArm,
            H::LeftUpperLeg | H::LeftLowerLeg | H::LeftFoot | H::LeftToes => BodyPart::LeftLeg,
            H::RightUpperLeg | H::RightLowerLeg | H::RightFoot | H::RightToes => BodyPart::RightLeg,
            H::LeftThumbMetacarpal
            | H::LeftThumbProximal
            | H::LeftThumbDistal
            | H::LeftIndexProximal
            | H::LeftIndexIntermediate
            | H::LeftIndexDistal
            | H::LeftMiddleProximal
            | H::LeftMiddleIntermediate
            | H::LeftMiddleDistal
            | H::LeftRingProximal
            | H::LeftRingIntermediate
            | H::LeftRingDistal
            | H::LeftLittleProximal
            | H::LeftLittleIntermediate
            | H::LeftLittleDistal => BodyPart::LeftFingers,
            _ => BodyPart::RightFingers,
        }
    }
}
