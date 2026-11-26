//! The Unity exporter's output against Unity itself (#36): clips sampled by civbb-anim on an exported GLB match
//! Unity sampling the same clip on the prefab, and the exported constraints match Unity's results. Local only: reads
//! `corpus/local/unity-truth/<name>.json` (recorded by `tools/unity-truth/Assets/Editor/Exported.cs`) with
//! `corpus/local/unity-glb/<name>.glb`, and skips when they are absent (third-party avatars stay out of git).

use std::collections::HashMap;
use std::path::Path;

use civbb_anim::constraint::Scratch;
use civbb_anim::{Constraints, Target, bind};
use civbb_core::math::{self, world_poses};
use civbb_format::Avatar;
use civbb_format::hierarchy::HumanBone;
use civbb_format::math::{Quat, Transform};
use civbb_pipeline::gltf_import::{Limits, import};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    paths: Vec<String>,
    clips: Vec<Sample>,
    constraints: Vec<Pose>,
}

#[derive(Deserialize)]
struct Sample {
    clip: String,
    time: f32,
    /// Unity-space `[p, q, s]` per recorded path.
    locals: Vec<[f32; 10]>,
    /// Renderer path to shape name to weight (0 to 100); zeros left out.
    shapes: HashMap<String, HashMap<String, f32>>,
}

#[derive(Deserialize)]
struct Pose {
    locals: Vec<[f32; 10]>,
}

/// A recorded Unity local transform in glTF space (ADR 0038's X mirror).
fn mirrored(l: &[f32; 10]) -> Transform {
    Transform {
        translation: [-l[0], l[1], l[2]],
        rotation: [l[3], -l[4], -l[5], l[6]],
        scale: [l[7], l[8], l[9]],
    }
}

/// Angle between two rotations in degrees, either sign.
fn angle(a: Quat, b: Quat) -> f32 {
    let n = |q: Quat| {
        let l = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        q.map(|x| x / l)
    };
    let (a, b) = (n(a), n(b));
    // For unit quaternions |a - b| = 2 sin(angle / 4); stable near 0, where acos is not (LESSONS).
    let d = |s: f32| a.iter().zip(b).map(|(x, y)| (x - s * y).powi(2)).sum::<f32>().sqrt();
    (4.0 * (d(1.0).min(d(-1.0)) / 2.0).min(1.0).asin()).to_degrees()
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
}

/// Largest errors of one comparison, with where they happened.
/// One bone of the skeleton Unity built an `Avatar` from, in Unity's axes.
#[derive(Deserialize)]
struct SkeletonBone {
    name: String,
    position: [f32; 3],
    rotation: [f32; 4],
}

#[derive(Default)]
struct Worst {
    rotation: (f32, String),
    position: (f32, String),
    scale: (f32, String),
    shape: (f32, String),
    /// The hips placed from the body pose, in world space (ADR 0063).
    hips_rotation: (f32, String),
    hips_position: (f32, String),
    compared: usize,
}

impl Worst {
    fn note(slot: &mut (f32, String), e: f32, at: impl FnOnce() -> String) {
        if e > slot.0 {
            *slot = (e, at());
        }
    }
}

struct Loaded {
    avatar: Avatar,
    /// Recorded path index to hierarchy node, for unambiguous paths.
    node: Vec<Option<usize>>,
}

fn load(glb: &Path, rec: &Recording) -> Loaded {
    let bytes = std::fs::read(glb).unwrap();
    let avatar = import(&bytes, &Limits::default(), |_| None)
        .unwrap_or_else(|e| panic!("{}: {e}", glb.display()))
        .avatar;
    let mut by_path: HashMap<String, Option<usize>> = HashMap::new();
    for (i, p) in node_paths(&avatar.hierarchy).into_iter().enumerate() {
        by_path.entry(p).and_modify(|n| *n = None).or_insert(Some(i));
    }
    let node = rec.paths.iter().map(|p| by_path.get(p).copied().flatten()).collect();
    Loaded { avatar, node }
}

/// Samples each recorded clip with civbb-anim and compares the nodes and shapes it animates with Unity's.
fn clips(l: &Loaded, rec: &Recording) -> HashMap<String, Worst> {
    let a = &l.avatar;
    let bound = bind(a);
    let targets = &bound.bindings.targets;
    let hips = a.humanoid.as_ref().and_then(|h| h.node(HumanBone::Hips));
    let rest: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
    let mut out: HashMap<String, Worst> = HashMap::new();
    for s in &rec.clips {
        let ci = a
            .clips
            .iter()
            .position(|c| c.name == s.clip)
            .unwrap_or_else(|| panic!("no exported clip {}", s.clip));
        let clip = &bound.clips[ci];
        let mut slots = bound.bindings.defaults.clone();
        clip.sample(s.time, &mut slots);
        // What the clip drives: its slots' nodes and, with muscles, every framed human bone.
        let mut locals = rest.clone();
        let mut driven = vec![false; locals.len()];
        let mut muscles = bound.rig.as_ref().map(|r| r.rest);
        let mut any_muscle = false;
        for &slot in &clip.slots {
            let v = slots[slot as usize];
            match targets[slot as usize] {
                Target::Position(n, k) => {
                    locals[n as usize].translation[k as usize] = v;
                    driven[n as usize] = true;
                }
                Target::Rotation(n, k) => {
                    locals[n as usize].rotation[k as usize] = v;
                    driven[n as usize] = true;
                }
                Target::Scale(n, k) => {
                    locals[n as usize].scale[k as usize] = v;
                    driven[n as usize] = true;
                }
                Target::Muscle(m) => {
                    if let Some(ms) = &mut muscles {
                        ms[m as usize] = v;
                        any_muscle = true;
                    }
                }
                _ => {}
            }
        }
        if let (true, Some(rig), Some(ms)) = (any_muscle, &bound.rig, &muscles) {
            for (n, q) in rig.rotations(ms) {
                locals[n as usize].rotation = q;
                driven[n as usize] = true;
            }
        }
        // A clip with body pose curves places the hips (ADR 0063).
        if let (Some(b), Some(body), Some(h)) = (&clip.body, &bound.body, hips) {
            let pose: [f32; 7] = std::array::from_fn(|i| slots[b.slot as usize + i]);
            body.place(pose, &mut locals);
            let mut theirs = locals.clone();
            theirs[h as usize] = mirrored(&s.locals[l.node.iter().position(|n| *n == Some(h as usize)).unwrap()]);
            let parents: Vec<_> = a.hierarchy.nodes.iter().map(|n| n.parent).collect();
            let ours = world_poses(&parents, math::Pose::IDENTITY, &locals)[h as usize];
            let theirs = world_poses(&parents, math::Pose::IDENTITY, &theirs)[h as usize];
            let at = || format!("{} t={}", s.clip, s.time);
            let w = out.entry(s.clip.clone()).or_default();
            Worst::note(&mut w.hips_rotation, angle(ours.rot, theirs.rot), at);
            Worst::note(&mut w.hips_position, dist(ours.pos, theirs.pos), at);
        }
        let w = out.entry(s.clip.clone()).or_default();
        for (ri, n) in l.node.iter().enumerate() {
            let Some(n) = *n else { continue };
            if !driven[n] || n == 0 || Some(n as u32) == hips {
                continue; // the root and the hips follow the body pose, which is #52
            }
            let unity = mirrored(&s.locals[ri]);
            let at = || format!("{} t={} {}", s.clip, s.time, rec.paths[ri]);
            Worst::note(&mut w.rotation, angle(locals[n].rotation, unity.rotation), at);
            Worst::note(&mut w.position, dist(locals[n].translation, unity.translation), at);
            Worst::note(&mut w.scale, dist(locals[n].scale, unity.scale), at);
            w.compared += 1;
        }
        let paths = node_paths(&a.hierarchy);
        for &slot in &clip.slots {
            if let Target::BlendShape(m, i) = targets[slot as usize] {
                let mesh = &a.meshes[m as usize];
                let name = &mesh.blend_shapes[i as usize].name;
                let unity = s
                    .shapes
                    .get(&paths[mesh.node as usize])
                    .and_then(|w| w.get(name))
                    .copied()
                    .unwrap_or(0.0);
                let at = || format!("{} t={} {name}", s.clip, s.time);
                Worst::note(&mut w.shape, (slots[slot as usize] - unity).abs(), at);
                w.compared += 1;
            }
        }
    }
    out
}

/// Runs the exported constraints on Unity's recorded source poses and compares their targets.
fn constraints(l: &Loaded, rec: &Recording) -> Worst {
    let a = &l.avatar;
    let pass = Constraints::new(a);
    let mut scratch = Scratch::default();
    let mut w = Worst::default();
    let targets: Vec<usize> = a.constraints.iter().map(|c| c.target as usize).collect();
    for (pi, p) in rec.constraints.iter().enumerate() {
        let mut locals: Vec<Transform> = a.hierarchy.nodes.iter().map(|n| n.rest).collect();
        for (ri, n) in l.node.iter().enumerate() {
            if let Some(n) = *n
                && !targets.contains(&n)
            {
                locals[n] = mirrored(&p.locals[ri]);
            }
        }
        pass.evaluate(&mut locals, &mut scratch);
        for (ri, n) in l.node.iter().enumerate() {
            if let Some(n) = *n
                && targets.contains(&n)
            {
                let unity = mirrored(&p.locals[ri]);
                let at = || format!("pose {pi} {}", rec.paths[ri]);
                Worst::note(&mut w.rotation, angle(locals[n].rotation, unity.rotation), at);
                Worst::note(&mut w.position, dist(locals[n].translation, unity.translation), at);
                w.compared += 1;
            }
        }
    }
    w
}

#[test]
fn exported_avatars_match_unity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local");
    let Ok(dir) = std::fs::read_dir(root.join("unity-truth")) else {
        eprintln!("corpus/local/unity-truth absent; skipped");
        return;
    };
    let mut failures = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let glb = root.join("unity-glb").join(format!("{name}.glb"));
        if path.extension().is_none_or(|e| e != "json") || !glb.exists() {
            continue;
        }
        let rec: Recording = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let l = load(&glb, &rec);
        let mut report = |what: String, w: &Worst| {
            eprintln!(
                "{name} {what}: {} compared, rotation {:.4} deg ({}), position {:.6} m ({}), scale {:.6} ({}), shape {:.4} ({})",
                w.compared,
                w.rotation.0,
                w.rotation.1,
                w.position.0,
                w.position.1,
                w.scale.0,
                w.scale.1,
                w.shape.0,
                w.shape.1
            );
            // Unity's humanoid evaluation and ours agree to 0.1 degrees on recorded skeletons (ADR 0014).
            if w.hips_position.0 > 0.0 {
                eprintln!(
                    "{name} {what}: hips rotation {:.3} deg, position {:.4} m ({})",
                    w.hips_rotation.0, w.hips_position.0, w.hips_position.1
                );
            }
            // Hips placed from the body pose with the fitted mass model (ADR 0073): within 1.5 mm.
            let hips = w.hips_position.0 > 0.0015 || w.hips_rotation.0 > 0.1;
            if w.compared == 0
                || w.rotation.0 > 0.1
                || w.position.0 > 1e-4
                || w.scale.0 > 1e-4
                || w.shape.0 > 0.01
                || hips
            {
                failures.push(format!("{name} {what}"));
            }
        };
        let mut by_clip: Vec<_> = clips(&l, &rec).into_iter().collect();
        by_clip.sort_by(|x, y| x.0.cmp(&y.0));
        for (clip, w) in &by_clip {
            report(format!("clip {clip}"), w);
        }
        if !rec.constraints.is_empty() {
            report("constraints".into(), &constraints(&l, &rec));
        }
    }
    assert!(failures.is_empty(), "differ from Unity: {failures:?}");
}

/// The descriptor's standard colliders export as senders with VRChat's body-part tags on or below the matching
/// humanoid bone (ADR 0061; the SDK puts feet on toes and fingers on their last bone): a swapped side or finger would
/// make other avatars' receivers fire on the wrong hand.
#[test]
fn exported_standard_senders_sit_on_their_bones() {
    use HumanBone as H;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/unity-glb");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("corpus/local/unity-glb absent; skipped");
        return;
    };
    let expected = [
        ("Head", H::Head),
        ("HandL", H::LeftHand),
        ("HandR", H::RightHand),
        ("FootL", H::LeftFoot),
        ("FootR", H::RightFoot),
        ("FingerIndexL", H::LeftIndexProximal),
        ("FingerMiddleL", H::LeftMiddleProximal),
        ("FingerRingL", H::LeftRingProximal),
        ("FingerLittleL", H::LeftLittleProximal),
        ("FingerIndexR", H::RightIndexProximal),
        ("FingerMiddleR", H::RightMiddleProximal),
        ("FingerRingR", H::RightRingProximal),
        ("FingerLittleR", H::RightLittleProximal),
    ];
    let mut checked = 0;
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "glb") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let avatar = import(&bytes, &Limits::default(), |_| None).unwrap().avatar;
        let Some(h) = &avatar.humanoid else { continue };
        let senders = &avatar.contacts.senders;
        assert!(
            senders.iter().any(|s| s.tags.iter().any(|t| t == "Torso")),
            "{}: no Torso sender",
            path.display()
        );
        for (tag, bone) in expected {
            let (Some(node), Some(s)) = (h.node(bone), senders.iter().find(|s| s.tags.iter().any(|t| t == tag))) else {
                continue;
            };
            let nodes = &avatar.hierarchy.nodes;
            let below = std::iter::successors(Some(s.volume.node), |&n| nodes[n as usize].parent).any(|n| n == node);
            assert!(below, "{}: {tag} not on or below {bone:?}", path.display());
            checked += 1;
        }
    }
    eprintln!("{checked} standard senders on their bones");
}

/// Muscle frames derived (ADR 0064) against the ones Unity built for each export, from the skeleton Unity built the
/// `Avatar` from (`corpus/local/unity-truth/<name>.skeleton.json` by `BuildSkeleton.cs`: thumbs and fingers often
/// differ from the prefab's rest, ADR 0072): every bone within 0.15 degrees (Blackbird's shoulders 1.75). Without
/// that file the prefab's rest is used: body and limbs within 2.5 degrees, fingers 8, thumbs and eyes reported, and
/// avatars built from another pose (KohzieBoi, Megumi, Taro) only reported.
#[test]
fn derived_muscle_frames_match_unity_exports() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/unity-glb");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("corpus/local/unity-glb absent; skipped");
        return;
    };
    let mut failures = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "glb") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).unwrap();
        let mut avatar = import(&bytes, &Limits::default(), |_| None).unwrap().avatar;
        let skeleton = dir.join(format!("../unity-truth/{name}.skeleton.json"));
        let built = std::fs::read_to_string(&skeleton).ok().map(|text| {
            let bones: Vec<SkeletonBone> = serde_json::from_str(&text).unwrap();
            for b in bones {
                if let Some(n) = avatar.hierarchy.nodes.iter_mut().find(|n| n.name == b.name) {
                    let (p, q) = (b.position, b.rotation);
                    n.rest.translation = [-p[0], p[1], p[2]];
                    n.rest.rotation = [q[0], -q[1], -q[2], q[3]];
                }
            }
        });
        let Some(h) = &avatar.humanoid else { continue };
        let derived = civbb_anim::frames::derive(&avatar.hierarchy, h);
        let (mut worst, mut at) = (0.0f32, String::new());
        for f in &h.muscle_frames {
            let Some(d) = derived.iter().find(|d| d.bone == f.bone) else {
                failures.push(format!("{name} {:?}: no derived frame", f.bone));
                continue;
            };
            let e = angle(d.pre, f.pre).max(angle(d.post, f.post));
            let bone = format!("{:?}", f.bone);
            let finger = ["Proximal", "Intermediate", "Distal"].iter().any(|j| bone.ends_with(j));
            let checked = built.is_some() || (!bone.contains("Thumb") && !bone.ends_with("Eye"));
            // From the prefab's rest, fingers get a wider bound: Unity built them from another pose.
            let e = if finger && built.is_none() { e * 2.5 / 8.0 } else { e };
            if e > 0.1 && std::env::var_os("CIVBB_FRAMES").is_some() {
                eprintln!(
                    "  {name} {:?}: pre {:.2} post {:.2}",
                    f.bone,
                    angle(d.pre, f.pre),
                    angle(d.post, f.post)
                );
            }
            if checked && e > worst {
                (worst, at) = (e, format!("{:?}", f.bone));
            }
            if d.sign != f.sign {
                failures.push(format!("{name} {:?}: sign {:?}, Unity {:?}", f.bone, d.sign, f.sign));
            }
        }
        eprintln!(
            "{name}: {} frames, worst {worst:.3} degrees ({at})",
            h.muscle_frames.len()
        );
        let bound = match (&built, name.starts_with("Blackbird")) {
            (Some(()), false) => 0.15,
            (Some(()), true) => 1.8,
            (None, _) => 2.5,
        };
        if worst > bound && (built.is_some() || !["KohzieBoi", "Megumi", "Taro"].contains(&name.as_str())) {
            failures.push(format!("{name}: {at} off by {worst} degrees"));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// Unity's path of every node, as the recordings name them: names below the root joined by `/`.
fn node_paths(h: &civbb_format::hierarchy::Hierarchy) -> Vec<String> {
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
