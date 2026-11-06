//! Offline reference application. All learning/provider logic stays outside rndr.
use rndr_app::{button, child, column, field, label, navigate, Preferences, ViewState, INSET};
use rndr_core::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Versioned, deliberately nonsecret reference settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Schema version; unknown versions are rejected, not overwritten on load.
    pub version: u8,
    /// A local display name, not a credential.
    pub display_name: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            display_name: "Desktop explorer".into(),
        }
    }
}
impl Settings {
    fn validate(&self) -> bool {
        self.version == 1
            && !self.display_name.trim().is_empty()
            && self.display_name.len() <= 80
            && !self.display_name.chars().any(char::is_control)
    }
}

#[derive(Clone, Copy)]
struct Ui {
    home: Entity,
    settings: Entity,
    status: Entity,
    name: Entity,
    notice: Entity,
    start: Entity,
    cancel: Entity,
    retry: Entity,
    fail: Entity,
    save: Entity,
}
struct Model {
    committed: Settings,
    store: Option<Preferences>,
    state: ViewState,
    task: Option<TaskHandle>,
    epoch: u64,
    notice: String,
}
#[derive(Clone)]
struct Finished {
    epoch: u64,
    failed: bool,
}
impl Event for Finished {}

/// Install the example in an existing App (or Sim's app). None keeps preferences
/// entirely in memory. Install before the first update. No network is used.
pub fn install(app: &mut App, store: Option<Preferences>) {
    let (committed, notice) = match store.as_ref().map(|p| p.load::<Settings>()) {
        Some(Ok(Some(settings))) if settings.validate() => (settings, String::new()),
        Some(Ok(None)) | None => (Settings::default(), String::new()),
        _ => (
            Settings::default(),
            "Could not load preferences. Defaults are in use; saving replaces the file.".into(),
        ),
    };
    app.add_plugin(rndr_app::FormNavigation);
    app.add_system_front(Schedule::Input, apply_scroll);
    app.add_system_front(Schedule::Input, apply_scrollbars);
    app.world_mut().insert_resource(Model {
        committed,
        store,
        state: ViewState::Empty,
        task: None,
        epoch: 0,
        notice,
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
}

fn setup(world: &mut World) {
    let theme = theme_of(world);
    let root = world.spawn((
        Layout {
            padding: INSET,
            ..column(Size::Fill)
        },
        theme.canvas,
    ));
    label(world, root, "Desktop Workshop", true);
    label(
        world,
        root,
        "An offline starter • Tab / Shift-Tab to move • Enter to activate",
        false,
    );
    let nav = world.spawn((Layout {
        height: Size::Hug,
        ..Layout::row()
    },));
    child(world, root, nav);
    button(world, nav, "home", "Workspace");
    button(world, nav, "settings", "Settings");
    let home = world.spawn((column(Size::Fill), Scroll::default()));
    child(world, root, home);
    label(world, home, "Make yourself at home", true);
    label(world, home, "This example uses real rndr fields, focus, scrolling, tasks, text shaping and accessibility. Nothing is sent over the network.", false);
    let status = label(world, home, "", false);
    let start = button(world, home, "start", "Start local sample");
    let fail = button(world, home, "fail", "Simulate a recoverable error");
    let cancel = button(world, home, "cancel", "Cancel current work");
    let retry = button(world, home, "retry", "Retry locally (succeeds)");
    label(world, home, "A practical desktop checklist", true);
    for text in [
        "01 • Text: edit a nonsecret display name in Settings. Unicode is welcome: café, 日本語, 👋.",
        "02 • Focus: use Tab and Shift-Tab, then Enter. The focused control gets a contrasting fill.",
        "03 • Tasks: start, cancel before completion, simulate an error, then retry. No hidden provider or paid calls.",
        "04 • Resize: narrow the window, scroll this page, and reach every control without horizontal scrolling.",
        "05 • Preferences: Save commits the draft; Cancel discards it. Restart restores only saved nonsecret settings.",
        "06 • Accessibility: the core tree exposes buttons and fields. Screen-reader quality still needs native review.",
        "07 • Native quality: IME, clipboard, multiple displays and window lifecycle need real desktop acceptance.",
        "08 • Distribution: the local macOS bundle launches without Rust or Cargo. Signing is a separate release step.",
    ] { label(world, home, text, false); }
    let settings = world.spawn((column(Size::Fill), Scroll::default(), Hidden));
    child(world, root, settings);
    label(world, settings, "Preferences", true);
    label(world, settings, "Local, nonsecret preferences only. Never enter passwords, API keys or personal secrets here.", false);
    let value = world
        .resource::<Model>()
        .unwrap()
        .committed
        .display_name
        .clone();
    let name = field(world, settings, "Display name (1–80 UTF-8 bytes)", &value);
    let notice = label(world, settings, "", false);
    let save = button(world, settings, "save", "Save preferences");
    button(world, settings, "discard", "Cancel changes");
    label(
        world,
        settings,
        "Changes take effect only after a successful save. Save never starts background work.",
        false,
    );
    world.insert_resource(Ui {
        home,
        settings,
        status,
        name,
        notice,
        start,
        cancel,
        retry,
        fail,
        save,
    });
    world.resource_mut::<Focus>().unwrap().entity = Some(start);
}

fn action(world: &mut World, action: &str) {
    let ui = *world.resource::<Ui>().unwrap();
    match action {
        "home" | "discard" => {
            let name = world
                .resource::<Model>()
                .unwrap()
                .committed
                .display_name
                .clone();
            reset_field_draft(world, ui.name, &name);
            navigate(world, &[ui.home, ui.settings], ui.home, ui.start);
        }
        "settings" => {
            navigate(world, &[ui.home, ui.settings], ui.settings, ui.name);
        }
        "save" => {
            let candidate = Settings {
                version: 1,
                display_name: world.get::<Label>(ui.name).unwrap().text.trim().into(),
            };
            if !candidate.validate() {
                world.resource_mut::<Model>().unwrap().notice =
                    "Use 1–80 UTF-8 bytes without control characters.".into();
                return;
            }
            if world
                .resource::<Model>()
                .unwrap()
                .store
                .as_ref()
                .is_some_and(|p| p.save(&candidate).is_err())
            {
                world.resource_mut::<Model>().unwrap().notice = "Save failed. Check disk space and permissions. Your saved preferences are unchanged.".into();
                return;
            }
            let model = world.resource_mut::<Model>().unwrap();
            model.committed = candidate;
            model.notice = "Preferences saved. No background work was started.".into();
            request_focus(world, ui.save);
        }
        "start" | "fail" | "retry" => {
            if world.resource::<Model>().unwrap().task.is_some() {
                return;
            }
            let model = world.resource_mut::<Model>().unwrap();
            model.epoch += 1;
            let epoch = model.epoch;
            model.state = ViewState::Loading;
            let failed = action == "fail";
            let task = spawn(world, async move {
                sleep(Duration::from_millis(1200)).await;
                Finished { epoch, failed }
            });
            world.resource_mut::<Model>().unwrap().task = Some(task);
        }
        "cancel" => {
            let model = world.resource_mut::<Model>().unwrap();
            if model.task.take().is_some() {
                model.epoch += 1;
                model.state = ViewState::Cancelled;
            }
        }
        _ => {}
    }
}
fn update(world: &mut World) {
    for intent in take_action_intents(world) {
        // Earlier actions can hide or disable later queued targets.
        if access_details(world, intent.entity).activate {
            action(world, &intent.name);
        }
    }
    // Cancellation/action is processed first: even an already delivered result
    // cannot overwrite a same-frame cancel or newer operation.
    for done in events::<Finished>(world).to_vec() {
        let model = world.resource_mut::<Model>().unwrap();
        if done.epoch != model.epoch || model.task.is_none() {
            continue;
        }
        model.task = None;
        model.state = if done.failed {
            ViewState::Error(
                "The simulated operation failed. Nothing was changed. Retry is safe.".into(),
            )
        } else {
            ViewState::Ready(format!(
                "Welcome, {}. Local work completed successfully.",
                model.committed.display_name
            ))
        };
    }
    let ui = *world.resource::<Ui>().unwrap();
    let model = world.resource::<Model>().unwrap();
    let state = model.state.clone();
    let busy = model.task.is_some();
    let notice = model.notice.clone();
    world.get_mut::<Label>(ui.status).unwrap().text = state.message().into();
    world.get_mut::<Label>(ui.notice).unwrap().text = notice;
    for (entity, show) in [
        (ui.cancel, busy),
        (ui.retry, matches!(state, ViewState::Error(_))),
    ] {
        if show {
            world.remove::<Hidden>(entity);
        } else {
            world.insert(entity, Hidden);
        }
    }
    for entity in [ui.start, ui.fail] {
        world.get_mut::<Button>(entity).unwrap().enabled = !busy;
    }
    // Reuse Theme colors, no separate renderer/decoration path. Focus is
    // intentionally visible even on a disabled button.
    let theme = theme_of(world);
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    for entity in world.entities_with::<Button>() {
        let enabled = world.get::<Button>(entity).unwrap().enabled;
        world.insert(
            entity,
            if focus == Some(entity) {
                theme.selection
            } else if enabled {
                theme.title
            } else {
                theme.pane
            },
        );
    }
    world.insert(
        ui.name,
        if focus == Some(ui.name) {
            theme.title
        } else {
            theme.pane
        },
    );
    // A newly hidden cancel/retry must not hold focus.
    if focus.is_some_and(|f| world.has::<Hidden>(f)) {
        request_focus(world, ui.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boot() -> Sim {
        let mut sim = Sim::new();
        install(&mut sim.app, None);
        sim.world().insert_resource(Viewport::new(880.0, 760.0));
        sim.update();
        sim.update();
        sim
    }
    fn press(sim: &mut Sim, action: &str) {
        let target = sim
            .world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| sim.world().get::<Button>(*e).unwrap().action == action)
            .unwrap();
        sim.world().resource_mut::<Focus>().unwrap().entity = Some(target);
        sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Enter);
        sim.update();
    }
    #[test]
    fn real_keyboard_navigation_edit_save_and_discard() {
        let mut sim = boot();
        press(&mut sim, "settings");
        let ui = *sim.world().resource::<Ui>().unwrap();
        assert!(sim.world().has::<Hidden>(ui.home));
        assert_eq!(
            sim.world().resource::<Focus>().unwrap().entity,
            Some(ui.name)
        );
        sim.world().resource_mut::<Input>().unwrap().chars.push('é');
        sim.update();
        assert!(sim
            .world()
            .get::<Label>(ui.name)
            .unwrap()
            .text
            .ends_with('é'));
        sim.world().resource_mut::<Input>().unwrap().key = Some(Key::Tab);
        sim.update();
        assert_eq!(
            sim.world().resource::<Focus>().unwrap().entity,
            Some(ui.save)
        );
        press(&mut sim, "save");
        assert!(sim
            .world()
            .resource::<Model>()
            .unwrap()
            .committed
            .display_name
            .ends_with('é'));
        assert!(sim.world().resource::<Model>().unwrap().task.is_none());
        sim.world().get_mut::<Label>(ui.name).unwrap().text = "unsaved".into();
        press(&mut sim, "discard");
        assert_ne!(sim.world().get::<Label>(ui.name).unwrap().text, "unsaved");
        assert!(sim.world().has::<Hidden>(ui.settings));
    }
    fn queued(sim: &mut Sim, keys: impl IntoIterator<Item = KeyEvent>) {
        let platform = platform_of(sim.world());
        for key in keys {
            sim.world()
                .resource_mut::<Input>()
                .unwrap()
                .push_key(key, platform);
        }
        sim.update();
        // Sim does not present a window frame. Mirror the runner's clearing
        // of unconsumed keys (notably Enter on a button) before the next frame.
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys.clear();
        input.chars.clear();
        input.key = None;
    }
    fn command(ch: char, shift: bool) -> KeyEvent {
        let mut modifiers = Modifiers::new();
        modifiers.super_key = true;
        modifiers.shift = shift;
        KeyEvent::character(ch, modifiers)
    }
    fn queued_press(sim: &mut Sim, action: &str) {
        let target = sim
            .world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| sim.world().get::<Button>(*e).unwrap().action == action)
            .unwrap();
        sim.world().resource_mut::<Focus>().unwrap().entity = Some(target);
        queued(sim, [KeyEvent::named(NamedKey::Enter, Modifiers::new())]);
    }
    #[test]
    fn queued_final_character_tab_enter_saves_complete_draft() {
        let mut sim = boot();
        queued_press(&mut sim, "settings");
        let ui = *sim.world().resource::<Ui>().unwrap();
        queued(
            &mut sim,
            [
                KeyEvent::character('é', Modifiers::new()),
                KeyEvent::named(NamedKey::Tab, Modifiers::new()),
                KeyEvent::named(NamedKey::Enter, Modifiers::new()),
            ],
        );
        assert_eq!(
            sim.world().get::<Label>(ui.name).unwrap().text,
            "Desktop exploreré"
        );
        assert_eq!(
            sim.world()
                .resource::<Model>()
                .unwrap()
                .committed
                .display_name,
            "Desktop exploreré"
        );
        assert_eq!(
            sim.world().resource::<Focus>().unwrap().entity,
            Some(ui.save)
        );
    }
    #[test]
    fn discarded_queued_edits_cannot_return_through_undo_or_redo() {
        // Exercise both a nonempty undo stack and a nonempty redo stack at reset.
        for undo_before_discard in [false, true] {
            let mut sim = boot();
            sim.world().insert_resource(Platform::Mac);
            queued_press(&mut sim, "settings");
            let ui = *sim.world().resource::<Ui>().unwrap();
            queued(&mut sim, [KeyEvent::character('X', Modifiers::new())]);
            sim.advance(Duration::from_millis(700));
            queued(&mut sim, [KeyEvent::character('Y', Modifiers::new())]);
            assert_eq!(
                sim.world().get::<Label>(ui.name).unwrap().text,
                "Desktop explorerXY"
            );
            if undo_before_discard {
                queued(&mut sim, [command('z', false)]);
                assert_eq!(
                    sim.world().get::<Label>(ui.name).unwrap().text,
                    "Desktop explorerX"
                );
            }
            queued(&mut sim, [command('a', false)]); // a discarded selection, too
            queued_press(&mut sim, "discard");
            assert_eq!(
                sim.world().get::<Label>(ui.name).unwrap().text,
                "Desktop explorer"
            );
            assert!(!sim.world().has::<Caret>(ui.name));
            assert!(!sim.world().has::<ImePreview>(ui.name));
            queued_press(&mut sim, "settings");
            for shift in [true, false, false, true] {
                queued(&mut sim, [command('z', shift)]);
                assert_eq!(
                    sim.world().get::<Label>(ui.name).unwrap().text,
                    "Desktop explorer"
                );
            }
            // New edits still have working history, with the reset value as base.
            queued(&mut sim, [KeyEvent::character('N', Modifiers::new())]);
            assert_eq!(
                sim.world().get::<Label>(ui.name).unwrap().text,
                "Desktop explorerN"
            );
            queued(&mut sim, [command('z', false)]);
            assert_eq!(
                sim.world().get::<Label>(ui.name).unwrap().text,
                "Desktop explorer"
            );
            queued(&mut sim, [command('z', true)]);
            assert_eq!(
                sim.world().get::<Label>(ui.name).unwrap().text,
                "Desktop explorerN"
            );
            assert_eq!(
                sim.world().resource::<Model>().unwrap().committed,
                Settings::default()
            );
        }
    }
    #[test]
    fn cancellation_error_retry_and_stale_completion() {
        let mut sim = boot();
        assert_eq!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Empty
        );
        press(&mut sim, "start");
        assert_eq!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Loading
        );
        let old = sim.world().resource::<Model>().unwrap().epoch;
        press(&mut sim, "cancel");
        sim.advance(Duration::from_secs(2));
        sim.update();
        assert_eq!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Cancelled
        );
        press(&mut sim, "fail");
        sim.update();
        send(
            sim.world(),
            Finished {
                epoch: old,
                failed: false,
            },
        );
        sim.update();
        assert_eq!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Loading
        );
        sim.advance(Duration::from_secs(2));
        sim.update();
        assert!(matches!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Error(_)
        ));
        press(&mut sim, "retry");
        sim.update();
        sim.advance(Duration::from_secs(2));
        sim.update();
        assert!(matches!(
            sim.world().resource::<Model>().unwrap().state,
            ViewState::Ready(_)
        ));
    }
    #[test]
    fn narrow_layout_scroll_and_access_tree_are_structural_not_visual_signoff() {
        let mut sim = boot();
        sim.world().insert_resource(Viewport::new(420.0, 640.0));
        sim.update();
        sim.update();
        let ui = *sim.world().resource::<Ui>().unwrap();
        let port = *sim.world().get::<Rect>(ui.home).unwrap();
        let scroll = *sim.world().get::<Scroll>(ui.home).unwrap();
        assert!(port.w > 0.0 && scroll.content[1] > port.h);
        sim.world()
            .resource_mut::<Input>()
            .unwrap()
            .pointer
            .position = [port.x + 20.0, port.y + 20.0];
        sim.world().resource_mut::<Input>().unwrap().scroll = [0.0, -200.0];
        sim.update();
        assert!(sim.world().get::<Scroll>(ui.home).unwrap().offset[1] < 0.0);
        press(&mut sim, "settings");
        sim.update();
        let tree = access_tree(sim.world());
        assert!(tree.nodes.iter().any(|n| n.role == AccessRole::TextInput
            && n.value == "Desktop explorer"
            && n.name == "Display name (1–80 UTF-8 bytes)"));
        assert!(!tree.nodes.iter().any(|n| n.name == "Start local sample"));
        for entity in [ui.name, ui.save] {
            let rect = sim.world().get::<Rect>(entity).unwrap();
            assert!(rect.w > 0.0 && rect.x >= 0.0 && rect.x + rect.w <= 420.1);
        }
    }
    #[test]
    fn saved_preferences_survive_a_fresh_app_and_load_failures_are_visible() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/desktop-evidence/settings-tests")
            .join(format!("{}/prefs.json", std::process::id()));
        let store = Preferences::at(path);
        let mut sim = Sim::new();
        install(&mut sim.app, Some(store.clone()));
        sim.update();
        press(&mut sim, "settings");
        let ui = *sim.world().resource::<Ui>().unwrap();
        sim.world().get_mut::<Label>(ui.name).unwrap().text = "Saved café".into();
        press(&mut sim, "save");
        let mut fresh = Sim::new();
        install(&mut fresh.app, Some(store.clone()));
        fresh.update();
        assert_eq!(
            fresh
                .world()
                .resource::<Model>()
                .unwrap()
                .committed
                .display_name,
            "Saved café"
        );
        assert!(fresh.world().resource::<Model>().unwrap().task.is_none());
        let unknown = Settings {
            version: 99,
            display_name: "unsupported".into(),
        };
        store.save(&unknown).unwrap();
        let mut fresh = Sim::new();
        install(&mut fresh.app, Some(store.clone()));
        fresh.update();
        assert_eq!(
            fresh.world().resource::<Model>().unwrap().committed,
            Settings::default()
        );
        assert!(!fresh.world().resource::<Model>().unwrap().notice.is_empty());
        assert_eq!(store.load::<Settings>().unwrap().unwrap(), unknown);
        let blocked = store.path().parent().unwrap().join("blocked");
        std::fs::create_dir_all(&blocked).unwrap();
        fresh.world().resource_mut::<Model>().unwrap().store = Some(Preferences::at(&blocked));
        press(&mut fresh, "settings");
        let ui = *fresh.world().resource::<Ui>().unwrap();
        fresh.world().get_mut::<Label>(ui.name).unwrap().text = "Not committed".into();
        press(&mut fresh, "save");
        assert_eq!(
            fresh.world().resource::<Model>().unwrap().committed,
            Settings::default()
        );
        assert!(fresh
            .world()
            .resource::<Model>()
            .unwrap()
            .notice
            .starts_with("Save failed"));
    }
    #[test]
    fn static_text_without_background_produces_paint_on_both_pages() {
        let mut sim = boot();
        for page in ["home", "settings"] {
            press(&mut sim, page);
            let before = rndr_paint::frame(&mut sim.app).quads.len();
            let labels: Vec<_> = sim
                .world()
                .entities_with::<Label>()
                .into_iter()
                .filter(|e| !sim.world().has::<Color>(*e))
                .collect();
            let saved: Vec<_> = labels
                .iter()
                .map(|e| (*e, sim.world().get::<Label>(*e).unwrap().text.clone()))
                .collect();
            for entity in labels {
                sim.world().get_mut::<Label>(entity).unwrap().text.clear();
            }
            let after = rndr_paint::frame(&mut sim.app).quads.len();
            assert!(
                before > after + 50,
                "static text must emit glyph geometry on {page}"
            );
            for (entity, text) in saved {
                sim.world().get_mut::<Label>(entity).unwrap().text = text;
            }
        }
    }
    #[test]
    fn invalid_draft_does_not_commit() {
        let mut sim = boot();
        press(&mut sim, "settings");
        let ui = *sim.world().resource::<Ui>().unwrap();
        sim.world().get_mut::<Label>(ui.name).unwrap().text = "  ".into();
        press(&mut sim, "save");
        assert_eq!(
            sim.world().resource::<Model>().unwrap().committed,
            Settings::default()
        );
        assert!(sim.world().resource::<Model>().unwrap().task.is_none());
    }
}
