//! One avatar island: its compiled systems and running state, stepped animator, constraints, IK, dynamics (ADR 0020).

use std::sync::Arc;

use civbb_anim::behaviour::coerce;
use civbb_anim::material::{self, Field};
use civbb_anim::{Constraints, Controller, Instance, Slot, Target};
use civbb_core::math::{Pose, world_poses_into};
use civbb_core::{Counters, System};
use civbb_dynamics::contact::Participant;
use civbb_dynamics::{ContactRig, ContactState, State};
use civbb_format::Avatar;
use civbb_format::animator::{ParamIndex, ParamKind};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::material::Material;
use civbb_format::math::Transform;
use civbb_format::mesh::MeshIndex;

/// An avatar's systems compiled once, shared by every island showing that avatar.
#[derive(Clone, Debug)]
pub struct Model {
    controller: Option<Controller>,
    constraints: Constraints,
    chains: civbb_dynamics::Rig,
    contacts: ContactRig,
    parents: Vec<Option<NodeIndex>>,
    rest: Vec<Transform>,
    active: Vec<bool>,
    /// Per mesh, each blendshape's default weight (0 to 1).
    shapes: Vec<Vec<f32>>,
    /// Per contact receiver, the animator parameter its value goes to; `None` when the animator lacks it.
    receivers: Vec<Option<(ParamIndex, ParamKind)>>,
    /// Renderers with animated material properties, by mesh index.
    renderers: Vec<AnimatedRenderer>,
}

/// A renderer whose material properties the animator drives: each island shows its own copy of its materials
/// (ADR 0025).
#[derive(Clone, Debug)]
struct AnimatedRenderer {
    mesh: MeshIndex,
    /// The avatar materials its submeshes use, each once, in submesh order.
    materials: Vec<Material>,
    /// Their avatar material indices.
    indices: Vec<u32>,
    /// The slot of each bound field component.
    slots: Vec<(Field, u8, Slot)>,
}

impl Model {
    /// Compiles a validated avatar.
    pub fn new(avatar: &Avatar) -> Self {
        let controller = Controller::new(avatar);
        let contacts = ContactRig::new(avatar);
        let receivers = contacts
            .receivers
            .iter()
            .map(|r| {
                let c = controller.as_ref()?;
                let p = c.param(&r.parameter)?;
                Some((p, c.animator.parameters[p as usize].kind))
            })
            .collect();
        let mut renderers: Vec<AnimatedRenderer> = Vec::new();
        for (slot, t) in controller
            .iter()
            .flat_map(|c| c.bound.bindings.targets.iter().enumerate())
        {
            let Target::Material(mesh, field, component) = *t else {
                continue;
            };
            let i = match renderers.iter().position(|r| r.mesh == mesh) {
                Some(i) => i,
                None => {
                    let mut indices: Vec<u32> = Vec::new();
                    for s in &avatar.meshes[mesh as usize].submeshes {
                        if !indices.contains(&s.material) {
                            indices.push(s.material);
                        }
                    }
                    renderers.push(AnimatedRenderer {
                        mesh,
                        materials: indices.iter().map(|&m| avatar.materials[m as usize].clone()).collect(),
                        indices,
                        slots: Vec::new(),
                    });
                    renderers.len() - 1
                }
            };
            renderers[i].slots.push((field, component, slot as Slot));
        }
        renderers.sort_by_key(|r| r.mesh);
        let nodes = &avatar.hierarchy.nodes;
        Self {
            controller,
            constraints: Constraints::new(avatar),
            chains: civbb_dynamics::Rig::new(avatar),
            contacts,
            parents: nodes.iter().map(|n| n.parent).collect(),
            rest: nodes.iter().map(|n| n.rest).collect(),
            active: nodes.iter().map(|n| n.active).collect(),
            shapes: avatar
                .meshes
                .iter()
                .map(|m| m.blend_shapes.iter().map(|s| s.default_weight).collect())
                .collect(),
            receivers,
            renderers,
        }
    }

    /// Renderers whose material properties are animated, in mesh order, each with the avatar materials its
    /// submeshes use: every island needs its own record of each (ADR 0025). [`Island::materials`] lists them in
    /// this order.
    pub fn animated_materials(&self) -> impl Iterator<Item = (MeshIndex, &[u32])> {
        self.renderers.iter().map(|r| (r.mesh, r.indices.as_slice()))
    }

    /// The compiled animator, if the avatar has one.
    pub fn controller(&self) -> Option<&Controller> {
        self.controller.as_ref()
    }
}

/// One avatar in the scene: a [`Model`] and the state of every system running it.
pub struct Island {
    /// Stable avatar id, the same every frame.
    pub id: u64,
    /// This client owns the avatar.
    pub local: bool,
    /// Where the avatar's root parent is in the world.
    pub origin: Pose,
    model: Arc<Model>,
    animator: Option<Instance>,
    chains: State,
    contacts: ContactState,
    locals: Vec<Transform>,
    /// Each node's own active flag, before its ancestors are taken into account.
    own_active: Vec<bool>,
    active: Vec<bool>,
    world: Vec<Pose>,
    /// The constraint pass's world placements, kept between frames.
    constraint_scratch: civbb_anim::constraint::Scratch,
}

impl Island {
    /// A fresh island for `model`, at rest until the first [`Island::step`].
    pub fn new(model: Arc<Model>, id: u64, local: bool, origin: Pose) -> Self {
        let n = model.rest.len();
        Self {
            id,
            local,
            origin,
            animator: model.controller.as_ref().map(|c| Instance::new(c, local)),
            chains: State::new(&model.chains),
            contacts: ContactState::new(&model.contacts),
            locals: model.rest.clone(),
            own_active: model.active.clone(),
            active: vec![false; n],
            world: Vec::with_capacity(n),
            constraint_scratch: Default::default(),
            model,
        }
    }

    /// The compiled avatar.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The running animator, to set parameters between frames.
    pub fn animator_mut(&mut self) -> Option<&mut Instance> {
        self.animator.as_mut()
    }

    /// Every node's local transform after the last step.
    pub fn locals(&self) -> &[Transform] {
        &self.locals
    }

    /// Every node's world placement after the last step.
    pub fn world(&self) -> &[Pose] {
        &self.world
    }

    /// Whether each node is active after the last step, its ancestors included.
    pub fn active(&self) -> &[bool] {
        &self.active
    }

    /// Writes each mesh's blendshape weights (0 to 1) after the last step into `out`: the animator's value where a
    /// clip binds the shape, its default weight elsewhere.
    pub fn shape_weights(&self, out: &mut Vec<Vec<f32>>) {
        out.clone_from(&self.model.shapes);
        if let (Some(c), Some(a)) = (&self.model.controller, &self.animator) {
            for (t, &v) in c.bound.bindings.targets.iter().zip(a.pose()) {
                if let Target::BlendShape(m, s) = *t {
                    out[m as usize][s as usize] = v / 100.0;
                }
            }
        }
    }

    /// Writes the materials of every animated renderer after the last step into `out`, flattened in
    /// [`Model::animated_materials`] order: the avatar's material with the animator's values set on every material
    /// of the renderer that has the field (ADR 0025). Reusing `out` across frames avoids reallocating.
    pub fn materials(&self, out: &mut Vec<Material>) {
        let m = &self.model;
        let n: usize = m.renderers.iter().map(|r| r.materials.len()).sum();
        if out.len() != n {
            out.clear();
            out.extend(m.renderers.iter().flat_map(|r| r.materials.iter().cloned()));
        }
        let Some(a) = &self.animator else { return };
        let pose = a.pose();
        let mut out = out.iter_mut();
        for r in &m.renderers {
            for mat in out.by_ref().take(r.materials.len()) {
                for &(field, component, slot) in &r.slots {
                    material::set(mat, field, component, pose[slot as usize]);
                }
            }
        }
    }

    /// Each contact receiver's value from the last contact pass.
    pub fn contact_values(&self) -> &[f32] {
        self.contacts.values()
    }

    /// Runs the avatar's systems for `dt` seconds in pipeline order, reporting each one's time to `counters`:
    /// last frame's receiver values into their parameters and the animator, constraints, IK, dynamics, then world
    /// placements and activity for the contact pass.
    pub fn step(&mut self, dt: f32, counters: &mut Counters) {
        let Self {
            origin,
            model,
            animator,
            chains,
            contacts,
            locals,
            own_active,
            active,
            world,
            constraint_scratch,
            ..
        } = self;
        let m = &**model;
        counters.time(System::Animator, || {
            locals.copy_from_slice(&m.rest);
            own_active.copy_from_slice(&m.active);
            if let (Some(c), Some(a)) = (&m.controller, animator) {
                for (r, &v) in m.receivers.iter().zip(contacts.values()) {
                    if let &Some((p, kind)) = r {
                        a.params[p as usize] = coerce(kind, v);
                    }
                }
                a.update(c, dt);
                a.write_nodes(c, locals, own_active);
                a.write_constraints(c, &m.constraints, constraint_scratch);
            }
        });
        counters.time(System::Constraints, || {
            m.constraints.evaluate(locals, constraint_scratch)
        });
        counters.time(System::Ik, || ik(locals));
        counters.time(System::Dynamics, || chains.step(&m.chains, *origin, locals, dt));
        counters.time(System::World, || {
            world_poses_into(&m.parents, *origin, locals, world);
            for i in 0..active.len() {
                active[i] = own_active[i] && m.parents[i].is_none_or(|p| active[p as usize]);
            }
        });
    }

    /// This island's part in the contact pass, after [`Island::step`].
    pub fn participant(&mut self) -> Participant<'_> {
        Participant {
            id: self.id,
            rig: &self.model.contacts,
            state: &mut self.contacts,
            world: &self.world,
            active: &self.active,
            local: self.local,
        }
    }
}

/// Full-body IK: a stub until the solver exists (design doc §7, Phase 1); it keeps its place and timer in the order.
fn ik(_locals: &mut [Transform]) {}
