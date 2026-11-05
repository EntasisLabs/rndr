//! Retained source view contracts, without a native renderer.
use rndr_core::editing::*;
use rndr_core::*;
use rndr_ui::editing::*;
use rndr_ui::{identity, Surface, UiError, UiId, UiPlugin};

fn boot() -> (App, Entity, DocumentId) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let world = app.world_mut();
    let mut docs = SourceDocuments::default();
    let document = docs.open("\u{feff}---\r\né 👩‍💻\r\n```opaque\r\n");
    world.insert_resource(docs);
    let parent = world.spawn((Layout::column(),));
    (app, parent, document)
}
fn mount(app: &mut App, parent: Entity, document: DocumentId) -> EditorSurfaceHandle {
    EditorSurface::mount(
        app.world_mut(),
        parent,
        UiId(7),
        EditorSurfaceProps::new(document, "Source"),
    )
    .unwrap()
}
fn append(app: &mut App, document: DocumentId) {
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    let end = session.text().len();
    session
        .commit(SourceBatch {
            document,
            base: session.revision(),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: end..end,
                replacement: "tail".into(),
            }],
        })
        .unwrap();
}
#[test]
fn retained_update_preserves_identity_buffer_viewport_and_history() {
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    append(&mut app, doc);
    app.update();
    app.world_mut()
        .get_mut::<Editor>(handle.editor)
        .unwrap()
        .offset = 80.0;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.editor);
    let mut props = EditorSurfaceProps::new(doc, "Renamed source");
    props.gutter = false;
    let same = EditorSurface::mount(app.world_mut(), parent, UiId(7), props).unwrap();
    assert_eq!(handle, same);
    assert_eq!(identity(app.world(), handle.root), Some(UiId(7)));
    assert_eq!(identity(app.world(), handle.editor), None);
    assert_eq!(app.world().get::<Editor>(same.editor).unwrap().offset, 80.0);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(same.editor)
    );
    assert_eq!(
        app.world()
            .get::<Semantics>(same.editor)
            .unwrap()
            .name
            .as_deref(),
        Some("Renamed source")
    );
    assert!(!children_of(app.world(), same.root)
        .iter()
        .any(|e| app.world().has::<Gutter>(*e)));
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session.undo().unwrap();
    assert!(!session.text().ends_with("tail"));
}
#[test]
fn destroy_remount_keeps_document_history_and_borrowed_children() {
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    append(&mut app, doc);
    let borrowed = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), borrowed, handle.root);
    handle.destroy(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(handle.editor));
    assert!(app.world().is_alive(borrowed));
    assert!(!app.world().has::<Parent>(borrowed));
    assert_eq!(handle.validate(app.world()), Err(UiError::Stale));
    let new = mount(&mut app, parent, doc);
    assert_ne!(handle, new);
    assert!(app
        .world()
        .get::<Buffer>(new.editor)
        .unwrap()
        .text()
        .ends_with("tail"));
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session.undo().unwrap();
    assert!(!session.text().ends_with("tail"));
    session.redo().unwrap();
    assert!(session.text().ends_with("tail"));
}
#[test]
fn readonly_is_session_authoritative_across_retained_views() {
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    let mut props = EditorSurfaceProps::new(doc, "Read only");
    props.readonly = true;
    EditorSurface::mount(app.world_mut(), parent, UiId(7), props).unwrap();
    assert!(app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .is_readonly());
    assert!(app.world().has::<CapturesKeys>(handle.editor));
    assert!(app.world().has::<Focusable>(handle.editor));
    let mut peer_props = EditorSurfaceProps::new(doc, "Second pane");
    peer_props.readonly = true;
    let peer = EditorSurface::mount(app.world_mut(), parent, UiId(8), peer_props).unwrap();
    assert_ne!(peer.editor, handle.editor);
    app.update();
    assert!(app.world().get::<Semantics>(peer.editor).unwrap().readonly);
    assert!(
        app.world()
            .get::<Semantics>(handle.editor)
            .unwrap()
            .readonly
    );
    assert_eq!(attached_source(app.world(), peer.editor), Some(doc));
    assert!(app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .is_readonly());
    assert_eq!(
        children_of(app.world(), parent),
        vec![handle.root, peer.root]
    );
    peer.destroy(app.world_mut()).unwrap();
    assert!(handle.validate(app.world()).is_ok());
    assert_eq!(attached_source(app.world(), handle.editor), Some(doc));
}
#[test]
fn existing_editor_blocks_other_component_kinds_and_external_detach_is_stale() {
    use rndr_ui::controls::{Button as UiButton, ButtonProps, HostIntent};
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    assert_eq!(
        Surface::Content.mount(app.world_mut(), parent, UiId(7)),
        Err(UiError::IdentityConflict)
    );
    assert!(matches!(
        UiButton::mount(
            app.world_mut(),
            parent,
            UiId(7),
            ButtonProps::new("Button", HostIntent::Command(1))
        ),
        Err(UiError::IdentityConflict)
    ));
    detach_source(app.world_mut(), handle.editor).unwrap();
    assert_eq!(handle.validate(app.world()), Err(UiError::Stale));
}

#[test]
fn direct_host_policy_changes_refresh_semantics_and_input_without_remount() {
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    let mut props = EditorSurfaceProps::new(doc, "Readonly");
    props.readonly = true;
    EditorSurface::mount(app.world_mut(), parent, UiId(7), props).unwrap();
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap()
        .set_readonly(false)
        .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.editor);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('!', Modifiers::new()));
    app.update();
    assert!(
        !app.world()
            .get::<Semantics>(handle.editor)
            .unwrap()
            .readonly
    );
    assert!(app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(doc)
        .unwrap()
        .text()
        .ends_with('!'));
}

#[test]
fn changed_document_blank_name_conflicting_component_and_malformed_handle_reject() {
    let (mut app, parent, doc) = boot();
    let handle = mount(&mut app, parent, doc);
    let other = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open("Other");
    assert_eq!(
        EditorSurface::mount(
            app.world_mut(),
            parent,
            UiId(7),
            EditorSurfaceProps::new(other, "Other")
        ),
        Err(EditorSurfaceError::Ui(UiError::IdentityConflict))
    );
    assert_eq!(
        EditorSurface::mount(
            app.world_mut(),
            parent,
            UiId(7),
            EditorSurfaceProps::new(doc, "  ")
        ),
        Err(EditorSurfaceError::Ui(UiError::InvalidProps))
    );
    Surface::Content
        .mount(app.world_mut(), parent, UiId(8))
        .unwrap();
    assert_eq!(
        EditorSurface::mount(
            app.world_mut(),
            parent,
            UiId(8),
            EditorSurfaceProps::new(other, "Other")
        ),
        Err(EditorSurfaceError::Ui(UiError::IdentityConflict))
    );
    app.world_mut().remove::<Editor>(handle.editor);
    assert_eq!(
        mount_result(&mut app, parent, doc),
        Err(EditorSurfaceError::Ui(UiError::Stale))
    );
}
fn mount_result(
    app: &mut App,
    parent: Entity,
    doc: DocumentId,
) -> Result<EditorSurfaceHandle, EditorSurfaceError> {
    EditorSurface::mount(
        app.world_mut(),
        parent,
        UiId(7),
        EditorSurfaceProps::new(doc, "Source"),
    )
}
