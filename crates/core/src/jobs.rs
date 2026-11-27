//! The job runner: avatar islands stepped in parallel on a fork-join pool that wakes every worker at once (ADRs 0020, 0105).

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::counters::Counters;

/// How long a pool thread keeps polling for the next job after finishing one before it sleeps (ADR 0105): the
/// extract step starts a few µs after the island step, and threads that slept between them woke late (#140).
const SPIN: Duration = Duration::from_micros(200);

/// Runs `step` once on every island on `pool`: the calling thread and every pool thread take the next island not yet
/// taken until none are left, so a thread that wakes late takes fewer or none (#140). Returns the counters of every
/// step, summed. Allocates nothing.
pub fn islands<T: Send>(pool: &Pool, islands: &mut [T], step: impl Fn(&mut T, &mut Counters) + Sync) -> Counters {
    let next = Mutex::new(islands.iter_mut());
    let sum = Mutex::new(Counters::default());
    pool.broadcast(&|| {
        let mut c = Counters::default();
        // The guard must drop before the step runs: `while let` would hold it for the loop body.
        let take = || lock(&next).next();
        while let Some(island) = take() {
            let _zone = crate::profile::zone("island");
            step(island, &mut c);
        }
        lock(&sum).merge(&c);
    });
    sum.into_inner().unwrap_or_else(PoisonError::into_inner)
}

/// A fork-join pool (ADR 0105): [`Pool::broadcast`] runs one closure on the calling thread and on every pool thread,
/// waking them all at once. bevy_tasks' executor woke sleeping threads one after another, and only about 5 of 13
/// joined a short parallel step (#140).
pub struct Pool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

struct Shared {
    state: Mutex<State>,
    /// Counts broadcasts, so a pool thread runs each job at most once; written under `state`'s lock, read without it
    /// by spinning threads.
    generation: AtomicU64,
    /// Signalled when a job starts or the pool shuts down.
    start: Condvar,
    /// Signalled when the last pool thread running a job finishes it.
    done: Condvar,
}

#[derive(Default)]
struct State {
    /// The job being broadcast, while the calling thread still runs it; pool threads that wake after that skip it.
    job: Option<Job>,
    /// Pool threads running the job.
    running: usize,
    /// The first panic of a pool thread running the job, resumed on the calling thread.
    panic: Option<Box<dyn Any + Send>>,
    shutdown: bool,
}

/// A broadcast closure with its lifetime erased; [`Pool::broadcast`] keeps it alive while any pool thread runs it.
#[derive(Clone, Copy)]
struct Job(*const (dyn Fn() + Sync));

// SAFETY: the pointee is `Sync`, so calling it from other threads is sound; `Pool::broadcast` keeps it alive.
unsafe impl Send for Job {}

impl Pool {
    /// Pool threads, not counting the calling thread.
    pub fn thread_num(&self) -> usize {
        self.threads.len()
    }

    /// Runs `job` on the calling thread and on every pool thread that wakes before the calling thread's run ends;
    /// returns once each of those runs ended. A panic in any run is resumed here.
    pub fn broadcast(&self, job: &(dyn Fn() + Sync)) {
        if !self.threads.is_empty() {
            // SAFETY: only the lifetime changes. Pool threads call the pointer only after taking it from `State::job`
            // while it was set (counted in `running`), and this function clears it and waits for `running` to reach
            // 0 before returning, so no call outlives the borrow.
            let erased: *const (dyn Fn() + Sync + 'static) = unsafe { std::mem::transmute(job) };
            let mut s = lock(&self.shared.state);
            s.job = Some(Job(erased));
            self.shared.generation.fetch_add(1, Ordering::Release);
            drop(s);
            self.shared.start.notify_all();
        }
        let mine = catch_unwind(AssertUnwindSafe(job));
        let mut s = lock(&self.shared.state);
        s.job = None;
        while s.running > 0 {
            s = self.shared.done.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        let theirs = s.panic.take();
        drop(s);
        if let Err(p) = mine {
            resume_unwind(p);
        }
        if let Some(p) = theirs {
            resume_unwind(p);
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        lock(&self.shared.state).shutdown = true;
        self.shared.start.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// A pool thread: runs each broadcast job it wakes in time for, polling for the next one for [`SPIN`] before it
/// sleeps, until shutdown.
fn work(shared: &Shared) {
    let mut seen = 0;
    let mut s = lock(&shared.state);
    loop {
        if s.shutdown {
            return;
        }
        let generation = shared.generation.load(Ordering::Acquire);
        match s.job {
            Some(job) if generation != seen => {
                seen = generation;
                s.running += 1;
                drop(s);
                // SAFETY: taken from `State::job` while set and counted in `running`; `Pool::broadcast` keeps the
                // closure alive until `running` is back to 0.
                let r = catch_unwind(AssertUnwindSafe(|| unsafe { (*job.0)() }));
                s = lock(&shared.state);
                if let Err(p) = r {
                    s.panic.get_or_insert(p);
                }
                s.running -= 1;
                if s.running == 0 {
                    // All: concurrent broadcasts may each wait here.
                    shared.done.notify_all();
                }
                drop(s);
                let since = Instant::now();
                while shared.generation.load(Ordering::Acquire) == seen && since.elapsed() < SPIN {
                    std::hint::spin_loop();
                }
                s = lock(&shared.state);
            }
            _ => s = shared.start.wait(s).unwrap_or_else(PoisonError::into_inner),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A pool where `workers` threads run jobs, counting the thread that waits for them: `workers - 1` pool threads, each
/// calling `on_spawn` first. With 1 worker every job runs on the calling thread.
pub fn pool(workers: usize, on_spawn: impl Fn() + Send + Sync + 'static) -> Pool {
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        generation: AtomicU64::new(0),
        start: Condvar::new(),
        done: Condvar::new(),
    });
    let on_spawn = Arc::new(on_spawn);
    let threads = (1..workers.max(1))
        .map(|_| {
            let (shared, on_spawn) = (Arc::clone(&shared), Arc::clone(&on_spawn));
            std::thread::Builder::new()
                .name("civbb worker".into())
                .spawn(move || {
                    on_spawn();
                    work(&shared);
                })
                .expect("the OS refused a worker thread")
        })
        .collect();
    Pool { shared, threads }
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
            let pool = pool(workers, || {});
            let mut v: Vec<u32> = (0..100).collect();
            for round in 1..=50 {
                let c = islands(&pool, &mut v, |x, c| {
                    c.time(System::Animator, || *x += 1);
                });
                assert_eq!(v, (round..round + 100).collect::<Vec<_>>());
                assert_eq!(c.runs[System::Animator as usize], 100);
            }
        }
    }

    #[test]
    fn a_pool_of_n_workers_has_n_minus_1_threads() {
        assert_eq!(pool(1, || {}).thread_num(), 0);
        assert_eq!(pool(4, || {}).thread_num(), 3);
    }

    #[test]
    fn concurrent_callers_each_step_their_own_islands() {
        let pool = pool(4, || {});
        std::thread::scope(|s| {
            for _ in 0..3 {
                s.spawn(|| {
                    let mut v = [0u32; 20];
                    for _ in 0..500 {
                        islands(&pool, &mut v, |x, _| *x += 1);
                    }
                    assert_eq!(v, [500; 20]);
                });
            }
        });
    }

    #[test]
    fn a_panicking_job_reaches_the_caller_and_the_pool_still_works() {
        let pool = pool(4, || {});
        let r = catch_unwind(AssertUnwindSafe(|| pool.broadcast(&|| panic!("job"))));
        assert!(r.is_err());
        let mut v = [0u32; 10];
        islands(&pool, &mut v, |x, _| *x += 1);
        assert_eq!(v, [1; 10]);
    }
}
