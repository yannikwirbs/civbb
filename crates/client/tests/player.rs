//! The desktop player on the frame schedule (ADR 0106): input moves and turns only the player's island, the camera
//! sits at its viewpoint in first person with its own head shrunk away, and the extract step sees the moved island.

use civbb_client::extract::{Camera, Snapshot};
use civbb_client::frame::Runtime;
use civbb_client::package::Package;
use civbb_client::player::{Input, Player, View, WALK};
use civbb_core::math::Pose;
use civbb_format::hierarchy::HumanBone;

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
