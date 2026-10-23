//! Scroll pin, link clicks, copy, and stable block entities.

use rndr_core::{
    children_of, events, platform_of, App, Clipboard, Entity, Input, KeyEvent, Label,
    LinkActivated, MarkdownPlugin, MarkdownView, Modifiers, Platform, PointerButton, Rect,
    TextLayout, TextSelection, Viewport, World, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
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
    assert_eq!(selected, "Alpha\n\nBeta");
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.text, selected);
    if matches!(platform, Platform::X11 | Platform::Wayland) {
        assert_eq!(clipboard.primary, selected);
    }
    assert!(app.world().entities_with::<TextSelection>().len() >= 2);
    assert!(app.world().entities_with::<rndr_core::Caret>().is_empty());
}

#[test]
fn block_height_tracks_the_width_in_the_same_update() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    let (mut app, view) = boot(text, 420.0, 320.0);
    let wide = block_height(app.world(), view);
    app.world_mut().insert_resource(Viewport::new(160.0, 320.0));
    app.update();
    let narrow = block_height(app.world(), view);
    assert!(
        narrow > wide + 8.0,
        "narrow {narrow} should grow past wide {wide}"
    );
    app.world_mut().insert_resource(Viewport::new(420.0, 320.0));
    app.update();
    let again = block_height(app.world(), view);
    assert!(
        (again - wide).abs() < 1.0,
        "restored {again} should match the first width {wide}"
    );
}

#[test]
fn press_in_the_left_padding_anchors_the_first_line() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let (mut app, view) = boot(text, 160.0, 280.0);
    let label = block_label(app.world(), view, 0);
    let rect = *app.world().get::<Rect>(label).unwrap();
    let layout = app.world().get::<TextLayout>(label).unwrap().clone();
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    let y = rect.y + TEXT_ORIGIN_Y + layout.lines[0].y + 1.0;
    let head = layout.lines[0].end.clamp(1, 5);
    let (x_head, _) = layout.caret_xy(head);
    point(&mut app, rect.x + 2.0, y, true, false, true);
    app.update();
    point(
        &mut app,
        rect.x + TEXT_ORIGIN_X + x_head,
        y,
        false,
        false,
        true,
    );
    app.update();
    point(
        &mut app,
        rect.x + TEXT_ORIGIN_X + x_head,
        y,
        false,
        true,
        false,
    );
    app.update();
    let selected = app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .selected_text();
    assert!(
        selected.starts_with("alpha"),
        "copied {selected:?} dropped the first line"
    );
    let selection = app.world().get::<TextSelection>(label).unwrap();
    assert_eq!(selection.anchor.min(selection.head), 0);
    if matches!(platform_of(app.world()), Platform::X11 | Platform::Wayland) {
        assert_eq!(
            app.world().resource::<Clipboard>().unwrap().primary,
            selected
        );
    }
}

#[test]
fn select_all_copies_markdown_structure() {
    let source = "# Title\n\nHello.\n\n- a\n  - b\n\n> quote\n\n---\n\n```\nlet x = 1;\n```\n";
    let (mut app, view) = boot(source, 420.0, 640.0);
    let rect = *app.world().get::<Rect>(view).unwrap();
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
    let copied = app.world().resource::<Clipboard>().unwrap().text.clone();
    assert_eq!(
        copied,
        "# Title\n\nHello.\n\n- a\n  - b\n\n> quote\n\n---\n\n```\nlet x = 1;\n```"
    );
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

fn block_height(world: &World, view: Entity) -> f32 {
    world.get::<Rect>(block_label(world, view, 0)).unwrap().h
}

#[test]
fn wrapped_list_and_quote_press_anchors_the_line_under_the_pointer() {
    let item = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron";
    let source = format!("1. {item}\n\n> {item}\n");
    let (mut app, view) = boot(&source, 220.0, 640.0);
    let list = block_label(app.world(), view, 0);
    assert_line_press(&mut app, view, list, 22.0);
    let quote = block_label(app.world(), view, 1);
    assert_line_press(&mut app, view, quote, 24.0);
}

#[test]
fn wrapped_paragraph_press_still_anchors_the_line_under_the_pointer() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron";
    let (mut app, view) = boot(text, 220.0, 320.0);
    let label = block_label(app.world(), view, 0);
    assert_line_press(&mut app, view, label, 22.0);
}

#[test]
fn gutter_press_selects_the_nearest_line_and_does_not_scroll() {
    let (mut app, view) = boot(&long_answer(), 360.0, 140.0);
    let rect = *app.world().get::<Rect>(view).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
        input.scroll = [0.0, 100_000.0];
    }
    app.update();
    assert!(!app.world().get::<MarkdownView>(view).unwrap().pinned());
    let parked = app.world().get::<rndr_core::Scroll>(view).unwrap().offset[1];
    assert!(parked.abs() < 1.5, "offset {parked} should be the top");

    let label = block_label(app.world(), view, 0);
    let label_rect = *app.world().get::<Rect>(label).unwrap();
    let layout = app.world().get::<TextLayout>(label).unwrap().clone();
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    let y = label_rect.y + TEXT_ORIGIN_Y + layout.lines[1].y + 1.0;
    let start = layout.lines[1].start;
    assert_ne!(start, 0);

    point(&mut app, rect.x + 4.0, y, true, false, true);
    app.update();
    point(&mut app, rect.x + 40.0, y, false, false, true);
    app.update();
    let selection = app.world().get::<TextSelection>(label).unwrap();
    assert_eq!(selection.anchor.min(selection.head), start);
    point(&mut app, rect.x + 40.0, y, false, true, false);
    app.update();

    point(&mut app, rect.x + 4.0, y, true, false, true);
    app.update();
    point(&mut app, rect.x + 4.0, y - 36.0, false, false, true);
    app.update();
    let offset = app.world().get::<rndr_core::Scroll>(view).unwrap().offset[1];
    assert!(
        (offset - parked).abs() < 1.0,
        "gutter drag scrolled from {parked} to {offset}"
    );
}

#[test]
fn list_markers_share_a_column_and_checks_read_apart() {
    let source = "\
9. nine
10. ten
- bullet item
- [x] done task
- [ ] open task
8. parent item
   1. nested one
   2. nested two
";
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut()
        .insert_resource(rndr_core::MarkdownTheme::default());
    app.world_mut().insert_resource(Viewport::new(640.0, 720.0));
    let view = rndr_core::markdown_view(app.world_mut(), source);
    app.update();

    let nine = label_rect(app.world(), "9.");
    let ten = label_rect(app.world(), "10.");
    let nine_ink = nine.x + nine.w - TEXT_ORIGIN_X;
    let ten_ink = ten.x + ten.w - TEXT_ORIGIN_X;
    assert!(
        (nine_ink - ten_ink).abs() < 0.5,
        "number ink {nine_ink} vs {ten_ink}"
    );

    let content = [
        "nine",
        "ten",
        "bullet item",
        "done task",
        "open task",
        "parent item",
    ];
    let xs: Vec<f32> = content
        .iter()
        .map(|text| label_rect(app.world(), text).x)
        .collect();
    for x in &xs {
        assert!((x - xs[0]).abs() < 0.5, "content x {x} vs {}", xs[0]);
    }
    let nested_one = label_rect(app.world(), "nested one").x;
    let nested_two = label_rect(app.world(), "nested two").x;
    assert!((nested_one - nested_two).abs() < 0.5);
    assert!(nested_one > xs[0] + 8.0);

    let theme = rndr_core::MarkdownTheme::default();
    let mut holes: Vec<(f32, rndr_core::Color)> = app
        .world()
        .entities_with::<rndr_core::Color>()
        .into_iter()
        .filter_map(|entity| {
            let rect = app.world().get::<Rect>(entity).copied()?;
            if (rect.w - 10.0).abs() > 0.5 || (rect.h - 10.0).abs() > 0.5 {
                return None;
            }
            Some((
                rect.y,
                app.world().get::<rndr_core::Color>(entity).copied()?,
            ))
        })
        .collect();
    holes.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(holes.len(), 2, "holes {holes:?}");
    assert_eq!(holes[0].1, theme.text);
    assert_eq!(holes[1].1, theme.surface);
    let _ = view;
}

fn assert_line_press(app: &mut App, view: Entity, label: Entity, x: f32) {
    let rect = *app.world().get::<Rect>(view).unwrap();
    let layout = app.world().get::<TextLayout>(label).unwrap().clone();
    let label_rect = *app.world().get::<Rect>(label).unwrap();
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    let start = layout.lines[1].start;
    assert_ne!(start, 0);
    let y = label_rect.y + TEXT_ORIGIN_Y + layout.lines[1].y + 1.0;
    let into = (start + 4).min(layout.lines[1].end.max(start));
    let (x_head, _) = layout.caret_xy(into);
    point(app, rect.x + x, y, true, false, true);
    app.update();
    point(
        app,
        label_rect.x + TEXT_ORIGIN_X + x_head,
        y,
        false,
        false,
        true,
    );
    app.update();
    let selection = app.world().get::<TextSelection>(label).unwrap();
    assert_eq!(
        selection.anchor.min(selection.head),
        start,
        "anchor landed on byte {} of {:?}",
        selection.anchor.min(selection.head),
        app.world().get::<Label>(label).unwrap().text
    );
    point(
        app,
        label_rect.x + TEXT_ORIGIN_X + x_head,
        y,
        false,
        true,
        false,
    );
    app.update();
}

fn label_rect(world: &World, text: &str) -> Rect {
    let entity = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text == text)
        })
        .unwrap_or_else(|| panic!("missing label {text}"));
    *world.get::<Rect>(entity).unwrap()
}

fn block_label(world: &World, view: Entity, index: usize) -> Entity {
    let text = world.get::<MarkdownView>(view).unwrap().blocks()[index]
        .text
        .clone();
    let root = children_of(world, view)[index];
    if world
        .get::<Label>(root)
        .is_some_and(|label| label.text == text)
    {
        return root;
    }
    children_of(world, root)
        .into_iter()
        .find(|child| {
            world
                .get::<Label>(*child)
                .is_some_and(|label| label.text == text)
        })
        .expect("block label")
}
