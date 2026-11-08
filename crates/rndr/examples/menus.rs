//! Context and popup menus drawn by rndr.
//!
//! Right-click the card (Control-click on macOS, or focus it and press the
//! Menu key or Shift+F10) for a context menu with a submenu. Click "File" for a
//! menu below the button. The status line shows the last pick or dismissal.
//! With `--features menubar` on macOS or Windows the window also gets a
//! native menu bar; Linux has none.
//!
//! `cargo run -p rndr --example menus` opens a window;
//! `cargo run -p rndr --example menus -- --png OUT.png` writes the open menus headless.
use rndr_core::*;
use rndr_ui::menu::{open_menu, MenuAt, MenuDismissed, MenuEntry, MenuItem, MenuPicked};
use rndr_ui::{UiPlugin, UiTheme};

const W: f32 = 640.0;
const H: f32 = 420.0;

struct Scene {
    card: Entity,
    status: Entity,
    wrap: bool,
}

fn context_entries(wrap: bool) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Group("Clipboard".into()),
        MenuItem::new(1, "Cut").shortcut("Ctrl+X").into(),
        MenuItem::new(2, "Copy").shortcut("Ctrl+C").into(),
        MenuItem::new(3, "Paste")
            .shortcut("Ctrl+V")
            .enabled(false)
            .into(),
        MenuEntry::Separator,
        MenuItem::new(4, "Wrap lines").checked(wrap).into(),
        MenuEntry::submenu(
            "Sort by",
            vec![
                MenuItem::new(10, "Name").checked(true).into(),
                MenuItem::new(11, "Date modified").checked(false).into(),
                MenuItem::new(12, "Size").checked(false).into(),
            ],
        ),
        MenuEntry::Separator,
        MenuItem::new(5, "Delete").shortcut("Del").danger().into(),
    ]
}

fn file_entries() -> Vec<MenuEntry> {
    vec![
        MenuItem::new(20, "New window").shortcut("Ctrl+N").into(),
        MenuItem::new(21, "Open…").shortcut("Ctrl+O").into(),
        MenuEntry::submenu(
            "Open recent",
            vec![
                MenuItem::new(30, "notes.md").into(),
                MenuItem::new(31, "budget.csv").into(),
            ],
        ),
        MenuEntry::Separator,
        MenuItem::new(22, "Close").shortcut("Ctrl+W").into(),
    ]
}

fn open_file(world: &mut World, button: Entity) {
    let _ = open_menu(world, button, MenuAt::Below, &file_entries());
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    let e = world.spawn((
        Layout {
            width: Size::Hug,
            ..Layout::column()
        },
        Label {
            color: Some(Color::rgb(0.1, 0.1, 0.12)),
            ..Label::new(text)
        },
    ));
    set_parent(world, e, parent);
    e
}

fn build(app: &mut App) -> Entity {
    app.add_plugin(UiPlugin);
    let world = app.world_mut();
    world.insert_resource(UiTheme::default());
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.0,
            gap: 16.0,
            ..Layout::column()
        },
        Color::rgb(0.95, 0.95, 0.96),
    ));
    let file = world.spawn((
        Layout {
            width: Size::Hug,
            padding: 8.0,
            ..Layout::row()
        },
        Color::rgb(0.86, 0.87, 0.9),
        Radius(6.0),
        Focusable,
    ));
    world.insert(file, Button::new("file"));
    world.insert(file, ReleaseActivation);
    world.insert(file, ActivationHandler(open_file));
    set_parent(world, file, root);
    label(world, file, "File");
    let card = world.spawn((
        Layout {
            width: Size::Px(260.0),
            height: Size::Px(120.0),
            padding: 16.0,
            ..Layout::column()
        },
        Color::rgb(1.0, 1.0, 1.0),
        Radius(12.0),
        ContextTarget,
        Focusable,
    ));
    set_parent(world, card, root);
    label(world, card, "Right-click me");
    let status = label(world, root, "No menu used yet");
    world.insert_resource(Scene {
        card,
        status,
        wrap: false,
    });
    let mut bar = Menu::new();
    bar.submenu("File", |m| {
        m.add(Item::new("New window").shortcut("Ctrl+N"));
        m.separator();
        m.add(Item::new("Close").shortcut("Ctrl+W"));
    });
    bar.submenu("View", |m| {
        m.add(Item::new("Wrap lines").checked(false));
        m.submenu("Zoom", |z| {
            z.add(Item::new("Zoom in").shortcut("Ctrl+="));
            z.add(Item::new("Zoom out").shortcut("Ctrl+-"));
        });
    });
    set_app_menu(world, bar);
    app.add_system(Schedule::Update, update);
    file
}

fn update(world: &mut World) {
    let presses = events::<SecondaryPress>(world).to_vec();
    let picks = events::<MenuPicked>(world).to_vec();
    let dismissals = events::<MenuDismissed>(world).len();
    let bar_picks = events::<AppMenuPicked>(world).to_vec();
    let Some(scene) = world.resource::<Scene>() else {
        return;
    };
    let (card, status, mut wrap) = (scene.card, scene.status, scene.wrap);
    for press in presses.into_iter().filter(|p| p.entity == card) {
        let _ = open_menu(world, card, MenuAt::Point(press.at), &context_entries(wrap));
    }
    let mut text = None;
    for pick in picks {
        if pick.item == 4 {
            wrap = !wrap;
        }
        text = Some(format!("Picked item {}", pick.item));
    }
    for pick in bar_picks {
        let name = app_menu(world)
            .and_then(|m| m.item(pick.item))
            .map(|i| i.label.clone())
            .unwrap_or_default();
        text = Some(format!("Menu bar: {name}"));
    }
    if text.is_none() && dismissals > 0 {
        text = Some("Menu dismissed".into());
    }
    world.resource_mut::<Scene>().unwrap().wrap = wrap;
    if let (Some(text), Some(label)) = (text, world.get_mut::<Label>(status)) {
        label.text = text;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mut app = App::new();
    let file = build(&mut app);
    if let Some(out) = args
        .iter()
        .position(|a| a == "--png")
        .and_then(|i| args.get(i + 1))
    {
        let engine: std::sync::Arc<dyn TextEngine> =
            std::sync::Arc::new(rndr_render::ParleyText::bundled());
        app.world_mut().insert_resource(engine);
        app.world_mut().insert_resource(Viewport::new(W, H));
        rndr_paint::frame(&mut app);
        let card = app.world().resource::<Scene>().unwrap().card;
        let r = *app.world().get::<Rect>(card).unwrap();
        let menu = open_menu(
            app.world_mut(),
            card,
            MenuAt::Point([r.x + 120.0, r.y + 30.0]),
            &context_entries(true),
        )
        .map_err(|e| format!("{e:?}"))?;
        rndr_paint::frame(&mut app);
        let sort = menu.submenu_row(app.world(), "Sort by").unwrap();
        request_focus(app.world_mut(), sort);
        try_activate(app.world_mut(), sort);
        let date = menu.item(app.world(), 11).unwrap();
        request_focus(app.world_mut(), date);
        let _ = file;
        let frame = rndr_paint::frame(&mut app);
        let image = rndr_render::Exporter::new()?.render_frame(
            Some(app.world()),
            &frame,
            W as u32 * 2,
            H as u32 * 2,
            2.0,
        )?;
        std::fs::write(out, rndr_render::encode_png(&image)?)?;
        return Ok(());
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr menus".into(),
        size: LogicalSize::new(W, H),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}
