//! Island steps and whole frames allocate nothing on the heap after warm-up (design doc §5 "Scheduling rules", #135,
//! #139): a counting global allocator, process-wide, so it has a file of its own; counts are per thread.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use civbb_anim::ik::{Goal, Targets};
use civbb_client::frame::{self, Runtime};
use civbb_client::{Island, Model};
use civbb_core::math::Pose;

/// The system allocator, counting allocations made on the current thread.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: forwards every call unchanged to the system allocator; the count is a const-initialised thread local,
// which never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        // SAFETY: the caller upholds `alloc`'s contract, passed on as is.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `dealloc`'s contract, passed on as is.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        // SAFETY: the caller upholds `realloc`'s contract, passed on as is.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn island_steps_do_not_allocate_after_warm_up() {
    for (name, spec) in [("lean", civbb_corpus::lean()), ("bloated", civbb_corpus::bloated())] {
        let avatar = civbb_corpus::generate(&spec);
        let mut island = Island::new(Arc::new(Model::new(&avatar)), 0, true, Pose::IDENTITY);
        // IK with every target set (ADR 0111): hands in front of the chest, the head lowered, the view pitched.
        let goal = |pos| Goal {
            pos,
            rot: [0.0, 0.0, 0.0, 1.0],
        };
        island.targets = Targets {
            head: Some(goal([0.0, 1.3, 0.1])),
            left_hand: Some(goal([0.2, 1.1, 0.3])),
            right_hand: Some(goal([-0.2, 1.1, 0.3])),
            look: 0.4,
        };
        let mut counters = Default::default();
        let controller = island.model().controller().cloned();
        let entries = controller.as_ref().map_or(0, |c| c.state.entries.len());
        // Warm up, then toggle every parameter so states change and transitions run during the counted frames.
        for _ in 0..10 {
            island.step(1.0 / 90.0, &mut counters);
        }
        let before = allocations();
        for frame in 0..200 {
            if frame % 50 == 25 {
                let a = island.animator_mut().expect("the corpus avatars have animators");
                for p in a.params.iter_mut().take(entries) {
                    *p = 1.0 - *p;
                }
            }
            island.step(1.0 / 90.0, &mut counters);
        }
        assert_eq!(
            allocations() - before,
            0,
            "{name}: heap allocations in 200 island steps"
        );
    }
}

#[test]
fn frames_do_not_allocate_after_warm_up() {
    for (name, spec) in [("lean", civbb_corpus::lean()), ("bloated", civbb_corpus::bloated())] {
        let model = Arc::new(Model::new(&civbb_corpus::generate(&spec)));
        // Close enough that contacts of neighbours touch.
        let mut islands: Vec<Island> = (0..4)
            .map(|id| {
                let origin = Pose {
                    pos: [id as f32 * 0.3, 0.0, 0.0],
                    ..Pose::IDENTITY
                };
                Island::new(Arc::clone(&model), id, id == 0, origin)
            })
            .collect();
        // One worker: the calling thread runs every job, so the thread-local count sees them all.
        let pool = Arc::new(civbb_core::jobs::pool(1, || {}));
        let mut runtime = Runtime::with_workers(Arc::clone(&pool));
        for island in &islands {
            runtime.spawn(Island::new(Arc::clone(&model), island.id, island.local, island.origin));
        }
        let sway = |frame: usize, k: usize, island: &mut Island| {
            island.origin.pos[0] = k as f32 * 0.3 + (frame as f32 * 0.05).sin() * 0.2;
        };
        for _ in 0..10 {
            frame::step_on(&pool, &mut islands, |_, _| true, 1.0 / 90.0);
            runtime.step(1.0 / 90.0);
        }
        let before = allocations();
        for f in 0..200 {
            for (k, island) in islands.iter_mut().enumerate() {
                sway(f, k, island);
            }
            frame::step_on(&pool, &mut islands, |_, _| true, 1.0 / 90.0);
        }
        assert_eq!(allocations() - before, 0, "{name}: heap allocations in 200 frames");
        let before = allocations();
        for f in 0..200 {
            for k in 0..runtime.len() {
                sway(f, k, &mut runtime.island_mut(k));
            }
            runtime.step(1.0 / 90.0);
        }
        assert_eq!(
            allocations() - before,
            0,
            "{name}: heap allocations in 200 runtime frames"
        );
    }
}
