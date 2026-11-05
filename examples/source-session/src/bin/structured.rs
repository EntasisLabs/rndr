//! E6.1 nested structured document demo. No Markdown, filesystem or persistence.
use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    BlockKind, ContainerKind, ListKind, Marks, Operation, StructuredDocument, StructuredSelection,
    TextSelection, TextStyle, Transaction,
};
use rndr_core::structured_native::*;
use rndr_core::*;
use rndr_ui::controls::{host_intent, Button as UiButton, ButtonProps, ButtonVariant, HostIntent};
use rndr_ui::structured::*;
use rndr_ui::{apply_theme_bridge, Token, Typography, UiId, UiPlugin, UiRecipe, UiStyle, UiTheme};
use std::sync::Arc;

const INITIAL: &str = "A real structured document\nSelect text, then format a tree, not Markdown.\nA task item — click its checkbox.\nNested task — Tab / Shift-Tab changes ownership.\nAnother task — Enter continues; empty Enter exits.\nQuoted text: é, مرحبا and 👩‍💻.\nEvery edit shares tree-owned undo history.";
#[derive(Clone)]
struct Demo {
    root: Entity,
    view: StructuredSurfaceHandle,
    document: StructuredHandle,
    status: Entity,
    tree: Entity,
    message: String,
}
fn label(world: &mut World, parent: Entity, text: &str, typography: Typography) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new(text),
        UiRecipe(UiStyle {
            foreground: Some(Token::Text.into()),
            typography: Some(typography),
            ..Default::default()
        }),
    ));
    set_parent(world, entity, parent);
    entity
}
fn row(world: &mut World, parent: Entity) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Fill,
        gap: 6.0,
        ..Layout::row()
    },));
    set_parent(world, entity, parent);
    entity
}
fn button(world: &mut World, parent: Entity, id: u64, text: &str) {
    let mut props = ButtonProps::new(text, HostIntent::Command(id));
    props.variant = ButtonVariant::Secondary;
    UiButton::mount(world, parent, UiId(id), props).unwrap();
}
fn setup(world: &mut World) {
    apply_theme_bridge(
        world,
        UiTheme {
            reading_scale: 1.2,
            ..Default::default()
        },
    );
    let mut doc = StructuredDocument::new(INITIAL);
    doc.set_block_kind(BlockKind::Heading(1), EditOrigin::Host(0))
        .unwrap();
    let first = doc.blocks()[2].id;
    let last = doc.blocks()[4].id;
    let range = TextSelection {
        anchor: rndr_core::structured::Position {
            block: first,
            offset: 0,
            affinity: Affinity::After,
        },
        head: rndr_core::structured::Position {
            block: last,
            offset: doc.blocks()[4].len(),
            affinity: Affinity::After,
        },
    };
    let nested = doc.blocks()[3].id;
    let quote = doc.blocks()[5].id;
    let q = rndr_core::structured::Position {
        block: quote,
        offset: 0,
        affinity: Affinity::After,
    };
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Host(0),
        group: None,
        operations: vec![
            Operation::Wrap {
                range,
                kind: ContainerKind::List(ListKind::Task),
            },
            Operation::Indent { block: nested },
            Operation::Wrap {
                range: TextSelection { anchor: q, head: q },
                kind: ContainerKind::Quote,
            },
        ],
    })
    .unwrap();
    let start = INITIAL.find("a tree").unwrap();
    let a = doc.position_at(start, Affinity::After).unwrap();
    let h = doc.position_at(start + 6, Affinity::After).unwrap();
    doc.set_selection(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: h,
    }))
    .unwrap();
    doc.set_marks(
        Marks {
            strong: true,
            ..Default::default()
        },
        EditOrigin::Host(0),
    )
    .unwrap();
    let mut docs = StructuredDocuments::default();
    let document = docs.insert(doc);
    world.insert_resource(docs);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.0,
            gap: 10.0,
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::Canvas.into()),
            ..Default::default()
        }),
    ));
    label(world, root, "Structured / E6.1", Typography::Heading);
    label(
        world,
        root,
        "Real nested lists · tasks · quotes · rules · one tree-owned history",
        Typography::Secondary,
    );
    let toolbar = row(world, root);
    for (id, text) in [
        (1, "Strong"),
        (2, "Emphasis"),
        (3, "Link"),
        (4, "Unlink"),
        (5, "Heading"),
        (6, "Paragraph"),
        (7, "Undo"),
        (8, "Redo"),
    ] {
        button(world, toolbar, id, text);
    }
    let toolbar = row(world, root);
    for (id, text) in [
        (12, "Quote"),
        (13, "Bullet"),
        (14, "Ordered"),
        (15, "Tasks"),
        (16, "Indent"),
        (17, "Outdent"),
        (18, "Check"),
        (19, "Rule"),
    ] {
        button(world, toolbar, id, text);
    }
    let toolbar = row(world, root);
    for (id, text) in [
        (9, "Select block"),
        (10, "Delete block"),
        (11, "Readonly"),
        (20, "Blue mono"),
        (21, "Clear style"),
        (22, "Metadata"),
    ] {
        button(world, toolbar, id, text);
    }
    let status = label(world, root, "", Typography::Secondary);
    let paper = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 14.0,
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::Content.into()),
            ..Default::default()
        }),
    ));
    set_parent(world, paper, root);
    let view = StructuredSurface::mount(
        world,
        paper,
        UiId(100),
        StructuredSurfaceProps::new(document, "Generic structured document"),
    )
    .unwrap();
    let tree = label(world, root, "", Typography::Code);
    world.get_mut::<Layout>(tree).unwrap().height = Size::Px(88.0);
    label(
        world,
        root,
        "Tab nests items; Shift-Tab promotes. Rule needs a caret. Readonly permits selection/copy.",
        Typography::Secondary,
    );
    world.insert_resource(Demo {
        root,
        view,
        document,
        status,
        tree,
        message: "Ready — ‘a tree’ is selected. Format it, type, split, join, then undo.".into(),
    });
    world.insert_resource(Focus {
        entity: Some(view.editor),
    });
}
fn update(world: &mut World) {
    let Some(mut demo) = world.resource::<Demo>().cloned() else {
        return;
    };
    let commands = take_action_intents(world)
        .iter()
        .filter_map(|request| host_intent(world, request))
        .collect::<Vec<_>>();
    for intent in commands {
        let HostIntent::Command(id) = intent else {
            continue;
        };
        let command = match id {
            1 => Some(StructuredCommand::Strong),
            2 => Some(StructuredCommand::Emphasis),
            3 => Some(StructuredCommand::Link(Some("https://example.com".into()))),
            4 => Some(StructuredCommand::Link(None)),
            5 => Some(StructuredCommand::Block(BlockKind::Heading(2))),
            6 => Some(StructuredCommand::Block(BlockKind::Paragraph)),
            7 => Some(StructuredCommand::Undo),
            8 => Some(StructuredCommand::Redo),
            9 => Some(StructuredCommand::SelectBlock),
            10 => Some(StructuredCommand::DeleteBlock),
            12 => Some(StructuredCommand::Quote),
            13 => Some(StructuredCommand::List(ListKind::Bullet)),
            14 => Some(StructuredCommand::List(ListKind::Ordered { start: 1 })),
            15 => Some(StructuredCommand::List(ListKind::Task)),
            16 => Some(StructuredCommand::Indent),
            17 => Some(StructuredCommand::Outdent),
            18 => Some(StructuredCommand::ToggleTask),
            19 => Some(StructuredCommand::HorizontalRule),
            20 => Some(StructuredCommand::TextStyle(TextStyle {
                color: Some([60, 145, 230]),
                family: Some("monospace".into()),
            })),
            21 => Some(StructuredCommand::TextStyle(TextStyle::default())),
            22 => Some(StructuredCommand::Attributes(
                [("label".into(), "Host-owned inert metadata".into())].into(),
            )),
            _ => None,
        };
        if let Some(command) = command {
            demo.message = match demo.view.command(world, command) {
                Ok(true) => "Applied to the saved tree selection.".into(),
                Ok(false) => "No document change.".into(),
                Err(error) => format!("Refused without changing the document: {error:?}"),
            };
        } else if id == 11 {
            demo.message = match command_ready(world, demo.view.editor) {
                Ok(id) => {
                    let doc = world
                        .resource_mut::<StructuredDocuments>()
                        .unwrap()
                        .get_mut(id)
                        .unwrap();
                    let value = !doc.is_readonly();
                    doc.set_readonly(value);
                    format!("Readonly {value} — selection/copy remain available.")
                }
                Err(error) => format!("Refused: {error:?}"),
            };
        }
    }
    let doc = world
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(demo.document)
        .unwrap();
    let status = format!(
        "r{} · {} blocks · {} · {}",
        doc.revision().0,
        doc.blocks().len(),
        if doc.is_readonly() {
            "READONLY"
        } else {
            "editable"
        },
        demo.message
    );
    let mut tree = String::from("GENERIC TREE  /  display is not Markdown serialization\n");
    for block in doc.blocks().iter().take(5) {
        let marks = block
            .runs
            .iter()
            .map(|r| {
                format!(
                    "{}{}{}:{}",
                    if r.marks.strong { "B" } else { "" },
                    if r.marks.emphasis { "I" } else { "" },
                    if r.marks.link.is_some() { "L" } else { "" },
                    r.text.chars().count()
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        tree.push_str(&format!(
            "node {}  {:?} parent={:?}  [{}]\n",
            block.id.0, block.kind, block.parent, marks
        ));
    }
    if doc.blocks().len() > 5 {
        tree.push_str("… more blocks\n");
    }
    world.get_mut::<Label>(demo.status).unwrap().text = status;
    world.get_mut::<Label>(demo.tree).unwrap().text = tree;
    world.insert_resource(demo);
}
fn demo_app() -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(WindowConfig {
        title: "Structured document — rndr".into(),
        size: LogicalSize::new(1000.0, 820.0),
        min_size: Some(LogicalSize::new(880.0, 720.0)),
        ..Default::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}
pub fn main() {
    let mut args = std::env::args().skip(1);
    let output = match args.next().as_deref() {
        None => None,
        Some("--capture") => Some(args.next().expect("capture needs OUTPUT.png")),
        _ => panic!("usage: structured [--capture OUTPUT.png]"),
    };
    assert!(args.next().is_none(), "unexpected argument");
    let mut app = demo_app();
    if let Some(output) = output {
        app.world_mut()
            .insert_resource(rndr_render::observe::ObservationContent::Public);
        app.world_mut()
            .insert_resource(Viewport::new(1000.0, 820.0));
        let _ = rndr_paint::frame(&mut app);
        let root = app.world().resource::<Demo>().unwrap().root;
        let mut exporter = rndr_render::Exporter::new().expect("offscreen adapter");
        let capture = rndr_render::capture::capture(
            &mut app,
            &mut exporter,
            rndr_render::capture::Target::Entity(root),
            |w, e| rndr_ui::identity(w, e).map(|id| id.0),
        )
        .expect("public structured fixture");
        let png = rndr_render::encode_png(&capture.image).unwrap();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap()
            .write_all(&png)
            .unwrap();
    } else {
        rndr_render::run(app);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_demo_layout_keeps_editor_and_buttons_reachable() {
        for (width, height) in [(1000.0, 820.0), (880.0, 720.0)] {
            let mut app = demo_app();
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            let _ = rndr_paint::frame(&mut app);
            let demo = app.world().resource::<Demo>().unwrap();
            let rect = app.world().get::<Rect>(demo.view.editor).unwrap();
            assert!(
                rect.h >= 200.0,
                "editor height {} at {width}x{height}",
                rect.h
            );
            assert!(rect.y + rect.h <= height);
            let foreground = app
                .world()
                .get::<Label>(demo.view.editor)
                .unwrap()
                .color
                .unwrap();
            assert_ne!(
                foreground,
                *app.world().get::<Color>(demo.view.editor).unwrap()
            );
            assert!(app
                .world()
                .get::<Spans>(demo.view.editor)
                .unwrap()
                .runs
                .iter()
                .all(|span| span.color == foreground));
            for entity in app.world().entities_with::<rndr_core::Button>() {
                let rect = app.world().get::<Rect>(entity).unwrap();
                assert!(
                    rect.x >= 0.0
                        && rect.y >= 0.0
                        && rect.x + rect.w <= width
                        && rect.y + rect.h <= height
                );
            }
        }
    }
    #[test]
    fn structured_demo_is_shaped_and_shows_real_blocks() {
        let mut app = demo_app();
        app.world_mut()
            .insert_resource(Viewport::new(1000.0, 820.0));
        let _ = rndr_paint::frame(&mut app);
        let demo = app.world().resource::<Demo>().unwrap().clone();
        let display = structured_display(app.world(), demo.view.editor).unwrap();
        assert_eq!(display.blocks.len(), 7);
        assert!(!display.containers.is_empty());
        assert!(display.blocks[2].parent.is_some());
        assert_eq!(display.blocks[0].kind, BlockKind::Heading(1));
        let access = access_text(app.world_mut(), demo.view.editor).unwrap();
        assert!(!access.layout.lines.is_empty());
        assert!(access.caret_bounds().h > 0.0);
    }
}
