//! The job runner: one task per avatar island, run in parallel across islands on a bevy_tasks pool (ADRs 0020, 0103).

use bevy_tasks::{TaskPool, TaskPoolBuilder};

use crate::counters::Counters;

pub use bevy_tasks::TaskPool as Pool;

/// Runs `step` once on every island as its own task on `pool`; the calling thread runs tasks too until all are done.
/// Returns the counters every task reported, summed.
pub fn islands<T: Send>(pool: &TaskPool, islands: &mut [T], step: impl Fn(&mut T, &mut Counters) + Sync) -> Counters {
    let step = &step;
    pool.scope(|s| {
        for island in islands.iter_mut() {
            s.spawn(async move {
                let _zone = crate::profile::zone("island");
                let mut c = Counters::default();
                step(island, &mut c);
                c
            });
        }
    })
    .iter()
    .fold(Counters::default(), |mut sum, c| {
        sum.merge(c);
        sum
    })
}

/// A pool where `workers` threads run jobs, counting the thread that waits for them: `workers - 1` pool threads, each
/// calling `on_spawn` first. With 1 worker every job runs on the calling thread.
pub fn pool(workers: usize, on_spawn: impl Fn() + Send + Sync + 'static) -> TaskPool {
    TaskPoolBuilder::new()
        .num_threads(workers.max(1) - 1)
        .thread_name("civbb worker".into())
        .on_thread_spawn(on_spawn)
        .build()
}

/// The client's worker count: logical cores minus 2 (the render and one spare thread, design doc §5), at least 1.
pub fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(2).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counters::System;

    #[test]
    fn every_island_steps_once_and_counters_sum() {
        for workers in [1, 4] {
            let mut v: Vec<u32> = (0..100).collect();
            let c = islands(&pool(workers, || {}), &mut v, |x, c| {
                c.time(System::Animator, || *x += 1);
            });
            assert_eq!(v, (1..101).collect::<Vec<_>>());
            assert_eq!(c.runs[System::Animator as usize], 100);
        }
    }

    #[test]
    fn a_pool_of_n_workers_has_n_minus_1_threads() {
        assert_eq!(pool(1, || {}).thread_num(), 0);
        assert_eq!(pool(4, || {}).thread_num(), 3);
    }
}
