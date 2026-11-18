//! Host-supplied diff presentation. Headless only; not a native certificate.
use rndr_core::*;
use rndr_ui::{
    collections::{ItemId, VariableAnchor, VirtualViewHandle},
    diff::{DiffAnnotation, DiffHandle, DiffMarker, DiffProps, DiffRow, DiffView, HunkId},
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
        .filter(|entity| !world.has::<Hidden>(*entity))
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
    assert_eq!(shown, ["…", text.as_str()]);
    assert_eq!(
        app.world()
            .get::<Label>(*children_of(app.world(), slot.content).last().unwrap())
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

#[test]
fn numbered_code_has_compact_metrics_full_width_fills_and_exact_spans() {
    let rows = vec![
        row(1, 1, "@@ context @@", DiffMarker::Header, 20.),
        row(2, 1, "    let café = 42;", DiffMarker::Removed, 20.),
        row(3, 1, "    let café = 43;", DiffMarker::Added, 20.),
        row(4, 1, "", DiffMarker::Unchanged, 20.),
    ];
    let (mut app, parent, handle) = setup(&rows, 120.);
    let spans = vec![TextSpan::new(4, 7, Color::rgb(1., 0.4, 0.8))];
    let annotations = vec![
        DiffAnnotation {
            id: ItemId(2),
            old_line: Some(999),
            new_line: None,
            spans: spans.clone(),
        },
        DiffAnnotation {
            id: ItemId(3),
            old_line: None,
            new_line: Some(1000),
            spans: spans.clone(),
        },
    ];
    DiffView::mount_annotated(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Code"),
        &rows,
        &annotations,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(
        content_labels(&app, handle, 2),
        ["999", "", "-", "    let café = 42;"]
    );
    assert_eq!(
        content_labels(&app, handle, 3),
        ["", "1000", "+", "    let café = 43;"]
    );
    let slot = (VirtualViewHandle { root: handle.root })
        .row(app.world(), ItemId(3))
        .unwrap();
    let text = *children_of(app.world(), slot.content).last().unwrap();
    let style = app.world().get::<TextStyle>(text).unwrap();
    assert_eq!(
        style.family,
        app.world()
            .resource::<rndr_ui::UiTheme>()
            .unwrap()
            .code_family
    );
    assert_eq!(app.world().get::<LabelBox>(text).unwrap().inset_y, 0.);
    assert_eq!(app.world().get::<Spans>(text).unwrap().runs, spans);
    let rect = app.world().get::<Rect>(text).unwrap();
    let shaped = app.world().get::<TextLayout>(text).unwrap();
    assert!(
        shaped.height <= rect.h,
        "20-point rows must not crop text: {shaped:?} {rect:?}"
    );
    let fill = app.world().get::<Color>(slot.root).unwrap();
    assert_eq!(
        *fill,
        app.world()
            .resource::<rndr_ui::UiTheme>()
            .unwrap()
            .color(rndr_ui::Token::SuccessFill)
    );
    assert!(
        app.world().get::<Rect>(slot.root).unwrap().w
            >= app.world().get::<Rect>(handle.root).unwrap().w
    );
    assert_eq!(content_labels(&app, handle, 4).last().unwrap(), "");
}

#[test]
fn longest_unrealized_line_sets_scroll_extent_and_short_replacement_clamps_it() {
    let mut rows: Vec<_> = (1..=120)
        .map(|id| row(id, id, "short", DiffMarker::Unchanged, 20.))
        .collect();
    rows[119].text = format!("{} END", "long_".repeat(100));
    let (mut app, parent, handle) = setup(&rows, 100.);
    let before = *app.world().get::<Scroll>(handle.root).unwrap();
    assert!(
        before.content[0] > 2000.,
        "unrealized line must set width {before:?}"
    );
    assert!((VirtualViewHandle { root: handle.root })
        .row(app.world(), ItemId(120))
        .is_none());
    app.world_mut()
        .get_mut::<Scroll>(handle.root)
        .unwrap()
        .offset[0] = -500.;
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().get::<Scroll>(handle.root).unwrap().offset[0],
        -500.
    );
    handle.jump_hunk(app.world_mut(), HunkId(120)).unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().get::<Scroll>(handle.root).unwrap().offset[0],
        -500.
    );
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(120)));
    rows[119].text = "short".into();
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Changes"),
        &rows,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().get::<Scroll>(handle.root).unwrap().offset[0],
        0.
    );
    assert_eq!(handle.focused_hunk(app.world()), Some(HunkId(120)));
    assert!(
        app.world().entities_with::<Label>().len() < 100,
        "rows remain virtualized"
    );
}

#[test]
fn invalid_annotations_refuse_before_publication_and_remount_clears_styled_runs() {
    let rows = vec![row(1, 1, "café", DiffMarker::Added, 20.)];
    let (mut app, parent, handle) = setup(&rows, 80.);
    let valid = DiffAnnotation {
        id: ItemId(1),
        old_line: None,
        new_line: Some(4),
        spans: vec![TextSpan::new(0, 3, Color::rgb(1., 0., 0.))],
    };
    let mut unknown = valid.clone();
    unknown.id = ItemId(2);
    let mut split_utf8 = valid.clone();
    split_utf8.spans[0].end = 4;
    let mut beyond = valid.clone();
    beyond.spans[0].end = 20;
    let anchor = handle.anchor(app.world());
    let count = app.world().entities_with::<Layout>().len();
    for invalid in [
        vec![unknown],
        vec![split_utf8],
        vec![beyond],
        vec![valid.clone(), valid.clone()],
    ] {
        assert!(DiffView::mount_annotated(
            app.world_mut(),
            parent,
            UiId(7),
            DiffProps::new("Code"),
            &rows,
            &invalid
        )
        .is_err());
        assert_eq!(handle.anchor(app.world()), anchor);
        assert_eq!(app.world().entities_with::<Layout>().len(), count);
    }
    DiffView::mount_annotated(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Code"),
        &rows,
        &[valid],
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    DiffView::mount(
        app.world_mut(),
        parent,
        UiId(7),
        DiffProps::new("Code"),
        &rows,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(content_labels(&app, handle, 1), ["+", "café"]);
    let slot = (VirtualViewHandle { root: handle.root })
        .row(app.world(), ItemId(1))
        .unwrap();
    let text = *children_of(app.world(), slot.content).last().unwrap();
    assert!(app.world().get::<Spans>(text).unwrap().runs.is_empty());
}

#[test]
fn code_scale_and_viewport_resize_refresh_width_without_resetting_anchor() {
    let rows: Vec<_> = (1..=30)
        .map(|id| row(id, id, &"word ".repeat(30), DiffMarker::Unchanged, 30.))
        .collect();
    let (mut app, _, handle) = setup(&rows, 100.);
    handle.jump_hunk(app.world_mut(), HunkId(12)).unwrap();
    rndr_paint::frame(&mut app);
    let anchor = handle.anchor(app.world());
    let width = app.world().get::<Scroll>(handle.root).unwrap().content[0];
    app.world_mut()
        .resource_mut::<rndr_ui::UiTheme>()
        .unwrap()
        .reading_scale = 1.5;
    rndr_paint::frame(&mut app);
    assert!(app.world().get::<Scroll>(handle.root).unwrap().content[0] > width * 1.4);
    assert_eq!(handle.anchor(app.world()), anchor);
    app.world_mut().resource_mut::<Viewport>().unwrap().width = 240.;
    rndr_paint::frame(&mut app);
    assert_eq!(handle.anchor(app.world()), anchor);
}
