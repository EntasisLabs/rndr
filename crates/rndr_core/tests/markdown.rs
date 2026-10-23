//! Scroll pin, link clicks, copy, and stable block entities.

use rndr_core::{
    children_of, events, platform_of, App, Caret, Clipboard, Entity, Input, KeyEvent, Label,
    LinkActivated, MarkdownPlugin, MarkdownView, Modifiers, PointerButton, TextLayout, Viewport,
    World, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn boot(source: &str, width: f32, height: f32) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    let view = rndr_core::markdown_view(app.world_mut(), source);
    app.update();
    (app, view)
}

fn long_answer() -> String {
    let mut text = String::new();
    for index in 0..24 {
        text.push_str(&format!(
            "Paragraph {index} fills a line of the answer.\n\n"
        ));
    }
    text
}

fn at_bottom(world: &World, view: Entity) -> bool {
    let scroll = world.get::<rndr_core::Scroll>(view).copied().unwrap();
    let rect = world.get::<rndr_core::Rect>(view).copied().unwrap();
    let min_y = (rect.h - scroll.content[1]).min(0.0);
    (scroll.offset[1] - min_y).abs() <= 1.5
}

#[test]
fn stream_stays_pinned_until_the_wheel_leaves_the_bottom() {
    let (mut app, view) = boot(&long_answer(), 360.0, 140.0);
    let scroll = app.world().get::<rndr_core::Scroll>(view).copied().unwrap();
    let rect = app.world().get::<rndr_core::Rect>(view).copied().unwrap();
    assert!(scroll.content[1] > rect.h + 8.0);
    assert!(at_bottom(app.world(), view));
    assert!(app.world().get::<MarkdownView>(view).unwrap().pinned());

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [rect.x + 8.0, rect.y + 8.0];
        input.scroll = [0.0, 400.0];
    }
    app.update();
    assert!(!app.world().get::<MarkdownView>(view).unwrap().pinned());
    assert!(!at_bottom(app.world(), view));
    let parked = app.world().get::<rndr_core::Scroll>(view).unwrap().offset[1];

    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .push_str("\n\nA tail that must not yank the offset.\n");
    app.update();
    let offset = app.world().get::<rndr_core::Scroll>(view).unwrap().offset[1];
    assert!((offset - parked).abs() < 1.0);
    assert!(!app.world().get::<MarkdownView>(view).unwrap().pinned());

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [rect.x + 8.0, rect.y + 8.0];
        input.scroll = [0.0, -100_000.0];
    }
    app.update();
    assert!(app.world().get::<MarkdownView>(view).unwrap().pinned());
    assert!(at_bottom(app.world(), view));
}

#[test]
fn link_click_sends_the_destination() {
    let (mut app, view) = boot("See [the docs](https://example.com).", 420.0, 200.0);
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    let link = block.links[0].clone();
    assert_eq!(link.href, "https://example.com");
    let label = block_label(app.world(), view, 0);
    let rect = *app.world().get::<rndr_core::Rect>(label).unwrap();
    let layout = app.world().get::<TextLayout>(label).unwrap();
    let (x0, y) = layout.caret_xy(link.start);
    let (x1, _) = layout.caret_xy(link.end);
    let x = rect.x + TEXT_ORIGIN_X + (x0 + x1) * 0.5;
    let y = rect.y + TEXT_ORIGIN_Y + y + 2.0;

    point(&mut app, x, y, true, false, true);
    app.update();
    point(&mut app, x, y, false, true, false);
    app.update();

    let sent = events::<LinkActivated>(app.world());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].view, view);
    assert_eq!(sent[0].href, "https://example.com");
}

#[test]
fn select_all_and_copy_write_the_clipboard() {
    let (mut app, view) = boot("Alpha\n\nBeta", 420.0, 240.0);
    let rect = *app.world().get::<rndr_core::Rect>(view).unwrap();
    point(&mut app, rect.x + 4.0, rect.y + 4.0, true, false, true);
    app.update();
    point(&mut app, rect.x + 4.0, rect.y + 4.0, false, true, false);
    app.update();

    let platform = platform_of(app.world());
    let mut mods = Modifiers::new();
    if platform.primary_is_super() {
        mods.super_key = true;
    } else {
        mods.ctrl = true;
    }
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.keys.clear();
        input.key = None;
        input.push_key(KeyEvent::character('a', mods), platform);
        input.push_key(KeyEvent::character('c', mods), platform);
    }
    app.update();

    let selected = app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .selected_text();
    assert_eq!(selected, "Alpha\nBeta");
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, selected);
    assert!(!app.world().entities_with::<Caret>().is_empty());
}

#[test]
fn append_keeps_the_prefix_entity_and_parses_the_tail() {
    let (mut app, view) = boot("Stable paragraph stays.\n\n\nTail", 420.0, 320.0);
    assert!(
        app.world()
            .get::<MarkdownView>(view)
            .unwrap()
            .stable_prefix()
            > 0
    );
    let first = children_of(app.world(), view)[0];
    let before = app.world().get::<Label>(first).unwrap().text.clone();
    assert_eq!(before, "Stable paragraph stays.");

    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .push_str(" end");
    app.update();

    let doc = app.world().get::<MarkdownView>(view).unwrap();
    assert!(doc.parsed_from() > 0);
    assert_eq!(doc.blocks()[0].text, before);
    assert!(doc.source().ends_with("Tail end"));
    assert_eq!(children_of(app.world(), view)[0], first);
    assert_eq!(app.world().get::<Label>(first).unwrap().text, before);
}

fn point(app: &mut App, x: f32, y: f32, pressed: bool, released: bool, down: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [x, y];
    input.pointer.pressed = pressed;
    input.pointer.released = released;
    input.pointer.down = down;
    input.pointer.button = PointerButton::Left;
    input.scroll = [0.0, 0.0];
}

fn block_label(world: &World, view: Entity, index: usize) -> Entity {
    let root = children_of(world, view)[index];
    if world.has::<Label>(root) {
        return root;
    }
    children_of(world, root)
        .into_iter()
        .find(|child| world.has::<Label>(*child))
        .expect("block label")
}
