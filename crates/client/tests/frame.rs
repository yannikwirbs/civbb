//! Islands and frames (ADR 0020): animator toggles reach node activity, contact values reach parameters one frame
//! later, every system reports per island, results do not depend on the number of workers, and animated materials
//! (ADR 0025) and constraint fields (ADR 0040) belong to their island.

use std::sync::Arc;

use civbb_client::{Island, Model, frame};
use civbb_core::math::Pose;
use civbb_core::{System, jobs};
use civbb_format::Document;
use civbb_format::animator::{MeshProperty, Target};
use civbb_format::dynamics::ReceiverKind;
use civbb_format::material::{Material, MaterialField, MaterialModel};
use civbb_format::state::{Entry, EntryKind};

const DT: f32 = 1.0 / 90.0;

fn island(avatar: &Document, id: u64, local: bool) -> Island {
    let origin = Pose {
        pos: [id as f32 * 2.0, 0.0, 0.0],
        ..Pose::IDENTITY
    };
    Island::new(Arc::new(Model::new(avatar)), id, local, origin)
}

#[test]
fn a_toggle_parameter_switches_its_node() {
    let avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let name = avatar.state.entries[0].name.clone();
    let node = avatar.hierarchy.nodes.iter().position(|n| n.name == name).unwrap();
    let mut i = island(&avatar, 0, true);
    let mut c = Default::default();
    i.step(DT, &mut c);
    let start = i.active()[node];
    assert_eq!(start, avatar.hierarchy.nodes[node].active);

    let p = i.model().controller().unwrap().param(&name).unwrap() as usize;
    i.animator_mut().unwrap().params[p] = f32::from(u8::from(!start));
    i.step(DT, &mut c);
    assert_eq!(i.active()[node], !start);
}

#[test]
fn receiver_values_reach_parameters_on_the_next_frame() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    // The corpus receiver sits on the head; put a sender there too and let the avatar touch itself.
    let receiver = &mut avatar.contacts.receivers[0];
    receiver.allow_self = true;
    // Constant: 1 from the first frame of contact (Proximity starts on the second, ADR 0047).
    receiver.kind = ReceiverKind::Constant;
    let p = avatar.state.entries.len();
    receiver.parameter = Some(p as u32);
    let head = receiver.volume.node;
    avatar.contacts.senders[0].volume.node = head;
    avatar.state.entries.push(Entry::new("HeadPat", EntryKind::Float, 0.0));
    let mut islands = vec![island(&avatar, 0, true)];

    frame::step(&mut islands, |_, _| true, DT);
    assert_eq!(islands[0].contact_values(), [1.0]);
    assert_eq!(islands[0].animator_mut().unwrap().params[p], 0.0);
    frame::step(&mut islands, |_, _| true, DT);
    assert_eq!(islands[0].animator_mut().unwrap().params[p], 1.0);
}

/// Chain outputs (ADR 0088) write their entries by index: the bloated corpus's first tail reports its angle.
#[test]
fn chain_outputs_reach_their_entries() {
    let avatar = civbb_corpus::generate(&civbb_corpus::bloated());
    let outputs = avatar.dynamics.chains[0].outputs.clone();
    let (angle, grabbed) = (outputs.angle.unwrap() as usize, outputs.is_grabbed.unwrap() as usize);
    let mut i = island(&avatar, 0, true);
    let mut c = Default::default();
    i.step(DT, &mut c);
    assert_eq!(
        i.animator_mut().unwrap().params[angle],
        0.0,
        "no angle before the first dynamics step"
    );
    // Swing the avatar sideways so the tail lags behind its rest direction.
    for f in 1..30 {
        i.origin.pos = [f as f32 * 0.05, 0.0, 0.0];
        i.step(DT, &mut c);
    }
    let params = &i.animator_mut().unwrap().params;
    assert!(params[angle] > 0.0 && params[angle] <= 1.0, "angle {}", params[angle]);
    assert_eq!(params[grabbed], 0.0);
}

#[test]
fn every_system_reports_per_island_and_the_contact_pass_once() {
    let avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let mut islands: Vec<Island> = (0..5).map(|id| island(&avatar, id, id == 0)).collect();
    let c = frame::step(&mut islands, |_, _| true, DT);
    for s in System::ALL {
        let expected = if s == System::Contacts { 1 } else { 5 };
        assert_eq!(c.runs[s as usize], expected, "{}", s.name());
    }
}

#[test]
fn results_do_not_depend_on_the_worker_count() {
    let avatar = civbb_corpus::generate(&civbb_corpus::bloated());
    let model = Arc::new(Model::new(&avatar));
    let run = |threads: usize| {
        let mut islands: Vec<Island> = (0..8)
            .map(|id| {
                let origin = Pose {
                    pos: [id as f32 * 0.3, 0.0, 0.0],
                    ..Pose::IDENTITY
                };
                Island::new(Arc::clone(&model), id, id == 0, origin)
            })
            .collect();
        jobs::pool(threads).unwrap().install(|| {
            for _ in 0..30 {
                frame::step(&mut islands, |_, _| true, DT);
            }
        });
        islands
            .iter()
            .map(|i| (i.world().to_vec(), i.contact_values().to_vec()))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn animated_materials_are_per_island() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    // A toggle that starts off: with Write Defaults off, a value an island once had would persist.
    let params = &avatar.state.entries;
    let name = params.iter().find(|p| p.default == 0.0).unwrap().name.clone();
    // The toggle's on clip also lights up the body's emission.
    let on = avatar
        .clips
        .iter_mut()
        .find(|c| c.name == format!("{name}_On"))
        .unwrap();
    let mut emission = on.curves[0].clone();
    emission.target = Target::Mesh {
        mesh: 0,
        property: MeshProperty::Material {
            field: MaterialField::Emission,
            component: 1,
            srgb: true,
        },
    };
    emission.keys[0].value = 0.5;
    on.curves.push(emission);
    let model = Arc::new(Model::new(&avatar));
    let animated: Vec<_> = model.animated_materials().map(|(m, i)| (m, i.to_vec())).collect();
    let used = avatar.meshes[0].submeshes[0].material;
    assert_eq!(animated.len(), 1);
    assert_eq!(animated[0].0, 0);
    assert!(animated[0].1.contains(&used));

    let mut islands: Vec<Island> = (0..2)
        .map(|id| Island::new(Arc::clone(&model), id, id == 0, Pose::IDENTITY))
        .collect();
    let p = model.controller().unwrap().param(&name).unwrap() as usize;
    for (i, island) in islands.iter_mut().enumerate() {
        island.animator_mut().unwrap().params[p] = if i == 0 { 1.0 } else { 0.0 };
    }
    let mut out = Vec::new();
    for _ in 0..3 {
        frame::step(&mut islands, |_, _| true, DT);
    }
    let k = animated[0].1.iter().position(|&m| m == used).unwrap();
    let emissive = |m: &Material| match &m.model {
        MaterialModel::Toon(t) => t.emission.expect("the corpus toons emit").color,
        _ => unreachable!("the corpus uses toon materials"),
    };
    islands[0].materials(&mut out);
    let lit = emissive(&out[k]);
    assert!((lit[1] - civbb_anim::material::to_linear(0.5)).abs() < 1e-6, "{lit:?}");
    assert_eq!([lit[0], lit[2]], [0.0, 0.0]);
    // The other island keeps the material's own value, and the avatar's material is untouched.
    islands[1].materials(&mut out);
    assert_eq!(emissive(&out[k]), [0.0; 3]);
    assert_eq!(emissive(&avatar.materials[used as usize]), [0.0; 3]);
}

#[test]
fn a_toggle_can_swap_a_material_slot_per_island() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let params = &avatar.state.entries;
    let name = params.iter().find(|p| p.default == 0.0).unwrap().name.clone();
    // A second material only the toggle's on clip puts into the body's first slot.
    let mut swapped = avatar.materials[0].clone();
    swapped.name = "swapped".into();
    avatar.materials.push(swapped);
    let index = avatar.materials.len() as u32 - 1;
    let on = avatar
        .clips
        .iter_mut()
        .find(|c| c.name == format!("{name}_On"))
        .unwrap();
    let mut slot = on.curves[0].clone();
    slot.target = Target::Mesh {
        mesh: 0,
        property: MeshProperty::MaterialSlot { submesh: 0 },
    };
    slot.keys[0].value = index as f32;
    on.curves.push(slot);

    let model = Arc::new(Model::new(&avatar));
    let mut islands: Vec<Island> = (0..2)
        .map(|id| Island::new(Arc::clone(&model), id, id == 0, Pose::IDENTITY))
        .collect();
    let p = model.controller().unwrap().param(&name).unwrap() as usize;
    islands[0].animator_mut().unwrap().params[p] = 1.0;
    for _ in 0..3 {
        frame::step(&mut islands, |_, _| true, DT);
    }
    let own: Vec<u32> = avatar.meshes[0].submeshes.iter().map(|s| s.material).collect();
    let mut out = Vec::new();
    islands[0].submesh_materials(&avatar, 0, &mut out);
    assert_eq!(out[0], index);
    assert_eq!(out[1..], own[1..]);
    islands[1].submesh_materials(&avatar, 0, &mut out);
    assert_eq!(out, own, "the other island keeps the mesh's own materials");
    islands[0].submesh_materials(&avatar, 1, &mut out);
    assert!(out.is_empty(), "no swaps on other meshes");
}

#[test]
fn a_toggle_can_switch_a_constraint_per_island() {
    use civbb_format::constraint::{ConstraintField, ConstraintKind};
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    // The first corpus constraint becomes a position constraint, so it visibly moves its target off the rest pose.
    let c = &mut avatar.constraints[0];
    c.kind = ConstraintKind::Position {
        offset: [0.0, 0.1, 0.0],
    };
    c.axes = [true; 3];
    let target = c.target as usize;
    // A toggle that starts off; its on clip switches the constraint off.
    let params = &avatar.state.entries;
    let name = params.iter().find(|p| p.default == 0.0).unwrap().name.clone();
    let on = avatar
        .clips
        .iter_mut()
        .find(|c| c.name == format!("{name}_On"))
        .unwrap();
    let mut off = on.curves[0].clone();
    off.target = Target::Constraint {
        constraint: 0,
        field: ConstraintField::Active,
    };
    off.keys[0].value = 0.0;
    on.curves.push(off);

    let model = Arc::new(Model::new(&avatar));
    let mut islands: Vec<Island> = (0..2)
        .map(|id| Island::new(Arc::clone(&model), id, id == 0, Pose::IDENTITY))
        .collect();
    let p = model.controller().unwrap().param(&name).unwrap() as usize;
    islands[0].animator_mut().unwrap().params[p] = 1.0;
    for _ in 0..3 {
        frame::step(&mut islands, |_, _| true, DT);
    }
    let rest = avatar.hierarchy.nodes[target].rest.translation;
    // Island 0 switched the constraint off: the target keeps its rest place; island 1 still moves it.
    assert_eq!(islands[0].locals()[target].translation, rest);
    assert_ne!(islands[1].locals()[target].translation, rest);
}
