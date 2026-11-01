//! Promoted missing code=None cases from independent04642 review.
use rndr_core::*;
struct Backend;
impl hotkey::HotkeyBackend for Backend {
    fn register(
        &mut self,
        _: hotkey::HotkeyId,
        _: &hotkey::PlatformAccel,
    ) -> Result<(), hotkey::HotkeyFailure> {
        Ok(())
    }
    fn unregister(&mut self, _: hotkey::HotkeyId) {}
    fn poll(&mut self) -> Vec<hotkey::HotkeyId> {
        vec![]
    }
}
fn logical(k: NamedKey) -> KeyEvent {
    let mut e = KeyEvent::named(k, Modifiers::new());
    e.code = None;
    assert!(e.code.is_none());
    e
}
fn fixture() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(GenericInteraction);
    hotkey::install_backend(a.world_mut(), Backend);
    let b = a
        .world_mut()
        .spawn((Layout::column(), Button::new("go"), ReleaseActivation));
    a.update();
    assert!(request_focus(a.world_mut(), b));
    (a, b)
}
fn frame(a: &mut App, events: Vec<KeyEvent>) {
    a.world_mut().insert_resource(Input::default());
    for e in events {
        a.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .push_key(e, Platform::Mac);
    }
    a.update();
}
fn count(a: &mut App, n: usize) {
    assert_eq!(take_action_intents(a.world_mut()).len(), n);
    assert!(a.world().resource::<Fired>().unwrap().name.is_none());
}
#[test]
fn true_logical_only_nonmatching_registration_matrix_160() {
    for p in [
        Platform::Mac,
        Platform::Windows,
        Platform::X11,
        Platform::Wayland,
    ] {
        for k in [NamedKey::Space, NamedKey::Enter] {
            for split in [false, true] {
                for chord in [
                    "Alt+Space",
                    "Ctrl+Space",
                    "Alt+Enter",
                    "Ctrl+Enter",
                    "primary+Space",
                ] {
                    let (mut a, _) = fixture();
                    a.world_mut().insert_resource(p);
                    let _h = hotkey::register(a.world(), chord).unwrap();
                    let d = logical(k);
                    if split {
                        frame(&mut a, vec![d.clone()]);
                        frame(&mut a, vec![d.released()]);
                    } else {
                        frame(&mut a, vec![d.clone(), d.released()]);
                    }
                    count(&mut a, 1);
                }
            }
        }
    }
}
#[test]
fn physical_binding_and_registration_drop_do_not_own_logical_release() {
    for k in [NamedKey::Space, NamedKey::Enter] {
        let (mut a, _) = fixture();
        let h = hotkey::register(
            a.world(),
            Keystroke::code(if k == NamedKey::Space {
                KeyCode::Space
            } else {
                KeyCode::Enter
            })
            .alt(),
        )
        .unwrap();
        frame(&mut a, vec![logical(k)]);
        drop(h);
        frame(&mut a, vec![logical(k).released()]);
        count(&mut a, 1);
    }
}
#[test]
fn true_logical_focus_roundtrip_repeat_and_modified_up_cancel() {
    let (mut a, b) = fixture();
    let _h = hotkey::register(a.world(), "Alt+Space").unwrap();
    let other = a
        .world_mut()
        .spawn((Layout::column(), Button::new("other"), ReleaseActivation));
    frame(&mut a, vec![logical(NamedKey::Space)]);
    request_focus(a.world_mut(), other);
    request_focus(a.world_mut(), b);
    frame(&mut a, vec![logical(NamedKey::Space).released()]);
    count(&mut a, 0);
    frame(
        &mut a,
        vec![
            logical(NamedKey::Space),
            logical(NamedKey::Space).released().repeating(),
        ],
    );
    count(&mut a, 0);
    frame(&mut a, vec![logical(NamedKey::Space).released()]);
    count(&mut a, 0);
    frame(&mut a, vec![logical(NamedKey::Space)]);
    let mut up = logical(NamedKey::Space).released();
    up.modifiers.ctrl = true;
    frame(&mut a, vec![up]);
    frame(&mut a, vec![logical(NamedKey::Space).released()]);
    count(&mut a, 0);
    frame(
        &mut a,
        vec![
            logical(NamedKey::Space),
            logical(NamedKey::Space).released(),
        ],
    );
    count(&mut a, 1);
}
