//! One simulation frame's avatar work: every island as its own parallel job, then the cross-avatar contact pass
//! (design doc §5 steps 5 and 6, ADR 0020).

use civbb_core::{Counters, System, jobs};
use civbb_dynamics::contact::{self, Participant};

use crate::island::Island;

/// Steps every island `dt` seconds, one job each on the current worker pool, then runs one contact pass over all of
/// them; its receiver values reach the animators on the next frame. `permits(receiver, sender)` is the receiving
/// owner's setting for whose contacts may affect their avatar. Returns the frame's per-system counters (CPU time).
pub fn step(islands: &mut [Island], permits: impl Fn(u64, u64) -> bool, dt: f32) -> Counters {
    let mut counters = jobs::islands(islands, |island, c| island.step(dt, c));
    counters.time(System::Contacts, || {
        let mut all: Vec<Participant> = islands.iter_mut().map(Island::participant).collect();
        contact::step(&mut all, permits, dt);
    });
    counters
}
