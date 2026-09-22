use rndr_core::{
    detail_pane, field_height, field_height_lines, set_editing_field, set_selection, spawn_node,
    wrap_lines, App, Body, Caret, Color, EditField, Field, Input, Key, Layout, NodeSpec, Pane,
    Size, Viewport, VisualLine, World,
};

fn ranges(text: &str, cols: usize) -> Vec<(usize, usize)> {
    wrap_lines(text, cols)
        .into_iter()
        .map(|line| (line.start, line.end))
        .collect()
}

fn slice<'a>(text: &'a str, line: VisualLine) -> &'a str {
    &text[line.start..line.end]
}

fn pane_field(world: &World, kind: EditField) -> rndr_core::Entity {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    for entity in world.get::<Pane>(pane).unwrap().spawned.iter().copied() {
        if world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == kind)
        {
            return entity;
        }
    }
    panic!("no pane field {kind:?}");
}

fn type_char(app: &mut App, ch: char) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push(ch);
    app.update();
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
}

fn boot_narrow() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport {
        width: 200.0,
        height: 400.0,
    });
    let node = {
        let world = app.world_mut();
        let (layout, scroll, pane) = detail_pane();
        world.spawn((layout, scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
        let node = spawn_node(world, NodeSpec::leaf("Alpha", Color::rgb(0.2, 0.3, 0.4)));
        set_selection(world, Some(node));
        set_editing_field(world, node, EditField::Body);
        node
    };
    app.update();
    app.update();
    (app, node)
}

#[test]
fn hard_break_splits_a_long_word() {
    assert_eq!(ranges("aaaaabbbbb", 5), vec![(0, 5), (5, 10)]);
}

#[test]
fn space_break_keeps_the_word() {
    let text = "hello world";
    let lines = wrap_lines(text, 8);
    assert_eq!(lines.len(), 2);
    assert_eq!(slice(text, lines[0]), "hello");
    assert_eq!(slice(text, lines[1]), "world");
}

#[test]
fn newline_stays_two_lines_on_a_wide_field() {
    let text = "a\nb";
    let lines = wrap_lines(text, 80);
    assert_eq!(lines.len(), 2);
    assert_eq!(slice(text, lines[0]), "a");
    assert_eq!(slice(text, lines[1]), "b");
    assert!(text.contains('\n'));
}

#[test]
fn narrow_body_wraps_without_inserting_newlines() {
    let (mut app, alpha) = boot_narrow();
    for _ in 0..20 {
        type_char(&mut app, 'a');
    }

    assert_eq!(app.world().get::<Body>(alpha).unwrap().text, "a".repeat(20));
    assert!(!app.world().get::<Body>(alpha).unwrap().text.contains('\n'));
    let body = pane_field(app.world(), EditField::Body);
    let height = app.world().get::<Layout>(body).unwrap().height;
    assert_ne!(height, Size::Px(field_height("aaaaaaaaaaaaaaaaaaaa")));
    assert_eq!(height, Size::Px(field_height_lines(3)));
}

#[test]
fn down_then_up_walks_visual_lines() {
    let (mut app, _alpha) = boot_narrow();
    for _ in 0..20 {
        type_char(&mut app, 'a');
    }
    let body = pane_field(app.world(), EditField::Body);
    if let Some(caret) = app.world_mut().get_mut::<Caret>(body) {
        caret.index = 0;
    }
    press(&mut app, Key::Down);
    assert_eq!(app.world().get::<Caret>(body).unwrap().index, 9);
    press(&mut app, Key::Up);
    assert_eq!(app.world().get::<Caret>(body).unwrap().index, 0);
}
