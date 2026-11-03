# civbb-client

The client (design doc §5). Glue crate: wires `anim`, `dynamics` and `core` together; nothing depends on it.

- `island`: `Model`, an avatar's systems compiled once and shared, and `Island`, one avatar's running state.
  `Island::step` runs design doc §5 step 5 in order: last frame's contact values into their animator parameters,
  the animator (pose written to node transforms and activity), constraints, an IK stub, bone dynamics, then world
  placements and activity, each timed into `civbb_core::Counters`.
- `frame`: `step` runs every island as its own rayon job, then the cross-avatar contact pass (step 6) once.

Decisions are in ADR 0020.
