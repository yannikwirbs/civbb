//! One simulation frame's avatar work: every island as its own parallel job, then the cross-avatar contact pass
//! (design doc §5 steps 5 and 6, ADR 0020), as plain functions and as systems on the frame schedule (ADR 0103).

use std::borrow::BorrowMut;
use std::sync::{Arc, OnceLock};

use bevy_ecs::prelude::*;
use civbb_core::math::Pose;
use civbb_core::schedule::{self, Dt, Step};
use civbb_core::{Counters, System, jobs, profile};
use civbb_dynamics::contact::{self, Participant};

use crate::affinity;
use crate::extract::{Camera, Snapshot, extract_system};
use crate::island::Island;
use crate::package::Handle;

/// The client's worker pool, made on first use: [`jobs::default_workers`] workers, pool threads kept to performance
/// cores and no more of them than those cores have logical CPUs (ADR 0103).
pub fn workers() -> Arc<jobs::Pool> {
    static WORKERS: OnceLock<Arc<jobs::Pool>> = OnceLock::new();
    let pool = WORKERS.get_or_init(|| {
        // The waiting thread is a worker too, but not kept to those cores.
        let n = jobs::default_workers().min(affinity::performance_cpus().map_or(usize::MAX, |n| n + 1));
        Arc::new(jobs::pool(n, || {
            affinity::keep_to_performance_cores();
        }))
    });
    Arc::clone(pool)
}

/// Steps every island `dt` seconds, one job each on [`workers`], then runs one contact pass over all of them; its
/// receiver values reach the animators on the next frame. `permits(receiver, sender)` is the receiving owner's
/// setting for whose contacts may affect their avatar. Returns the frame's per-system counters (CPU time).
pub fn step<I: BorrowMut<Island> + Send>(islands: &mut [I], permits: impl Fn(u64, u64) -> bool, dt: f32) -> Counters {
    step_on(&workers(), islands, permits, dt)
}

/// [`step`] on `pool`.
pub fn step_on<I: BorrowMut<Island> + Send>(
    pool: &jobs::Pool,
    islands: &mut [I],
    permits: impl Fn(u64, u64) -> bool,
    dt: f32,
) -> Counters {
    let mut counters = jobs::islands(pool, islands, |island, c| island.borrow_mut().step(dt, c));
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

/// The pool the island system runs its jobs on.
#[derive(Resource)]
pub struct Workers(pub Arc<jobs::Pool>);

/// The island step of [`Step::Avatars`]: one job per island; replaces the frame's [`Counters`].
fn islands_system(mut q: Query<&mut Island>, dt: Res<Dt>, workers: Res<Workers>, mut counters: ResMut<Counters>) {
    let _zone = profile::zone("avatars");
    let mut all: Vec<&mut Island> = q.iter_mut().map(Mut::into_inner).collect();
    *counters = jobs::islands(&workers.0, &mut all, |island, c| island.step(dt.0, c));
}

/// The contact pass of [`Step::CrossAvatar`], in query order: a receiver's value is the largest over its senders,
/// so the order islands come in does not change results (ADR 0104).
fn contacts_system(mut q: Query<&mut Island>, dt: Res<Dt>, permits: Res<Permits>, mut counters: ResMut<Counters>) {
    let mut all: Vec<&mut Island> = q.iter_mut().map(Mut::into_inner).collect();
    contacts(&mut all, &permits.0, dt.0, &mut counters);
}

/// When an island was spawned: the extract step copies islands in this order (ADR 0104).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spawned(pub u64);

/// The package an island shows; the extract step copies only islands that have one.
#[derive(Component)]
pub struct Shows(pub Handle);

/// Avatar islands as entities in an ECS world and the frame schedule that steps them (ADR 0103). Islands are spawned
/// and despawned between frames and keep the order they were spawned in (ADR 0104).
pub struct Runtime {
    /// The ECS world: one entity per island, and the frame's resources.
    pub world: World,
    schedule: Schedule,
    /// Live islands in spawn order.
    order: Vec<Entity>,
    /// Islands spawned so far, the next [`Spawned`].
    spawned: u64,
}

impl Default for Runtime {
    /// A runtime on [`workers`].
    fn default() -> Self {
        Self::with_workers(workers())
    }
}

impl Runtime {
    /// A runtime whose island jobs run on `pool`.
    pub fn with_workers(pool: Arc<jobs::Pool>) -> Self {
        let mut schedule = schedule::frame();
        schedule.add_systems((
            islands_system.in_set(Step::Avatars),
            contacts_system.in_set(Step::CrossAvatar),
        ));
        let mut world = World::new();
        world.init_resource::<Dt>();
        world.init_resource::<Counters>();
        world.init_resource::<Permits>();
        world.insert_resource(Workers(pool));
        Self {
            world,
            schedule,
            order: Vec::new(),
            spawned: 0,
        }
    }

    /// Adds the `Extract` step: each frame ends by copying every island showing a package and the [`Camera`]
    /// resource into the [`Snapshot`] resource.
    pub fn extract_to_snapshot(&mut self) {
        self.world.init_resource::<Camera>();
        self.world.init_resource::<Snapshot>();
        self.schedule.add_systems(extract_system.in_set(Step::Extract));
    }

    /// Adds `island` as an entity, after every island already spawned.
    pub fn spawn(&mut self, island: Island) -> Entity {
        let e = self.world.spawn((Spawned(self.spawned), island)).id();
        self.spawned += 1;
        self.order.push(e);
        e
    }

    /// Spawns an island showing `package`, after every island already spawned.
    pub fn spawn_package(&mut self, package: &Handle, id: u64, local: bool, origin: Pose) -> Entity {
        let e = self.spawn(Island::new(Arc::clone(&package.model), id, local, origin));
        self.world.entity_mut(e).insert(Shows(Arc::clone(package)));
        e
    }

    /// Removes the island `e`; the others keep their order. Returns whether it was there.
    pub fn despawn(&mut self, e: Entity) -> bool {
        let Some(k) = self.order.iter().position(|&o| o == e) else {
            return false;
        };
        self.order.remove(k);
        self.world.despawn(e)
    }

    /// Runs one frame of `dt` seconds; returns its per-system counters (CPU time).
    pub fn step(&mut self, dt: f32) -> Counters {
        self.world.insert_resource(Dt(dt));
        self.schedule.run(&mut self.world);
        profile::frame_mark();
        *self.world.resource::<Counters>()
    }

    /// Every island, in spawn order.
    pub fn islands(&self) -> Vec<&Island> {
        self.order
            .iter()
            .map(|&e| self.world.get::<Island>(e).expect("islands in the order are live"))
            .collect()
    }

    /// The `k`th live island in spawn order.
    pub fn island_mut(&mut self, k: usize) -> Mut<'_, Island> {
        self.world
            .get_mut::<Island>(self.order[k])
            .expect("islands in the order are live")
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
