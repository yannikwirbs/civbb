# civbb-dynamics

Bone dynamics and contacts (design doc §7 "Bone dynamics", "Contacts and constraints"). Engine crate: depends only
on `civbb-format` and `civbb-core` (whose `math` it uses).

- `rig`: compiles an avatar's chains into one flat particle list (parents first, per-particle parameters
  evaluated from their graded curves), the layout the solver loops over.
- `solver`: per-avatar `State`: steps every chain against the animated pose and writes the simulated local
  rotations back; grab, release and pose; the `_IsGrabbed`, `_IsPosed`, `_Angle`, `_Stretch` and `_Squish` values.
- `contact`: compiled senders and receivers, and `step`, the cross-avatar pass (design doc §5 step 6): one grid
  broadphase over every avatar's contacts on this client, updating each receiver's value for the next frame's
  animator parameters.

Chains step as the VRChat SDK's runtime does, fitted to its recordings: ADR 0049 (which supersedes ADR 0015's
XPBD model) and ADRs 0068 to 0071 and 0074; colliders and limits in ADR 0016; contacts in ADRs 0017 and 0047.
Grab and pose are still ADR 0015's and unverified against the SDK.

Input is an avatar that passed `civbb_format::validate`; indices are not re-checked, parameter ranges are clamped.
