//! The render thread: owns the renderer and the GPU resources of what is shown, draws each snapshot it is handed
//! while the schedule simulates the next frame, and hands the snapshot back to be reused (design doc §5 step 10, ADR
//! 0103).

use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::Instant;

use civbb_render::Renderer;

use crate::extract::Snapshot;
use crate::gpu::{Gpu, Scratch, draw};

enum Message {
    Frame(Snapshot),
    Resize(u32, u32),
}

/// What the render thread did with one snapshot.
pub(crate) struct Drawn {
    /// Draws and skinned meshes; `None` when the frame was skipped (a minimised window).
    pub(crate) counts: Option<(usize, usize)>,
    /// CPU time recording and submitting, waits for vsync and the GPU left out, in milliseconds.
    pub(crate) cpu_ms: f32,
    /// The GPU time of the last frame that has one.
    pub(crate) gpu_ms: Option<f32>,
}

/// A snapshot handed back with what became of it.
pub(crate) type Handed = (Snapshot, civbb_render::Result<Drawn>);

/// The render thread and its two channels: snapshots in (at most one waiting), drawn snapshots out.
pub(crate) struct RenderThread {
    to: Option<SyncSender<Message>>,
    from: Receiver<Handed>,
    thread: Option<JoinHandle<u32>>,
}

impl RenderThread {
    /// Starts the thread; it ends when an error is handed back or after [`RenderThread::end`], destroying `gpu`.
    pub(crate) fn spawn(renderer: Renderer, gpu: Gpu) -> Self {
        let (to, inbox) = sync_channel(1);
        let (outbox, from) = channel();
        let thread = std::thread::Builder::new()
            .name("render".into())
            .spawn(move || run(renderer, gpu, &inbox, &outbox))
            .expect("the OS starts a thread");
        Self {
            to: Some(to),
            from,
            thread: Some(thread),
        }
    }

    /// Hands `snapshot` over to be drawn.
    pub(crate) fn draw(&mut self, snapshot: Snapshot) {
        self.send(Message::Frame(snapshot));
    }

    /// Resizes the window's swapchain before the next snapshot.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.send(Message::Resize(width, height));
    }

    fn send(&mut self, m: Message) {
        if self.to.as_ref().is_none_or(|to| to.send(m).is_err()) {
            self.gone();
        }
    }

    /// The next snapshot drawn, waiting for it.
    pub(crate) fn drawn(&mut self) -> Handed {
        match self.from.recv() {
            Ok(h) => h,
            Err(_) => self.gone(),
        }
    }

    /// The thread stopped early: it panicked (passed on here) or was handed more work after reporting an error.
    fn gone(&mut self) -> ! {
        if let Some(Err(panic)) = self.thread.take().map(JoinHandle::join) {
            std::panic::resume_unwind(panic);
        }
        panic!("the render thread was handed work after it failed");
    }

    /// Ends the thread after the snapshots already handed over; returns the validation errors reported, or the
    /// error one of those snapshots failed with.
    pub(crate) fn end(mut self) -> civbb_render::Result<u32> {
        self.to = None;
        let thread = self.thread.take().expect("joined only here and in drop");
        let errors = thread.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        match self.from.try_iter().find_map(|(_, drawn)| drawn.err()) {
            Some(e) => Err(e),
            None => Ok(errors),
        }
    }
}

impl Drop for RenderThread {
    /// Joins the thread, so the renderer is gone before the window it presents to.
    fn drop(&mut self) {
        self.to = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The thread's loop; returns the validation errors reported.
fn run(mut renderer: Renderer, mut gpu: Gpu, inbox: &Receiver<Message>, outbox: &Sender<Handed>) -> u32 {
    let mut scratch = Scratch::default();
    while let Ok(m) = inbox.recv() {
        match m {
            Message::Resize(width, height) => renderer.resize(width, height),
            Message::Frame(snapshot) => {
                let start = Instant::now();
                let zone = civbb_core::profile::zone("render");
                let drawn = draw(&mut renderer, &mut gpu, &snapshot, &mut scratch).map(|counts| Drawn {
                    counts,
                    cpu_ms: start.elapsed().as_secs_f32() * 1000.0 - renderer.blocked_ms(),
                    gpu_ms: renderer.gpu_ms(),
                });
                drop(zone);
                let failed = drawn.is_err();
                if outbox.send((snapshot, drawn)).is_err() || failed {
                    break;
                }
            }
        }
    }
    let errors = renderer.context().validation_errors();
    // SAFETY: after the wait nothing is in flight.
    unsafe {
        let _ = renderer.context().device().device_wait_idle();
        gpu.destroy(&renderer);
    }
    errors
}
