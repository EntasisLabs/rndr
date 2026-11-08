//! The desktop's appearance, every cursor shape and opening links.
//!
//! The window follows the OS light/dark setting and shows its accent colour
//! and reduced-motion setting. Hover a tile to see its cursor; click the top
//! tile to open a page in the default browser.
//!
//! `cargo run -p rndr_render --example os_basics`
use rndr_core::{
    events, hit_test, request_open_url, set_parent, system_appearance, App, Color, Component,
    Corners, CursorStyle, Entity, Input, Label, Layout, MotionWatch, Schedule, Size,
    SystemAppearance, SystemAppearanceChanged, WindowConfig, World,
};

const LINK: &str = "https://www.rust-lang.org/";
const COLUMNS: usize = 6;
const CURSORS: [(CursorStyle, &str); 35] = [
    (CursorStyle::Arrow, "arrow"),
    (CursorStyle::Hand, "hand"),
    (CursorStyle::Text, "text"),
    (CursorStyle::Hidden, "hidden"),
    (CursorStyle::Grab, "grab"),
    (CursorStyle::Grabbing, "grabbing"),
    (CursorStyle::Move, "move"),
    (CursorStyle::Crosshair, "crosshair"),
    (CursorStyle::NotAllowed, "not allowed"),
    (CursorStyle::NoDrop, "no drop"),
    (CursorStyle::Copy, "copy"),
    (CursorStyle::Alias, "alias"),
    (CursorStyle::ContextMenu, "context menu"),
    (CursorStyle::Help, "help"),
    (CursorStyle::Progress, "progress"),
    (CursorStyle::Wait, "wait"),
    (CursorStyle::Cell, "cell"),
    (CursorStyle::VerticalText, "vertical text"),
    (CursorStyle::AllScroll, "all scroll"),
    (CursorStyle::ZoomIn, "zoom in"),
    (CursorStyle::ZoomOut, "zoom out"),
    (CursorStyle::ResizeColumn, "col resize"),
    (CursorStyle::ResizeRow, "row resize"),
    (CursorStyle::ResizeEast, "e resize"),
    (CursorStyle::ResizeNorth, "n resize"),
    (CursorStyle::ResizeNorthEast, "ne resize"),
    (CursorStyle::ResizeNorthWest, "nw resize"),
    (CursorStyle::ResizeSouth, "s resize"),
    (CursorStyle::ResizeSouthEast, "se resize"),
    (CursorStyle::ResizeSouthWest, "sw resize"),
    (CursorStyle::ResizeWest, "w resize"),
    (CursorStyle::ResizeEastWest, "ew resize"),
    (CursorStyle::ResizeNorthSouth, "ns resize"),
    (CursorStyle::ResizeNorthEastSouthWest, "nesw resize"),
    (CursorStyle::ResizeNorthWestSouthEast, "nwse resize"),
];

#[derive(Clone, Copy)]
enum Tile {
    Link,
    Cursor(CursorStyle),
}
impl Component for Tile {}

/// Entities recoloured when the appearance changes.
struct Scene {
    root: Entity,
    status: Entity,
    swatch: Entity,
    tiles: Vec<Entity>,
}

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr OS basics".into(),
        ..Default::default()
    });
    let world = app.world_mut();
    let fill = |gap| Layout {
        width: Size::Fill,
        gap,
        ..Layout::row()
    };
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 10.0,
        padding: 16.0,
        ..Layout::column()
    },));
    let header = world.spawn((fill(10.0),));
    set_parent(world, header, root);
    let swatch = world.spawn((
        Layout {
            width: Size::Px(24.0),
            height: Size::Px(24.0),
            ..Layout::row()
        },
        Corners::all(12.0),
    ));
    set_parent(world, swatch, header);
    let status = world.spawn((Layout::column(), Label::new("")));
    set_parent(world, status, header);
    let mut tiles = Vec::new();
    let mut tile = |world: &mut World, parent, kind, text: String| {
        let entity = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(36.0),
                padding: 8.0,
                ..Layout::column()
            },
            Corners::all(6.0),
            Label::new(text),
            kind,
        ));
        set_parent(world, entity, parent);
        tiles.push(entity);
    };
    tile(world, root, Tile::Link, format!("Open {LINK}"));
    for row in CURSORS.chunks(COLUMNS) {
        let line = world.spawn((fill(8.0),));
        set_parent(world, line, root);
        for &(style, name) in row {
            tile(world, line, Tile::Cursor(style), name.into());
        }
    }
    world.insert_resource(Scene {
        root,
        status,
        swatch,
        tiles,
    });
    paint(world);
    app.add_system(Schedule::Input, pointer);
    app.add_system(Schedule::Update, |world| {
        if !events::<SystemAppearanceChanged>(world).is_empty() {
            paint(world);
        }
    });
    rndr_render::run(app);
}

fn pointer(world: &mut World) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    let ([x, y], clicked) = (input.pointer.position, input.pointer.released);
    let tile = hit_test(world, x, y).and_then(|e| world.get::<Tile>(e).copied());
    world.insert_resource(MotionWatch { active: true });
    world.insert_resource(match tile {
        Some(Tile::Link) => CursorStyle::Hand,
        Some(Tile::Cursor(style)) => style,
        None => CursorStyle::Arrow,
    });
    if clicked && matches!(tile, Some(Tile::Link)) {
        if let Err(error) = request_open_url(world, LINK) {
            eprintln!("refused: {error}");
        }
    }
}

fn paint(world: &mut World) {
    let system = system_appearance(world);
    let SystemAppearance {
        dark,
        accent,
        reduced_motion,
    } = system;
    let is_dark = dark.unwrap_or(false);
    let accent_or = accent.unwrap_or(Color::rgb(0.2, 0.36, 0.7));
    let (canvas, tile, text) = if is_dark {
        (
            Color::rgb(0.09, 0.1, 0.11),
            Color::rgb(0.17, 0.18, 0.2),
            Color::rgb(0.93, 0.93, 0.95),
        )
    } else {
        (
            Color::rgb(0.96, 0.96, 0.97),
            Color::rgb(1.0, 1.0, 1.0),
            Color::rgb(0.14, 0.15, 0.18),
        )
    };
    let describe = format!(
        "appearance: {}   accent: {}   reduced motion: {}",
        match dark {
            Some(true) => "dark",
            Some(false) => "light",
            None => "no preference",
        },
        accent.map_or("none".into(), |c| format!(
            "#{:02x}{:02x}{:02x}",
            (c.r * 255.0).round() as u8,
            (c.g * 255.0).round() as u8,
            (c.b * 255.0).round() as u8
        )),
        if reduced_motion { "on" } else { "off" },
    );
    eprintln!("{describe}");
    let Some(scene) = world.resource::<Scene>() else {
        return;
    };
    let (root, status, swatch, tiles) =
        (scene.root, scene.status, scene.swatch, scene.tiles.clone());
    world.insert(root, canvas);
    world.insert(swatch, accent_or);
    if let Some(label) = world.get_mut::<Label>(status) {
        label.text = describe;
        label.color = Some(text);
    }
    for entity in tiles {
        let link = matches!(world.get::<Tile>(entity), Some(Tile::Link));
        world.insert(entity, if link { accent_or } else { tile });
        if let Some(label) = world.get_mut::<Label>(entity) {
            label.color = Some(if link {
                Color::rgb(1.0, 1.0, 1.0)
            } else {
                text
            });
        }
    }
}
