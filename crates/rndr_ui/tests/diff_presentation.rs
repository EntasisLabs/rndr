//! Host-supplied diff presentation. Headless only; not a native certificate.
use rndr_core::*;
use rndr_ui::{
    collections::{ItemId, VariableAnchor, VirtualViewHandle},
    diff::{DiffHandle, DiffMarker, DiffProps, DiffRow, DiffView, HunkId},
    UiId, UiPlugin,
};

fn row(id: u64, hunk: u64, text: &str, marker: DiffMarker, height: f32) -> DiffRow {
    DiffRow {
        id: ItemId(id),
        hunk: HunkId(hunk),
        text: text.into(),
        marker,
        height,
    }
}

fn setup(rows: &[DiffRow], height: f32) -> (App, Entity, DiffHandle) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 400.));
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let handle = DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        rows,
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(height);
    rndr_paint::frame(&mut app);
    (app, parent, handle)
}

fn labels(world: &World, content: Entity) -> Vec<String> {
    children_of(world, content)
        .into_iter()
        .filter_map(|entity| world.get::<Label>(entity).map(|label| label.text.clone()))
        .collect()
}

fn content_labels(app: &App, handle: DiffHandle, id: u64) -> Vec<String> {
    let slot = VirtualViewHandle { root: handle.root }
        .row(app.world(), ItemId(id))
        .unwrap();
    labels(app.world(), slot.content)
}

#[test]
fn mixed_heights_keep_the_anchor_until_the_anchored_row_shrinks() {
    let rows = vec![
        row(1, 1, "short", DiffMarker::Unchanged, 8.),
        row(2, 2, "mid", DiffMarker::Added, 24.),
        row(3, 3, "tall", DiffMarker::Removed, 96.),
    ];
    let (mut app, parent, handle) = setup(&rows, 30.);
    assert_eq!(content_labels(&app, handle, 1), [" ", "short"]);
    assert_eq!(content_labels(&app, handle, 2), ["+", "mid"]);
    handle.jump_hunk(app.world_mut(), HunkId(3)).unwrap();
    assert_eq!(
        app.world()
            .get::<Rect>(
                VirtualViewHandle { root: handle.root }
                    .row(app.world(), ItemId(3))
                    .unwrap()
                    .root
            )
            .unwrap()
            .h,
        96.
    );
    assert_eq!(content_labels(&app, handle, 3), ["-", "tall"]);
    handle.jump_hunk(app.world_mut(), HunkId(2)).unwrap();
    VirtualViewHandle { root: handle.root }
        .jump(app.world_mut(), ItemId(2), 1_500)
        .unwrap();
    let mut taller = rows.clone();
    taller[0].height = 40.;
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &taller,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(2),
            remainder_mp: 1_500
        })
    );
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(2)));
    taller[1].height = 1.;
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &taller,
    )
    .unwrap();
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(2),
            remainder_mp: 1_000
        })
    );
}

#[test]
fn empty_diff_and_a_very_long_line_have_defined_motion() {
    let (mut app, parent, handle) = setup(&[], 120.);
    assert_eq!(handle.len(app.world()), 0);
    assert_eq!(handle.focused_hunk(app.world()), None);
    assert_eq!(handle.anchor(app.world()), None);
    handle.next_hunk(app.world_mut()).unwrap();
    handle.previous_hunk(app.world_mut()).unwrap();
    assert!(handle.jump_hunk(app.world_mut(), HunkId(1)).is_err());
    assert!(descendants(app.world(), handle.root).iter().any(|entity| {
        app.world()
            .get::<Label>(*entity)
            .is_some_and(|label| label.text == "No changes to display")
    }));

    let text = "x".repeat(4_096);
    let long = vec![row(1, 4, &text, DiffMarker::Header, 28.)];
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &long,
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(40.);
    rndr_paint::frame(&mut app);
    let slot = VirtualViewHandle { root: handle.root }
        .row(app.world(), ItemId(1))
        .unwrap();
    assert_eq!(app.world().get::<Rect>(slot.root).unwrap().h, 28.);
    let shown = labels(app.world(), slot.content);
    assert_eq!(shown, ["#", text.as_str()]);
    assert_eq!(
        app.world()
            .get::<Label>(children_of(app.world(), slot.content)[1])
            .unwrap()
            .overflow,
        TextOverflow::Visible
    );
    assert!(!has_component_gutter(app.world(), handle.root));
    handle.next_hunk(app.world_mut()).unwrap();
    handle.previous_hunk(app.world_mut()).unwrap();
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(4)));
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(1),
            remainder_mp: 0
        })
    );
}

#[test]
fn removing_the_focused_hunk_repairs_forward_then_backward() {
    let rows = vec![
        row(1, 1, "a", DiffMarker::Header, 16.),
        row(2, 2, "b", DiffMarker::Added, 16.),
        row(3, 2, "c", DiffMarker::Added, 32.),
        row(4, 3, "d", DiffMarker::Removed, 16.),
    ];
    let (mut app, parent, handle) = setup(&rows, 16.);
    handle.jump_hunk(app.world_mut(), HunkId(2)).unwrap();
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(2),
            remainder_mp: 0
        })
    );
    let without_middle: Vec<_> = rows
        .iter()
        .filter(|row| row.hunk != HunkId(2))
        .cloned()
        .collect();
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &without_middle,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(3)));
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(4),
            remainder_mp: 0
        })
    );
    let without_tail: Vec<_> = without_middle
        .iter()
        .filter(|row| row.hunk != HunkId(3))
        .cloned()
        .collect();
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &without_tail,
    )
    .unwrap();
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(1)));
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(1),
            remainder_mp: 0
        })
    );
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &[],
    )
    .unwrap();
    assert_eq!(handle.focused_hunk(app.world()), None);
    assert_eq!(handle.anchor(app.world()), None);
    assert_eq!(
        VirtualViewHandle { root: handle.root }.selected(app.world()),
        None
    );
}

#[test]
fn invalid_rows_refuse_before_mutation_and_motion_follows_hunks() {
    let rows = vec![
        row(1, 1, "one", DiffMarker::Unchanged, 20.),
        row(2, 2, "two", DiffMarker::Removed, 20.),
        row(3, 3, "three", DiffMarker::Added, 20.),
    ];
    let (mut app, parent, handle) = setup(&rows, 20.);
    handle.jump_hunk(app.world_mut(), HunkId(1)).unwrap();
    let saved = handle.anchor(app.world());
    let layouts = app.world().entities_with::<Layout>().len();
    for bad in [0., -1., f32::NAN, 0.0004] {
        let mut broken = rows.clone();
        broken[1].height = bad;
        assert!(DiffView::mount(
            app.world_mut(),
            parent,
            UiId(7),
            DiffProps::new("Changes"),
            &broken,
        )
        .is_err());
    }
    let mut blank = rows.clone();
    blank[0].text = "  ".into();
    assert!(DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &blank,
    )
    .is_err());
    let duplicate = vec![
        row(1, 1, "one", DiffMarker::Unchanged, 20.),
        row(1, 2, "again", DiffMarker::Added, 20.),
    ];
    assert!(DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &duplicate,
    )
    .is_err());
    assert_eq!(handle.anchor(app.world()), saved);
    assert_eq!(handle.len(app.world()), 3);
    assert_eq!(layouts, app.world().entities_with::<Layout>().len());
    handle.next_hunk(app.world_mut()).unwrap();
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(2)));
    handle.next_hunk(app.world_mut()).unwrap();
    handle.next_hunk(app.world_mut()).unwrap();
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(3)));
    handle.previous_hunk(app.world_mut()).unwrap();
    assert_eq!(
        handle.anchor(app.world()),
        Some(VariableAnchor {
            id: ItemId(2),
            remainder_mp: 0
        })
    );
    assert!(handle.jump_hunk(app.world_mut(), HunkId(9)).is_err());
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(2)));
}

fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = vec![root];
    for child in children_of(world, root) {
        out.extend(descendants(world, child));
    }
    out
}

fn has_component_gutter(world: &World, root: Entity) -> bool {
    descendants(world, root)
        .into_iter()
        .any(|entity| world.has::<Gutter>(entity))
}
