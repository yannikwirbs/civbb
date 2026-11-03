//! Render graph planning (ADRs 0021, 0023), no device needed: image transitions before each pass and into each
//! image's final use, depth attachments and transients, and buffer barriers.

use civbb_render::graph::{BufferBarrier, Plan, Transition};
use civbb_render::vk::{AccessFlags2 as A, PipelineStageFlags2 as S};
use civbb_render::{Access, BufferId, Graph, ImageDesc, ImageId, Load, Pass, Use, vk};

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
            buffers: vec![vec![]],
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

#[test]
fn depth_attachments_and_transients() {
    let mut g = Graph::new();
    let target = import(&mut g, Use::Undefined, Use::Present);
    let depth = g.transient(ImageDesc {
        format: vk::Format::D32_SFLOAT,
        extent: g.extent(target),
        usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
    });
    assert_eq!(g.extent(depth), vk::Extent2D { width: 4, height: 4 });
    g.add(
        Pass::new("draw")
            .color(target, Load::Clear([0.0; 4]))
            .depth(depth, Load::Clear([0.0; 4])),
    );
    g.add(Pass::new("more").color(target, Load::Load).depth(depth, Load::Load));
    let plan = g.plan();
    assert_eq!(
        plan.before,
        vec![
            vec![
                t(target, Use::Undefined, Use::ColorAttachment),
                t(depth, Use::Undefined, Use::DepthAttachment)
            ],
            vec![
                t(target, Use::ColorAttachment, Use::ColorAttachment),
                t(depth, Use::DepthAttachment, Use::DepthAttachment)
            ],
        ]
    );
    // A transient stays in its last use; the pool remembers it for the next frame.
    assert_eq!(plan.after, vec![t(target, Use::ColorAttachment, Use::Present)]);
}

fn b(buffer: BufferId, src: (S, A), dst: (S, A)) -> BufferBarrier {
    BufferBarrier {
        buffer,
        src_stage: src.0,
        src_access: src.1,
        dst_stage: dst.0,
        dst_access: dst.1,
    }
}

const STORAGE_RW: A = A::from_raw(A::SHADER_STORAGE_READ.as_raw() | A::SHADER_STORAGE_WRITE.as_raw());

#[test]
fn buffers_wait_for_writes_and_writes_wait_for_reads() {
    let mut g = Graph::new();
    // Last frame drew from it.
    let skinned = g.import_buffer(vk::Buffer::null(), &[Access::VertexRead]);
    g.add(Pass::new("skin").buffer(skinned, Access::ComputeWrite));
    g.add(
        Pass::new("draw")
            .buffer(skinned, Access::VertexRead)
            .buffer(skinned, Access::FragmentRead),
    );
    g.add(Pass::new("draw again").buffer(skinned, Access::VertexRead));
    g.add(Pass::new("skin again").buffer(skinned, Access::ComputeWrite));
    let plan = g.plan();
    let compute_write = (S::COMPUTE_SHADER, A::SHADER_STORAGE_WRITE | A::SHADER_STORAGE_READ);
    assert_eq!(
        plan.buffers,
        vec![
            // Write after read: an execution dependency only.
            vec![b(skinned, (S::VERTEX_SHADER, A::NONE), compute_write)],
            // Read after write, once per reading stage.
            vec![
                b(
                    skinned,
                    (S::COMPUTE_SHADER, STORAGE_RW),
                    (S::VERTEX_SHADER, A::SHADER_STORAGE_READ)
                ),
                b(
                    skinned,
                    (S::COMPUTE_SHADER, STORAGE_RW),
                    (S::FRAGMENT_SHADER, A::SHADER_STORAGE_READ)
                ),
            ],
            vec![],
            // Write after write and reads: waits for all of them.
            vec![b(
                skinned,
                (S::COMPUTE_SHADER | S::VERTEX_SHADER | S::FRAGMENT_SHADER, STORAGE_RW),
                compute_write
            )],
        ]
    );
}

#[test]
fn a_buffer_without_prior_use_needs_no_first_barrier() {
    let mut g = Graph::new();
    let fresh = g.import_buffer(vk::Buffer::null(), &[]);
    let read_only = g.import_buffer(vk::Buffer::null(), &[]);
    g.add(Pass::new("upload").buffer(fresh, Access::TransferWrite));
    g.add(Pass::new("read").buffer(read_only, Access::IndirectRead));
    g.add(Pass::new("upload again").buffer(fresh, Access::TransferWrite));
    let transfer = (S::ALL_TRANSFER, A::TRANSFER_WRITE);
    assert_eq!(
        g.plan().buffers,
        vec![vec![], vec![], vec![b(fresh, transfer, transfer)]]
    );
}
