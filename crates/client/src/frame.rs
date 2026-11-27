//! One simulation frame's avatar work: every island as its own parallel job, then the cross-avatar contact pass
//! (design doc §5 steps 5 and 6, ADR 0020), as plain functions and as systems on the frame schedule (ADR 0103).

use std::borrow::BorrowMut;

use bevy_ecs::prelude::*;
use civbb_core::schedule::{self, Dt, Step};
use civbb_core::{Counters, System, jobs};
use civbb_dynamics::contact::{self, Participant};

use crate::island::Island;

/// Steps every island `dt` seconds, one job each on the current worker pool, then runs one contact pass over all of
/// them; its receiver values reach the animators on the next frame. `permits(receiver, sender)` is the receiving
/// owner's setting for whose contacts may affect their avatar. Returns the frame's per-system counters (CPU time).
pub fn step<I: BorrowMut<Island> + Send>(islands: &mut [I], permits: impl Fn(u64, u64) -> bool, dt: f32) -> Counters {
    let mut counters = jobs::islands(islands, |island, c| island.borrow_mut().step(dt, c));
    contacts(islands, permits, dt, &mut counters);
    counters
}

/// The contact pass over `islands` after they stepped, timed as [`System::Contacts`].
fn contacts<I: BorrowMut<Island>>(
    islands: &mut [I],
    permits: impl Fn(u64, u64) -> bool,
    dt: f32,
    counters: &mut Counters,
) {
    counters.time(System::Contacts, || {
        let mut all: Vec<Participant> = islands.iter_mut().map(|i| i.borrow_mut().participant()).collect();
        contact::step(&mut all, permits, dt);
    });
}

/// Whose contacts may affect whose avatar: `(receiving owner, sending owner) -> allowed`. Everyone by default.
#[derive(Resource)]
pub struct Permits(pub Box<dyn Fn(u64, u64) -> bool + Send + Sync>);

impl Default for Permits {
    fn default() -> Self {
        Self(Box::new(|_, _| true))
    }
}

/// The island step of [`Step::Avatars`]: one job per island; replaces the frame's [`Counters`].
fn islands_system(mut q: Query<&mut Island>, dt: Res<Dt>, mut counters: ResMut<Counters>) {
    let mut all: Vec<&mut Island> = q.iter_mut().map(Mut::into_inner).collect();
    *counters = jobs::islands(&mut all, |island, c| island.step(dt.0, c));
}

/// The contact pass of [`Step::CrossAvatar`], in spawn order.
fn contacts_system(mut q: Query<&mut Island>, dt: Res<Dt>, permits: Res<Permits>, mut counters: ResMut<Counters>) {
    let mut all: Vec<&mut Island> = q.iter_mut().map(Mut::into_inner).collect();
    contacts(&mut all, &permits.0, dt.0, &mut counters);
}

/// Avatar islands as entities in an ECS world and the frame schedule that steps them (ADR 0103). Islands keep the
/// order they were spawned in.
pub struct Runtime {
    /// The ECS world: one entity per island, and the frame's resources.
    pub world: World,
    schedule: Schedule,
    order: Vec<Entity>,
}

impl Default for Runtime {
    fn default() -> Self {
        let mut schedule = schedule::frame();
        schedule.add_systems((
            islands_system.in_set(Step::Avatars),
            contacts_system.in_set(Step::CrossAvatar),
        ));
        let mut world = World::new();
        world.init_resource::<Dt>();
        world.init_resource::<Counters>();
        world.init_resource::<Permits>();
        Self {
            world,
            schedule,
            order: Vec::new(),
        }
    }
}

impl Runtime {
    /// Adds `island` as an entity, after every island already spawned.
    pub fn spawn(&mut self, island: Island) -> Entity {
        let e = self.world.spawn(island).id();
        self.order.push(e);
        e
    }

    /// Runs one frame of `dt` seconds on the current worker pool; returns its per-system counters (CPU time).
    pub fn step(&mut self, dt: f32) -> Counters {
        self.world.insert_resource(Dt(dt));
        self.schedule.run(&mut self.world);
        *self.world.resource::<Counters>()
    }

    /// Every island, in spawn order.
    pub fn islands(&self) -> Vec<&Island> {
        self.order
            .iter()
            .map(|&e| self.world.get::<Island>(e).expect("spawned islands are never removed"))
            .collect()
    }

    /// The `k`th island spawned.
    pub fn island_mut(&mut self, k: usize) -> Mut<'_, Island> {
        self.world
            .get_mut::<Island>(self.order[k])
            .expect("spawned islands are never removed")
    }

    /// The number of islands.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// No islands spawned.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}
