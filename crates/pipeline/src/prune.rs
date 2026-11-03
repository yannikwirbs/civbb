//! Compiler stage 3 pruning: skin joints no vertex uses, nodes nothing references and blendshapes nothing drives,
//! after checking animations, face setup and face-tracking names (design doc §9, ADR 0026).

use std::collections::{HashMap, HashSet};

use civbb_format::Avatar;
use civbb_format::animator::Property;
use civbb_format::constraint::{ConstraintKind, WorldUp};
use civbb_format::face::EyeDriver;
use civbb_format::hierarchy::{Hierarchy, NodeIndex};

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
pub fn prune(a: &mut Avatar) -> Pruned {
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
fn prune_joints(a: &mut Avatar) -> usize {
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

/// Removes blendshapes that rest at 0 and nothing drives: no clip curve on the mesh's path names them, the face
/// setup does not bind them and their name is not a face-tracking name.
fn prune_shapes(a: &mut Avatar) -> usize {
    let paths = node_paths(&a.hierarchy);
    let animated: HashSet<(&str, &str)> = a
        .clips
        .iter()
        .flat_map(|c| &c.curves)
        .filter_map(|c| match &c.binding.property {
            Property::BlendShape(name) => Some((c.binding.path.as_str(), name.as_str())),
            _ => None,
        })
        .collect();
    let mut bound = HashSet::new();
    if let Some(face) = &a.face {
        let mut binds: Vec<_> = face.visemes.iter().flat_map(|(_, b)| b).chain(&face.blink).collect();
        if let Some(EyeDriver::Shapes { up, down, left, right }) = face.eyes.as_ref().map(|e| &e.driver) {
            binds.extend(up.iter().chain(down).chain(left).chain(right));
        }
        bound.extend(binds.iter().map(|b| (b.mesh, b.shape)));
    }
    let mut removed = 0;
    let mut maps = Vec::with_capacity(a.meshes.len());
    for (mi, m) in a.meshes.iter_mut().enumerate() {
        let path = paths[m.node as usize].as_str();
        let keep: Vec<bool> = m
            .blend_shapes
            .iter()
            .enumerate()
            .map(|(si, s)| {
                s.default_weight != 0.0
                    || bound.contains(&(mi as u32, si as u32))
                    || animated.contains(&(path, s.name.as_str()))
                    || FACE_TRACKING.iter().any(|n| n.eq_ignore_ascii_case(&s.name))
            })
            .collect();
        let mut map = Vec::with_capacity(keep.len());
        let mut next = 0u32;
        for &k in &keep {
            map.push(next);
            next += u32::from(k);
        }
        removed += keep.len() - next as usize;
        retain(&mut m.blend_shapes, &keep);
        maps.push(map);
    }
    if let Some(face) = &mut a.face {
        let mut binds: Vec<_> = face
            .visemes
            .iter_mut()
            .flat_map(|(_, b)| b)
            .chain(&mut face.blink)
            .collect();
        if let Some(EyeDriver::Shapes { up, down, left, right }) = face.eyes.as_mut().map(|e| &mut e.driver) {
            binds.extend(up.iter_mut().chain(down).chain(left).chain(right));
        }
        for b in binds {
            b.shape = maps[b.mesh as usize][b.shape as usize];
        }
    }
    removed
}

/// Removes nodes that nothing references and that have no referenced descendant, so every kept node keeps its
/// path, parent and world placement. Descendants of a chain root are referenced: the chain simulates them.
fn prune_nodes(a: &mut Avatar) -> usize {
    let n = a.hierarchy.nodes.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    for_each_node(a, |&mut i| keep[i as usize] = true);
    let paths = node_paths(&a.hierarchy);
    let by_path: HashMap<&str, usize> = paths.iter().enumerate().rev().map(|(i, p)| (p.as_str(), i)).collect();
    for c in a.clips.iter().flat_map(|c| &c.curves) {
        let on_node = !matches!(
            c.binding.property,
            Property::Parameter(_) | Property::Muscle(_) | Property::RootMotion(_)
        );
        if let (true, Some(&i)) = (on_node, by_path.get(c.binding.path.as_str())) {
            keep[i] = true;
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
    n - kept
}

/// Calls `f` on every node reference outside the hierarchy itself.
fn for_each_node(a: &mut Avatar, mut f: impl FnMut(&mut NodeIndex)) {
    for m in &mut a.meshes {
        f(&mut m.node);
        if let Some(skin) = &mut m.skin {
            skin.joints.iter_mut().for_each(&mut f);
        }
    }
    if let Some(h) = &mut a.humanoid {
        h.bones.iter_mut().for_each(|(_, n)| f(n));
    }
    if let Some(EyeDriver::Bones { left, right }) = a.face.as_mut().and_then(|f| f.eyes.as_mut()).map(|e| &mut e.driver)
    {
        left.iter_mut().chain(right).for_each(&mut f);
    }
    for c in &mut a.dynamics.chains {
        f(&mut c.root);
        c.ignore.iter_mut().for_each(&mut f);
    }
    a.dynamics.colliders.iter_mut().for_each(|c| f(&mut c.node));
    a.contacts.senders.iter_mut().for_each(|s| f(&mut s.volume.node));
    a.contacts.receivers.iter_mut().for_each(|r| f(&mut r.volume.node));
    for c in &mut a.constraints {
        f(&mut c.target);
        c.sources.iter_mut().for_each(|s| f(&mut s.node));
        match &mut c.kind {
            ConstraintKind::Aim {
                world_up: WorldUp::ObjectUp(n) | WorldUp::ObjectRotationUp(n, _),
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

/// Unity-style binding path of every node, as `civbb-anim` resolves clip paths: names below the root joined by `/`.
fn node_paths(h: &Hierarchy) -> Vec<String> {
    let mut paths: Vec<String> = Vec::with_capacity(h.nodes.len());
    for n in &h.nodes {
        paths.push(match n.parent {
            None => String::new(),
            Some(0) => n.name.clone(),
            Some(p) => format!("{}/{}", paths[p as usize], n.name),
        });
    }
    paths
}
