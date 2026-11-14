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
        KeyEvent::character(
            'c',
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ),
        Platform::X11,
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
        KeyEvent::character(
            'v',
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ),
        Platform::X11,
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
