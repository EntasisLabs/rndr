//! Headless hotkey parsing, registry, and event delivery.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rndr_core::hotkey::{
    self, platform_accel, HotkeyBackend, HotkeyFailure, HotkeyId, PlatformAccel,
};
use rndr_core::{
    events, window, Axis, CapturesKeys, Chrome, Color, Focus, Focusable, GlobalHotkey,
    HotkeyBinding, HotkeyError, HotkeyRejected, Input, KeyCode, KeyEvent, Keystroke, Label, Layout,
    LogicalKey, Modifiers, NamedKey, Platform, Sim, Size, TextField, Viewport, WindowEffect,
};

struct FakeInner {
    live: HashSet<u32>,
    calls: u32,
    unregisters: u32,
    fail_next: Option<HotkeyFailure>,
    presses: Vec<HotkeyId>,
}

struct Fake {
    inner: Arc<Mutex<FakeInner>>,
}

impl Fake {
    fn new() -> (Self, Arc<Mutex<FakeInner>>) {
        let inner = Arc::new(Mutex::new(FakeInner {
            live: HashSet::new(),
            calls: 0,
            unregisters: 0,
            fail_next: None,
            presses: Vec::new(),
        }));
        (
            Self {
                inner: Arc::clone(&inner),
            },
            inner,
        )
    }
}

impl HotkeyBackend for Fake {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        let mut inner = self.inner.lock().expect("fake");
        inner.calls += 1;
        let _ = accel;
        if let Some(failure) = inner.fail_next.take() {
            return Err(failure);
        }
        if !inner.live.insert(id.get()) {
            return Err(HotkeyFailure::Taken);
        }
        Ok(())
    }

    fn unregister(&mut self, id: HotkeyId) {
        let mut inner = self.inner.lock().expect("fake");
        inner.unregisters += 1;
        inner.live.remove(&id.get());
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        std::mem::take(&mut self.inner.lock().expect("fake").presses)
    }
}

fn install(sim: &mut Sim) -> Arc<Mutex<FakeInner>> {
    let (fake, inner) = Fake::new();
    hotkey::install_backend(sim.world(), fake);
    inner
}

fn binding(text: &str) -> HotkeyBinding {
    HotkeyBinding::parse(text).expect(text)
}

#[test]
fn alt_space_encodes_every_platform() {
    let accel = platform_accel(&binding("Alt+Space"), Platform::Mac).unwrap();
    let mac = accel.macos.unwrap();
    assert_eq!(mac.key_code, 0x31);
    assert_eq!(mac.modifiers, 0x0800);
    assert!(accel.alt);
    assert!(!accel.ctrl && !accel.shift && !accel.super_key);

    let windows = accel.windows.unwrap();
    assert_eq!(windows.vk, 0x20);
    assert_eq!(windows.modifiers, 0x0001);

    let x11 = accel.x11.unwrap();
    assert_eq!(x11.keysym, 0x0020);
    assert_eq!(x11.modifiers, 1 << 3);
    assert_eq!(x11.grab_masks.len(), 8);
    let alt = 1 << 3;
    let lock = 1 << 1;
    let num = 1 << 4;
    let mod5 = 1 << 7;
    assert!(x11.grab_masks.contains(&alt));
    assert!(x11.grab_masks.contains(&(alt | lock)));
    assert!(x11.grab_masks.contains(&(alt | num)));
    assert!(x11.grab_masks.contains(&(alt | lock | num)));
    assert!(x11.grab_masks.contains(&(alt | mod5)));
    assert!(x11.grab_masks.contains(&(alt | lock | num | mod5)));
    assert!(x11
        .grab_masks
        .iter()
        .all(|mask| mask & ((1 << 0) | (1 << 2) | (1 << 6)) == 0));

    assert_eq!(accel.wayland_trigger.as_deref(), Some("ALT+space"));
}

#[test]
fn primary_follows_the_platform() {
    let parsed = binding("primary+Space");
    let mac = platform_accel(&parsed, Platform::Mac).unwrap();
    assert_eq!(mac.macos.unwrap().modifiers, 0x0100);
    assert!(mac.super_key && !mac.ctrl);
    assert_eq!(mac.wayland_trigger.as_deref(), Some("LOGO+space"));

    let windows = platform_accel(&parsed, Platform::Windows).unwrap();
    assert_eq!(windows.windows.unwrap().modifiers, 0x0002);
    assert_eq!(windows.windows.unwrap().vk, 0x20);
    assert!(windows.ctrl && !windows.super_key);
    assert_eq!(windows.wayland_trigger.as_deref(), Some("CTRL+space"));

    let x11 = platform_accel(&parsed, Platform::X11).unwrap();
    assert_eq!(x11.x11.unwrap().modifiers, 1 << 2);
    assert_eq!(x11.x11.unwrap().keysym, 0x0020);

    let wayland = platform_accel(&parsed, Platform::Wayland).unwrap();
    assert_eq!(wayland.wayland_trigger.as_deref(), Some("CTRL+space"));
    assert_eq!(
        platform_accel(&parsed, Platform::Windows).unwrap(),
        platform_accel(&binding("Ctrl+Space"), Platform::Windows).unwrap()
    );
    assert_eq!(
        platform_accel(&parsed, Platform::Mac).unwrap(),
        platform_accel(&binding("Cmd+Space"), Platform::Mac).unwrap()
    );
    assert_ne!(
        platform_accel(&parsed, Platform::Mac).unwrap(),
        platform_accel(&binding("Ctrl+Space"), Platform::Mac).unwrap()
    );
}

#[test]
fn letter_chord_uses_physical_codes() {
    let accel = platform_accel(&binding("Ctrl+Shift+K"), Platform::Windows).unwrap();
    assert_eq!(accel.windows.unwrap().vk, u32::from(b'K'));
    assert_eq!(accel.windows.unwrap().modifiers, 0x0002 | 0x0004);
    assert_eq!(accel.macos.unwrap().key_code, 0x28);
    assert_eq!(accel.x11.unwrap().keysym, u32::from(b'k'));
    assert_eq!(accel.wayland_trigger.as_deref(), Some("CTRL+SHIFT+k"));
}

#[test]
fn the_same_key_parses_from_text_and_a_keystroke() {
    let from_text = binding("alt+space");
    let from_stroke =
        HotkeyBinding::from_keystroke(Keystroke::named(NamedKey::Space).alt()).unwrap();
    let from_code = HotkeyBinding::from_keystroke(Keystroke::code(KeyCode::Space).alt()).unwrap();
    assert_eq!(
        platform_accel(&from_text, Platform::X11).unwrap(),
        platform_accel(&from_stroke, Platform::X11).unwrap()
    );
    assert_eq!(
        platform_accel(&from_stroke, Platform::X11).unwrap(),
        platform_accel(&from_code, Platform::X11).unwrap()
    );
    let letter = HotkeyBinding::from_keystroke(Keystroke::character('K').primary()).unwrap();
    let physical =
        HotkeyBinding::from_keystroke(Keystroke::code(KeyCode::letter('k').unwrap()).primary())
            .unwrap();
    assert_eq!(
        platform_accel(&letter, Platform::Mac).unwrap(),
        platform_accel(&physical, Platform::Mac).unwrap()
    );
}

#[test]
fn f21_is_invalid_on_macos_only() {
    let binding = binding("F21");
    assert_eq!(
        platform_accel(&binding, Platform::Mac).unwrap_err(),
        HotkeyError::InvalidBinding
    );
    let windows = platform_accel(&binding, Platform::Windows).unwrap();
    assert_eq!(windows.windows.unwrap().vk, 0x70 + 20);
    assert!(windows.macos.is_none());
    assert_eq!(windows.wayland_trigger.as_deref(), Some("F21"));
    assert_eq!(windows.x11.unwrap().keysym, 0xffbe + 20);
}

#[test]
fn invalid_text_is_rejected_before_a_backend() {
    for text in [
        "",
        "   ",
        "Alt+",
        "+Space",
        "Alt++Space",
        "Alt",
        "Nope",
        "Ctrl+NotAKey",
        "primary+F25",
        "Ctrl+K+L",
        "Space+Alt",
    ] {
        assert_eq!(
            HotkeyBinding::parse(text).unwrap_err(),
            HotkeyError::InvalidBinding,
            "{text}"
        );
    }
}

#[test]
fn register_without_a_backend_is_unsupported() {
    let mut sim = Sim::new();
    assert_eq!(
        hotkey::register(sim.world(), "Alt+Space").unwrap_err(),
        HotkeyError::Unsupported
    );
}

#[test]
fn registry_is_idempotent_and_releases_on_the_last_drop() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::Windows);
    let inner = install(&mut sim);
    let first = hotkey::register(sim.world(), "primary+Space").unwrap();
    let second = hotkey::register(sim.world(), "Ctrl+Space").unwrap();
    let third = hotkey::register(sim.world(), Keystroke::named(NamedKey::Space).ctrl()).unwrap();
    assert_eq!(first.id(), second.id());
    assert_eq!(second.id(), third.id());
    assert_eq!(inner.lock().unwrap().calls, 1);
    assert_eq!(inner.lock().unwrap().live.len(), 1);

    let other = hotkey::register(sim.world(), "Alt+K").unwrap();
    assert_ne!(other.id(), first.id());
    assert_eq!(inner.lock().unwrap().calls, 2);

    drop(first);
    assert_eq!(inner.lock().unwrap().live.len(), 2);
    second.unregister();
    assert!(inner.lock().unwrap().live.contains(&third.id().get()));
    drop(third);
    assert!(!inner.lock().unwrap().live.contains(&1));
    assert_eq!(inner.lock().unwrap().unregisters, 1);
    drop(other);
    assert!(inner.lock().unwrap().live.is_empty());
    assert_eq!(inner.lock().unwrap().unregisters, 2);

    let again = hotkey::register(sim.world(), "Ctrl+Space").unwrap();
    assert_eq!(inner.lock().unwrap().calls, 3);
    assert_eq!(inner.lock().unwrap().live.len(), 1);
    drop(again);
}

#[test]
fn primary_is_not_the_same_grab_on_macos() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::Mac);
    let inner = install(&mut sim);
    let primary = hotkey::register(sim.world(), "primary+Space").unwrap();
    let ctrl = hotkey::register(sim.world(), "Ctrl+Space").unwrap();
    assert_ne!(primary.id(), ctrl.id());
    assert_eq!(inner.lock().unwrap().calls, 2);
}

#[test]
fn backend_errors_map_and_do_not_stick() {
    let mut sim = Sim::new();
    let inner = install(&mut sim);
    inner.lock().unwrap().fail_next = Some(HotkeyFailure::Taken);
    assert_eq!(
        hotkey::register(sim.world(), "Alt+Space").unwrap_err(),
        HotkeyError::AlreadyTaken
    );
    assert!(inner.lock().unwrap().live.is_empty());

    inner.lock().unwrap().fail_next = Some(HotkeyFailure::Unsupported);
    assert_eq!(
        hotkey::register(sim.world(), "Alt+Space").unwrap_err(),
        HotkeyError::Unsupported
    );
    inner.lock().unwrap().fail_next = Some(HotkeyFailure::Invalid);
    assert_eq!(
        hotkey::register(sim.world(), "Alt+Space").unwrap_err(),
        HotkeyError::InvalidBinding
    );
    assert_eq!(inner.lock().unwrap().calls, 3);

    let handle = hotkey::register(sim.world(), "Alt+Space").unwrap();
    assert_eq!(inner.lock().unwrap().live.len(), 1);
    drop(handle);
    assert!(inner.lock().unwrap().live.is_empty());
}

#[test]
fn a_press_is_an_event_on_the_waking_frame() {
    let mut sim = Sim::new();
    let (fake, inner) = Fake::new();
    hotkey::install_backend(sim.world(), fake);
    let handle = hotkey::register(sim.world(), "Alt+Space").unwrap();
    let id = handle.id();
    // The fake moved into the registry. Presses go through the shared state.
    inner.lock().unwrap().presses.push(id);
    sim.update();
    let got = events::<GlobalHotkey>(sim.world());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, id);
    assert_eq!(
        platform_accel(&got[0].binding, Platform::host())
            .unwrap()
            .wayland_trigger
            .as_deref(),
        platform_accel(&binding("Alt+Space"), Platform::host())
            .unwrap()
            .wayland_trigger
            .as_deref()
    );
    assert!(window::is_visible(sim.world()));
    assert!(window::drain_effects(sim.world()).is_empty());

    sim.update();
    assert!(events::<GlobalHotkey>(sim.world()).is_empty());
    drop(handle);
}

#[test]
fn toggle_window_hides_then_shows_and_focuses() {
    let mut sim = Sim::new();
    let inner = install(&mut sim);
    let handle = hotkey::toggle_window(sim.world(), "Alt+Space").unwrap();
    inner.lock().unwrap().presses.push(handle.id());
    sim.update();
    assert!(!window::is_visible(sim.world()));
    assert_eq!(
        events::<GlobalHotkey>(sim.world())
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![handle.id()]
    );
    assert_eq!(window::drain_effects(sim.world()), vec![WindowEffect::Hide]);

    inner.lock().unwrap().presses.push(handle.id());
    sim.update();
    assert!(window::is_visible(sim.world()));
    assert!(window::is_focused(sim.world()));
    assert_eq!(
        window::drain_effects(sim.world()),
        vec![WindowEffect::Show, WindowEffect::Focus]
    );
    drop(handle);
    assert_eq!(inner.lock().unwrap().live.len(), 0);
}

#[test]
fn a_registered_hotkey_and_its_repeats_do_not_type() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::X11);
    install(&mut sim);
    sim.app.add_plugin(Chrome);
    sim.world().insert_resource(Viewport::new(240.0, 80.0));
    sim.world().insert_resource(Focus::default());
    let entity = sim.world().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("ab"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    sim.world().resource_mut::<Focus>().unwrap().entity = Some(entity);
    let _hotkey = hotkey::register(sim.world(), "Ctrl+Alt+Space").unwrap();
    sim.update();

    let chord = Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        super_key: false,
    };
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(
        KeyEvent::named(NamedKey::Space, chord).with_text(" "),
        Platform::X11,
    );
    input.push_key(
        KeyEvent::named(NamedKey::Space, chord)
            .with_text(" ")
            .repeating(),
        Platform::X11,
    );
    input.push_key(
        KeyEvent::character('z', Modifiers::default()),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "abz");

    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(
        KeyEvent::named(NamedKey::Space, Modifiers::default()),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "abz ");
}

fn spawn_field(sim: &mut Sim, text: &str) -> rndr_core::Entity {
    sim.app.add_plugin(Chrome);
    sim.world().insert_resource(Viewport::new(240.0, 80.0));
    sim.world().insert_resource(Focus::default());
    let entity = sim.world().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new(text),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    sim.world().resource_mut::<Focus>().unwrap().entity = Some(entity);
    sim.update();
    entity
}

fn space_text() -> KeyEvent {
    KeyEvent::named(NamedKey::Space, Modifiers::default()).with_text(" ")
}

#[test]
fn a_fired_hotkey_suppresses_its_base_key_until_release() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::X11);
    let inner = install(&mut sim);
    let entity = spawn_field(&mut sim, "");
    let handle = hotkey::register(sim.world(), "Ctrl+Alt+Space").unwrap();

    inner.lock().unwrap().presses.push(handle.id());
    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(space_text(), Platform::X11);
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "");

    sim.world().resource_mut::<Input>().unwrap().push_key(
        KeyEvent {
            logical: LogicalKey::Unidentified,
            text: Some(" ".to_string()),
            code: None,
            modifiers: Modifiers::default(),
            repeat: false,
            released: false,
        },
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "");

    sim.world().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::character('z', Modifiers::default()),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "z");

    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(
        KeyEvent::named(NamedKey::Escape, Modifiers::default()).released(),
        Platform::X11,
    );
    input.push_key(space_text(), Platform::X11);
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "z");

    sim.world().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "z");

    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(space_text(), Platform::X11);
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "z ");
}

#[test]
fn release_then_press_in_one_frame_types_after_the_hotkey_was_armed() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::X11);
    let inner = install(&mut sim);
    let entity = spawn_field(&mut sim, "");
    let handle = hotkey::register(sim.world(), "Alt+Space").unwrap();
    inner.lock().unwrap().presses.push(handle.id());
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "");

    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
        Platform::X11,
    );
    input.push_key(space_text(), Platform::X11);
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, " ");
}

#[test]
fn press_then_release_on_the_fire_frame_types_nothing() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::X11);
    let inner = install(&mut sim);
    let entity = spawn_field(&mut sim, "");
    let handle = hotkey::register(sim.world(), "Alt+Space").unwrap();
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(space_text(), Platform::X11);
    input.push_key(
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
        Platform::X11,
    );
    inner.lock().unwrap().presses.push(handle.id());
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "");

    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(space_text(), Platform::X11);
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, " ");
}

#[test]
fn a_letter_hotkey_suppresses_that_letter_until_release() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Platform::X11);
    let inner = install(&mut sim);
    let entity = spawn_field(&mut sim, "");
    let handle = hotkey::register(sim.world(), "Ctrl+K").unwrap();
    inner.lock().unwrap().presses.push(handle.id());
    sim.world().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::character('k', Modifiers::default()),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "");

    let input = sim.world().resource_mut::<Input>().unwrap();
    input.push_key(
        KeyEvent::character('k', Modifiers::default()).released(),
        Platform::X11,
    );
    input.push_key(
        KeyEvent::character('k', Modifiers::default()),
        Platform::X11,
    );
    sim.update();
    assert_eq!(sim.world().get::<Label>(entity).unwrap().text, "k");
}

#[test]
fn note_unregistered_keeps_the_error_for_the_app() {
    let mut sim = Sim::new();
    hotkey::note_unregistered(sim.world(), "Alt+Space", HotkeyError::AlreadyTaken);
    let notice = sim.world().resource::<HotkeyRejected>().unwrap();
    assert_eq!(notice.binding, "Alt+Space");
    assert_eq!(notice.error, HotkeyError::AlreadyTaken);
    assert!(events::<HotkeyRejected>(sim.world()).is_empty());
    sim.update();
    let got = events::<HotkeyRejected>(sim.world());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].binding, "Alt+Space");
    assert_eq!(got[0].error, HotkeyError::AlreadyTaken);
}

#[test]
fn a_dropped_id_does_not_deliver() {
    let mut sim = Sim::new();
    let inner = install(&mut sim);
    let handle = hotkey::register(sim.world(), "Alt+Space").unwrap();
    let id = handle.id();
    drop(handle);
    inner.lock().unwrap().presses.push(id);
    sim.update();
    assert!(events::<GlobalHotkey>(sim.world()).is_empty());
}
