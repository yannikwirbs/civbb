//! Islands and frames (ADR 0020): animator toggles reach node activity, contact values reach parameters one frame
//! later, every system reports per island, and results do not depend on the number of workers.

use std::sync::Arc;

use civbb_client::{Island, Model, frame};
use civbb_core::math::Pose;
use civbb_core::{System, jobs};
use civbb_format::Avatar;
use civbb_format::animator::{ParamKind, Parameter};

const DT: f32 = 1.0 / 90.0;

fn island(avatar: &Avatar, id: u64, local: bool) -> Island {
    let origin = Pose {
        pos: [id as f32 * 2.0, 0.0, 0.0],
        ..Pose::IDENTITY
    };
    Island::new(Arc::new(Model::new(avatar)), id, local, origin)
}

#[test]
fn a_toggle_parameter_switches_its_node() {
    let avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let name = avatar.animator.as_ref().unwrap().parameters[0].name.clone();
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
    let (head, param) = (receiver.volume.node, receiver.parameter.clone());
    avatar.contacts.senders[0].volume.node = head;
    avatar.animator.as_mut().unwrap().parameters.push(Parameter {
        name: param.clone(),
        kind: ParamKind::Float,
        default: 0.0,
    });
    let mut islands = vec![island(&avatar, 0, true)];
    let p = islands[0].model().controller().unwrap().param(&param).unwrap() as usize;

    frame::step(&mut islands, |_, _| true, DT);
    assert_eq!(islands[0].contact_values(), [1.0]);
    assert_eq!(islands[0].animator_mut().unwrap().params[p], 0.0);
    frame::step(&mut islands, |_, _| true, DT);
    assert_eq!(islands[0].animator_mut().unwrap().params[p], 1.0);
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
