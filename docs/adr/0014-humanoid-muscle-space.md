# 0014: Humanoid muscles follow Unity's muscle space, recorded per bone (format v4)

Status: accepted
Date: 2026-10-28

## Context

Legacy humanoid clips (gestures, locomotion, emotes) animate Unity muscles, not bones (#48). Design doc §7 asks for
legacy animators to work unchanged. Unity documents the 95 muscle names and their default ranges
(`HumanTrait`), but not how a muscle value becomes a local bone rotation. ADR 0013 lets us record that in Unity.

Recordings (`crates/anim/tests/unity/`, Unity 6000.3.27f1): the muscle table with clip property names, and three
generated skeletons (a T-pose, the same joints with arbitrary bone orientations, and non-default twist settings),
each with Unity's per-bone axes (read from `Avatar` internals) and `HumanPoseHandler.SetHumanPose` results for
every muscle alone at -1, -0.5, 0.5 and 1 plus 24 random mixes.

## Decision

`FORMAT_VERSION` goes to 4.

1. **Table.** `civbb_anim::muscle::MUSCLES` lists Unity's 95 muscles in Unity's order: clip property name, bone,
   axis (0 = X twist, 1 = Y, 2 = Z) and default range in degrees. Clips name muscles by clip property, which for
   fingers differs from `HumanTrait.MuscleName` (`LeftHand.Index.1 Stretched`, not `Left Index 1 Stretched`);
   only clip names bind. Verified: a test compares the table with the recording.
2. **Muscle frames in the format.** `Humanoid::muscle_frames` holds per bone: `pre` (muscle space to the parent
   node's space), `post` (muscle space to the bone's own space), `sign` per axis, `min`/`max` per axis in
   degrees (the avatar's limits, Unity's defaults unless the creator changed them), and `twist`. All in our
   axes: from Unity, mirror X, so `pre`/`post` become `(x, -y, -z, w)` and `sign` becomes `(sx, -sy, -sz)`; the
   formulas below are then unchanged. The Unity exporter copies Unity's values (`Avatar.GetPreRotation`,
   `GetPostRotation`, `GetLimitSign`, `HumanDescription` limits and twists). glTF and VRM imports have none yet;
   deriving them from the skeleton is #53.
3. **Muscles to rotation.** Per axis `k` with muscle value `v`: `angle_k = sign_k * v * (v < 0 ? -min_k : max_k)`
   degrees (no clamping; values beyond ±1 extrapolate). With `t_k = tan(angle_k / 2)`:
   `zyroll = normalize(tx, ty + tx*tz, tz - tx*ty, 1)`, a swing `(0, ty, tz, 1)` after a twist `(tx, 0, 0, 1)`,
   both in half-angle tangents. The bone's local rotation is `pre * zyroll * post⁻¹`. Verified to 0.05 degrees on
   all recorded poses. The simpler "swing by angle-axis of (0, ay, az)" reconstruction found online is off by up
   to 22 degrees when Y and Z combine.
4. **Twist split.** Upper arm, lower arm, upper leg and lower leg keep `twist` (Unity's `upperArmTwist` etc.,
   default 0.5) of their X angle. The next bone of the limb (lower arm, hand, lower leg, foot) is turned by the
   remaining `(1 - twist) * angle` about the giving bone's X axis: its local rotation is premultiplied by
   `post_p * twist(rest) * post_p⁻¹`. Verified with twists 0.2, 0.3, 0.7 and 0.9.
5. **Inverse.** `Rig::muscles` inverts 3 and 4 (half-angle tangents solved from the quaternion). It gives the
   rest pose's muscle values, which are the muscle slots' defaults (ADR 0007): an avatar whose animator writes no
   muscle stays in its rest pose. Verified against `HumanPoseHandler.GetHumanPose` on the rest skeletons.
6. **Runtime.** Muscle curves bind to `Target::Muscle(i)` slots and mix like any scalar slot (Write Defaults,
   additive, blend trees). A layer's avatar mask gates muscle slots by the body part of the muscle's bone
   (`BodyPart::of`), and its transform list only gates non-muscle targets. `Instance::human_pose` turns the
   muscle slots into local rotations of every framed bone, unwritten muscles at rest; when any muscle is bound
   these take precedence over rotation slots on the same bones.
7. Not covered: the hips. Their placement comes from body position and rotation (`RootT`/`RootQ`), which Unity
   derives from the centre of mass; with body pose fixed, the recorded hips move with spine and leg muscles.
   Body pose, root motion, IK goal and `TDOF` curves stay unbound (#52).

## Consequences

- Avatars exported from Unity get exact muscle poses. glTF/VRM avatars cannot play muscle clips until #53.
- Unverified, to record later (#52, #54): what Unity does with muscles no layer writes (we hold the rest pose);
  whether generic rotation curves on human bones are ignored on humanoid avatars (we let muscles win when any are
  bound); whether muscle values are clamped anywhere in the animator (we do not clamp); additive humanoid layers
  (we add muscle values like scalars); custom per-bone limits (stored, but recorded only with defaults);
  skeletons that are not in T-pose or lack optional bones.
- Format v4 packages carry about 100 bytes of CBOR per framed bone (5.5 KB for all 55 bones).
