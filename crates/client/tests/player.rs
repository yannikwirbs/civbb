//! The desktop player on the frame schedule (ADRs 0106, 0107, 0114): input moves and turns only the player's island and sets
//! its platform values, the camera
//! sits at its viewpoint in first person with its own head shrunk away, and the extract step sees the moved island.

use civbb_client::extract::{Camera, Snapshot};
use civbb_client::frame::Runtime;
use civbb_client::package::Package;
use civbb_client::player::{Input, Player, Posture, RUN, View, WALK};
use civbb_client::station::Station;
use civbb_core::math::{Pose, conj, rotate};
use civbb_format::hierarchy::HumanBone;
use civbb_format::state::{Entry, Platform};

const DT: f32 = 1.0 / 60.0;

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

#[test]
fn input_moves_the_players_island_and_the_camera_follows() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let viewpoint = package.avatar.avatar().expect("an avatar").viewpoint;
    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.extract_to_snapshot();
    let start = [1.0, 0.0, 0.0];
    let at = |pos| Pose { pos, ..Pose::IDENTITY };
    let e = runtime.spawn_package(&package, 0, true, at(start));
    runtime.spawn_package(&package, 1, false, at([3.0, 0.0, 0.0]));
    {
        let mut player = runtime.world.resource_mut::<Player>();
        (player.island, player.pos, player.view) = (Some(e), start, View::First);
    }
    // A quarter turn left, then one second forward: towards +X.
    runtime.world.insert_resource(Input {
        look: [std::f32::consts::FRAC_PI_2, 0.0],
        ..Default::default()
    });
    runtime.step(0.0);
    runtime.world.resource_mut::<Input>().forward = 1.0;
    for _ in 0..60 {
        runtime.step(DT);
    }
    let expected = [1.0 + WALK, 0.0, 0.0];
    let islands = runtime.islands();
    assert!(close(islands[0].origin.pos, expected), "{:?}", islands[0].origin.pos);
    assert_eq!(islands[1].origin, at([3.0, 0.0, 0.0]), "only the player's island moves");
    // First person: the eye at the turned viewpoint, looking along +X.
    let eye = islands[0].origin.point(viewpoint);
    assert!(
        close(eye, [1.0 + WALK + viewpoint[2], viewpoint[1], -viewpoint[0]]),
        "{eye:?}"
    );
    let camera = *runtime.world.resource::<Camera>();
    assert!(
        close(camera.eye, eye) && close(camera.target, [eye[0] + 1.0, eye[1], eye[2]]),
        "{camera:?}"
    );
    let snapshot = runtime.world.resource::<Snapshot>();
    assert!(close(snapshot.islands[0].origin, expected));
    assert_eq!(snapshot.camera, camera);
}

#[test]
fn the_players_animator_gets_its_platform_values_and_others_keep_theirs() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let declared = [Platform::VelocityZ, Platform::GestureLeft, Platform::Upright];
    let first = avatar.state.entries.len();
    for p in declared {
        avatar.state.entries.push(Entry::new(p.name(), p.kind(), 0.0));
    }
    let package = Package::new(avatar, Vec::new());
    let mut runtime = Runtime::default();
    runtime.add_player();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.spawn_package(&package, 1, false, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    runtime.world.insert_resource(Input {
        forward: 1.0,
        run: true,
        gestures: [3, 0],
        crouch: true,
        ..Default::default()
    });
    runtime.step(DT);
    let params =
        |runtime: &mut Runtime, k| runtime.island_mut(k).animator_mut().expect("an animator").params[first..].to_vec();
    let player = params(&mut runtime, 0);
    // Crouching halves the run.
    assert!((player[0] - RUN / 2.0).abs() < 1e-3, "{player:?}");
    assert_eq!(player[1..], [3.0, Posture::Crouching.upright()]);
    // A remote copy stands still at rest.
    assert_eq!(params(&mut runtime, 1), [0.0, 0.0, 1.0]);
}

#[test]
fn first_person_shrinks_the_players_head_to_a_point() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let avatar = &package.avatar;
    let head = (avatar.avatar().and_then(|a| a.humanoid.as_ref()))
        .and_then(|h| h.node(HumanBone::Head))
        .expect("a head");
    let mut runtime = Runtime::default();
    runtime.add_player();
    runtime.extract_to_snapshot();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.spawn_package(&package, 1, false, Pose::IDENTITY);
    // Each skin joint of each mesh and whether it is the head.
    let joints: Vec<(usize, usize)> = (avatar.meshes.iter().enumerate())
        .flat_map(|(m, mesh)| {
            let skin = mesh.skin.as_ref().map_or(&[][..], |s| &s.joints[..]);
            skin.iter()
                .enumerate()
                .filter(|(_, j)| **j == head)
                .map(move |(k, _)| (m, k))
        })
        .collect();
    assert!(!joints.is_empty(), "a mesh skinned to the head");
    let head_matrix = |runtime: &Runtime, island: usize, (m, k): (usize, usize)| {
        let s = runtime.world.resource::<Snapshot>();
        s.islands[island].joints[m].as_ref().expect("an active mesh")[k]
    };
    for view in [View::First, View::Third, View::First] {
        {
            let mut player = runtime.world.resource_mut::<Player>();
            (player.island, player.view) = (Some(e), view);
        }
        runtime.step(DT);
        let at = runtime.islands()[0].world()[head as usize].pos;
        for &j in &joints {
            let m = head_matrix(&runtime, 0, j);
            let collapsed = m[..12].iter().all(|&x| x == 0.0) && close([m[12], m[13], m[14]], at);
            assert_eq!(collapsed, view == View::First, "{view:?}: {m:?}");
            // Other islands never hide their head.
            assert!(head_matrix(&runtime, 1, j)[..12].iter().any(|&x| x != 0.0));
        }
    }
}

#[test]
fn looking_up_and_down_turns_the_players_head_only() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let head = (package.avatar.avatar().and_then(|a| a.humanoid.as_ref()))
        .and_then(|h| h.node(HumanBone::Head))
        .expect("a head") as usize;
    let mut runtime = Runtime::default();
    runtime.add_player();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.spawn_package(&package, 1, false, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    runtime.step(DT);
    let level = |runtime: &Runtime, k: usize| runtime.islands()[k].world()[head].rot;
    let start = [level(&runtime, 0), level(&runtime, 1)];
    for pitch in [0.5_f32, -0.5] {
        let turned = pitch - runtime.world.resource::<Player>().pitch;
        runtime.world.resource_mut::<Input>().look = [0.0, turned];
        runtime.step(DT);
        // The head's forward, measured against its level placement, tilts by the pitch.
        let forward = rotate(level(&runtime, 0), rotate(conj(start[0]), [0.0, 0.0, 1.0]));
        assert!((forward[1] - pitch.sin()).abs() < 1e-3, "{pitch}: {forward:?}");
        assert_eq!(level(&runtime, 1), start[1], "other islands keep their head");
    }
}

#[test]
fn a_scaled_player_sees_from_its_scaled_viewpoint_and_its_bones_scale() {
    let package = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
    let viewpoint = package.avatar.avatar().expect("an avatar").viewpoint;
    let mut runtime = Runtime::default();
    runtime.add_player();
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.spawn_package(&package, 1, false, Pose::IDENTITY);
    {
        let mut player = runtime.world.resource_mut::<Player>();
        (player.island, player.view) = (Some(e), View::First);
    }
    runtime.step(DT);
    let unscaled = runtime.islands()[0].world().to_vec();
    runtime.world.resource_mut::<Input>().resize = Some(2.0);
    runtime.step(DT);
    let player = *runtime.world.resource::<Player>();
    assert_eq!((player.eye_height, player.scale), (Some(2.0 * viewpoint[1]), 2.0));
    let islands = runtime.islands();
    assert_eq!(islands[0].origin.scale, 2.0);
    assert_eq!(islands[1].origin.scale, 1.0, "others keep their scale");
    // Every bone, IK and dynamics included, sits twice as far from the origin.
    for (a, b) in islands[0].world().iter().zip(&unscaled) {
        assert!(close(a.pos, b.pos.map(|x| 2.0 * x)), "{:?} {:?}", a.pos, b.pos);
    }
    let camera = *runtime.world.resource::<Camera>();
    assert!(close(camera.eye, viewpoint.map(|x| 2.0 * x)), "{camera:?}");
}

#[test]
fn using_a_station_seats_the_player_and_jumping_leaves() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    let declared = [Platform::InStation, Platform::Seated];
    let first = avatar.state.entries.len();
    for p in declared {
        avatar.state.entries.push(Entry::new(p.name(), p.kind(), 0.0));
    }
    let package = Package::new(avatar, Vec::new());
    let mut runtime = Runtime::default();
    runtime.add_player();
    let station = Station {
        seat: [0.0, 0.0, 1.0],
        yaw: 1.0,
        exit: [0.0, 0.0, 2.0],
        seated: true,
    };
    let far = Station {
        seat: [0.0, 0.0, 9.0],
        ..station
    };
    runtime.world.spawn_batch([station, far]);
    let e = runtime.spawn_package(&package, 0, true, Pose::IDENTITY);
    runtime.world.resource_mut::<Player>().island = Some(e);
    let params =
        |runtime: &mut Runtime| runtime.island_mut(0).animator_mut().expect("an animator").params[first..].to_vec();
    runtime.world.resource_mut::<Input>().interact = true;
    runtime.step(DT);
    assert_eq!(runtime.islands()[0].origin.pos, station.seat);
    assert_eq!(params(&mut runtime), [1.0, 1.0]);
    // Walking does nothing; jumping gets off at the exit.
    runtime.world.resource_mut::<Input>().forward = 1.0;
    runtime.step(DT);
    assert_eq!(runtime.islands()[0].origin.pos, station.seat);
    runtime.world.resource_mut::<Input>().forward = 0.0;
    runtime.world.resource_mut::<Input>().jump = true;
    runtime.step(DT);
    assert_eq!(runtime.islands()[0].origin.pos, station.exit);
    assert_eq!(params(&mut runtime), [0.0, 0.0]);
    // Out of reach: using does nothing.
    runtime.world.resource_mut::<Player>().pos = [0.0, 0.0, 5.0];
    runtime.world.resource_mut::<Input>().interact = true;
    runtime.step(DT);
    assert_eq!(runtime.world.resource::<Player>().station, None);
}
