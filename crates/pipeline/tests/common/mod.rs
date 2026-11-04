//! Shared by the stage tests: validity, stepping islands through an avatar's states, and what an avatar shows.

use std::cmp::Ordering;

use civbb_client::Island;
pub use civbb_client::parity::placed;
use civbb_format::{Avatar, Caps, validate};

pub fn valid(a: &Avatar) {
    if let Err(e) = validate(a, &Caps::default(), |_| true) {
        panic!("{e:?}");
    }
}

/// Steps one island per avatar through the same states as the parity renders (`civbb_client::parity::states`): the
/// rest pose, then each of the first avatar's parameters set away from its default in turn. Calls `check` with the
/// state number and the islands after each.
pub fn states(avatars: &[&Avatar], mut check: impl FnMut(usize, &[Island])) {
    // The parity stepper skips a parameter an avatar lacks; a stage must keep them all.
    let names = |a: &Avatar| -> Vec<String> {
        a.animator
            .iter()
            .flat_map(|x| &x.parameters)
            .map(|p| p.name.clone())
            .collect()
    };
    for a in avatars {
        let have = names(a);
        assert!(
            names(avatars[0]).iter().all(|n| have.contains(n)),
            "a parameter is gone"
        );
    }
    civbb_client::parity::states(avatars, |state, _, islands| check(state, islands));
}

/// Steps both avatars through the same states and compares what they show.
pub fn same_look(before: &Avatar, after: &Avatar, name: &str) {
    states(&[before, after], |state, i| {
        assert!(
            look(before, &i[0]) == look(after, &i[1]),
            "{name}: state {state} differs"
        );
        let (mut mb, mut ma) = (Vec::new(), Vec::new());
        i[0].materials(&mut mb);
        i[1].materials(&mut ma);
        assert_eq!(mb, ma, "{name}: state {state} animated materials differ");
    });
}

/// What an avatar shows: every visible triangle with its material, its vertices in world space after shape
/// weights and skinning, sorted.
pub fn look(avatar: &Avatar, island: &Island) -> Vec<(u32, [[f32; 3]; 3])> {
    let mut out = Vec::new();
    for (mesh, placed) in avatar.meshes.iter().zip(placed(avatar, island)) {
        if !island.active()[mesh.node as usize] {
            continue;
        }
        for s in &mesh.submeshes {
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            for t in mesh.indices[range].chunks(3) {
                out.push((s.material, std::array::from_fn(|k| placed[t[k] as usize])));
            }
        }
    }
    out.sort_by(|x, y| {
        x.0.cmp(&y.0).then_with(|| {
            x.1.as_flattened()
                .iter()
                .zip(y.1.as_flattened())
                .map(|(a, b)| a.total_cmp(b))
                .find(|o| *o != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
    });
    out
}
