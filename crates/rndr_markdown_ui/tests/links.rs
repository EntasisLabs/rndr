//! Headless pointer/accessibility contract; no native screen-reader certification.
use rndr_core::*;
use rndr_markdown_ui::*;
use rndr_ui::UiId;

fn boot(source: &str) -> (App, MarkdownSurfaceHandle, MarkdownId) {
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    app.world_mut().insert_resource(Platform::X11);
    app.world_mut().insert_resource(Viewport::new(640., 600.));
    let id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(source);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(640.),
        height: Size::Px(600.),
        ..Layout::column()
    },));
    let handle = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(10),
        MarkdownSurfaceProps::new(id, "Note"),
    )
    .unwrap();
    app.update();
    app.update();
    (app, handle, id)
}

fn selection(app: &App, id: MarkdownId) -> (usize, usize, String) {
    let session = app
        .world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    (
        session.anchor(),
        session.caret(),
        session.bytes().to_owned(),
    )
}

fn point(app: &App, needle: &str) -> [f32; 2] {
    let world = app.world();
    let entity = world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| world.get::<Label>(*entity).unwrap().text.contains(needle))
        .expect("rendered link");
    let label = &world.get::<Label>(entity).unwrap().text;
    let index = label.find(needle).unwrap();
    let rect = world.get::<Rect>(entity).unwrap();
    let style = world.get::<TextStyle>(entity).cloned().unwrap_or_default();
    let spans = world
        .get::<Spans>(entity)
        .map(|spans| spans.runs.as_slice())
        .unwrap_or_default();
    let label_box = world.get::<LabelBox>(entity);
    let inset = label_box
        .map(|b| [b.inset_x, b.inset_y])
        .unwrap_or([0., 0.]);
    let width = if label_box.is_some() {
        rect.w - 2. * inset[0]
    } else {
        rect.w - 8.
    };
    let layout = text_engine(world).layout(label, &style, spans, width.max(1.));
    let glyph = layout
        .glyphs
        .iter()
        .find(|glyph| glyph.start == index)
        .expect("first link glyph");
    let (x, y) = layout.caret_xy(index);
    [
        rect.x + inset[0] + x + glyph.w * 0.25,
        rect.y + inset[1] + y + 8.,
    ]
}

fn pointer(
    app: &mut App,
    at: [f32; 2],
    pressed: bool,
    down: bool,
    released: bool,
    ctrl: bool,
    cmd: bool,
) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    *input = Input::default();
    input.ctrl = ctrl;
    input.super_key = cmd;
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.button = PointerButton::Left;
    input.pointer.pressed = pressed;
    input.pointer.down = down;
    input.pointer.released = released;
    app.update();
}

#[test]
fn modified_clicks_emit_root_and_written_target_without_editing() {
    for (source, label, expected) in [
        (
            "prefix **bold** [café](notes/a.md#héading \"Title\") end\n",
            "café",
            "notes/a.md#héading",
        ),
        (
            "prefix [[Folder/Note#Heading|alias]] end\n",
            "alias",
            "Folder/Note#Heading",
        ),
        (
            "prefix <https://example.test/a> end\n",
            "https://",
            "https://example.test/a",
        ),
        (
            "prefix https://example.test/a end\n",
            "https://",
            "https://example.test/a",
        ),
        (
            "prefix [unsafe](javascript:alert(1)) end\n",
            "unsafe",
            "javascript:alert(1)",
        ),
        (
            "- prefix [nested](src/main.rs#L12) end\n",
            "nested",
            "src/main.rs#L12",
        ),
        (
            "- [ ] prefix [tasklink](note.md) end\n",
            "tasklink",
            "note.md",
        ),
        (
            "| Header |\n| --- |\n| [celllink](note.md) |\n",
            "celllink",
            "note.md",
        ),
    ] {
        for cmd in [false, true] {
            let (mut app, handle, id) = boot(source);
            app.world_mut()
                .insert_resource(if cmd { Platform::Mac } else { Platform::X11 });
            let before = selection(&app, id);
            let at = point(&app, label);
            pointer(&mut app, at, true, true, false, !cmd, cmd);
            assert!(
                events::<LinkActivated>(app.world()).is_empty(),
                "not on press"
            );
            assert_eq!(
                selection(&app, id),
                before,
                "link press must preserve selection: {source}, at {at:?}"
            );
            pointer(&mut app, at, false, false, true, !cmd, cmd);
            let activated = events::<LinkActivated>(app.world());
            assert_eq!(activated.len(), 1, "{source}");
            assert_eq!(activated[0].view, handle.root);
            assert_eq!(activated[0].href, expected);
            assert_eq!(selection(&app, id), before);
            pointer(&mut app, at, false, false, false, false, false);
            assert!(
                events::<LinkActivated>(app.world()).is_empty(),
                "exactly once"
            );
        }
    }
}

#[test]
fn ordinary_click_drag_and_typing_remain_editing_and_modified_drag_cancels() {
    let (mut app, _, id) = boot("prefix [label](note.md) tail\n");
    let at = point(&app, "label");
    let before = selection(&app, id);
    pointer(&mut app, at, true, true, false, false, false);
    assert_ne!(selection(&app, id).1, before.1);
    assert!(events::<LinkActivated>(app.world()).is_empty());
    pointer(
        &mut app,
        [at[0] + 18., at[1]],
        false,
        true,
        false,
        false,
        false,
    );
    let (anchor, caret, _) = selection(&app, id);
    assert_ne!(anchor, caret);
    pointer(&mut app, at, false, false, true, false, false);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    *input = Input::default();
    input.push_key(
        KeyEvent::character('X', Modifiers::default()),
        Platform::X11,
    );
    app.update();
    assert!(selection(&app, id).2.contains('X'));

    let (mut app, _, id) = boot("prefix [label](note.md) tail\n");
    let before = selection(&app, id);
    let at = point(&app, "label");
    pointer(&mut app, at, true, true, false, true, false);
    pointer(
        &mut app,
        [at[0] + 20., at[1]],
        false,
        true,
        false,
        true,
        false,
    );
    pointer(&mut app, at, false, false, true, true, false);
    assert!(events::<LinkActivated>(app.world()).is_empty());
    assert_eq!(selection(&app, id), before);
}

#[test]
fn whitespace_nonlinks_right_click_and_stale_press_do_not_activate() {
    let (mut app, _, id) = boot("prefix [label](note.md) tail\n");
    let at = point(&app, "label");
    for at in [[550., at[1]], point(&app, "prefix")] {
        pointer(&mut app, at, true, true, false, true, false);
        pointer(&mut app, at, false, false, true, true, false);
        assert!(events::<LinkActivated>(app.world()).is_empty());
    }
    let at = point(&app, "label");
    pointer(&mut app, at, true, true, false, true, false);
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .edit(
            rndr_markdown::Command::Insert {
                at: 0,
                text: "new ".into(),
            },
            100,
        )
        .unwrap();
    pointer(&mut app, at, false, false, true, true, false);
    assert!(events::<LinkActivated>(app.world()).is_empty());
}

#[test]
fn multiple_surfaces_have_distinct_ids_and_only_the_hit_root_activates() {
    let (mut app, first, first_id) = boot("[first](a.md)\n");
    let second_id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open("[second](b.md)\n");
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(640.),
        height: Size::Px(600.),
        ..Layout::column()
    },));
    let second = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(20),
        MarkdownSurfaceProps::new(second_id, "Second"),
    )
    .unwrap();
    let first_parent = app.world().get::<Parent>(first.root).unwrap().entity;
    app.world_mut()
        .get_mut::<Layout>(first_parent)
        .unwrap()
        .width = Size::Px(320.);
    app.world_mut().get_mut::<Layout>(parent).unwrap().width = Size::Px(320.);
    let row = app.world_mut().spawn((Layout {
        width: Size::Px(640.),
        height: Size::Px(600.),
        ..Layout::row()
    },));
    set_parent(app.world_mut(), first_parent, row);
    set_parent(app.world_mut(), parent, row);
    app.update();
    app.update();
    let a = access_snapshot(app.world(), first.root).unwrap();
    let b = access_snapshot(app.world(), second.root).unwrap();
    assert!(a.nodes.iter().all(|a| b.nodes.iter().all(|b| a.id != b.id)));
    let a_before = selection(&app, first_id);
    let b_before = selection(&app, second_id);
    let at = point(&app, "second");
    pointer(&mut app, at, true, true, false, true, false);
    pointer(&mut app, at, false, false, true, true, false);
    assert_eq!(
        events::<LinkActivated>(app.world()),
        &[LinkActivated {
            view: second.root,
            href: "b.md".into()
        }]
    );
    assert_eq!(selection(&app, first_id), a_before);
    assert_eq!(selection(&app, second_id), b_before);
    let node = b
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::Link)
        .unwrap();
    let command = AccessCommand::ActivateOverlay {
        entity: second.root,
        node: node.id,
    };
    assert_eq!(
        try_apply_access(app.world_mut(), command.clone()),
        AccessCommandResult::Applied
    );
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(second_id)
        .unwrap()
        .edit(
            rndr_markdown::Command::Insert {
                at: 0,
                text: "new ".into(),
            },
            1,
        )
        .unwrap();
    assert_eq!(
        try_apply_access(app.world_mut(), command.clone()),
        AccessCommandResult::Rejected,
        "unpublished revision"
    );
    app.update();
    assert_eq!(
        try_apply_access(app.world_mut(), command),
        AccessCommandResult::Rejected,
        "retired ID after publication"
    );
}

#[test]
fn access_activation_uses_current_overlay_and_rejects_stale_or_nonlink() {
    let (mut app, handle, id) =
        boot("[label](note.md \"title\") [[Other#Heading|alias]] <https://example.test>\n");
    let snapshot = access_snapshot(app.world(), handle.root).unwrap();
    let links: Vec<_> = snapshot
        .nodes
        .iter()
        .filter(|node| node.role == AccessRole::Link)
        .collect();
    assert_eq!(
        links
            .iter()
            .map(|node| node.value.as_str())
            .collect::<Vec<_>>(),
        ["note.md", "Other#Heading", "https://example.test"]
    );
    let before = selection(&app, id);
    for link in &links {
        assert_eq!(
            activate_access_link(app.world_mut(), handle.root, &snapshot, link.id),
            AccessCommandResult::Applied
        );
    }
    app.update();
    assert_eq!(events::<LinkActivated>(app.world()).len(), 3);
    assert_eq!(selection(&app, id), before);
    assert_eq!(
        activate_access_link(app.world_mut(), handle.root, &snapshot, 1),
        AccessCommandResult::Rejected
    );
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .edit(
            rndr_markdown::Command::Insert {
                at: 0,
                text: "changed ".into(),
            },
            100,
        )
        .unwrap();
    let fresh = access_snapshot(app.world(), handle.root).unwrap();
    assert_eq!(
        activate_access_link(app.world_mut(), handle.root, &fresh, links[0].id),
        AccessCommandResult::Rejected,
        "unpublished overlay revision"
    );
    assert_eq!(
        activate_access_link(app.world_mut(), handle.root, &snapshot, links[0].id),
        AccessCommandResult::Rejected
    );
}
