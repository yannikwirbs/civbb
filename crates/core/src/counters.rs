//! Per-system timing counters: lightweight, always-on time and run counts for each system of a frame (design doc §5).

use std::time::Instant;

/// The systems of a simulation frame that report their time, in pipeline order (design doc §5 steps 5 and 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum System {
    /// Animator state machines, blend trees and the pose they write.
    Animator,
    /// Transform constraints.
    Constraints,
    /// Full-body IK.
    Ik,
    /// Bone dynamics.
    Dynamics,
    /// World placements of every node after dynamics, for contacts and rendering.
    World,
    /// The cross-avatar contact pass.
    Contacts,
}

impl System {
    /// Every system, in pipeline order.
    pub const ALL: [Self; 6] = [
        Self::Animator,
        Self::Constraints,
        Self::Ik,
        Self::Dynamics,
        Self::World,
        Self::Contacts,
    ];

    /// Lower-case name for reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::Animator => "animator",
            Self::Constraints => "constraints",
            Self::Ik => "ik",
            Self::Dynamics => "dynamics",
            Self::World => "world",
            Self::Contacts => "contacts",
        }
    }
}

/// Time spent and runs per system, summed over every job that reported. Time summed over parallel jobs is CPU
/// time, not wall time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Nanoseconds per system, indexed by `System as usize`.
    pub nanos: [u64; System::ALL.len()],
    /// Runs per system.
    pub runs: [u64; System::ALL.len()],
}

impl Counters {
    /// Runs `f` and adds its time to `system`.
    pub fn time<R>(&mut self, system: System, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let r = f();
        self.nanos[system as usize] += start.elapsed().as_nanos() as u64;
        self.runs[system as usize] += 1;
        r
    }

    /// Adds `other`'s counts to these.
    pub fn merge(&mut self, other: &Self) {
        for k in 0..System::ALL.len() {
            self.nanos[k] += other.nanos[k];
            self.runs[k] += other.runs[k];
        }
    }

    /// Seconds spent in `system`.
    pub fn seconds(&self, system: System) -> f64 {
        self.nanos[system as usize] as f64 * 1e-9
    }

    /// Nanoseconds over every system.
    pub fn total_nanos(&self) -> u64 {
        self.nanos.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_counts_runs_and_merge_adds() {
        let mut a = Counters::default();
        assert_eq!(a.time(System::Dynamics, || 7), 7);
        a.time(System::Dynamics, || {
            std::thread::sleep(std::time::Duration::from_millis(1))
        });
        assert_eq!(a.runs[System::Dynamics as usize], 2);
        assert!(a.seconds(System::Dynamics) >= 0.001);
        let mut b = Counters::default();
        b.time(System::Ik, || ());
        b.merge(&a);
        assert_eq!(b.runs, [0, 0, 1, 2, 0, 0]);
        assert_eq!(b.total_nanos(), a.total_nanos() + b.nanos[System::Ik as usize]);
    }
}
