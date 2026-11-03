//! Opens a window and clears it every frame, following resizes; the real window loop lives in the client (#27).
//!
//! `cargo run -p civbb-render --example clear [-- <frames>]`: with a frame count, resizes the window halfway, exits
//! after that many frames and fails if validation reported errors.

use civbb_render::{FrameStatus, Load, Pass, Renderer, Window};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::WindowId;

#[derive(Default)]
struct App {
    // Before `window`: the renderer must be dropped first.
    renderer: Option<Renderer>,
    window: Option<winit::window::Window>,
    limit: Option<u32>,
    presented: u32,
    errors: u32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = winit::window::Window::default_attributes()
            .with_title("civbb-render clear")
            .with_inner_size(PhysicalSize::new(800, 600));
        let window = event_loop.create_window(attrs).expect("window");
        let handles = Window {
            display: window.display_handle().expect("display").as_raw(),
            window: window.window_handle().expect("window").as_raw(),
        };
        let size = window.inner_size();
        // SAFETY: the window is stored next to the renderer and dropped after it.
        let renderer = unsafe { Renderer::new(cfg!(debug_assertions), handles, size.width, size.height) };
        self.renderer = Some(renderer.expect("renderer"));
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let t = self.presented as f32 / 120.0;
                let color = [0.5 + 0.5 * t.sin(), 0.3, 0.5 + 0.5 * t.cos(), 1.0];
                let status = renderer
                    .frame(|graph, target| graph.add(Pass::new("clear").color(target, Load::Clear(color))))
                    .expect("frame");
                if status == FrameStatus::Presented {
                    self.presented += 1;
                }
                if let Some(limit) = self.limit {
                    if self.presented == limit / 2 {
                        let _ = window.request_inner_size(PhysicalSize::new(640, 360));
                    }
                    if self.presented >= limit {
                        self.errors = renderer.context().validation_errors();
                        println!("presented {} frames, last at {:?}", self.presented, renderer.extent());
                        event_loop.exit();
                    }
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let mut app = App {
        limit: std::env::args().nth(1).map(|n| n.parse().expect("frame count")),
        ..App::default()
    };
    EventLoop::new().expect("event loop").run_app(&mut app).expect("run");
    app.renderer = None;
    if app.errors > 0 {
        eprintln!("validation reported {} errors", app.errors);
        std::process::exit(1);
    }
}
