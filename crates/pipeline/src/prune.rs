//! Compiler stage 3 pruning: skin joints no vertex uses, nodes nothing references and blendshapes nothing drives,
//! after checking animations, face setup and face-tracking names (design doc §9, ADR 0026).

use std::collections::HashSet;

use civbb_format::Document;
use civbb_format::animator::{MeshProperty, Target};
use civbb_format::constraint::{ConstraintKind, WorldUp};
use civbb_format::face::EyeDriver;
use civbb_format::hierarchy::NodeIndex;

use crate::renumber::Renumber;

/// How much pruning removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    /// Skin joints, summed over meshes.
    pub joints: usize,
    /// Nodes.
    pub nodes: usize,
    /// Blendshapes, summed over meshes.
    pub shapes: usize,
}

/// Prunes a validated avatar in place; it stays valid and animates, simulates and draws as before.
pub fn prune(a: &mut Document) -> Pruned {
    let joints = prune_joints(a);
    let shapes = prune_shapes(a);
    let nodes = prune_nodes(a);
    Pruned { joints, nodes, shapes }
}

/// Face-tracking blendshape names, kept even when nothing else drives them: Unified Expressions (docs.vrcft.io)
/// and Apple ARKit. Compared without case.
const FACE_TRACKING: &[&str] = &[
    // Unified Expressions base shapes.
    "EyeLookOutRight",
    "EyeLookInRight",
    "EyeLookUpRight",
    "EyeLookDownRight",
    "EyeLookOutLeft",
    "EyeLookInLeft",
    "EyeLookUpLeft",
    "EyeLookDownLeft",
    "EyeClosedRight",
    "EyeClosedLeft",
    "EyeSquintRight",
    "EyeSquintLeft",
    "EyeWideRight",
    "EyeWideLeft",
    "EyeDilationRight",
    "EyeDilationLeft",
    "EyeConstrictRight",
    "EyeConstrictLeft",
    "BrowPinchRight",
    "BrowPinchLeft",
    "BrowLowererRight",
    "BrowLowererLeft",
    "BrowInnerUpRight",
    "BrowInnerUpLeft",
    "BrowOuterUpRight",
    "BrowOuterUpLeft",
    "NoseSneerRight",
    "NoseSneerLeft",
    "NasalDilationRight",
    "NasalDilationLeft",
    "NasalConstrictRight",
    "NasalConstrictLeft",
    "CheekSquintRight",
    "CheekSquintLeft",
    "CheekPuffRight",
    "CheekPuffLeft",
    "CheekSuckRight",
    "CheekSuckLeft",
    "JawOpen",
    "MouthClosed",
    "JawRight",
    "JawLeft",
    "JawForward",
    "JawBackward",
    "JawClench",
    "JawMandibleRaise",
    "LipSuckUpperRight",
    "LipSuckUpperLeft",
    "LipSuckLowerRight",
    "LipSuckLowerLeft",
    "LipSuckCornerRight",
    "LipSuckCornerLeft",
    "LipFunnelUpperRight",
    "LipFunnelUpperLeft",
    "LipFunnelLowerRight",
    "LipFunnelLowerLeft",
    "LipPuckerUpperRight",
    "LipPuckerUpperLeft",
    "LipPuckerLowerRight",
    "LipPuckerLowerLeft",
    "MouthUpperUpRight",
    "MouthUpperUpLeft",
    "MouthLowerDownRight",
    "MouthLowerDownLeft",
    "MouthUpperDeepenRight",
    "MouthUpperDeepenLeft",
    "MouthUpperRight",
    "MouthUpperLeft",
    "MouthLowerRight",
    "MouthLowerLeft",
    "MouthCornerPullRight",
    "MouthCornerPullLeft",
    "MouthCornerSlantRight",
    "MouthCornerSlantLeft",
    "MouthFrownRight",
    "MouthFrownLeft",
    "MouthStretchRight",
    "MouthStretchLeft",
    "MouthDimpleRight",
    "MouthDimpleLeft",
    "MouthRaiserUpper",
    "MouthRaiserLower",
    "MouthPressRight",
    "MouthPressLeft",
    "MouthTightenerRight",
    "MouthTightenerLeft",
    "TongueOut",
    "TongueUp",
    "TongueDown",
    "TongueRight",
    "TongueLeft",
    "TongueRoll",
    "TongueBendDown",
    "TongueCurlUp",
    "TongueSquish",
    "TongueFlat",
    "TongueTwistRight",
    "TongueTwistLeft",
    "SoftPalateClose",
    "ThroatSwallow",
    "NeckFlexRight",
    "NeckFlexLeft",
    // Unified Expressions blended shapes.
    "EyeClosed",
    "EyeWide",
    "EyeSquint",
    "EyeDilation",
    "EyeConstrict",
    "BrowDownRight",
    "BrowDownLeft",
    "BrowDown",
    "BrowInnerUp",
    "BrowUpRight",
    "BrowUpLeft",
    "BrowUp",
    "NoseSneer",
    "NasalDilation",
    "NasalConstrict",
    "CheekPuff",
    "CheekSuck",
    "CheekSquint",
    "LipSuckUpper",
    "LipSuckLower",
    "LipSuck",
    "LipFunnelUpper",
    "LipFunnelLower",
    "LipFunnel",
    "LipPuckerUpper",
    "LipPuckerLower",
    "LipPucker",
    "MouthUpperUp",
    "MouthLowerDown",
    "MouthOpen",
    "MouthRight",
    "MouthLeft",
    "MouthSmileRight",
    "MouthSmileLeft",
    "MouthSmile",
    "MouthSadRight",
    "MouthSadLeft",
    "MouthSad",
    "MouthStretch",
    "MouthDimple",
    "MouthTightener",
    "MouthPress",
    // ARKit names Unified Expressions lacks (the other ARKit names match it without case).
    "eyeBlinkLeft",
    "eyeBlinkRight",
    "mouthClose",
    "mouthFunnel",
    "mouthPucker",
    "mouthRollLower",
    "mouthRollUpper",
    "mouthShrugLower",
    "mouthShrugUpper",
];

/// Removes joints no vertex weights from each skin. Skins whose vertices use every joint, or none, are kept.
fn prune_joints(a: &mut Document) -> usize {
    let mut removed = 0;
    for m in &mut a.meshes {
        let Some(skin) = &mut m.skin else { continue };
        let mut used = vec![false; skin.joints.len()];
        for (js, ws) in skin.vertex_joints.iter().zip(&skin.vertex_weights) {
            for (&j, &w) in js.iter().zip(ws) {
                if w != 0.0 {
                    used[j as usize] = true;
                }
            }
        }
        let kept = used.iter().filter(|&&u| u).count();
        if kept == used.len() || kept == 0 {
            continue;
        }
        let mut map = vec![0u16; used.len()];
        let mut next = 0;
        for (i, &u) in used.iter().enumerate() {
            if u {
                map[i] = next;
                next += 1;
            }
        }
        retain(&mut skin.joints, &used);
        retain(&mut skin.inverse_bind, &used);
        for (js, ws) in skin.vertex_joints.iter_mut().zip(&skin.vertex_weights) {
            for (j, &w) in js.iter_mut().zip(ws) {
                // A zero weight's joint does not matter; point it at a joint that exists.
                *j = if w != 0.0 { map[*j as usize] } else { 0 };
            }
        }
        removed += used.len() - kept;
    }
    removed
}

/// Removes blendshapes that rest at 0 and nothing drives: no clip curve targets them, the face
/// setup does not bind them and their name is not a face-tracking name.
fn prune_shapes(a: &mut Document) -> usize {
    let animated: HashSet<(u32, u32)> = (a.clips.iter().flat_map(|c| &c.curves))
        .filter_map(|c| match c.target {
            Target::Mesh {
                mesh,
                property: MeshProperty::Weight { shape: s },
            } => Some((mesh, s)),
            _ => None,
        })
        .collect();
    let mut bound = HashSet::new();
    if let Some(face) = a.avatar().and_then(|r| r.face.as_ref()) {
        let mut binds: Vec<_> = face.visemes.iter().flat_map(|v| &v.binds).chain(&face.blink).collect();
        if let Some(EyeDriver::Shapes { up, down, left, right }) = face.eyes.as_ref().map(|e| &e.driver) {
            binds.extend(up.iter().chain(down).chain(left).chain(right));
        }
        bound.extend(binds.iter().map(|b| (b.mesh, b.shape)));
    }
    let mut removed = 0;
    let mut maps = Vec::with_capacity(a.meshes.len());
    for (mi, m) in a.meshes.iter_mut().enumerate() {
        let keep: Vec<bool> = m
            .blend_shapes
            .iter()
            .enumerate()
            .map(|(si, s)| {
                s.default_weight != 0.0
                    || bound.contains(&(mi as u32, si as u32))
                    || animated.contains(&(mi as u32, si as u32))
                    || FACE_TRACKING.iter().any(|n| n.eq_ignore_ascii_case(&s.name))
            })
            .collect();
        let mut map = Vec::with_capacity(keep.len());
        let mut next = 0u32;
        for &k in &keep {
            map.push(k.then_some(next));
            next += u32::from(k);
        }
        removed += keep.len() - next as usize;
        retain(&mut m.blend_shapes, &keep);
        maps.push(map);
    }
    if let Some(face) = a.avatar_mut().and_then(|r| r.face.as_mut()) {
        let mut binds: Vec<_> = face
            .visemes
            .iter_mut()
            .flat_map(|v| &mut v.binds)
            .chain(&mut face.blink)
            .collect();
        if let Some(EyeDriver::Shapes { up, down, left, right }) = face.eyes.as_mut().map(|e| &mut e.driver) {
            binds.extend(up.iter_mut().chain(down).chain(left).chain(right));
        }
        for b in binds {
            b.shape = maps[b.mesh as usize][b.shape as usize].expect("bound shapes are kept");
        }
    }
    Renumber {
        shapes: maps,
        ..Renumber::default()
    }
    .apply(a);
    removed
}

/// Removes nodes that nothing references and that have no referenced descendant, so every kept node keeps its
/// path, parent and world placement. Descendants of a chain root are referenced: the chain simulates them.
fn prune_nodes(a: &mut Document) -> usize {
    let n = a.hierarchy.nodes.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    for_each_node(a, |&mut i| keep[i as usize] = true);
    // Curves on meshes and constraints keep their nodes through `for_each_node`.
    for c in a.clips.iter().flat_map(|c| &c.curves) {
        if let Target::Node { node, .. } = c.target {
            keep[node as usize] = true;
        }
    }
    let parents: Vec<Option<NodeIndex>> = a.hierarchy.nodes.iter().map(|n| n.parent).collect();
    let mut in_chain = vec![false; n];
    for c in &a.dynamics.chains {
        in_chain[c.root as usize] = true;
    }
    // Parents precede children: one pass down marks chain descendants, one pass up keeps ancestors.
    for i in 1..n {
        let p = parents[i].expect("only the root lacks a parent") as usize;
        in_chain[i] |= in_chain[p];
        keep[i] |= in_chain[i];
    }
    for i in (1..n).rev() {
        if keep[i] {
            keep[parents[i].expect("not the root") as usize] = true;
        }
    }
    let kept = keep.iter().filter(|&&k| k).count();
    if kept == n {
        return 0;
    }
    let mut map = vec![0; n];
    let mut next = 0;
    for i in 0..n {
        map[i] = next;
        next += NodeIndex::from(keep[i]);
    }
    retain(&mut a.hierarchy.nodes, &keep);
    for node in &mut a.hierarchy.nodes {
        if let Some(p) = &mut node.parent {
            *p = map[*p as usize];
        }
    }
    for_each_node(a, |i| *i = map[*i as usize]);
    Renumber {
        nodes: (0..n).map(|i| keep[i].then_some(map[i])).collect(),
        ..Renumber::default()
    }
    .apply(a);
    n - kept
}

/// Calls `f` on every node reference outside the hierarchy itself.
fn for_each_node(a: &mut Document, mut f: impl FnMut(&mut NodeIndex)) {
    for m in &mut a.meshes {
        f(&mut m.node);
        if let Some(skin) = &mut m.skin {
            skin.joints.iter_mut().for_each(&mut f);
        }
    }
    if let Some(h) = a.avatar_mut().and_then(|r| r.humanoid.as_mut()) {
        h.bones.iter_mut().for_each(|b| f(&mut b.node));
    }
    if let Some(EyeDriver::Bones { left, right }) = a
        .avatar_mut()
        .and_then(|r| r.face.as_mut())
        .and_then(|f| f.eyes.as_mut())
        .map(|e| &mut e.driver)
    {
        left.iter_mut().chain(right).for_each(&mut f);
    }
    for c in &mut a.dynamics.chains {
        f(&mut c.root);
        c.ignore.iter_mut().for_each(&mut f);
    }
    a.dynamics.colliders.iter_mut().for_each(|c| f(&mut c.node));
    let volumes = a.contacts.senders.iter_mut().map(|s| &mut s.volume);
    for v in volumes.chain(a.contacts.receivers.iter_mut().map(|r| &mut r.volume)) {
        f(&mut v.node);
        v.component.iter_mut().for_each(&mut f);
    }
    for c in &mut a.constraints {
        f(&mut c.target);
        c.sources.iter_mut().for_each(|s| f(&mut s.node));
        match &mut c.kind {
            ConstraintKind::Aim {
                world_up: WorldUp::ObjectUp { node: n } | WorldUp::ObjectRotationUp { node: n, up: _ },
                ..
            } => f(n),
            ConstraintKind::LookAt {
                world_up_node: Some(n), ..
            } => f(n),
            _ => {}
        }
    }
}

/// Keeps the elements whose flag in `keep` is set.
fn retain<T>(v: &mut Vec<T>, keep: &[bool]) {
    let mut k = keep.iter();
    v.retain(|_| *k.next().expect("a flag per element"));
}
