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

#[test]
fn claimed_wheel_survives_the_layout_scroll_pass() {
    let mut world = world();
    world.insert_resource(Input {
        pointer: Pointer {
            position: [300.0, 200.0],
            ..Pointer::default()
        },
        scroll: [0.0, 40.0],
        ..Input::default()
    });
    view::take_wheel(&mut world);
    assert_eq!(world.resource::<Input>().unwrap().scroll, [0.0, 0.0]);
    view::update(&mut world);
    assert_eq!(
        world.resource::<CanvasView>().unwrap().editor.camera.pan_y,
        40.0
    );

    world.insert_resource(Input {
        pointer: Pointer {
            position: [20.0, 20.0],
            ..Pointer::default()
        },
        scroll: [0.0, 40.0],
        ..Input::default()
    });
    view::take_wheel(&mut world);
    assert_eq!(world.resource::<Input>().unwrap().scroll, [0.0, 40.0]);
}

#[test]
fn controls_laid_over_the_canvas_keep_their_presses() {
    let mut world = world();
    // A floating toolbar the host paints above the slot: a later sibling
    // under the same parent.
    let host = world.spawn((Layout::column(),));
    world.insert(
        host,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        },
    );
    let slot = world.entities_with::<rndr_core::SurfaceSlot>()[0];
    rndr_core::set_parent(&mut world, slot, host);
    let bar = world.spawn((Layout::column(),));
    rndr_core::set_parent(&mut world, bar, host);
    world.insert(
        bar,
        Rect {
            x: 300.0,
            y: 380.0,
            w: 200.0,
            h: 50.0,
        },
    );
    // Pressing on the bar draws nothing.
    step(&mut world, [320.0, 400.0], true, true, false);
    step(&mut world, [420.0, 300.0], false, true, false);
    step(&mut world, [420.0, 300.0], false, false, true);
    let nodes = |w: &World| {
        w.resource::<CanvasView>()
            .unwrap()
            .editor
            .mutation()
            .document
            .scene
            .nodes
            .len()
    };
    assert_eq!(nodes(&world), 0);
    // A drag that starts on the canvas keeps going under the bar.
    step(&mut world, [150.0, 100.0], true, true, false);
    step(&mut world, [350.0, 410.0], false, true, false);
    step(&mut world, [350.0, 410.0], false, false, true);
    assert_eq!(nodes(&world), 1);
}

#[test]
fn a_fast_click_selects_in_one_frame() {
    let mut world = world();
    // Draw a rectangle, then switch to Select.
    step(&mut world, [120.0, 70.0], true, true, false);
    step(&mut world, [220.0, 120.0], false, true, false);
    step(&mut world, [220.0, 120.0], false, false, true);
    {
        let view = world.resource_mut::<CanvasView>().unwrap();
        view.editor.set_tool(Tool::Select);
        view.editor.select(&[]);
    }
    // Press and release delivered together, as a trackpad tap often is, on
    // the outline: an unfilled shape is picked by its stroke.
    step(&mut world, [120.0, 95.0], true, false, true);
    let view = world.resource::<CanvasView>().unwrap();
    assert_eq!(view.editor.selection().len(), 1);
}
