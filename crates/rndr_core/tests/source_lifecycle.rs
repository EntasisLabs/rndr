use rndr_core::editing::*;
use rndr_core::*;

fn view(app: &mut App, text: &str) -> (Entity, DocumentId) {
    let id = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open(text);
    let e = app.world_mut().spawn((
        Layout::column(),
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
        Focusable,
    ));
    attach_source(app.world_mut(), e, id).unwrap();
    (e, id)
}
fn boot() -> App {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(SourceDocuments::default());
    app.world_mut().insert_resource(Viewport::new(600.0, 400.0));
    app
}
fn focus(app: &mut App, e: Entity) {
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
}
fn preedit(app: &mut App, e: Entity) {
    focus(app, e);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
}
fn late_commit(app: &mut App) {
    let i = app.world_mut().resource_mut::<Input>().unwrap();
    i.ime_preedit.clear();
    i.ime_commit = "日".into();
    // Native adapters may publish duplicate key text alongside the commit.
    i.chars = "日".into();
    i.keys.push(KeyEvent::character('日', Modifiers::new()));
    app.update();
}
fn text(app: &App, id: DocumentId) -> &str {
    app.world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .text()
}
#[test]
fn dead_owner_cannot_retarget_commit_even_if_document_is_reattached() {
    for reattach in [false, true] {
        let mut app = boot();
        let (a, aid) = view(&mut app, "A");
        let (b, bid) = view(&mut app, "B");
        preedit(&mut app, a);
        app.world_mut().despawn_entity(a);
        if reattach {
            let replacement =
                app.world_mut()
                    .spawn((Buffer::from_str(""), Editor::default(), Label::new("")));
            attach_source(app.world_mut(), replacement, aid).unwrap();
            focus(&mut app, replacement);
        } else {
            focus(&mut app, b);
        }
        late_commit(&mut app);
        assert_eq!(text(&app, aid), "A");
        assert_eq!(text(&app, bid), "B");
        let docs = app.world_mut().resource_mut::<SourceDocuments>().unwrap();
        let a = docs.get_mut(aid).unwrap();
        assert!(!a.is_composing());
        assert!(a.reload(a.revision(), "A!").is_ok());
        assert_eq!(docs.get(bid).unwrap().revision(), Revision(0));
    }
}
#[test]
fn readonly_focus_filter_cannot_turn_preedit_into_native_cancellation() {
    let mut app = boot();
    let (a, aid) = view(&mut app, "A");
    let readonly = app.world_mut().spawn((
        Layout::column(),
        Label::new("readonly"),
        TextField,
        Focusable,
        Semantics {
            readonly: true,
            ..Semantics::default()
        },
    ));
    let (c, cid) = view(&mut app, "C");
    preedit(&mut app, a);
    focus(&mut app, readonly);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    assert!(app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(aid)
        .unwrap()
        .is_composing());
    focus(&mut app, c);
    late_commit(&mut app);
    assert_eq!(text(&app, aid), "A");
    assert_eq!(text(&app, cid), "C");
    assert!(!app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(aid)
        .unwrap()
        .is_composing());
}
#[test]
fn blur_cancels_composition_and_releases_host_edit_admission() {
    let mut app = boot();
    let (a, aid) = view(&mut app, "A");
    preedit(&mut app, a);
    send(app.world_mut(), WindowFocus::Blurred);
    late_commit(&mut app);
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(aid)
        .unwrap();
    assert_eq!(doc.text(), "A");
    assert!(!doc.is_composing());
    assert!(doc.reload(doc.revision(), "A!").is_ok());
}

#[test]
fn owner_destroyed_during_host_update_cannot_deliver_commit_to_new_focus() {
    let mut app = boot();
    let (a, aid) = view(&mut app, "A");
    let (b, bid) = view(&mut app, "B");
    preedit(&mut app, a);
    app.world_mut().insert_resource((a, b));
    app.add_system(Schedule::Update, |world| {
        let (a, b) = *world.resource::<(Entity, Entity)>().unwrap();
        world.despawn_entity(a);
        world.resource_mut::<Focus>().unwrap().entity = Some(b);
    });
    late_commit(&mut app);
    assert_eq!(text(&app, aid), "A");
    assert_eq!(text(&app, bid), "B");
    assert!(!app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(aid)
        .unwrap()
        .is_composing());
}

#[test]
fn typing_and_undo_resume_after_orphan_commit_without_stale_input() {
    let mut app = boot();
    app.world_mut().insert_resource(Platform::Windows);
    let (a, aid) = view(&mut app, "A");
    let (b, bid) = view(&mut app, "B");
    preedit(&mut app, a);
    app.world_mut().despawn_entity(a);
    focus(&mut app, b);
    // While native preedit persists the dead document is already unlocked,
    // but the tombstone still prevents a future commit reaching B.
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    let d = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(aid)
        .unwrap();
    assert!(!d.is_composing());
    d.reload(d.revision(), "A!").unwrap();
    late_commit(&mut app);
    assert_eq!(text(&app, bid), "B");
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('x', Modifiers::new()));
    app.update();
    assert_eq!(text(&app, bid), "Bx");
    let mut command = Modifiers::new();
    command.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('z', command));
    app.update();
    assert_eq!(text(&app, bid), "B");
    assert_eq!(text(&app, aid), "A!");
}

#[test]
fn candidate_tab_escape_and_page_keys_never_traverse_or_mutate_the_source() {
    let mut app = boot();
    let (a, aid) = view(&mut app, "A");
    let (_, _) = view(&mut app, "B");
    preedit(&mut app, a);
    for named in [
        NamedKey::Tab,
        NamedKey::Escape,
        NamedKey::PageDown,
        NamedKey::Home,
    ] {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit = "に".into();
        input.keys.push(KeyEvent::named(named, Modifiers::new()));
        app.update();
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(a));
        assert_eq!(text(&app, aid), "A");
        assert!(app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(aid)
            .unwrap()
            .is_composing());
    }
    late_commit(&mut app);
    assert_eq!(text(&app, aid), "A日");
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(aid)
        .unwrap();
    assert_eq!(doc.revision(), Revision(1));
    doc.undo().unwrap();
    assert_eq!(doc.text(), "A");
}
#[test]
fn source_tab_and_popup_dismissal_tab_traverse_in_both_directions() {
    for popup in [false, true] {
        for shift in [false, true] {
            let mut app = boot();
            let before = app.world_mut().spawn((Layout::column(), Focusable));
            let (a, aid) = view(&mut app, "A");
            let after = app.world_mut().spawn((Layout::column(), Focusable));
            app.update();
            focus(&mut app, a);
            let root = popup.then(|| {
                let root = app.world_mut().spawn((
                    Layout::column(),
                    Hidden,
                    Focusable,
                    AnchoredPopup {
                        anchor: a,
                        max_height: 80.0,
                        closed: |_, _| {},
                    },
                ));
                assert!(open_popup(app.world_mut(), root));
                focus(&mut app, root);
                root
            });
            let mut mods = Modifiers::new();
            mods.shift = shift;
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .keys
                .push(KeyEvent::named(NamedKey::Tab, mods));
            app.update();
            assert_eq!(
                app.world().resource::<Focus>().unwrap().entity,
                Some(if shift { before } else { after })
            );
            assert_eq!(text(&app, aid), "A");
            if let Some(root) = root {
                assert!(app.world().has::<Hidden>(root));
            }
        }
    }
}
