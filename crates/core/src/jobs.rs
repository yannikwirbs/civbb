//! The job runner: one job per avatar island, run in parallel across islands with rayon (ADR 0003, ADR 0020).

use rayon::prelude::*;

use crate::counters::Counters;

pub use rayon::{ThreadPool, ThreadPoolBuildError};

/// Runs `step` once on every island as its own job, in parallel on the current pool (the global one unless called
/// inside [`ThreadPool::install`]); returns the counters every job reported, summed.
pub fn islands<T: Send>(islands: &mut [T], step: impl Fn(&mut T, &mut Counters) + Sync) -> Counters {
    islands
        .par_iter_mut()
        .with_max_len(1)
        .fold(Counters::default, |mut c, island| {
            step(island, &mut c);
            c
        })
        .reduce(Counters::default, |mut a, b| {
            a.merge(&b);
            a
        })
}

/// A worker pool with `threads` workers; 0 means one per logical core.
pub fn pool(threads: usize) -> Result<ThreadPool, ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new().num_threads(threads).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counters::System;

    #[test]
    fn every_island_steps_once_and_counters_sum() {
        let mut v: Vec<u32> = (0..100).collect();
        let c = pool(4).unwrap().install(|| {
            islands(&mut v, |x, c| {
                c.time(System::Animator, || *x += 1);
            })
        });
        assert_eq!(v, (1..101).collect::<Vec<_>>());
        assert_eq!(c.runs[System::Animator as usize], 100);
    }
}
