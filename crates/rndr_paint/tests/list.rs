//! Selected and unselected rows, with the text the row builder supplied.

use rndr_core::{
    App, Axis, Chrome, Color, Context, Entity, Focus, Label, Layout, List, ListItem, ListKeys,
    ListRows, Size, Spans, TextSpan, Viewport, World,
};
use rndr_paint::{frame, quads_snapshot};

fn build(world: &mut rndr_core::World, row: rndr_core::Entity, index: u32) {
    let selected = world.get::<ListItem>(row).is_some_and(|item| item.selected);
    world.insert(
        row,
        if selected {
            Color::rgb(0.24, 0.40, 0.62)
        } else {
            Color::rgb(0.12, 0.13, 0.15)
        },
    );
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text = if index == 0 { "A" } else { "B" }.to_string();
    }
}

#[test]
fn selected_row_paints_the_builder_text() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 80.0));
    app.world_mut().insert_resource(Focus::default());
    let root = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Context::new("Palette"),
    ));
    let list = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.1, 0.1, 0.12),
        List::new(2, 20.0),
        ListKeys,
        ListRows(build),
    ));
    rndr_core::set_parent(app.world_mut(), list, root);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(root);
    // The first frame places the list. The next one pools the rows.
    frame(&mut app);
    let painted = frame(&mut app);
    insta::assert_snapshot!(quads_snapshot(&painted));
}

/// Palette-style row: the shortcut is a dim label color, and the matched
/// letters carry a highlight. The list pool's labels are measured by
/// `TextEngine` in `apply_text_layout`.
fn styled_build(world: &mut World, row: Entity, _index: u32) {
    world.insert(row, Color::rgb(0.12, 0.13, 0.15));
    if let Some(layout) = world.get_mut::<Layout>(row) {
        layout.axis = Axis::Row;
        layout.gap = 12.0;
    }
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text.clear();
    }
    let mut labels: Vec<Entity> = rndr_core::children_of(world, row)
        .into_iter()
        .filter(|child| world.has::<Label>(*child))
        .collect();
    while labels.len() < 2 {
        let width = if labels.is_empty() {
            Size::Fill
        } else {
            Size::Hug
        };
        let child = world.spawn((
            Layout {
                width,
                height: Size::Fill,
                ..Layout::row()
            },
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            Label::new(""),
        ));
        rndr_core::set_parent(world, child, row);
        labels.push(child);
    }
    let dim = Color::rgb(0.62, 0.66, 0.74);
    let text = Color::rgb(0.94, 0.95, 0.97);
    let mark = Color::rgba(0.95, 0.78, 0.25, 0.70);
    if let Some(label) = world.get_mut::<Label>(labels[0]) {
        label.text = "Search memory".to_string();
        label.color = None;
    }
    if let Some(label) = world.get_mut::<Label>(labels[1]) {
        label.text = "Ctrl+K".to_string();
        label.color = Some(dim);
    }
    world.insert(
        labels[0],
        Spans {
            runs: vec![
                TextSpan::new(0, 1, text).highlight(mark),
                TextSpan::new(7, 8, text).highlight(mark),
            ],
        },
    );
}

#[test]
fn styled_row_dims_the_shortcut_and_highlights_matches() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(480.0, 40.0));
    app.world_mut().insert_resource(Focus::default());
    let root = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(480.0),
            height: Size::Px(32.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Context::new("Palette"),
    ));
    let list = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.1, 0.1, 0.12),
        List::new(1, 32.0),
        ListKeys,
        ListRows(styled_build),
    ));
    rndr_core::set_parent(app.world_mut(), list, root);
    frame(&mut app);
    let painted = frame(&mut app);
    let (title_width, runs) = {
        let world = app.world();
        let title = world
            .query::<Label>()
            .into_iter()
            .find(|(_, label)| label.text == "Search memory")
            .map(|(entity, _)| entity)
            .unwrap();
        let width = world.get::<rndr_core::TextLayout>(title).unwrap().width;
        let runs = world
            .get::<Spans>(title)
            .map(|spans| spans.runs.clone())
            .unwrap_or_default();
        (width, runs)
    };
    let style = rndr_core::style_of(app.world());
    let measured =
        rndr_core::text_engine(app.world()).measure("Search memory", &style, &runs, f32::INFINITY);
    assert!((title_width - measured.width).abs() < 0.01);
    insta::assert_snapshot!(quads_snapshot(&painted));

    let dim = Color::rgb(0.62, 0.66, 0.74);
    let mark = Color::rgba(0.95, 0.78, 0.25, 0.70);
    let glyphs: Vec<_> = painted
        .quads
        .iter()
        .filter(|quad| quad.uv[2] > quad.uv[0] + 0.01)
        .collect();
    assert!(glyphs.len() > 7, "title plus shortcut glyphs");
    assert!(glyphs.iter().any(|quad| quad.color == dim));
    assert_eq!(
        painted
            .quads
            .iter()
            .filter(|quad| quad.color == mark)
            .count(),
        2
    );
}
