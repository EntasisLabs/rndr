//! Headless frame strip of the motion runtime on semantic cards: an enter,
//! a bouncy move with rotation, and an exit. Writes one PNG, one row per frame.
//!
//! `cargo run -p rndr --example motion_strip -- OUTPUT.png`
use std::time::Duration;

use rndr_core::motion::{self, Prop, Spring, Transition};
use rndr_core::{set_parent, Axis, Color, Entity, Layout, Sim, Size, Viewport};
use rndr_render::{encode_png, Exporter, RgbaImage};
use rndr_ui::{Card, Typography, UiId, UiPlugin};

const W: u32 = 600;
const H: u32 = 130;

fn card(sim: &mut Sim, row: Entity, id: u64, title: &str, body: &str) -> Entity {
    let host = sim.world().spawn((Layout {
        width: Size::Px(170.0),
        height: Size::Px(90.0),
        ..Layout::column()
    },));
    set_parent(sim.world(), host, row);
    let card = Card::mount(sim.world(), host, UiId(id), title).unwrap();
    rndr_ui::text(sim.world(), card.content, body, Typography::Label).unwrap();
    host
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: motion_strip OUTPUT.png")?;
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world()
        .insert_resource(Viewport::new(W as f32, H as f32));
    #[cfg(feature = "parley")]
    {
        let engine: std::sync::Arc<dyn rndr_core::TextEngine> =
            std::sync::Arc::new(rndr_render::ParleyText::bundled());
        sim.world().insert_resource(engine);
    }
    let row = sim.world().spawn((
        Layout {
            axis: Axis::Row,
            gap: 20.0,
            padding: 20.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::row()
        },
        Color::rgb(0.93, 0.94, 0.96),
    ));
    let entering = card(&mut sim, row, 1, "Enter", "Fade + scale in.");
    let moving = card(&mut sim, row, 2, "Spring", "Bounce + rotate.");
    let leaving = card(&mut sim, row, 3, "Exit", "Slides out, despawns.");
    rndr_paint::frame(&mut sim.app);
    motion::enter(sim.world(), entering, Transition::scale(0.85));
    motion::animate(sim.world(), moving, Prop::Y, 18.0, Spring::BOUNCY);
    motion::animate(sim.world(), moving, Prop::Rotate, -0.12, Spring::BOUNCY);
    motion::exit(sim.world(), leaving, Transition::slide(40.0, 0.0));
    let mut exporter = Exporter::new()?;
    let mut strip = RgbaImage {
        width: W,
        height: 0,
        rgba: Vec::new(),
    };
    for ms in [0u64, 60, 60, 80, 120, 200, 400] {
        sim.advance(Duration::from_millis(ms));
        let frame = rndr_paint::frame(&mut sim.app);
        let image = exporter.render_frame(Some(sim.world()), &frame, W, H, 1.0)?;
        strip.rgba.extend_from_slice(&image.rgba);
        strip
            .rgba
            .extend(std::iter::repeat_n(80u8, W as usize * 4 * 2));
        strip.height += H + 2;
    }
    std::fs::write(out, encode_png(&strip)?)?;
    Ok(())
}
