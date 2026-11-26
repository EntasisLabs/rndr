//! Benign local interaction/lifecycle example. No server, files or guest bridge.
//! `cargo run -p rndr_browser --example artifact_preview`
//! Click the counter, type a name and move the slider. Every 15 seconds the host
//! replaces the document with a fresh session. Close the window to drop it.
use rndr_browser::{ArtifactPreview, ClientRect};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

#[derive(Default)]
struct App {
    // Drop the preview before its parent window.
    preview: Option<ArtifactPreview>,
    window: Option<Box<dyn Window>>,
    next_replacement: Option<Instant>,
    generation: u64,
}
fn html(generation: u64) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><style>
    body{{font:18px system-ui;max-width:600px;margin:40px;color:#24303a;background:#f6f8fa}}
    button,input{{font:inherit;margin:12px 0;padding:8px}} output{{display:block;margin:16px 0}}
    </style><h1>Local artifact · generation {generation}</h1>
    <p>No network, shared storage, files or native commands. This resets every 15 seconds.</p>
    <button onclick="this.textContent=++window.clicks+' local clicks'">0 local clicks</button>
    <p><label>Your local name <input oninput="document.getElementById('greeting').textContent='Hello, '+this.value"></label></p>
    <output id="greeting">Hello</output><label>Local slider <input type="range" oninput="document.getElementById('slider-value').textContent=this.value"></label>
    <output id="slider-value">50</output><script>window.clicks=0</script>"#
    )
}
impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, events: &dyn ActiveEventLoop) {
        let window = events
            .create_window(
                WindowAttributes::default().with_title("RNDR isolated in-memory artifact"),
            )
            .unwrap();
        match ArtifactPreview::attach(&window, &html(self.generation)) {
            Ok(mut preview) => {
                preview.show();
                self.preview = Some(preview);
            }
            Err(error) => {
                eprintln!("Preview refused (no browsing fallback): {error}");
                events.exit();
                return;
            }
        }
        self.window = Some(window);
        self.next_replacement = Some(Instant::now() + Duration::from_secs(15));
    }
    fn window_event(&mut self, events: &dyn ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            self.preview = None;
            self.window = None;
            events.exit();
        }
    }
    fn about_to_wait(&mut self, events: &dyn ActiveEventLoop) {
        let (Some(window), Some(preview)) = (&self.window, &mut self.preview) else {
            return;
        };
        let logical = window
            .surface_size()
            .to_logical::<f32>(window.scale_factor());
        preview
            .set_bounds(ClientRect::try_new(0.0, 0.0, logical.width, logical.height).unwrap())
            .unwrap();
        if self
            .next_replacement
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.generation += 1;
            if let Err(error) = preview.replace(&html(self.generation)) {
                eprintln!("replace: {error}");
                events.exit();
                return;
            }
            self.next_replacement = Some(Instant::now() + Duration::from_secs(15));
        }
        if let Err(error) = preview.frame() {
            eprintln!("policy/load failed: {error}");
            events.exit();
            return;
        }
        events.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}
fn main() {
    EventLoop::new().unwrap().run_app(App::default()).unwrap();
}
