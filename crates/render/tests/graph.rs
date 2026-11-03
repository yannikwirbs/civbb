//! Render graph planning (ADR 0021), no device needed: transitions before each pass and into each image's final use.

use civbb_render::graph::{Plan, Transition};
use civbb_render::{Graph, ImageId, Load, Pass, Use, vk};

fn import(g: &mut Graph, initial: Use, last: Use) -> ImageId {
    g.import(
        vk::Image::null(),
        vk::ImageView::null(),
        vk::Extent2D { width: 4, height: 4 },
        initial,
        last,
    )
}

fn t(image: ImageId, from: Use, to: Use) -> Transition {
    Transition { image, from, to }
}

#[test]
fn clear_and_present() {
    let mut g = Graph::new();
    let target = import(&mut g, Use::Undefined, Use::Present);
    g.add(Pass::new("clear").color(target, Load::Clear([0.0, 0.0, 0.0, 1.0])));
    assert_eq!(
        g.plan(),
        Plan {
            before: vec![vec![t(target, Use::Undefined, Use::ColorAttachment)]],
            after: vec![t(target, Use::ColorAttachment, Use::Present)],
        }
    );
}

#[test]
fn write_after_write_gets_a_barrier() {
    let mut g = Graph::new();
    let target = import(&mut g, Use::Undefined, Use::TransferSrc);
    g.add(Pass::new("a").color(target, Load::Clear([1.0; 4])));
    g.add(Pass::new("b").color(target, Load::Load));
    let plan = g.plan();
    assert_eq!(
        plan.before[1],
        vec![t(target, Use::ColorAttachment, Use::ColorAttachment)]
    );
    assert_eq!(plan.after, vec![t(target, Use::ColorAttachment, Use::TransferSrc)]);
}

#[test]
fn images_are_tracked_independently() {
    let mut g = Graph::new();
    let a = import(&mut g, Use::Undefined, Use::Present);
    let b = import(&mut g, Use::Undefined, Use::TransferSrc);
    g.add(Pass::new("a").color(a, Load::DontCare));
    g.add(Pass::new("b").color(b, Load::DontCare));
    let plan = g.plan();
    assert_eq!(
        plan.before,
        vec![
            vec![t(a, Use::Undefined, Use::ColorAttachment)],
            vec![t(b, Use::Undefined, Use::ColorAttachment)],
        ]
    );
    assert_eq!(
        plan.after,
        vec![
            t(a, Use::ColorAttachment, Use::Present),
            t(b, Use::ColorAttachment, Use::TransferSrc)
        ]
    );
}

#[test]
fn passes_without_attachments_need_no_transitions() {
    let mut g = Graph::new();
    let target = import(&mut g, Use::Undefined, Use::Present);
    g.add(Pass::new("compute"));
    let plan = g.plan();
    assert_eq!(plan.before, vec![vec![]]);
    // An untouched image still reaches its final use.
    assert_eq!(plan.after, vec![t(target, Use::Undefined, Use::Present)]);
}

#[test]
fn an_image_already_in_its_final_read_use_is_left_alone() {
    let mut g = Graph::new();
    import(&mut g, Use::TransferSrc, Use::TransferSrc);
    assert!(g.plan().after.is_empty());
}
