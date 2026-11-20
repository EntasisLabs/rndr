//! Headless editing. No platform IME and no screen reader.

use rndr_core::*;
use rndr_markdown::Session;
use rndr_markdown_ui::*;
use rndr_ui::{Appearance, Token, UiId, UiTheme};

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
    assert!(inside.contains("tone"), "{inside}");
    assert!(inside.contains("tip"), "{inside}");
    assert!(inside.contains("Hello"), "{inside}");
    assert!(inside.contains("Body text"), "{inside}");
    assert!(
        inside.lines().filter(|line| line.trim() == "tone").count() >= 1,
        "tone should be its own field label:\n{inside}"
    );
    assert!(
        inside.contains("tip"),
        "editing view should show the field value:\n{inside}"
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
fn images_callouts_and_marks_display_without_their_markers() {
    let image = "![alt](pic.png \"t\")\n";
    let tree = rndr_markdown::parse(image);
    let blocks = project(image, &tree, &Palette::default());
    assert!(
        matches!(blocks[0].kind, DisplayKind::Image { ref alt } if alt == "alt"),
        "{:?}",
        blocks[0].kind
    );
    assert!(blocks[0].text.contains("alt"), "{}", blocks[0].text);
    assert!(!blocks[0].text.contains("pic.png"), "{}", blocks[0].text);

    let embed = "![[shot.png|400]]\n";
    let tree = rndr_markdown::parse(embed);
    let blocks = project(embed, &tree, &Palette::default());
    assert!(
        matches!(blocks[0].kind, DisplayKind::Image { .. }),
        "{:?}",
        blocks[0]
    );
    assert!(!blocks[0].text.contains("400"), "{}", blocks[0].text);

    let note = "- [Note] keep\n";
    let tree = rndr_markdown::parse(note);
    let blocks = project(note, &tree, &Palette::default());
    let item = blocks
        .iter()
        .find(|block| block.kind != DisplayKind::Gap)
        .unwrap();
    assert!(matches!(item.kind, DisplayKind::Bullet), "{:?}", item.kind);
    assert!(item.text.contains("[Note]"), "{}", item.text);

    let ordered = "1. [ ] buy\n";
    let tree = rndr_markdown::parse(ordered);
    let blocks = project(ordered, &tree, &Palette::default());
    let item = blocks
        .iter()
        .find(|block| block.kind != DisplayKind::Gap)
        .unwrap();
    assert!(
        matches!(item.kind, DisplayKind::Ordered(1)),
        "{:?}",
        item.kind
    );
    assert!(item.text.contains("[ ]"), "{}", item.text);

    let task = "- [ ] buy\n";
    let tree = rndr_markdown::parse(task);
    let blocks = project(task, &tree, &Palette::default());
    let item = blocks
        .iter()
        .find(|block| matches!(block.kind, DisplayKind::Task(false)))
        .unwrap();
    assert_eq!(item.chrome, "☐ ");
    assert_eq!(item.text, "buy");

    let highlight = "A ==hi== word\n";
    let tree = rndr_markdown::parse(highlight);
    let blocks = project(highlight, &tree, &Palette::default());
    assert_eq!(blocks[0].text, "A hi word");
    assert!(blocks[0].spans.iter().any(|span| span.highlight.is_some()));
    let inside = highlight.find("hi").unwrap();
    let shown = present(&blocks[0], highlight, &tree, inside, &Palette::default());
    assert!(shown.text.contains("=="), "{}", shown.text);

    let link = "[l](u \"t\")\n";
    let tree = rndr_markdown::parse(link);
    let blocks = project(link, &tree, &Palette::default());
    assert_eq!(blocks[0].text, "l");
    let shown = present(&blocks[0], link, &tree, 1, &Palette::default());
    assert!(shown.text.contains("(u"), "{}", shown.text);

    let callout = "> [!note] Title\n> Body\n";
    let tree = rndr_markdown::parse(callout);
    let blocks = project(callout, &tree, &Palette::default());
    assert!(
        matches!(blocks[0].kind, DisplayKind::Callout { ref tone } if tone == "note"),
        "{:?}",
        blocks[0].kind
    );
    assert!(blocks[0].chrome.contains("Note"), "{}", blocks[0].chrome);
    assert!(blocks[0].text.contains("Title"), "{}", blocks[0].text);
    assert!(blocks[0].text.contains("Body"), "{}", blocks[0].text);
    assert!(!blocks[0].text.contains("[!note]"), "{}", blocks[0].text);
    let shown = present(&blocks[0], callout, &tree, 0, &Palette::default());
    assert!(shown.text.contains("[!note]"), "{}", shown.text);
}

#[test]
fn clicking_the_task_checkbox_writes_one_byte() {
    let (mut app, handle, id) = boot("- [ ] buy milk\n");
    let before = bytes(&app, id);
    assert!(before.contains("[ ]"), "{before}");
    let slot = app
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            app.world()
                .get::<Label>(*entity)
                .is_some_and(|label| label.text.starts_with('☐'))
        })
        .expect("task label");
    let rect = *app.world().get::<Rect>(slot).unwrap();
    let mut toggled = false;
    for dx in [2.0, 6.0, 10.0, 14.0, 18.0, 24.0, 32.0] {
        let at = [rect.x + dx, rect.y + 10.0];
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = at;
        input.pointer.press_at = Some(at);
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.button = PointerButton::Left;
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
        app.update();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.down = false;
        input.pointer.released = true;
        app.update();
        if bytes(&app, id).contains("[x]") {
            toggled = true;
            break;
        }
    }
    let next = bytes(&app, id);
    assert!(
        toggled,
        "checkbox click did not toggle, rect={rect:?} text={next}"
    );
    assert_eq!(next, before.replacen("[ ]", "[x]", 1));
    assert_eq!(next.matches("[x]").count(), 1);
}

#[test]
fn light_theme_marks_use_text_token_and_math_is_italic_serif() {
    let src =
        "Say **bold** and `code` and $E=mc^2$ plus {{font:serif|words}} and {{size:lg|big}}.\n";
    let (mut app, handle, _) = boot(src);
    let theme = UiTheme {
        appearance: Appearance::Light,
        ..UiTheme::default()
    };
    app.world_mut().insert_resource(theme.clone());
    app.update();
    let ink = theme.color(Token::Text);
    let world = app.world();
    let label = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text.contains("bold"))
        })
        .expect("paragraph");
    let spans = world.get::<Spans>(label).unwrap();
    let slice = |span: &TextSpan| {
        world
            .get::<Label>(label)
            .unwrap()
            .text
            .get(span.start..span.end)
            .unwrap_or("")
    };
    let bold = spans
        .runs
        .iter()
        .find(|span| slice(span) == "bold")
        .expect("bold span");
    assert!(
        (bold.color.r - ink.r).abs() < 0.02 && bold.weight == Some(FONT_WEIGHT_BOLD),
        "bold ink {:?} theme {:?}",
        bold.color,
        ink
    );
    let code = spans
        .runs
        .iter()
        .find(|span| slice(span) == "code")
        .expect("code span");
    assert!(
        (code.color.r - ink.r).abs() < 0.02,
        "code ink {:?}",
        code.color
    );
    assert!(code.highlight.is_some());
    let math = spans
        .runs
        .iter()
        .find(|span| slice(span).contains('E'))
        .expect("math span");
    assert!(math.italic, "math should be italic");
    assert_eq!(math.family.as_deref(), Some("serif"));
    assert!(
        (math.color.r - ink.r).abs() < 0.02,
        "math ink {:?}",
        math.color
    );
    let styled = spans
        .runs
        .iter()
        .find(|span| slice(span) == "words" || slice(span) == "big")
        .expect("styled span");
    assert!(
        (styled.color.r - ink.r).abs() < 0.02,
        "styled ink {:?}",
        styled.color
    );
    assert!(styled.color.r < 0.4, "light theme ink should be dark");
    let _ = handle;
}

#[test]
fn task_checkbox_is_a_bordered_box_in_both_themes() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let (mut app, _, _) = boot("- [ ] buy\n- [x] done\n");
        app.world_mut().insert_resource(UiTheme {
            appearance,
            ..UiTheme::default()
        });
        app.update();
        let world = app.world();
        let boxes: Vec<_> = world
            .entities_with::<Border>()
            .into_iter()
            .filter(|entity| {
                world.get::<Layout>(*entity).is_some_and(|layout| {
                    matches!(layout.width, Size::Px(width) if (width - 14.0).abs() < 0.5)
                        && matches!(layout.height, Size::Px(height) if (height - 14.0).abs() < 0.5)
                })
            })
            .collect();
        assert!(
            boxes.len() >= 2,
            "{appearance:?} painted {} boxes",
            boxes.len()
        );
        for entity in boxes {
            let border = world.get::<Border>(entity).unwrap();
            assert!(
                border.width >= 1.5,
                "{appearance:?} border {}",
                border.width
            );
            let stroke = app
                .world()
                .resource::<UiTheme>()
                .unwrap()
                .color(Token::StrongStroke);
            assert!(
                (border.color.r - stroke.r).abs() < 0.02
                    && (border.color.g - stroke.g).abs() < 0.02
                    && (border.color.b - stroke.b).abs() < 0.02,
                "{appearance:?} border {:?} stroke {:?}",
                border.color,
                stroke
            );
        }
    }
}

#[test]
fn callout_paints_tone_fill_and_accent() {
    let (mut app, handle, id) = boot("> [!tip] Keep\n> the trail\n\nAfter\n");
    let at = bytes(&app, id).find("After").unwrap();
    caret_at(&mut app, id, at);
    app.update();
    let theme = app
        .world()
        .resource::<UiTheme>()
        .cloned()
        .unwrap_or_default();
    let fill = theme.color(Token::SuccessFill);
    let world = app.world();
    let slot = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text.contains("Keep"))
        })
        .expect("callout");
    let color = world.get::<Color>(slot).copied().expect("callout fill");
    assert!(
        (color.r - fill.r).abs() < 0.02 && (color.g - fill.g).abs() < 0.02,
        "fill {color:?} token {fill:?}"
    );
    let accent = theme.color(Token::SuccessStroke);
    let bar = children_of(world, slot)
        .into_iter()
        .find(|child| {
            world.get::<Layout>(*child).is_some_and(
                |layout| matches!(layout.width, Size::Px(width) if (width - 4.0).abs() < 0.5),
            )
        })
        .expect("accent bar");
    let bar_color = world.get::<Color>(bar).copied().unwrap();
    assert!((bar_color.r - accent.r).abs() < 0.02);
    let _ = handle;
}

#[test]
fn first_block_and_last_table_row_sit_inside_their_slots() {
    let src = "# Title\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    let (mut app, handle, id) = boot(src);
    let at = bytes(&app, id).find('2').unwrap();
    caret_at(&mut app, id, at);
    app.update();
    let world = app.world();
    let surface = *world.get::<Rect>(handle.root).unwrap();
    let title = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text.contains("Title"))
        })
        .expect("title");
    let title_rect = *world.get::<Rect>(title).unwrap();
    assert!(
        title_rect.y >= surface.y + 8.0,
        "title {title_rect:?} surface {surface:?}"
    );
    let last = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|label| label.text == "2")
        })
        .expect("last cell");
    let cell = *world.get::<Rect>(last).unwrap();
    let mut table = world.get::<Parent>(last).unwrap().entity;
    for _ in 0..8 {
        if world.get::<Layout>(table).is_some_and(
            |layout| matches!(layout.height, Size::Px(height) if height > cell.h * 2.0),
        ) {
            break;
        }
        table = world
            .get::<Parent>(table)
            .map(|parent| parent.entity)
            .unwrap_or(table);
    }
    let table_rect = *world.get::<Rect>(table).unwrap();
    assert!(
        cell.y + cell.h <= table_rect.y + table_rect.h + 0.5,
        "cell {cell:?} table {table_rect:?}"
    );
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
