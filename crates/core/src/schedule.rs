//! The frame schedule: one simulation frame's system sets in design doc §5's order, run chained (ADR 0103).

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

/// The schedule that runs one simulation frame.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Frame;

/// The steps of one simulation frame (design doc §5), each a system set; a set runs after the one before it. Steps
/// without systems yet are declared so later milestones only add systems.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// OpenXR poses, trackers, face and eye data.
    PosesIn,
    /// Remote snapshots into interpolation buffers.
    NetworkApply,
    /// World and avatar scripts against last frame's state.
    Scripts,
    /// Script commands merged in a deterministic order.
    CommandApply,
    /// One job per avatar island: animator, constraints, IK, dynamics.
    Avatars,
    /// The contact pass over every avatar.
    CrossAvatar,
    /// World physics.
    WorldPhysics,
    /// Culling, LOD and the performance governor.
    Visibility,
    /// Render data copied out for the renderer.
    Extract,
}

/// The frame's simulation time step in seconds.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Dt(pub f32);

/// A [`Frame`] schedule with every [`Step`] in order and no systems.
pub fn frame() -> Schedule {
    let mut schedule = Schedule::new(Frame);
    schedule.configure_sets(
        (
            Step::PosesIn,
            Step::NetworkApply,
            Step::Scripts,
            Step::CommandApply,
            Step::Avatars,
            Step::CrossAvatar,
            Step::WorldPhysics,
            Step::Visibility,
            Step::Extract,
        )
            .chain(),
    );
    schedule
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Order(Vec<Step>);

    #[test]
    fn steps_run_in_pipeline_order() {
        let mut schedule = frame();
        // Added in reverse: only the sets order them.
        schedule.add_systems((
            (|mut o: ResMut<Order>| o.0.push(Step::Extract)).in_set(Step::Extract),
            (|mut o: ResMut<Order>| o.0.push(Step::CrossAvatar)).in_set(Step::CrossAvatar),
            (|mut o: ResMut<Order>| o.0.push(Step::Avatars)).in_set(Step::Avatars),
            (|mut o: ResMut<Order>| o.0.push(Step::PosesIn)).in_set(Step::PosesIn),
        ));
        let mut world = World::new();
        world.init_resource::<Order>();
        schedule.run(&mut world);
        let order = &world.resource::<Order>().0;
        assert_eq!(order, &[Step::PosesIn, Step::Avatars, Step::CrossAvatar, Step::Extract]);
    }
}
