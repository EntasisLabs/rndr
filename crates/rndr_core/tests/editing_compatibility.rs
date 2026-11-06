//! E8 additive compatibility and explicit authority-transition gates.

struct EventProbe {
    document: DocumentId,
    update_revision: Option<Revision>,
    delivered: Vec<ChangeEvent>,
}

fn observe_update(world: &mut rndr_core::World) {
    let id = world.resource::<EventProbe>().unwrap().document;
    let revision = world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    world.resource_mut::<EventProbe>().unwrap().update_revision = Some(revision);
}

fn consume_layout(world: &mut rndr_core::World) {
    let id = world.resource::<EventProbe>().unwrap().document;
    let events: Vec<_> = world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .drain_events()
        .collect();
    world
        .resource_mut::<EventProbe>()
        .unwrap()
        .delivered
        .extend(events);
}

#[test]
fn one_layout_consumer_observes_native_and_host_commits_without_double_delivery() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Input::default());
    let mut docs = SourceDocuments::default();
    let id = docs.open("é\r\n");
    app.world_mut().insert_resource(EventProbe {
        document: id,
        update_revision: None,
        delivered: Vec::new(),
    });
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view);
    app.add_system(rndr_core::Schedule::Update, observe_update);
    app.add_system(rndr_core::Schedule::Layout, consume_layout);
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push('x');
    app.update();
    let probe = app.world().resource::<EventProbe>().unwrap();
    assert_eq!(probe.update_revision, Some(Revision(0)));
    assert_eq!(probe.delivered.len(), 1);
    assert_eq!(probe.delivered[0].before, Revision(0));
    assert_eq!(probe.delivered[0].after, Revision(1));
    let source = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let returned = source
        .commit(SourceBatch {
            document: id,
            base: source.revision(),
            origin: EditOrigin::Host(9),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "!".into(),
            }],
        })
        .unwrap()
        .unwrap();
    app.update();
    let probe = app.world().resource::<EventProbe>().unwrap();
    assert_eq!(probe.update_revision, Some(Revision(2)));
    assert_eq!(probe.delivered.len(), 2);
    assert_eq!(probe.delivered[1].after, returned.after);
    app.update();
    assert_eq!(
        app.world()
            .resource::<EventProbe>()
            .unwrap()
            .delivered
            .len(),
        2
    );
}
use rndr_core::editing::*;
use rndr_core::structured::StructuredDocument;
use rndr_core::structured_native::*;
use rndr_core::{
    edit_buffer, App, Buffer, BufferEditError, CapturesKeys, Chrome, Editor, Focus, Input,
    KeyEvent, Label, Modifiers, Platform,
};

fn press(app: &mut App, ch: char) {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(ch, modifiers));
    app.update();
}

#[test]
fn explicit_fork_retains_legacy_history_bytes_and_directed_selection() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Focus::default());
    let raw = "\u{feff}é task  \r\n```unknown\r\nopaque\r\n```\r\n";
    let mut buffer = Buffer::from_str(raw);
    buffer.anchor = raw.len();
    buffer.caret = 3;
    let legacy = app
        .world_mut()
        .spawn((buffer, Editor::default(), Label::new(""), CapturesKeys));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(legacy);
    app.update();
    assert_eq!(edit_buffer(app.world_mut(), legacy, 5..5, "!"), Ok(true));
    // Adoption copies a directed selection, independently of the edit's caret.
    let b = app.world_mut().get_mut::<Buffer>(legacy).unwrap();
    b.anchor = b.text().len();
    b.caret = 3;
    let b = app.world().get::<Buffer>(legacy).unwrap();
    let bytes = b.text();
    let selected = SourceSelection {
        anchor: b.anchor,
        head: b.caret,
    };
    let mut docs = SourceDocuments::default();
    let id = docs.open(bytes.clone());
    docs.get_mut(id).unwrap().set_selection(selected).unwrap();
    app.world_mut().insert_resource(docs);
    let managed = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), managed, id).unwrap();
    let b = app.world().get::<Buffer>(managed).unwrap();
    assert_eq!(b.text(), bytes);
    assert_eq!((b.anchor, b.caret), (selected.anchor, selected.head));
    assert!(selected.anchor > selected.head);
    assert_eq!(
        edit_buffer(app.world_mut(), managed, 0..0, "bad"),
        Err(BufferEditError::ManagedDocument)
    );
    let source = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    assert!(source.undo().unwrap().is_none()); // Legacy history was not imported.
    source
        .commit(SourceBatch {
            document: id,
            base: source.revision(),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: 5..6,
                replacement: "?".into(),
            }],
        })
        .unwrap();
    source.undo().unwrap();
    assert_eq!(source.text(), bytes);
    assert_eq!(source.selection(), selected);
    // Legacy Undo/Redo still addresses its original independent owner.
    press(&mut app, 'z');
    assert_eq!(app.world().get::<Buffer>(legacy).unwrap().text(), raw);
    press(&mut app, 'y');
    assert_eq!(app.world().get::<Buffer>(legacy).unwrap().text(), bytes);
    detach_source(app.world_mut(), managed).unwrap();
    attach_source(app.world_mut(), managed, id).unwrap();
    let source = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    source.redo().unwrap();
    assert_eq!(source.text(), bytes.replacen('!', "?", 1));
    assert_eq!(app.world().get::<Buffer>(legacy).unwrap().text(), bytes);
}

#[test]
fn legacy_raw_mutation_remains_history_free() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Focus::default());
    let view = app.world_mut().spawn((
        Buffer::from_str("é\r\n"),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view);
    app.world_mut()
        .get_mut::<Buffer>(view)
        .unwrap()
        .insert_str(0, "!");
    let raw_result = app.world().get::<Buffer>(view).unwrap().text();
    press(&mut app, 'z');
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), raw_result);
    let end = raw_result.len();
    edit_buffer(app.world_mut(), view, end..end, "?").unwrap();
    press(&mut app, 'z');
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), raw_result);
    press(&mut app, 'y');
    assert_eq!(
        app.world().get::<Buffer>(view).unwrap().text(),
        format!("{raw_result}?")
    );
}

#[test]
fn source_and_tree_ownership_refuse_in_both_attachment_orders_without_side_effects() {
    for source_first in [true, false] {
        let mut app = App::new();
        app.add_plugin(Chrome);
        let mut sources = SourceDocuments::default();
        let source = sources.open("\u{feff}source\r\n");
        let mut trees = StructuredDocuments::default();
        let tree = trees.insert(StructuredDocument::new("tree"));
        app.world_mut().insert_resource(sources);
        app.world_mut().insert_resource(trees);
        let view = app
            .world_mut()
            .spawn((Buffer::from_str(""), Editor::default(), Label::new("")));
        if source_first {
            attach_source(app.world_mut(), view, source).unwrap();
            let epoch = source_attachment_epoch(app.world(), view);
            assert_eq!(
                attach_structured(app.world_mut(), view, tree),
                Err(StructuredViewError::Unavailable)
            );
            assert_eq!(source_attachment_epoch(app.world(), view), epoch);
            assert!(attached_structured(app.world(), view).is_none());
            assert_eq!(
                app.world().get::<Buffer>(view).unwrap().text(),
                "\u{feff}source\r\n"
            );
        } else {
            attach_structured(app.world_mut(), view, tree).unwrap();
            assert_eq!(
                attach_source(app.world_mut(), view, source),
                Err(EditError::AlreadyAttached)
            );
            assert_eq!(attached_structured(app.world(), view), Some(tree));
            assert_eq!(attached_source(app.world(), view), None);
            assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), "tree");
        }
        assert_eq!(
            edit_buffer(app.world_mut(), view, 0..0, "bad"),
            Err(BufferEditError::ManagedDocument)
        );
        let source = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(source)
            .unwrap();
        assert_eq!(source.revision(), Revision(0));
        assert_eq!(source.text(), "\u{feff}source\r\n");
        let tree = app
            .world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(tree)
            .unwrap();
        assert_eq!(tree.revision(), Revision(0));
        assert_eq!(tree.plain_text(), "tree");
    }
}
