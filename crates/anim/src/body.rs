//! Unity's humanoid body pose (#52): a pose's centre of mass and body orientation from its bones' positions, the
//! quantities clips' `RootT` and `RootQ` curves set. Fitted to `HumanPoseHandler` recordings (`tests/unity/
//! body_pose.json`): orientation within 0.35 degrees, centre of mass within 6 mm at a 1.07 m human scale.

use civbb_core::math::{add, cross, normalize, scale, sub};
use civbb_format::hierarchy::HumanBone::{self, *};
use civbb_format::math::{Quat, Vec3};

/// Each bone's own mass and the bone its mass centre lies halfway to (none: at the bone). Unity's
/// `HumanTrait.GetBoneDefaultHierarchyMass` gives a subtree's mass; a bone's own is that minus its children's.
const MASSES: [(HumanBone, Option<HumanBone>, f32); 22] = [
    (Hips, None, 12.0),
    (Spine, Some(Chest), 2.5),
    (Chest, Some(UpperChest), 12.0),
    (UpperChest, Some(Neck), 12.0),
    (Neck, Some(Head), 1.0),
    (Head, None, 4.0),
    (LeftUpperLeg, Some(LeftLowerLeg), 10.0),
    (LeftLowerLeg, Some(LeftFoot), 4.0),
    (LeftFoot, Some(LeftToes), 0.8),
    (LeftToes, None, 0.2),
    (RightUpperLeg, Some(RightLowerLeg), 10.0),
    (RightLowerLeg, Some(RightFoot), 4.0),
    (RightFoot, Some(RightToes), 0.8),
    (RightToes, None, 0.2),
    (LeftShoulder, Some(LeftUpperArm), 0.5),
    (LeftUpperArm, Some(LeftLowerArm), 2.0),
    (LeftLowerArm, Some(LeftHand), 1.5),
    (LeftHand, None, 0.5),
    (RightShoulder, Some(RightUpperArm), 0.5),
    (RightUpperArm, Some(RightLowerArm), 2.0),
    (RightLowerArm, Some(RightHand), 1.5),
    (RightHand, None, 0.5),
];

/// The mass-weighted centre of the bones `pos` places. A missing bone adds no mass; a missing child puts the mass
/// at the bone (the recordings have every bone, so both are unverified).
pub fn center_of_mass(pos: impl Fn(HumanBone) -> Option<Vec3>) -> Vec3 {
    let (mut sum, mut total) = ([0.0; 3], 0.0);
    for (bone, child, mass) in MASSES {
        if let Some(p) = pos(bone) {
            let at = child.and_then(&pos).map_or(p, |c| scale(add(p, c), 0.5));
            sum = add(sum, scale(at, mass));
            total += mass;
        }
    }
    if total > 0.0 { scale(sum, 1.0 / total) } else { sum }
}

/// The body's axes as a rotation: Y from the upper legs' midpoint to the upper arms', X the sum of the upper arms'
/// and upper legs' left-to-right vectors made perpendicular to Y. Unity's body rotation is this relative to the
/// rest pose's: `orientation(pose) * conj(orientation(rest))`. `None` without both upper arms and upper legs.
pub fn orientation(pos: impl Fn(HumanBone) -> Option<Vec3>) -> Option<Quat> {
    let (la, ra) = (pos(LeftUpperArm)?, pos(RightUpperArm)?);
    let (ll, rl) = (pos(LeftUpperLeg)?, pos(RightUpperLeg)?);
    let up = normalize(sub(scale(add(la, ra), 0.5), scale(add(ll, rl), 0.5)));
    let side = add(sub(ra, la), sub(rl, ll));
    let z = normalize(cross(side, up));
    Some(basis(cross(up, z), up, z))
}

/// The rotation whose columns are these orthonormal axes.
fn basis(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    let trace = x[0] + y[1] + z[2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(y[2] - z[1]) / s, (z[0] - x[2]) / s, (x[1] - y[0]) / s, 0.25 * s]
    } else if x[0] > y[1] && x[0] > z[2] {
        let s = (1.0 + x[0] - y[1] - z[2]).sqrt() * 2.0;
        [0.25 * s, (y[0] + x[1]) / s, (z[0] + x[2]) / s, (y[2] - z[1]) / s]
    } else if y[1] > z[2] {
        let s = (1.0 + y[1] - x[0] - z[2]).sqrt() * 2.0;
        [(y[0] + x[1]) / s, 0.25 * s, (z[1] + y[2]) / s, (z[0] - x[2]) / s]
    } else {
        let s = (1.0 + z[2] - x[0] - y[1]).sqrt() * 2.0;
        [(z[0] + x[2]) / s, (z[1] + y[2]) / s, 0.25 * s, (x[1] - y[0]) / s]
    };
    civbb_core::math::qnormalize(q)
}
