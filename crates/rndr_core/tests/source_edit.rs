use rndr_core::{App, Buffer, CapturesKeys, Chrome, Editor, Focus, Input, KeyEvent, Label, Modifiers, Platform};

fn boot() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Platform::Windows);
    let entity = app.world_mut().spawn((Buffer::from_str("- [ ] task"), Editor::default(), Label::new(""), CapturesKeys));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn press(app: &mut App, key: char) {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys.push(KeyEvent::character(key, modifiers));
    app.update();
}

#[test]
fn source_range_edit_shares_keyboard_undo() {
    let (mut app, entity) = boot();
    app.world_mut().resource_mut::<Input>().unwrap().keys.push(KeyEvent::character('!', Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "- [ ] task!");
    // Existing public source-range mutation path, as used by a task control.
    let buffer = app.world_mut().get_mut::<Buffer>(entity).unwrap();
    buffer.delete_range(3, 4);
    buffer.insert_str(3, "x");
    assert_eq!(buffer.text(), "- [x] task!");
    press(&mut app, 'z');
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "- [ ] task!", "undo the source operation before unrelated typing");
    press(&mut app, 'z');
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "- [ ] task");
    press(&mut app, 'y');
    press(&mut app, 'y');
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "- [x] task!");
}
