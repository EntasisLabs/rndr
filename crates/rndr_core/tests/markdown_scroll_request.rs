//! Public requests work without a realized heading entity.
use rndr_core::*;

fn boot() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(640., 400.));
    let source = (0..200)
        .map(|i| format!("## Heading {i}\n\nParagraph {i} text with a modest height.\n\n"))
        .collect::<String>();
    let root = markdown_view(app.world_mut(), source);
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    view.set_follow(false);
    view.set_recycle_host_slots(true);
    app.update();
    app.update();
    (app, root)
}

#[test]
fn offscreen_block_request_realizes_target_preserves_selection_and_disables_follow() {
    let (mut app, root) = boot();
    assert!(!markdown_realized_blocks(app.world(), root).contains(&300));
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    view.select_all();
    let selected = view.selected_text();
    view.set_follow(true);
    assert!(view.scroll_to_block(300));
    app.update();
    app.update();
    let view = app.world().get::<MarkdownView>(root).unwrap();
    assert_eq!(view.selected_text(), selected);
    assert!(!view.follow());
    assert!(markdown_realized_blocks(app.world(), root).contains(&300));
    let target = app
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|e| app.world().get::<Label>(*e).unwrap().text == "Heading 150")
        .expect("offscreen heading realized");
    let rect = app.world().get::<Rect>(target).unwrap();
    assert!(rect.y.abs() < 60., "target at viewport top: {rect:?}");
    assert!(app.world().get::<Scroll>(root).unwrap().offset[1] < -1000.);
    for _ in 0..3 {
        app.update();
    }
    let rect = app.world().get::<Rect>(target).unwrap();
    assert!(rect.y.abs() < 60., "stable without bottom follow: {rect:?}");
}

#[test]
fn invalid_dirty_replacement_and_latest_requests_are_bounded() {
    let (mut app, root) = boot();
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    assert!(!view.scroll_to_block(usize::MAX));
    assert!(view.scroll_to_block(100));
    assert!(view.scroll_to_block(200));
    app.update();
    assert!(markdown_realized_blocks(app.world(), root).contains(&200));
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    assert!(view.scroll_to_block(350));
    view.set_markdown("# Replacement\n\nNew body\n");
    assert!(!view.scroll_to_block(0), "wait for source sync");
    app.update();
    app.update();
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    assert!(!view.scroll_to_block(350));
    assert!(view.scroll_to_block(0));
    app.update();
    assert!(app.world().get::<Scroll>(root).unwrap().offset[1].abs() < 1.);
    let view = app.world_mut().get_mut::<MarkdownView>(root).unwrap();
    view.push_str("\n\nTail");
    assert!(!view.scroll_to_block(0));
}
