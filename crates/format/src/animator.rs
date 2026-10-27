//! Animator data with Mecanim semantics: parameters, layers, state machines, blend trees, transitions, clips.
//!
//! Exporters flatten sub-state machines into one state list per layer and drop muted transitions, so the
//! runtime only sees states, entry transitions, any-state transitions and per-state transitions.

use serde::{Deserialize, Serialize};

use crate::hierarchy::HumanBone;

/// Index into [`Animator::parameters`].
pub type ParamIndex = u32;
/// Index into [`crate::Avatar::clips`].
pub type ClipIndex = u32;
/// Index into [`StateMachine::states`].
pub type StateIndex = u32;

/// An animator controller. Legacy playable layers (Base, Additive, Gesture, Action, FX) are merged into one
/// layer list in that order, each tagged with its [`LayerGroup`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Animator {
    /// Parameters, shared by all layers.
    pub parameters: Vec<Parameter>,
    /// Layer groups (legacy playable layers), weighted as a whole.
    pub groups: Vec<LayerGroup>,
    /// Layers, evaluated in order.
    pub layers: Vec<Layer>,
}

/// An animator parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// Name, as conditions, drivers, OSC and expressions refer to it.
    pub name: String,
    /// Type.
    pub kind: ParamKind,
    /// Initial value; bools and triggers use 0 or 1.
    pub default: f32,
}

/// Parameter types.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamKind {
    Float,
    Int,
    Bool,
    Trigger,
}

/// A group of layers with a shared weight (a legacy playable layer).
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
    /// Initial weight, 0 to 1. Unity ignores the weight of a controller's first layer and uses 1.
    pub weight: f32,
    /// How the layer combines with the layers below.
    pub blending: LayerBlending,
    /// Which bones and transforms the layer may write; `None` writes everything.
    pub mask: Option<AvatarMask>,
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

/// Unity avatar mask: humanoid body parts plus an optional transform path allowlist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AvatarMask {
    /// Enabled humanoid body parts.
    pub body_parts: Vec<BodyPart>,
    /// Transform paths the layer may animate; `None` allows all non-humanoid transforms.
    pub transforms: Option<Vec<String>>,
}

/// Humanoid body parts of an avatar mask.
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
    pub speed_param: Option<ParamIndex>,
    /// Normalised start offset.
    pub cycle_offset: f32,
    /// Parameter replacing `cycle_offset`.
    pub cycle_offset_param: Option<ParamIndex>,
    /// Parameter driving normalised time directly (Unity "Motion Time").
    pub time_param: Option<ParamIndex>,
    /// Write Defaults: animated properties not in this state's motion revert to their defaults.
    pub write_defaults: bool,
    /// Outgoing transitions, checked in order.
    pub transitions: Vec<Transition>,
    /// State behaviours run on enter.
    pub behaviours: Vec<Behaviour>,
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
    /// Normalise child weights for `Direct` trees (Unity "Normalized Blend Values").
    pub normalize: bool,
}

/// Blend algorithms.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlendTreeKind {
    Simple1D { param: ParamIndex },
    SimpleDirectional2D { x: ParamIndex, y: ParamIndex },
    FreeformDirectional2D { x: ParamIndex, y: ParamIndex },
    FreeformCartesian2D { x: ParamIndex, y: ParamIndex },
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
    pub direct_param: Option<ParamIndex>,
    /// Speed multiplier.
    pub time_scale: f32,
    /// Normalised time offset.
    pub cycle_offset: f32,
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
    pub param: ParamIndex,
    /// Comparison.
    pub mode: ConditionMode,
    /// Comparison value for numeric modes.
    pub threshold: f32,
}

/// Condition comparisons, as in Unity.
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

/// Transition interruption sources, as in Unity.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptionSource {
    None,
    Source,
    Destination,
    SourceThenDestination,
    DestinationThenSource,
}

/// State behaviours with legacy equivalents (design doc §7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Behaviour {
    /// Changes parameters on state enter.
    ParameterDriver {
        /// Run only for the avatar's owner.
        local_only: bool,
        /// Operations, in order.
        ops: Vec<DriverOp>,
    },
    /// Switches body parts between tracking and animation.
    TrackingControl(Vec<(TrackingPart, TrackingMode)>),
    /// Blends a layer's weight.
    LayerWeight {
        /// Target layer.
        layer: u32,
        /// Goal weight.
        goal: f32,
        /// Blend time in seconds.
        duration: f32,
    },
    /// Blends a layer group's weight (legacy playable layer control).
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

/// Parameter driver operations.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DriverOp {
    Set {
        param: ParamIndex,
        value: f32,
    },
    Add {
        param: ParamIndex,
        value: f32,
    },
    /// Random in `[min, max]`; for bools, true with probability `chance`.
    Random {
        param: ParamIndex,
        min: f32,
        max: f32,
        chance: f32,
    },
    /// Copies `src` to `dst`, optionally remapping `from` range to `to` range.
    Copy {
        src: ParamIndex,
        dst: ParamIndex,
        remap: Option<([f32; 2], [f32; 2])>,
    },
}

/// Body parts under tracking control.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingPart {
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

/// Who drives a body part.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingMode {
    Tracking,
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
    /// Animated properties.
    pub curves: Vec<Curve>,
}

/// One animated property.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// What it animates.
    pub binding: Binding,
    /// Keys in ascending time.
    pub keys: Vec<Key>,
}

/// A Hermite key, as in Unity. Infinite tangents mean a constant (stepped) segment.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub time: f32,
    pub value: f32,
    pub in_tangent: f32,
    pub out_tangent: f32,
}

/// The target of a curve: a node path (Unity style, `/`-separated, relative to the root) and a property.
/// Bindings are resolved to component indices at compile time; unresolved ones are reported, not fatal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// Node path; empty for the root and for animator parameters.
    pub path: String,
    /// Animated property.
    pub property: Property,
}

/// Animatable properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Property {
    /// Local position component (0 = x, 1 = y, 2 = z).
    Position(u8),
    /// Local rotation quaternion component (0 = x .. 3 = w).
    Rotation(u8),
    /// Local Euler angle component in degrees (Unity `localEulerAnglesRaw`).
    EulerRotation(u8),
    /// Local scale component.
    Scale(u8),
    /// Humanoid muscle, by Unity muscle name (e.g. `Left Arm Down-Up`).
    Muscle(String),
    /// Humanoid root motion: position (0..3) or rotation (3..7) component.
    RootMotion(u8),
    /// Blendshape weight, 0 to 100 as in Unity, on the mesh at `path`.
    BlendShape(String),
    /// Node active flag.
    Active,
    /// Mesh renderer enabled flag.
    RendererEnabled,
    /// Material property on the renderer at `path`, e.g. `_Color.r`.
    Material(String),
    /// Animator parameter driven by the clip (legacy FX technique).
    Parameter(String),
    /// Any other component property, kept so the compiler can report or map it later.
    Component {
        /// Source component type name.
        component: String,
        /// Property path on that component.
        property: String,
    },
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
            H::LeftThumbProximal
            | H::LeftThumbIntermediate
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
