//! Stations (design doc §7, ADR 0114): seats in the world that the player uses, sits in and leaves.

use bevy_ecs::prelude::*;
use civbb_core::math::{length, sub};
use civbb_format::math::Vec3;

/// How far the use key reaches a seat, in metres at scale 1.
pub const REACH: f32 = 1.5;

/// A seat: where the avatar's origin goes and faces, where the player gets off, and whether the avatar sits.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Station {
    /// Where the avatar's origin goes, in the world.
    pub seat: Vec3,
    /// Facing in radians about +Y; 0 faces +Z.
    pub yaw: f32,
    /// Where the player stands after leaving.
    pub exit: Vec3,
    /// The avatar sits (`Seated`, the default sit pose); else it keeps its locomotion pose.
    pub seated: bool,
}

/// The station whose seat is nearest to `pos` and within `reach`, if any.
pub fn nearest<'a>(stations: impl IntoIterator<Item = &'a Station>, pos: Vec3, reach: f32) -> Option<Station> {
    stations
        .into_iter()
        .map(|s| (length(sub(s.seat, pos)), s))
        .filter(|(d, _)| *d <= reach)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_seat_within_reach() {
        let at = |x| Station {
            seat: [x, 0.0, 0.0],
            yaw: 0.0,
            exit: [x, 0.0, 1.0],
            seated: true,
        };
        let stations = [at(1.0), at(-0.5), at(3.0)];
        assert_eq!(nearest(&stations, [0.0; 3], REACH), Some(at(-0.5)));
        assert_eq!(nearest(&stations, [5.0, 0.0, 0.0], REACH), None);
    }
}
