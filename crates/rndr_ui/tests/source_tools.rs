use rndr_core::editing::*;
use rndr_core::*;
use rndr_ui::editing::{EditorSurface, EditorSurfaceProps};
use rndr_ui::source_tools::*;
use rndr_ui::{UiId, UiPlugin};

#[test]
fn completion_popup_retains_editor_focus_and_retires_on_detach() {
    use rndr_core::source_extensions::*;
    use rndr_ui::completion::*;
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let w = app.world_mut();
    let mut docs = SourceDocuments::default();
    let id = docs.open("a");
    w.insert_resource(docs);
    let root = w.spawn((Layout::column(),));
    let editor =
        EditorSurface::mount(w, root, UiId(100), EditorSurfaceProps::new(id, "Source")).unwrap();
    let popup = CompletionPopup::mount(w, editor.editor).unwrap();
    request_focus(w, editor.editor);
    let mut extensions = SourceExtensions::default();
    let provider = extensions.register(1);
    let (ticket, _) = extensions
        .request(w, provider, editor.editor, RequestKind::Completion)
        .unwrap();
    extensions
        .publish_completions(
            w,
            &ticket,
            vec![CompletionItem {
                label: "alpha".into(),
                changes: vec![SourceChange {
                    range: 0..1,
                    replacement: "alpha".into(),
                }],
            }],
        )
        .unwrap();
    w.insert_resource(extensions);
    apply_completion_popups(w);
    assert!(!w.has::<Hidden>(popup.root));
    assert!(w.has::<PassivePopup>(popup.root));
    assert!(!w.has::<Modal>(popup.root));
    assert_eq!(w.resource::<Focus>().unwrap().entity, Some(editor.editor));
    assert!(w.get::<Label>(popup.root).unwrap().text.contains("› alpha"));
    detach_source(w, editor.editor).unwrap();
    apply_completion_popups(w);
    assert!(w.has::<Hidden>(popup.root));
    despawn(w, editor.root);
    apply_completion_popups(w);
    assert!(!w.is_alive(popup.root));
}

#[test]
fn find_bar_replace_all_and_navigation_use_document_history() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let w = app.world_mut();
    let mut docs = SourceDocuments::default();
    let id = docs.open("é and é");
    w.insert_resource(docs);
    let root = w.spawn((Layout::column(),));
    let editor =
        EditorSurface::mount(w, root, UiId(100), EditorSurfaceProps::new(id, "Source")).unwrap();
    let bar = FindBar::mount(w, root, editor.editor).unwrap();
    w.get_mut::<Label>(bar.query).unwrap().text = "é".into();
    w.get_mut::<Label>(bar.replacement).unwrap().text = "😀".into();
    apply_find_bars(w);
    assert_eq!(w.get::<Label>(bar.status).unwrap().text, "2 matches");
    fn button(w: &World, root: Entity, name: &str) -> Entity {
        children_of(w, root)
            .into_iter()
            .flat_map(|e| children_of(w, e))
            .find(|&e| w.get::<Semantics>(e).and_then(|s| s.name.as_deref()) == Some(name))
            .unwrap()
    }
    let next = button(w, bar.root, "Next");
    assert_eq!(try_activate(w, next), ActivationResult::Accepted);
    apply_find_bars(w);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .view_selection(editor.editor),
        Some(SourceSelection { anchor: 0, head: 2 })
    );
    let all = button(w, bar.root, "Replace all");
    assert_eq!(try_activate(w, all), ActivationResult::Accepted);
    apply_find_bars(w);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text(),
        "😀 and 😀"
    );
    w.resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .undo()
        .unwrap();
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text(),
        "é and é"
    );
    // The toolbar is outside this editor's readonly subtree, but cannot bypass it,
    // even after ordinary source projection in a later native frame.
    w.insert(
        editor.editor,
        Semantics {
            readonly: true,
            ..Default::default()
        },
    );
    project_source(w, editor.editor);
    assert!(access_details(w, editor.editor).readonly);
    assert_eq!(try_activate(w, all), ActivationResult::Accepted);
    apply_find_bars(w);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text(),
        "é and é"
    );
    w.insert(editor.editor, Semantics::default());
    project_source(w, editor.editor);
    assert!(!access_details(w, editor.editor).readonly);
    // Inherited view policy is equally authoritative even with a writable document.
    w.insert(
        editor.root,
        Semantics {
            readonly: true,
            ..Default::default()
        },
    );
    assert_eq!(try_activate(w, all), ActivationResult::Accepted);
    apply_find_bars(w);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text(),
        "é and é"
    );
    w.insert(editor.root, Semantics::default());
    // A queued click is retired while detached, never resurrected later.
    assert_eq!(try_activate(w, all), ActivationResult::Accepted);
    detach_source(w, editor.editor).unwrap();
    apply_find_bars(w);
    assert_eq!(w.get::<Label>(bar.status).unwrap().text, "Source detached");
    attach_source(w, editor.editor, id).unwrap();
    // Same document and revision are insufficient: this is a different attachment.
    assert_eq!(try_activate(w, all), ActivationResult::Accepted);
    apply_find_bars(w);
    assert_eq!(w.get::<Label>(bar.status).unwrap().text, "Source detached");
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text(),
        "é and é"
    );
}
