//! Headless editing. No platform IME and no screen reader.

use rndr_core::*;
use rndr_markdown::Session;
use rndr_markdown_ui::*;
use rndr_ui::UiId;

const SAMPLE: &str = "\
# Hello

Type *here*.

- one
- two

> quoted

```
code()
```

```callout
tone: tip
# keep
body: \"Hi\"
```
";

fn boot(src: &str) -> (App, MarkdownSurfaceHandle, MarkdownId) {
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    let id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(src);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(640.0),
        height: Size::Px(800.0),
        ..Layout::column()
    },));
    let handle = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(3),
        MarkdownSurfaceProps::new(id, "Notes"),
    )
    .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    app.update();
    (app, handle, id)
}

fn bytes(app: &App, id: MarkdownId) -> String {
    app.world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .bytes()
        .to_string()
}

#[test]
fn typing_enter_and_undo_round_trip() {
    let (mut app, handle, id) = boot("Hello\n");
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    let mut input = Input::default();
    input.push_key(
        KeyEvent::character('!', Modifiers::default()),
        Platform::X11,
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.update();
    assert!(
        bytes(&app, id).starts_with('!') || bytes(&app, id).contains('!'),
        "{}",
        bytes(&app, id)
    );
    handle
        .command(app.world_mut(), MarkdownCommand::Undo)
        .unwrap();
    app.update();
    assert_eq!(bytes(&app, id), "Hello\n");
}

#[test]
fn emphasis_strong_code_and_link() {
    let (mut app, handle, id) = boot("Hello\n");
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    session.move_caret(0, 5).unwrap();
    handle
        .command(app.world_mut(), MarkdownCommand::Emphasis)
        .unwrap();
    assert_eq!(bytes(&app, id), "*Hello*\n");
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    session.move_caret(1, 6).unwrap();
    handle
        .command(app.world_mut(), MarkdownCommand::Strong)
        .unwrap();
    assert!(bytes(&app, id).contains("**"));
    handle
        .command(app.world_mut(), MarkdownCommand::Undo)
        .unwrap();
    handle
        .command(app.world_mut(), MarkdownCommand::Code)
        .unwrap();
    assert!(bytes(&app, id).contains('`'), "{}", bytes(&app, id));
    handle
        .command(app.world_mut(), MarkdownCommand::Undo)
        .unwrap();
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    session.move_caret(1, 6).unwrap();
    handle
        .command(
            app.world_mut(),
            MarkdownCommand::Link("https://example.com".into()),
        )
        .unwrap();
    assert!(
        bytes(&app, id).contains("](https://example.com)"),
        "{}",
        bytes(&app, id)
    );
}

#[test]
fn heading_list_quote_and_fence() {
    let (mut app, handle, id) = boot("Hello\n");
    handle
        .command(app.world_mut(), MarkdownCommand::Heading(2))
        .unwrap();
    assert!(bytes(&app, id).starts_with("## "), "{}", bytes(&app, id));
    handle
        .command(app.world_mut(), MarkdownCommand::Bullet)
        .unwrap();
    assert!(bytes(&app, id).contains("- "), "{}", bytes(&app, id));
    handle
        .command(app.world_mut(), MarkdownCommand::Quote)
        .unwrap();
    assert!(bytes(&app, id).contains("> "), "{}", bytes(&app, id));
    handle
        .command(app.world_mut(), MarkdownCommand::CodeFence)
        .unwrap();
    assert!(bytes(&app, id).contains("```"), "{}", bytes(&app, id));
}

#[test]
fn liquid_field_edit_keeps_the_key_and_comment() {
    let (mut app, handle, id) = boot(SAMPLE);
    let session = app
        .world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    let src = session.bytes().to_string();
    let start = src.find("Hi").unwrap();
    handle
        .command(
            app.world_mut(),
            MarkdownCommand::SetField {
                start,
                end: start + 2,
                value: "There".into(),
            },
        )
        .unwrap();
    let next = bytes(&app, id);
    assert!(next.contains("body: \"There\""), "{next}");
    assert!(next.contains("# keep"), "{next}");
    assert!(next.contains("tone: tip"), "{next}");
}

/// Primary modifier for this process: Command on macOS, Control elsewhere.
fn primary_modifiers() -> Modifiers {
    if Platform::host().primary_is_super() {
        Modifiers {
            super_key: true,
            ..Modifiers::default()
        }
    } else {
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }
    }
}

#[test]
fn copy_and_paste_markdown() {
    let (mut app, handle, id) = boot("Hello\n");
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    session.move_caret(0, 5).unwrap();
    let mut input = Input::default();
    input.push_key(
        KeyEvent::character('c', primary_modifiers()),
        Platform::host(),
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    let copied = app.world().resource::<Clipboard>().unwrap().text.clone();
    assert_eq!(copied, "Hello");
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    session.move_caret(5, 5).unwrap();
    app.world_mut().resource_mut::<Clipboard>().unwrap().text = "**bold**".into();
    let mut input = Input::default();
    input.push_key(
        KeyEvent::character('v', primary_modifiers()),
        Platform::host(),
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    assert!(bytes(&app, id).contains("**bold**"), "{}", bytes(&app, id));
}

#[test]
fn preedit_stays_out_of_the_bytes_until_commit() {
    let (mut app, handle, id) = boot("Hello\n");
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "é".into();
    app.world_mut().resource_mut::<Input>().unwrap().ime_enabled = true;
    app.update();
    assert_eq!(bytes(&app, id), "Hello\n");
    assert_eq!(
        app.world()
            .resource::<MarkdownDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .preedit(),
        Some("é")
    );
    assert!(app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_mode(rndr_markdown::ViewMode::Raw)
        .is_err());
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.world_mut().resource_mut::<Input>().unwrap().ime_commit = "é".into();
    app.update();
    assert!(bytes(&app, id).contains('é'), "{}", bytes(&app, id));
    assert!(app
        .world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .preedit()
        .is_none());
}

#[test]
fn access_tree_names_blocks_and_refuses_a_stale_selection() {
    let (mut app, handle, id) = boot(SAMPLE);
    app.update();
    let tree = access_tree(app.world());
    let roles: Vec<_> = tree.nodes.iter().map(|node| node.role).collect();
    assert!(roles.contains(&AccessRole::Heading), "{roles:?}");
    assert!(roles.contains(&AccessRole::List), "{roles:?}");
    assert!(roles.contains(&AccessRole::ListItem), "{roles:?}");
    assert!(roles.contains(&AccessRole::Paragraph), "{roles:?}");
    assert!(roles.contains(&AccessRole::Code), "{roles:?}");
    let quote = tree.nodes.iter().find(|node| node.name == "Block quote");
    assert!(
        quote.is_some(),
        "{:?}",
        tree.nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
    );
    let liquid = tree.nodes.iter().find(|node| node.name == "callout");
    assert!(liquid.is_some());
    let heading = tree
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::Heading)
        .unwrap();
    assert_eq!(heading.level, Some(1));
    let root_name = tree.nodes.iter().find(|node| node.name == "Notes");
    assert!(root_name.is_some());
    let snapshot = access_snapshot(app.world(), handle.root).unwrap();
    assert_eq!(snapshot.document, id);
    handle
        .command(app.world_mut(), MarkdownCommand::Heading(3))
        .unwrap();
    assert_eq!(
        set_access_selection(app.world_mut(), handle.root, &snapshot, 0, 1),
        AccessCommandResult::Rejected
    );
    assert_eq!(
        try_apply_access(
            app.world_mut(),
            AccessCommand::SetValue {
                entity: handle.root,
                value: "no".into(),
            }
        ),
        AccessCommandResult::UnsupportedSetValue
    );
    let fresh = access_snapshot(app.world(), handle.root).unwrap();
    assert_eq!(
        set_access_selection(app.world_mut(), handle.root, &fresh, 0, 0),
        AccessCommandResult::Applied
    );
}

#[test]
fn backspace_joins_paragraphs() {
    let (mut app, handle, id) = boot("Hello\n\nWorld\n");
    let session = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let at = session.bytes().find("World").unwrap();
    session.move_caret(at, at).unwrap();
    app.update();
    let mut input = Input::default();
    input.push_key(
        KeyEvent::named(NamedKey::Backspace, Modifiers::default()),
        Platform::X11,
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    let next = bytes(&app, id);
    assert!(next.contains("Hello"), "{next}");
    assert!(next.contains("World"), "{next}");
    assert!(!next.contains("\n\n"), "{next}");
}

#[test]
fn projection_hides_markers_and_matches_after_an_edit() {
    let src = "say *hi* now\n";
    let tree = rndr_markdown::parse(src);
    let blocks = project(src, &tree, &Palette::default());
    let paragraph = blocks
        .iter()
        .find(|block| block.kind == DisplayKind::Paragraph)
        .unwrap();
    assert_eq!(paragraph.text, "say hi now");
    assert!(paragraph.spans.iter().any(|span| span.italic));
    let mut session = Session::open(src);
    session
        .edit(
            rndr_markdown::Command::Insert {
                at: 4,
                text: "Z".into(),
            },
            0,
        )
        .unwrap();
    let mut updated = blocks;
    update_projection(
        &mut updated,
        session.bytes(),
        session.tree(),
        session.last_edit().unwrap(),
        &Palette::default(),
    );
    let fresh = project(session.bytes(), session.tree(), &Palette::default());
    assert_eq!(updated.len(), fresh.len());
    for (left, right) in updated.iter().zip(fresh.iter()) {
        assert_eq!(left.text, right.text);
        assert_eq!(left.span, right.span);
        assert_eq!(left.kind, right.kind);
    }
}

fn labels_under(app: &App, root: Entity) -> String {
    fn walk(world: &World, entity: Entity, out: &mut String) {
        if let Some(label) = world.get::<Label>(entity) {
            out.push_str(&label.text);
            out.push('\n');
        }
        for child in children_of(world, entity) {
            walk(world, child, out);
        }
    }
    let mut out = String::new();
    walk(app.world(), root, &mut out);
    out
}

fn caret_at(app: &mut App, id: MarkdownId, at: usize) {
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .move_caret(at, at)
        .unwrap();
}

#[test]
fn liquid_widget_when_caret_is_outside_and_fields_when_inside() {
    let src = "```callout\ntone: tip\ntitle: Hello\nbody: Body text\n```\n";
    let (mut app, handle, id) = boot(src);
    // The widget's nested markdown syncs on the frame after it is mounted.
    app.update();
    let outside = labels_under(&app, handle.root);
    assert!(outside.contains("Hello"), "{outside}");
    assert!(outside.contains("Body text"), "{outside}");
    assert!(
        !outside.contains("tone  tip"),
        "widget collapsed the field into key/value text:\n{outside}"
    );

    let at = bytes(&app, id).find("Body text").unwrap();
    caret_at(&mut app, id, at);
    app.update();
    let inside = labels_under(&app, handle.root);
    assert!(inside.contains("tone"), "{inside}");
    assert!(inside.contains("Hello"), "{inside}");
    assert!(inside.contains("Body text"), "{inside}");
    assert!(
        inside.contains("tone  tip"),
        "editing view should show the field:\n{inside}"
    );
}

#[test]
fn timeline_separators_and_compare_cells_are_visible_while_editing() {
    let timeline = "```timeline\ntitle: Road\n---\nlabel: One\nts: Monday\nlane: product\n---\nlabel: Two\nts: Tuesday\n```\n";
    let (mut app, handle, id) = boot(timeline);
    let widget = labels_under(&app, handle.root);
    assert!(widget.contains("One") && widget.contains("Two"), "{widget}");
    assert!(widget.contains("product"), "{widget}");
    let at = bytes(&app, id).find("One").unwrap();
    caret_at(&mut app, id, at);
    app.update();
    let editing = labels_under(&app, handle.root);
    assert!(
        editing.contains("---"),
        "item separator missing:\n{editing}"
    );
    assert!(
        editing.contains("One") && editing.contains("Two"),
        "{editing}"
    );

    let compare = "```compare\ntitle: Pick\n| | A | B |\n|---|---|\n| Cost | 1 | 2 |\n```\n";
    let (mut app, handle, id) = boot(compare);
    let at = bytes(&app, id).find("Cost").unwrap();
    caret_at(&mut app, id, at);
    app.update();
    let editing = labels_under(&app, handle.root);
    assert!(editing.contains("Cost"), "{editing}");
    assert!(editing.contains('1') && editing.contains('2'), "{editing}");
    assert!(editing.contains("---"), "table rule missing:\n{editing}");
}

#[test]
fn multiline_field_edit_is_a_single_character_splice() {
    let src = "```callout\ntone: tip\nbody: |\n  Hello\n  there\n```\n";
    let (mut app, handle, id) = boot(src);
    let original = bytes(&app, id);
    let at = original.find("there").unwrap();
    caret_at(&mut app, id, at);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    let mut input = Input::default();
    input.push_key(
        KeyEvent::character('X', Modifiers::default()),
        Platform::host(),
    );
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.update();
    let next = bytes(&app, id);
    let mut expected = original.clone();
    expected.insert(at, 'X');
    assert_eq!(next, expected);
    assert!(next.contains("body: |"));
    assert!(next.contains("tone: tip"));
    assert!(next.contains("Hello"));
    assert!(next.contains("Xthere"));
}

#[test]
fn liquid_widget_and_field_paint_snapshots_differ_and_hold_still() {
    let src = "```callout\ntone: tip\ntitle: Hello\nbody: Body text\n```\n";
    let (mut app, handle, id) = boot(src);
    app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
    // The first frames mount the widget and sync its nested markdown.
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let widget = rndr_paint::frame(&mut app);
    let widget_again = rndr_paint::frame(&mut app);
    let widget_snap = rndr_paint::quads_snapshot(&widget);
    assert_eq!(widget_snap, rndr_paint::quads_snapshot(&widget_again));
    assert!(
        widget
            .quads
            .iter()
            .any(|quad| quad.uv != rndr_paint::solid_uv()),
        "widget painted no glyphs"
    );
    assert!(
        widget
            .quads
            .iter()
            .any(|quad| quad.uv == rndr_paint::solid_uv()),
        "widget painted no chrome"
    );
    assert!(
        labels_under(&app, handle.root).contains("Body text"),
        "widget snapshot is missing the callout body"
    );

    let at = bytes(&app, id).find("Body text").unwrap();
    caret_at(&mut app, id, at);
    let editing = rndr_paint::frame(&mut app);
    let editing_again = rndr_paint::frame(&mut app);
    let editing_snap = rndr_paint::quads_snapshot(&editing);
    assert_eq!(editing_snap, rndr_paint::quads_snapshot(&editing_again));
    assert_ne!(
        widget_snap, editing_snap,
        "caret inside the fence still painted the outside widget"
    );
    assert!(editing
        .quads
        .iter()
        .any(|quad| quad.uv != rndr_paint::solid_uv()));
    let _ = handle;
}

#[test]
fn editor_surface_is_not_this_mount() {
    // The source editor stays a different type. This test only checks the
    // markdown mount does not require SourceDocuments.
    let (app, _, _) = boot("Hi\n");
    assert!(app
        .world()
        .resource::<rndr_core::editing::SourceDocuments>()
        .is_none());
}
