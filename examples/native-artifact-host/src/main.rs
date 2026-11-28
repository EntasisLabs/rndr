//! UI-thread host example; never falls back to ordinary browsing.
//! cargo run -p native-artifact-host -- --smoke
//! Opt in explicitly with: --features rndr_browser/macos-private-webkit-artifacts
//! Without --smoke the counter remains interactive until the window closes.
use rndr_browser::{ArtifactPreview, ArtifactState, ClientRect, OverlayBitmap};
use rndr_core::{App, Button, Chrome, Color, Fired, Input, Label, Rect, Schedule, World};
use rndr_render::{NativeHost, NativeHostContext, Session};
use std::time::{Duration, Instant};

struct PreviewHost {
    preview: Option<ArtifactPreview>,
    attempted: bool,
    started: Instant,
    smoke: bool,
    stage: u8,
    last_state: Option<ArtifactState>,
    last_metrics: Option<([u32; 2], f64, bool)>,
}

fn html(replaced: bool) -> String {
    if let Some(path) = std::env::args().skip_while(|arg| arg != "--html").nth(1) {
        return std::fs::read_to_string(path).expect("local fixture HTML");
    }
    (if std::env::args().any(|arg| arg == "--scroll-stress") {
        r#"<style>body{margin:0;font:22px system-ui;background:#183c52;color:white}header{position:sticky;top:0;background:#183c52;padding:20px}button{font:inherit}button:hover{background:#fc6}main{height:30000px;background:repeating-linear-gradient(#267796 0px,#267796 90px,#825835 90px,#825835 180px)}</style><header>Native scrolling stress <button onclick="this.textContent=++window.n">0</button></header><main>Scroll rapidly. No white regions authored.</main><script>window.n=0;addEventListener('scroll',()=>document.querySelector('header').firstChild.textContent='Scroll Y '+scrollY+' ')</script>"#
    } else if replaced {
        "<h1>Fresh isolated replacement</h1><button onclick=\"this.textContent=++window.n\">0</button><script>window.n=0</script>"
    } else {
        "<style>body{font:22px system-ui;background:#edf4fa;padding:30px}button,input{font:inherit;padding:10px}</style><h1>RNDR native host</h1><p>Inline interaction, no network or native bridge.</p><button onclick=\"this.textContent=++window.n\">0</button><input placeholder=\"Local input\"><script>window.n=0</script>"
    }).to_owned()
}

impl NativeHost for PreviewHost {
    fn frame(&mut self, world: &mut World, context: NativeHostContext<'_>) -> Option<Duration> {
        let metrics = (context.physical_size, context.scale_factor, context.visible);
        if self.last_metrics != Some(metrics) {
            eprintln!("host metrics: {metrics:?}");
            self.last_metrics = Some(metrics);
        }
        if !self.attempted {
            self.attempted = true;
            // &context.window is sized and implements HasWindowHandle.
            match ArtifactPreview::attach(&context.window, &html(false)) {
                Ok(preview) => self.preview = Some(preview),
                Err(error) => eprintln!("Preview unavailable (no fallback): {error}"),
            }
        }
        if self.smoke {
            let elapsed = self.started.elapsed().as_secs();
            if elapsed >= 2 && self.stage == 0 {
                rndr_core::window::hide(world);
                context.request_redraw();
                self.stage = 1;
            } else if elapsed >= 3 && self.stage == 1 {
                rndr_core::window::show(world);
                context.request_redraw();
                self.stage = 2;
            } else if elapsed >= 4 && self.stage == 2 {
                if let Some(preview) = &mut self.preview {
                    preview.replace(&html(true)).expect("fresh replacement");
                    eprintln!("host replaced");
                    self.last_state = None;
                }
                self.stage = 3;
            } else if elapsed >= 5 && self.stage == 3 {
                let _ = context
                    .window
                    .request_surface_size(winit::dpi::PhysicalSize::new(900, 650).into());
                self.stage = 4;
            } else if elapsed >= 7 && self.stage == 4 {
                world.resource_mut::<Input>().unwrap().quit = true;
                context.request_redraw();
                self.stage = 5;
            }
        }
        if let Some(preview) = &mut self.preview {
            let scale = context.scale_factor as f32;
            // Leave a physical 40px host-owned strip above the native child.
            let [width, height] = context.physical_size;
            preview
                .set_bounds(
                    ClientRect::try_new(
                        0.0,
                        40.0 / scale,
                        width as f32 / scale,
                        height.saturating_sub(40) as f32 / scale,
                    )
                    .unwrap(),
                )
                .unwrap();
            if std::env::args().any(|arg| arg == "--overlay-probe") {
                let images = if world.resource::<Probe>().unwrap().clipped {
                    vec![probe_bitmap()]
                } else {
                    Vec::new()
                };
                preview.set_overlays(&images).unwrap();
            }
            if context.visible {
                preview.show();
            } else {
                preview.hide();
            }
            match preview.frame() {
                Ok(state) => {
                    if self.last_state != Some(state) {
                        eprintln!("host artifact: {state:?}");
                        self.last_state = Some(state);
                    }
                }
                Err(error) => {
                    eprintln!("Preview failed (no fallback): {error}");
                    self.preview = None;
                }
            }
        }
        // Poll Ready too: process failure can arrive after the initial load.
        Some(Duration::from_millis(
            if self.last_state == Some(ArtifactState::Ready) {
                100
            } else {
                16
            },
        ))
    }

    fn detach(&mut self, _: &mut World) {
        self.preview = None;
        eprintln!("host detached before parent destruction");
    }
}

fn probe_bitmap() -> OverlayBitmap {
    let (width, height) = (230u32, 100u32);
    let mut rgba = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let edge = x.min(width - 1 - x).min(y.min(height - 1 - y));
            let a = if edge < 12 { (edge * 8) as u8 } else { 220 };
            let rgb = if edge < 12 { [0, 0, 0] } else { [204, 38, 178] };
            rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
        }
    }
    OverlayBitmap {
        bounds: ClientRect::try_new(320., 140., 230., 100.).unwrap(),
        width,
        height,
        rgba,
        pointer_bounds: if std::env::args().any(|a| a == "--passive-overlay") {
            None
        } else {
            Some(ClientRect::try_new(332., 152., 206., 76.).unwrap())
        },
    }
}
#[derive(Default)]
struct Probe {
    clipped: bool,
    clicks: u32,
}
fn probe_actions(world: &mut World) {
    let name = world.resource::<Fired>().unwrap().name.clone();
    if name.as_deref() == Some("clip") {
        let probe = world.resource_mut::<Probe>().unwrap();
        probe.clipped = !probe.clipped;
        eprintln!("host clipping {}", probe.clipped);
    }
    if name.as_deref() == Some("popup") {
        let probe = world.resource_mut::<Probe>().unwrap();
        probe.clicks += 1;
        eprintln!("HOST POPUP CLICK {}", probe.clicks);
    }
}
fn main() {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut app = App::new();
    if std::env::args().any(|arg| arg == "--overlay-probe") {
        app.add_plugin(Chrome);
        app.world_mut().insert_resource(Probe {
            clipped: true,
            clicks: 0,
        });
        app.world_mut().spawn((
            Rect {
                x: 320.,
                y: 140.,
                w: 230.,
                h: 100.,
            },
            Color::rgb(0.8, 0.15, 0.7),
            Label::new("HOST POPUP"),
            Button::new("popup"),
        ));
        app.world_mut().spawn((
            Rect {
                x: 10.,
                y: 0.,
                w: 220.,
                h: 38.,
            },
            Color::rgb(0.5, 0.8, 0.7),
            Label::new("Toggle native overlay"),
            Button::new("clip"),
        ));
        app.add_system(Schedule::Update, probe_actions);
    }
    Session::new(app)
        .native_host(move |_| {
            Box::new(PreviewHost {
                preview: None,
                attempted: false,
                started: Instant::now(),
                smoke,
                stage: 0,
                last_state: None,
                last_metrics: None,
            })
        })
        .run();
}
