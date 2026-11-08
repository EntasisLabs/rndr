use rndr_canvas::*;
use rndr_canvas_ui::editor::{CanvasEditor, StyleChange, Tool};
use rndr_canvas_ui::panels;
use rndr_canvas_ui::view::{self, CanvasView};
use rndr_core::{access_tree, set_parent, App, Layout, Size, Viewport};
use rndr_ui::UiPlugin;
use std::sync::Arc;

fn app() -> App {
    let store = Arc::new(MemoryStore::new());
    let scene = Scene {
        nodes: vec![Node::new(
            "a",
            Content::Shape {
                shape: ShapeKind::Rectangle,
                fill: "#ff0000".into(),
                stroke: "#000000".into(),
                stroke_width: 2.0,
                line: None,
            },
            Transform {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
                rotation: 0.0,
            },
        )],
        ..Scene::default()
    };
    store
        .create(CreateRequest {
            id: "doc".into(),
            title: "Doc".into(),
            scene,
        })
        .unwrap();
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let world = app.world_mut();
    world.insert_resource(Viewport::new(1200.0, 2000.0));
    let root = world.spawn((Layout {
        width: Size::Px(1200.0),
        ..Layout::column()
    },));
    let toolbar = world.spawn((Layout::column(),));
    let side = world.spawn((Layout::column(),));
    set_parent(world, toolbar, root);
    set_parent(world, side, root);
    let mut editor = CanvasEditor::open(store, "doc").unwrap();
    editor.set_tool(Tool::Pen);
    view::install(world, CanvasView::new(editor, 1));
    panels::mount(world, toolbar, side, [1200.0, 400.0]);
    panels::refresh(world);
    app.update();
    app
}

#[test]
fn panel_buttons_expose_distinct_names_and_selected_state() {
    let app = app();
    let tree = access_tree(app.world());
    let node = |name: &str| {
        tree.nodes
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no access node named {name:?}"))
    };
    assert!(node("Pen").selected);
    assert!(!node("Select").selected);
    assert!(node("Stroke color Ink").selected);
    assert!(!node("Fill color Ink").selected);
    assert!(node("Fill color None").selected);
    assert!(node("Stroke width Medium").selected);
    assert!(node("Text size 24 pt").selected);
    assert!(!node("Layer Rectangle").selected);
    node("Hide Rectangle");
    node("Lock Rectangle");
    assert!(!node("Undo").enabled);
    assert!(!node("Delete").enabled);
}

#[test]
fn style_choices_refresh_without_a_document_change() {
    let mut app = app();
    let view = app.world_mut().resource_mut::<CanvasView>().unwrap();
    view.editor.apply_style(StyleChange::StrokeWidth(8.0));
    view.editor.apply_style(StyleChange::FontSize(36.0));
    panels::refresh(app.world_mut());
    app.update();
    let tree = access_tree(app.world());
    let selected = |name: &str| tree.nodes.iter().any(|n| n.name == name && n.selected);
    assert!(selected("Stroke width Heavy"));
    assert!(!selected("Stroke width Medium"));
    assert!(selected("Text size 36 pt"));
}
