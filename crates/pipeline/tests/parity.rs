//! FLIP parity on a device (ADR 0033): the lean and bloated corpus avatars after every compiler stage render like
//! their import in every state, with the same rig and poses, and a compiled avatar with a wrong skin colour does not.
//! Skips without a Vulkan device.

use civbb_client::parity::{self, Side};
use civbb_client::view::decode_textures;
use civbb_format::cost::{Light, Parity};
use civbb_format::material::MaterialModel;
use civbb_format::package::Platform;
use civbb_format::{Avatar, Package, ReadLimits, SigningKey};
use civbb_pipeline::{Imported, compile};

const KEY: [u8; 32] = [7; 32];

/// A corpus avatar as imported and as compiled, read back from its signed package.
fn compiled(spec: &civbb_corpus::Spec) -> (Avatar, Avatar) {
    let source = civbb_corpus::generate(spec);
    let imported = Imported {
        avatar: source.clone(),
        blobs: Default::default(),
        warnings: Vec::new(),
    };
    let (bytes, _) = compile(imported, Platform::Desktop, &SigningKey::from_bytes(&KEY), |_| {}).unwrap();
    let key = SigningKey::from_bytes(&KEY).verifying_key();
    let pkg = Package::decode_verified(&bytes, &key, &ReadLimits::default()).unwrap();
    (source, pkg.avatar(&ReadLimits::default()).unwrap())
}

/// Parity of `test` against `reference`; `None` without a device.
fn parity(reference: &Avatar, test: &Avatar) -> Option<Parity> {
    let none = |a: &Avatar| decode_textures(a, |_| None);
    let (rt, tt) = (none(reference), none(test));
    let side = |avatar, textures| Side { avatar, textures };
    match parity::run(side(reference, &rt), side(test, &tt), None) {
        Ok(p) => Some(p),
        Err(e @ (civbb_render::Error::Load(_) | civbb_render::Error::NoDevice)) => {
            eprintln!("skipped: {e}");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn lean_compiled_matches_its_import_in_every_state() {
    let (source, compiled) = compiled(&civbb_corpus::lean());
    let Some(p) = parity(&source, &compiled) else { return };
    let params = source.animator.iter().flat_map(|a| &a.parameters).count();
    assert!(params >= 2, "lean has an outfit and a prop toggle");
    assert_eq!(p.states.len(), params + 1);
    assert_eq!(p.states[0].name, "rest");
    for s in &p.states {
        assert_eq!(s.views.len(), 4);
        assert_eq!(s.light, Light::Green, "{}: {:?}", s.name, s.views);
        // Something is drawn in every view.
        assert!(s.views.iter().all(|v| v.pixels > 1000), "{}: {:?}", s.name, s.views);
    }
    assert_eq!(p.light, Light::Green);
    assert_eq!(p.reference, "source");
}

#[test]
fn a_wrong_skin_colour_is_red() {
    let (source, mut compiled) = compiled(&civbb_corpus::lean());
    let skin = compiled.materials.iter_mut().find(|m| m.name == "Skin").unwrap();
    let MaterialModel::Toon(toon) = &mut skin.model else {
        panic!("lean skin is toon")
    };
    toon.base_color = [0.2, 0.4, 1.0, 1.0];
    let Some(p) = parity(&source, &compiled) else { return };
    assert!(p.states.iter().all(|s| s.light == Light::Red), "{:?}", p.states);
}

#[test]
fn bloated_compiled_matches_its_import_in_every_state() {
    let (source, compiled) = compiled(&civbb_corpus::bloated());
    let Some(p) = parity(&source, &compiled) else { return };
    assert!(p.states.len() > 40);
    for s in &p.states {
        assert_eq!(s.light, Light::Green, "{}: {:?}", s.name, s.views);
    }
}
