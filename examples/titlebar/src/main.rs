//! A window with a titlebar the app draws. Drag the band to move the window,
//! double-click it to maximize, and drag the edges to resize. On Windows and
//! Linux the band ends with drawn minimize, maximize and close buttons; on
//! macOS the traffic lights sit in it. `--backdrop` asks for Mica or vibrancy.

use rndr_core::{
    Align, App, Backdrop, Button, Entity, Items, Justify, Label, Layout, Schedule, Size, Titlebar,
    WindowChrome, WindowConfig, WindowDrag,
};
use rndr_render::Session;
use rndr_ui::{window_controls, Token, UiPlugin, UiStyle};

const BAND: f32 = 40.0;

struct Spacer(Entity);

fn main() {
    let backdrop = std::env::args().any(|arg| arg == "--backdrop");
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "Custom titlebar".into(),
        titlebar: Some(Titlebar::new(BAND)),
        transparent: backdrop,
        backdrop: if backdrop {
            Backdrop::Window
        } else {
            Backdrop::None
        },
        ..WindowConfig::default()
    });
    app.add_plugin(UiPlugin);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, clear_native_controls);
    Session::new(app).run();
}

fn setup(world: &mut rndr_core::World) {
    let backdrop = world
        .resource::<WindowConfig>()
        .is_some_and(|config| config.backdrop != Backdrop::None);
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    if !backdrop {
        world.insert(
            root,
            UiStyle {
                background: Some(Token::Canvas.into()),
                ..UiStyle::default()
            },
        );
    }
    let band = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(BAND),
            gap: 8.0,
            ..Layout::row()
        },
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
        WindowDrag,
        UiStyle {
            background: Some(Token::Navigation.into()),
            ..UiStyle::default()
        },
    ));
    rndr_core::set_parent(world, band, root);
    let spacer = world.spawn((Layout {
        width: Size::Px(4.0),
        height: Size::Px(1.0),
        ..Layout::row()
    },));
    rndr_core::set_parent(world, spacer, band);
    world.insert_resource(Spacer(spacer));
    let title = world.spawn((Layout::row(), Label::new("Custom titlebar"), ink()));
    rndr_core::set_parent(world, title, band);
    let tab = world.spawn((
        Layout {
            padding: 6.0,
            ..Layout::row()
        },
        Button::new("tab"),
        Label::new("A button in the band"),
        UiStyle {
            background: Some(Token::ControlHover.into()),
            foreground: Some(Token::Text.into()),
            ..UiStyle::default()
        },
    ));
    rndr_core::set_parent(world, tab, band);
    let fill = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.0),
        ..Layout::row()
    },));
    rndr_core::set_parent(world, fill, band);
    window_controls(world, band);

    let body = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 24.0,
            ..Layout::column()
        },
        Label::new("Drag the band to move the window. Drag an edge to resize it."),
        ink(),
    ));
    rndr_core::set_parent(world, body, root);
}

fn ink() -> UiStyle {
    UiStyle {
        foreground: Some(Token::Text.into()),
        ..UiStyle::default()
    }
}

/// Keep the band's content clear of the macOS traffic lights.
fn clear_native_controls(world: &mut rndr_core::World) {
    let leading = world
        .resource::<WindowChrome>()
        .map(|chrome| chrome.leading)
        .unwrap_or_default();
    let Some(spacer) = world.resource::<Spacer>().map(|s| s.0) else {
        return;
    };
    let width = (leading + 8.0).max(4.0);
    if let Some(layout) = world.get_mut::<Layout>(spacer) {
        if layout.width != Size::Px(width) {
            layout.width = Size::Px(width);
        }
    }
}
