//! What a state plays: its motion's clips with weights and sample times, its length and whether it loops.

use civbb_format::animator::{ClipIndex, Motion};

use crate::clip::BoundClip;

/// One clip a motion plays, after blend tree weighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    /// Clip.
    pub clip: ClipIndex,
    /// Seconds into the clip.
    pub time: f32,
    /// Weight within the motion.
    pub weight: f32,
}

/// Seconds a motion takes at speed 1. Empty states and zero-length clips count as 1 s (ADR 0008).
pub fn length(motion: Option<&Motion>, clips: &[BoundClip]) -> f32 {
    let len = match motion {
        Some(Motion::Clip(c)) => clips[*c as usize].length,
        // Blend tree lengths come with #15.
        Some(Motion::BlendTree(_)) | None => 0.0,
    };
    if len > 0.0 { len } else { 1.0 }
}

/// Whether a motion loops, which repeats exit times below 1 every cycle.
pub fn loops(motion: Option<&Motion>, clips: &[BoundClip]) -> bool {
    match motion {
        Some(Motion::Clip(c)) => clips[*c as usize].looping,
        Some(Motion::BlendTree(_)) | None => false,
    }
}

/// Appends the clips `motion` plays at `normalized` time (cycle offset included) with their weights.
pub fn leaves(motion: &Motion, clips: &[BoundClip], normalized: f32, out: &mut Vec<Leaf>) {
    match motion {
        Motion::Clip(c) => out.push(Leaf {
            clip: *c,
            time: clip_time(&clips[*c as usize], normalized),
            weight: 1.0,
        }),
        // Blend trees come with #15; until then they play nothing.
        Motion::BlendTree(_) => {}
    }
}

/// Seconds into `clip` at a normalised time: wrapped for looping clips, clamped otherwise.
pub fn clip_time(clip: &BoundClip, normalized: f32) -> f32 {
    let n = if clip.looping {
        normalized - normalized.floor()
    } else {
        normalized.clamp(0.0, 1.0)
    };
    n * clip.length
}
