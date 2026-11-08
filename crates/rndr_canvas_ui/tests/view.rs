use rndr_canvas::*;
use rndr_canvas_ui::editor::{CanvasEditor, Tool};
use rndr_canvas_ui::view::{self, CanvasView};
use rndr_core::{Input, Layout, Pointer, Rect, SurfaceSlot, World};
use std::sync::Arc;

fn world() -> World {
    let store = Arc::new(MemoryStore::new());
    store
        .create(CreateRequest {
            id: "doc".into(),
            title: "Doc".into(),
            scene: Scene::default(),
        })
        .unwrap();
    let mut world = World::new();
    let slot = world.spawn((Layout::column(), SurfaceSlot { id: 7 }));
    world.insert(
        slot,
        Rect {
            x: 100.0,
            y: 50.0,
            w: 600.0,
            h: 400.0,
        },
    );
    let mut editor = CanvasEditor::open(store, "doc").unwrap();
    editor.set_tool(Tool::Rectangle);
    view::install(&mut world, CanvasView::new(editor, 7));
    world
}

fn step(world: &mut World, position: [f32; 2], pressed: bool, down: bool, released: bool) {
    let pointer = Pointer {
        position,
        pressed,
        down,
        released,
        ..Pointer::default()
    };
    world.insert_resource(Input {
        pointer,
        ..Input::default()
    });
    view::update(world);
}

#[test]
fn pointer_input_is_relative_to_the_slot() {
    let mut world = world();
    step(&mut world, [120.0, 70.0], true, true, false);
    step(&mut world, [220.0, 120.0], false, true, false);
    step(&mut world, [220.0, 120.0], false, false, true);
    let view = world.resource::<CanvasView>().unwrap();
    let nodes = &view.editor.mutation().document.scene.nodes;
    assert_eq!(nodes.len(), 1);
    let t = nodes[0].transform;
    assert_eq!((t.x, t.y, t.width, t.height), (20.0, 20.0, 100.0, 50.0));
}

#[test]
fn presses_outside_the_slot_are_ignored_but_drags_out_of_it_continue() {
    let mut world = world();
    step(&mut world, [10.0, 10.0], true, true, false);
    step(&mut world, [300.0, 300.0], false, true, false);
    step(&mut world, [300.0, 300.0], false, false, true);
    assert!(world
        .resource::<CanvasView>()
        .unwrap()
        .editor
        .mutation()
        .document
        .scene
        .nodes
        .is_empty());
    step(&mut world, [650.0, 400.0], true, true, false);
    step(&mut world, [900.0, 600.0], false, true, false);
    step(&mut world, [900.0, 600.0], false, false, true);
    let view = world.resource::<CanvasView>().unwrap();
    let t = view.editor.mutation().document.scene.nodes[0].transform;
    assert_eq!((t.x, t.y, t.width, t.height), (550.0, 350.0, 250.0, 200.0));
}
