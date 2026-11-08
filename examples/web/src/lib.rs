//! rndr in the browser: one WebGPU canvas with decorated cards, a spring that
//! runs on each click, and a read-only label whose copy reaches the browser
//! clipboard.
//!
//! ```text
//! scripts/web-build.sh
//! python3 -m http.server -d examples/web 8000
//! ```
//!
//! `cargo run -p web-hello` opens the same app in a native window.

use rndr_core::motion::{self, Prop, Spring, Transition};
use rndr_core::{
    set_parent, App, Axis, Clipboard, Color, Corners, Entity, Gradient, Input, Label, Layout,
    LogicalSize, Schedule, Selectable, SelectablePlugin, Shadow, Shadows, Size, TextStyle,
    WindowConfig, World,
};

const INK: Color = Color::rgb(0.1, 0.11, 0.14);

/// Cards the click handler animates.
struct Cards {
    cards: Vec<Entity>,
    flipped: bool,
}

/// The app that `start` runs in the browser and `main` runs natively.
pub fn app() -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr web".into(),
        size: LogicalSize {
            width: 720.0,
            height: 420.0,
        },
        ..WindowConfig::default()
    });
    app.world_mut().insert_resource(Clipboard::default());
    app.add_plugin(SelectablePlugin);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, on_click);
    app
}

fn label(world: &mut World, parent: Entity, text: &str, size: f32, weight: u16) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(text).with_color(INK),
        TextStyle {
            size,
            weight,
            ..TextStyle::default()
        },
    ));
    set_parent(world, entity, parent);
    entity
}

fn setup(world: &mut World) {
    let root = world.spawn((
        Layout {
            gap: 18.0,
            padding: 28.0,
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Color::rgb(0.95, 0.96, 0.98),
    ));
    label(world, root, "rndr on WebGPU", 28.0, 700);
    label(
        world,
        root,
        "Click anywhere to spring the cards.",
        16.0,
        400,
    );
    let row = world.spawn(Layout {
        axis: Axis::Row,
        gap: 20.0,
        // Room for the card shadows, which the row clips.
        padding: 16.0,
        width: Size::Fill,
        height: Size::Px(170.0),
        ..Layout::row()
    });
    set_parent(world, row, root);
    let tints = [
        (Color::rgb(0.38, 0.55, 0.98), Color::rgb(0.25, 0.32, 0.86)),
        (Color::rgb(0.98, 0.56, 0.42), Color::rgb(0.88, 0.3, 0.45)),
        (Color::rgb(0.36, 0.82, 0.64), Color::rgb(0.13, 0.6, 0.56)),
    ];
    let mut cards = Vec::new();
    for (top, bottom) in tints {
        let card = world.spawn((
            Layout {
                width: Size::Px(200.0),
                height: Size::Px(130.0),
                ..Layout::column()
            },
            Color::rgb(1.0, 1.0, 1.0),
            Corners::all(18.0),
            Gradient::vertical(top, bottom),
            Shadows(vec![Shadow::drop(
                8.0,
                18.0,
                Color::rgba(0.0, 0.0, 0.0, 0.25),
            )]),
        ));
        set_parent(world, card, row);
        motion::enter(world, card, Transition::scale(0.85));
        cards.push(card);
    }
    let note = label(
        world,
        root,
        "Drag across this line, then press Ctrl+C to copy it to the clipboard.",
        16.0,
        400,
    );
    world.insert(note, Selectable::default());
    world.insert_resource(Cards {
        cards,
        flipped: false,
    });
}

fn on_click(world: &mut World) {
    let pressed = world
        .resource::<Input>()
        .is_some_and(|input| input.pointer.pressed);
    if !pressed {
        return;
    }
    let Some(state) = world.resource_mut::<Cards>() else {
        return;
    };
    state.flipped = !state.flipped;
    let flipped = state.flipped;
    let cards = state.cards.clone();
    for (i, card) in cards.into_iter().enumerate() {
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let (y, turn) = if flipped {
            (24.0, 0.12 * sign)
        } else {
            (0.0, 0.0)
        };
        motion::animate(world, card, Prop::Y, y, Spring::BOUNCY);
        motion::animate(world, card, Prop::Rotate, turn, Spring::BOUNCY);
    }
}

/// Browser entry point that wasm-bindgen calls when the module loads.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    rndr_render::run(app());
}
