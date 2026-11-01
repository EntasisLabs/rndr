//! Regression for independent review of 01da8acd: registration is not ownership.
use rndr_core::*;
use std::sync::{Arc, Mutex};

type Edges = Arc<Mutex<Vec<hotkey::HotkeyEdge>>>;
struct Backend(Edges);
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
    fn poll_edges(&mut self) -> Vec<hotkey::HotkeyEdge> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}
fn setup() -> (App, Entity, Edges) {
    let mut a = App::new();
    a.add_plugin(GenericInteraction);
    a.world_mut().insert_resource(Platform::Mac);
    let edges = Edges::default();
    hotkey::install_backend(a.world_mut(), Backend(edges.clone()));
    let b = a
        .world_mut()
        .spawn((Layout::column(), Button::new("go"), ReleaseActivation));
    a.update();
    assert!(request_focus(a.world_mut(), b));
    (a, b, edges)
}
fn key(k: NamedKey) -> KeyEvent {
    KeyEvent::named(k, Modifiers::new())
}
fn frame(a: &mut App, keys: Vec<KeyEvent>) {
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.keys.clear();
    i.key = None;
    i.chars.clear();
    for e in keys {
        i.push_key(e, Platform::Mac);
    }
    a.update();
}
fn assert_count(a: &mut App, n: usize) {
    assert_eq!(take_action_intents(a.world_mut()).len(), n);
    assert!(a.world().resource::<Fired>().unwrap().name.is_none());
}
fn ordinary(same_frame: bool, registered: bool, k: NamedKey) {
    // Both logical-only and physical-code events; multiple modifier chords,
    // including another base key, must not claim this unmodified gesture.
    for chord in ["Alt+Space", "Ctrl+Space", "Alt+Enter", "Ctrl+Enter"] {
        for physical in [false, true] {
            let (mut a, _, _) = setup();
            let _binding = registered.then(|| hotkey::register(a.world_mut(), chord).unwrap());
            let mut down = key(k);
            if physical {
                down.code = Some(if k == NamedKey::Space {
                    KeyCode::Space
                } else {
                    KeyCode::Enter
                });
            } else {
                down.code = None;
            }
            assert_eq!(down.code.is_some(), physical);
            assert_eq!(down.code == key(k).code, physical);
            let up = down.clone().released();
            if same_frame {
                frame(&mut a, vec![down, up]);
            } else {
                frame(&mut a, vec![down]);
                frame(&mut a, vec![up]);
            }
            assert_count(&mut a, 1);
            frame(&mut a, vec![key(k).released()]);
            assert_count(&mut a, 0);
        }
    }
}
#[test]
fn nonmatching_space_same_frame() {
    ordinary(true, true, NamedKey::Space);
}
#[test]
fn nonmatching_space_split_frame() {
    ordinary(false, true, NamedKey::Space);
}
#[test]
fn nonmatching_enter_same_frame() {
    ordinary(true, true, NamedKey::Enter);
}
#[test]
fn nonmatching_enter_split_frame() {
    ordinary(false, true, NamedKey::Enter);
}
#[test]
fn no_registration_space_controls() {
    ordinary(true, false, NamedKey::Space);
    ordinary(false, false, NamedKey::Space);
}
#[test]
fn no_registration_enter_controls() {
    ordinary(true, false, NamedKey::Enter);
    ordinary(false, false, NamedKey::Enter);
}

#[test]
fn matching_chords_and_matching_release_cancel_stale_arm() {
    for k in [NamedKey::Space, NamedKey::Enter] {
        for release_only in [false, true] {
            let (mut a, _, _) = setup();
            let _binding = hotkey::register(
                a.world_mut(),
                if k == NamedKey::Space {
                    "Alt+Space"
                } else {
                    "Alt+Enter"
                },
            )
            .unwrap();
            frame(&mut a, vec![key(NamedKey::Space)]);
            let mut chord = key(k);
            chord.modifiers.alt = true;
            if release_only {
                frame(&mut a, vec![chord.released()]);
            } else {
                frame(
                    &mut a,
                    vec![chord.clone(), chord.repeating(), key(k).released()],
                );
            }
            frame(&mut a, vec![key(NamedKey::Space).released()]);
            assert_count(&mut a, 0);
            frame(
                &mut a,
                vec![key(NamedKey::Space), key(NamedKey::Space).released()],
            );
            assert_count(&mut a, 1);
        }
    }
}

#[test]
fn active_guard_owns_release_before_clear_then_allows_fresh_gesture() {
    for k in [NamedKey::Space, NamedKey::Enter] {
        let (mut a, _, edges) = setup();
        let binding = hotkey::register(
            a.world_mut(),
            if k == NamedKey::Space {
                "Alt+Space"
            } else {
                "Alt+Enter"
            },
        )
        .unwrap();
        // A different Space arm must also be canceled by an owned Enter edge.
        frame(&mut a, vec![key(NamedKey::Space)]);
        hotkey::note_key_order(a.world_mut(), 10);
        edges
            .lock()
            .unwrap()
            .push(hotkey::HotkeyEdge::Press(binding.id()));
        let mut stale = key(k).released();
        stale.order = 9;
        frame(
            &mut a,
            vec![stale.clone().synthetic(), stale, key(k).repeating()],
        );
        assert_count(&mut a, 0);
        let mut up = key(k).released();
        up.order = 11;
        hotkey::note_key_order(a.world_mut(), 11);
        frame(&mut a, vec![up]);
        frame(&mut a, vec![key(NamedKey::Space).released()]);
        assert_count(&mut a, 0);
        frame(&mut a, vec![key(k), key(k).released()]);
        assert_count(&mut a, 1);
    }
}

#[test]
fn nonmatching_registration_survives_traversal_and_focus_cancellation() {
    let (mut a, b, _) = setup();
    let _binding = hotkey::register(a.world_mut(), "Alt+Space").unwrap();
    a.world_mut().resource_mut::<Focus>().unwrap().entity = None;
    frame(
        &mut a,
        vec![
            key(NamedKey::Tab),
            key(NamedKey::Space),
            key(NamedKey::Space).released(),
        ],
    );
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(b));
    assert_count(&mut a, 1);
    let other = a
        .world_mut()
        .spawn((Layout::column(), Button::new("other")));
    frame(&mut a, vec![key(NamedKey::Space)]);
    request_focus(a.world_mut(), other);
    request_focus(a.world_mut(), b);
    frame(&mut a, vec![key(NamedKey::Space).released()]);
    assert_count(&mut a, 0);
}

#[test]
fn nonmatching_registration_preserves_legacy_enter_and_editor_space() {
    let (mut a, _, _) = setup();
    let _space = hotkey::register(a.world_mut(), "Alt+Space").unwrap();
    let _enter = hotkey::register(a.world_mut(), "Alt+Enter").unwrap();
    let legacy = a
        .world_mut()
        .spawn((Layout::column(), Button::new("legacy")));
    request_focus(a.world_mut(), legacy);
    frame(
        &mut a,
        vec![key(NamedKey::Enter), key(NamedKey::Enter).released()],
    );
    assert_eq!(
        a.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("legacy")
    );
    assert!(take_action_intents(a.world_mut()).is_empty());
    let field = a
        .world_mut()
        .spawn((Layout::column(), TextField, CapturesKeys, Label::new("")));
    request_focus(a.world_mut(), field);
    frame(
        &mut a,
        vec![
            key(NamedKey::Space).with_text(" "),
            key(NamedKey::Space).released(),
        ],
    );
    assert_eq!(a.world().get::<Label>(field).unwrap().text, " ");
    assert!(take_action_intents(a.world_mut()).is_empty());
    assert!(a.world().resource::<Input>().unwrap().keys.is_empty());
}

#[test]
fn nonmatching_registration_does_not_validate_synthetic_or_repeated_releases() {
    for release in [
        key(NamedKey::Space).released().synthetic(),
        key(NamedKey::Space).released().repeating(),
    ] {
        let (mut a, _, _) = setup();
        let _binding = hotkey::register(a.world_mut(), "Alt+Space").unwrap();
        frame(&mut a, vec![key(NamedKey::Space), release]);
        frame(&mut a, vec![key(NamedKey::Space).released()]);
        assert_count(&mut a, 0);
    }
}
