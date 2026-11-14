//! Command palette.
//!
//! The window is a thin caller of [`rndr_ui::palette::CommandPalette`]. Typing
//! emits `TextChanged` during Input. Update starts a background feed that waits,
//! then filters. A reply stamped with an older epoch is dropped, so a slow
//! result cannot replace a newer query. The palette moves the active command
//! and Enter activates it. Each row's shortcut comes from `Keymap::shortcut`.
//! Letters the filter matched are highlighted. A leading `>` selects the
//! Actions mode. Running a command spawns a task whose handle lives in ordinary
//! app state. Activating Ask Medousa switches the field to a multi-line prompt:
//! Enter sends, Shift+Enter inserts a newline, and the Ask chord
//! (Ctrl+Enter or Cmd+Enter) returns to the list. Esc returns when the
//! prompt is empty. The unsent draft is restored the next time Ask opens.
//! Filter replies are ignored while that prompt is open.
//!
//! The window is frameless, centered, and always on top. Escape and blur hide
//! it and leave the process running. Alt+Space shows or hides it from any
//! application (`RNDR_HOTKEY` or `--hotkey` overrides the chord). The clear
//! chord is still registered; window dismiss claims bare Escape first, except
//! while Ask is open and the prompt is empty. Then Escape leaves Ask.
//!
//! A tray icon stays up while the window is hidden. Left click toggles the
//! window. The menu is Show Palette (the same global hotkey), Settings
//! (disabled), a separator, and Quit.
//!
//! ```text
//! cargo run -p palette
//! cargo run -p palette -- --hotkey Ctrl+Space
//! ```

use std::sync::Arc;
use std::time::Duration;

use rndr_core::window::{self, WindowConfig, WindowKind};
use rndr_core::{
    events, feed, platform_of, set_parent, sleep, spawn, spawn_detached, theme_of, toggle_on_click,
    App, Caret, Context, Dismiss, DismissHold, Entity, Event, Focusable, HotkeyBinding,
    HotkeyRejected, Icon, Item, KeyBinding, Keymap, KeymapFired, Keystroke, Label, Layout,
    LogicalSize, Menu, NamedKey, Placeholder, Position, Prompt, Schedule, Size, Submitted, TabStop,
    TaskHandle, TextChanged, TextFace, TextOverflow, Theme, TrayConfig, TrayIcon, TrayItemId,
    TrayMenu, World,
};
use rndr_paint::LatinFace;
use rndr_render::{configured_hotkey, run, toggle_window};
use rndr_ui::collections::{CollectionEvent, CollectionEventKind};
#[cfg(test)]
use rndr_ui::collections::{ItemId, VirtualViewHandle};
use rndr_ui::palette::{
    fuzzy_match, CommandPalette, PaletteCommand, PaletteHandle, PaletteMode, PaletteProps,
};
use rndr_ui::{UiId, UiPlugin};

const CONTEXT: &str = "Palette";
const ROW_H: f32 = 32.0;
/// Simulated search latency. Overlapping queries finish out of order; the
/// feed's epoch keeps the newer one.
const SEARCH_LAG: Duration = Duration::from_millis(100);
const RUN_DELAY: Duration = Duration::from_millis(350);
const CLEAR: &str = "palette.clear";

/// Chord chosen in `main` and shown on the tray's Show Palette item.
struct Summon(String);

/// (action id, title)
const COMMANDS: &[(&str, &str)] = &[
    ("ask", "Ask Medousa"),
    ("new_session", "New session"),
    ("search_memory", "Search memory"),
    ("open_mori", "Open Mori space"),
    ("toggle_daemon", "Toggle daemon"),
    ("recent_sessions", "Recent sessions"),
    ("pin_memory", "Pin memory"),
    ("forget_memory", "Forget memory"),
    ("summarize", "✨ Summarize session"),
    ("export_md", "Export session as Markdown"),
    ("copy_last", "Copy last answer"),
    ("retry", "Retry last prompt"),
    ("switch_model", "Switch model"),
    ("tools", "Show tools"),
    ("mcp", "Connect MCP server"),
    ("daemon_logs", "Open daemon logs"),
    ("restart_daemon", "Restart daemon"),
    ("settings", "Open settings"),
    ("keyboard", "Keyboard shortcuts"),
    ("theme", "Toggle theme"),
    ("new_mori", "New Mori space"),
    ("index_folder", "Index folder"),
    ("reindex", "Reindex memory"),
    ("clipboard", "Ask about clipboard"),
    ("screenshot", "Ask about screenshot"),
    ("quick_note", "Résumé builder"),
    ("todo", "日本語メモ"),
    ("calendar", "What is next on my calendar"),
    ("focus_mode", "Toggle focus mode"),
    ("quit", "Quit Medousa"),
];

/// Filter result produced on the background feed.
///
/// The feed drops a result stamped with an older epoch before this event is
/// delivered. Update also ignores a result whose query is no longer the field.
struct Filtered {
    query: String,
    mode: Option<char>,
    epoch: u64,
    hits: Vec<usize>,
}

impl Event for Filtered {}

/// A command finished running.
struct Ran {
    title: &'static str,
}

impl Event for Ran {}

struct Palette {
    ui: PaletteHandle,
    field: Entity,
    list: Entity,
    status: Entity,
    show_item: TrayItemId,
    query: String,
    hits: Vec<usize>,
    message: String,
    /// Dropping this cancels a run that is still in flight.
    running: Option<TaskHandle>,
    /// Closing this tells an in-flight search to stop.
    search: feed::Feed<Filtered>,
    search_tx: feed::Sender<Filtered>,
    /// Ask Medousa replaced the filter field with a multi-line prompt.
    asking: bool,
    /// Unsent Ask prompt. Leaving Ask keeps it; opening Ask writes it back.
    draft: String,
}

fn bindings() -> Vec<KeyBinding> {
    let p = |ch| Keystroke::character(ch).primary();
    vec![
        KeyBinding::new(CLEAR, [Keystroke::named(NamedKey::Escape)]).context(CONTEXT),
        KeyBinding::new("ask", [Keystroke::named(NamedKey::Enter).primary()]),
        KeyBinding::new("new_session", [p('n')]),
        KeyBinding::new("search_memory", [p('k')]),
        KeyBinding::new("open_mori", [p('g'), Keystroke::character('m')]),
        KeyBinding::new("daemon_logs", [p('g'), Keystroke::character('l')]),
        KeyBinding::new("toggle_daemon", [p('d').shift()]),
        KeyBinding::new("recent_sessions", [p('r')]),
        KeyBinding::new("copy_last", [p('c').shift()]),
        KeyBinding::new("retry", [p('r').shift()]),
        KeyBinding::new("switch_model", [p('m')]),
        KeyBinding::new("settings", [p(',')]),
        KeyBinding::new("keyboard", [p('/')]),
        KeyBinding::new("quick_note", [p('j')]),
        KeyBinding::new("focus_mode", [Keystroke::named(NamedKey::F(6))]),
        KeyBinding::new("quit", [p('q')]),
    ]
}

fn text_layout(width: Size, height: Size) -> Layout {
    Layout {
        width,
        height,
        ..Layout::column()
    }
}

fn setup(world: &mut World) {
    let theme = theme_of(world);
    {
        let keymap = world
            .resource_mut::<Keymap>()
            .expect("Chrome installs Keymap");
        for binding in bindings() {
            keymap.bind(binding);
        }
    }

    let root = world.spawn((
        Layout {
            gap: 10.0,
            padding: 16.0,
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        theme.canvas,
    ));
    let anchor = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(1.0),
            ..Layout::column()
        },
        Focusable,
        TabStop(false),
    ));
    set_parent(world, anchor, root);
    let mut props = PaletteProps::new("Commands");
    props.placeholder = "Type to filter".into();
    props.loading = "Searching…".into();
    props.list_height = 340.0;
    props.row_height = ROW_H;
    props.width = 640.0;
    let ui = CommandPalette::mount(world, anchor, UiId(700), props).unwrap();
    ui.set_async(world, true).unwrap();
    ui.set_modes(world, &[PaletteMode::new('>', "Actions", "Filter actions")])
        .unwrap();
    world.insert(ui.root, Context::new(CONTEXT));
    assert!(ui.open(world));

    let status = world.spawn((
        Layout {
            min_height: 36.0,
            ..text_layout(Size::Fill, Size::Px(36.0))
        },
        theme.status,
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    set_parent(world, status, root);

    let show_item = install_tray(world);
    let (search, search_tx) = feed::open(world, rndr_core::FeedConfig::latest());
    world.insert_resource(Palette {
        ui,
        field: ui.field,
        list: ui.list,
        status,
        show_item,
        query: String::new(),
        hits: Vec::new(),
        message: "Searching…".into(),
        running: None,
        search,
        search_tx,
        asking: false,
        draft: String::new(),
    });
    start_search(world, String::new(), None, 0);
    paint_status(world);
}

fn status_hint(world: &World) -> String {
    let hint = "Type to filter. Up and Down move, Enter runs, Esc hides.";
    match world.resource::<HotkeyRejected>() {
        Some(rejected) => format!(
            "{}. {hint}",
            rejection_status(&rejected.binding, rejected.error)
        ),
        None => hint.to_string(),
    }
}

fn rejection_status(binding: &str, error: rndr_core::HotkeyError) -> String {
    let why = match error {
        rndr_core::HotkeyError::AlreadyTaken => "taken",
        rndr_core::HotkeyError::Unsupported => "unavailable",
        rndr_core::HotkeyError::InvalidBinding => "invalid",
    };
    format!("{binding} {why}")
}

fn palette_icon(platform: rndr_core::Platform) -> TrayIcon {
    // Ring on a clear square. macOS uses black pixels as a template image.
    // Elsewhere the same shape is white, so it stays visible on a dark panel.
    const N: u32 = 22;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    let center = (N as f32) / 2.0;
    let radius = 7.5;
    let inner = 3.4;
    let mac = platform == rndr_core::Platform::Mac;
    let ink = if mac { 0 } else { 255 };
    for y in 0..N {
        for x in 0..N {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let alpha = if distance >= radius || distance <= inner {
                0
            } else if distance >= radius - 1.0 {
                ((radius - distance) * 255.0).round().clamp(0.0, 255.0) as u8
            } else if distance <= inner + 1.0 {
                ((distance - inner) * 255.0).round().clamp(0.0, 255.0) as u8
            } else {
                255
            };
            let i = ((y * N + x) * 4) as usize;
            if alpha > 0 {
                rgba[i] = ink;
                rgba[i + 1] = ink;
                rgba[i + 2] = ink;
            }
            rgba[i + 3] = alpha;
        }
    }
    TrayIcon::rgba(N, N, rgba)
        .expect("palette icon")
        .template(mac)
}

#[cfg(test)]
fn summon_shortcut(binding: &str, platform: rndr_core::Platform) -> String {
    match HotkeyBinding::parse(binding) {
        Ok(binding) => binding.keystroke().display(platform),
        Err(_) => binding.to_string(),
    }
}

fn menu_chord(binding: &str, platform: rndr_core::Platform) -> String {
    match HotkeyBinding::parse(binding) {
        Ok(binding) => binding.keystroke().label(platform),
        Err(_) => binding.to_string(),
    }
}

fn install_tray(world: &mut World) -> TrayItemId {
    let platform = platform_of(world);
    let shortcut = world
        .resource::<Summon>()
        .map(|summon| menu_chord(&summon.0, platform))
        .unwrap_or_default();
    let mut menu = Menu::new();
    let show = menu.add(Item::new("Show Palette").shortcut(shortcut));
    menu.add(Item::new("Settings").enabled(false));
    menu.separator();
    menu.add(Item::new("Quit").quits());
    world.insert_resource(
        TrayConfig::new(palette_icon(platform), menu)
            .tooltip("Palette")
            .id("palette")
            .title("Palette"),
    );
    show
}

fn command_icon(index: usize) -> Icon {
    const ICONS: [Icon; 8] = [
        Icon::Search,
        Icon::File,
        Icon::Settings,
        Icon::Terminal,
        Icon::Brain,
        Icon::Calendar,
        Icon::Clipboard,
        Icon::Code,
    ];
    ICONS[index % ICONS.len()]
}

fn command_row(world: &World, index: usize) -> Option<PaletteCommand> {
    let (action, title) = COMMANDS.get(index)?;
    let mut command = PaletteCommand::new(index as u64, *title);
    command.icon = Some(command_icon(index));
    let platform = platform_of(world);
    command.shortcut = world
        .resource::<Keymap>()
        .map(|keymap| keymap.shortcut(action, &[], platform))
        .unwrap_or_default();
    Some(command)
}

fn commands_for(world: &World, hits: &[usize]) -> Vec<PaletteCommand> {
    hits.iter()
        .filter_map(|index| command_row(world, *index))
        .collect()
}

fn reconcile_commands(world: &mut World, list: Entity) {
    let _ = list;
    let (ui, hits) = {
        let palette = world.resource::<Palette>().unwrap();
        (palette.ui, palette.hits.clone())
    };
    let epoch = ui.epoch(world).unwrap_or(0);
    let commands = commands_for(world, &hits);
    let _ = ui.resolve(world, epoch, &commands);
}

fn fuzzy(query: &str, title: &str) -> Option<(i32, Vec<(usize, usize)>)> {
    fuzzy_match(query, title).map(|hit| (hit.score, hit.ranges))
}

fn bound(action: &str) -> bool {
    bindings().iter().any(|binding| binding.action == action)
}

#[cfg(test)]
fn filter(query: &str) -> Vec<usize> {
    filter_in(query, None)
}

fn filter_in(query: &str, mode: Option<char>) -> Vec<usize> {
    let mut hits: Vec<(i32, usize)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(index, (action, title))| {
            if mode == Some('>') && !bound(action) {
                return None;
            }
            fuzzy(query, title).map(|(score, _)| (score, index))
        })
        .collect();
    hits.sort_by_key(|(score, index)| (*score, *index));
    hits.into_iter().map(|(_, index)| index).collect()
}

fn run_command(world: &mut World, index: usize) {
    let title = COMMANDS[index].1;
    let handle = spawn(world, async move {
        sleep(RUN_DELAY).await;
        Ran { title }
    });
    let palette = world.resource_mut::<Palette>().unwrap();
    palette.message = format!("Running: {title} ...");
    palette.running = Some(handle);
}

fn palette_update(world: &mut World) {
    if !world.has_resource::<Palette>() {
        return;
    }
    toggle_on_click(world);
    let show_item = world.resource::<Palette>().map(|palette| palette.show_item);
    let tray_show = events::<TrayMenu>(world).iter().any(|event| match event {
        TrayMenu::Activated(id) => Some(*id) == show_item,
    });
    if tray_show {
        window::show(world);
        window::focus(world);
    }
    let changed: Vec<(Entity, String)> = events::<TextChanged>(world)
        .iter()
        .map(|event| (event.entity, event.text.clone()))
        .collect();
    let submitted: Vec<(Entity, String)> = events::<Submitted>(world)
        .iter()
        .map(|event| (event.entity, event.text.clone()))
        .collect();
    let filtered: Vec<(String, Option<char>, u64, Vec<usize>)> = events::<Filtered>(world)
        .iter()
        .map(|event| {
            (
                event.query.clone(),
                event.mode,
                event.epoch,
                event.hits.clone(),
            )
        })
        .collect();
    let ran: Vec<&'static str> = events::<Ran>(world)
        .iter()
        .map(|event| event.title)
        .collect();
    let list_root = world.resource::<Palette>().unwrap().list;
    let activated: Vec<usize> = events::<CollectionEvent>(world)
        .iter()
        .filter(|event| event.view == list_root)
        .filter_map(|event| {
            if let CollectionEventKind::Activated(id) = event.kind {
                Some(id.0 as usize)
            } else {
                None
            }
        })
        .collect();
    let actions = world
        .resource::<KeymapFired>()
        .map(|fired| fired.actions.clone())
        .unwrap_or_default();

    for (entity, text) in changed {
        let (field, asking) = {
            let palette = world.resource::<Palette>().unwrap();
            (palette.field, palette.asking)
        };
        if entity == field && !asking {
            let ui = world.resource::<Palette>().unwrap().ui;
            let parsed = ui.query(world);
            let (filter, mode, epoch) = parsed
                .map(|query| (query.filter, query.mode, query.epoch))
                .unwrap_or_else(|| (text, None, 0));
            world.resource_mut::<Palette>().unwrap().query = filter.clone();
            start_search(world, filter, mode, epoch);
        }
    }

    for (entity, text) in submitted {
        let (field, asking) = {
            let palette = world.resource::<Palette>().unwrap();
            (palette.field, palette.asking)
        };
        if entity == field && asking {
            let message = if text.trim().is_empty() {
                "Enter a prompt.".to_string()
            } else {
                format!("Asked: {}", text.replace('\n', " "))
            };
            world.resource_mut::<Palette>().unwrap().message = message;
            clear_prompt(world);
        }
    }

    for (query, mode, epoch, hits) in filtered {
        let (current, asking, current_mode) = {
            let palette = world.resource::<Palette>().unwrap();
            let ui = palette.ui;
            let query = palette.query.clone();
            let asking = palette.asking;
            (query, asking, ui)
        };
        let current_mode = current_mode.query(world).and_then(|query| query.mode);
        if asking || query != current || mode != current_mode {
            continue;
        }
        let list = world.resource::<Palette>().unwrap().list;
        let running = world.resource::<Palette>().unwrap().running.is_some();
        world.resource_mut::<Palette>().unwrap().hits = hits.clone();
        if !running {
            let hint = status_hint(world);
            world.resource_mut::<Palette>().unwrap().message = hint;
        }
        let ui = world.resource::<Palette>().unwrap().ui;
        let commands = commands_for(world, &hits);
        if ui.resolve(world, epoch, &commands).unwrap_or(false) {
            let _ = list;
        }
    }

    for title in ran {
        let palette = world.resource_mut::<Palette>().unwrap();
        palette.message = format!("Ran: {title}");
        palette.running = None;
    }

    for hit in activated {
        if let Some((command, _)) = COMMANDS.get(hit) {
            if *command == "ask" {
                toggle_ask(world);
            } else {
                run_command(world, hit);
            }
        }
    }

    for action in actions {
        if action == CLEAR {
            clear_query(world);
        } else if action == "ask" {
            toggle_ask(world);
        } else if let Some(index) = COMMANDS.iter().position(|(id, _)| *id == action) {
            run_command(world, index);
        }
    }

    sync_dismiss_hold(world);
    paint_status(world);
}

fn sync_dismiss_hold(world: &mut World) {
    let asking_field = world
        .resource::<Palette>()
        .filter(|palette| palette.asking)
        .map(|palette| palette.field);
    let hold = match asking_field {
        Some(field) => world
            .get::<Label>(field)
            .map(|label| label.text.trim().is_empty())
            .unwrap_or(true),
        None => false,
    };
    let ui = world.resource::<Palette>().map(|palette| palette.ui);
    if let Some(ui) = ui {
        let _ = ui.set_defer_escape(world, hold);
    }
    world.insert_resource(DismissHold { hold });
}

fn ask_hint(world: &World) -> String {
    let shortcut = world
        .resource::<Keymap>()
        .map(|keymap| keymap.shortcut("ask", &[], platform_of(world)))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Ctrl+Enter".to_string());
    format!("Enter sends. Shift+Enter line. {shortcut} back. Esc if empty.")
}

fn toggle_ask(world: &mut World) {
    let asking = world
        .resource::<Palette>()
        .is_some_and(|palette| palette.asking);
    if asking {
        leave_ask(world);
    } else {
        enter_ask(world);
    }
}

fn enter_ask(world: &mut World) {
    let (field, list, draft) = {
        let palette = world.resource::<Palette>().unwrap();
        (palette.field, palette.list, palette.draft.clone())
    };
    let ui = world.resource::<Palette>().unwrap().ui;
    let _ = ui.set_suspended(world, true);
    let _ = ui.set_query(world, "");
    world.insert(field, Prompt::new().max_lines(6));
    world.insert(field, Placeholder::new("Ask Medousa"));
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text = draft.clone();
        label.overflow = TextOverflow::Wrap;
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        let len = draft.len();
        caret.index = len;
        caret.anchor = len;
    }
    let hint = ask_hint(world);
    let palette = world.resource_mut::<Palette>().unwrap();
    palette.asking = true;
    palette.query.clear();
    palette.hits = (0..COMMANDS.len()).collect();
    palette.message = hint;
    reconcile_commands(world, list);
}

fn leave_ask(world: &mut World) {
    let (field, list) = {
        let palette = world.resource::<Palette>().unwrap();
        (palette.field, palette.list)
    };
    let draft = world
        .get::<Label>(field)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    world.remove::<Prompt>(field);
    world.remove::<Placeholder>(field);
    let ui = world.resource::<Palette>().unwrap().ui;
    let _ = ui.set_suspended(world, false);
    let _ = ui.set_query(world, "");
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text.clear();
        label.overflow = TextOverflow::Visible;
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        caret.index = 0;
        caret.anchor = 0;
    }
    let hint = status_hint(world);
    let palette = world.resource_mut::<Palette>().unwrap();
    palette.asking = false;
    palette.draft = draft;
    palette.query.clear();
    palette.hits = (0..COMMANDS.len()).collect();
    palette.message = hint;
    reconcile_commands(world, list);
}

fn clear_prompt(world: &mut World) {
    let field = world.resource::<Palette>().unwrap().field;
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text.clear();
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        caret.index = 0;
        caret.anchor = 0;
    }
    world.resource_mut::<Palette>().unwrap().draft.clear();
}

fn clear_query(world: &mut World) {
    let asking = world.resource::<Palette>().unwrap().asking;
    let field = world.resource::<Palette>().unwrap().field;
    if asking {
        let empty = world
            .get::<Label>(field)
            .map(|label| label.text.trim().is_empty())
            .unwrap_or(true);
        if empty {
            leave_ask(world);
        }
        return;
    }
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text.clear();
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        caret.index = 0;
        caret.anchor = 0;
    }
    let ui = world.resource::<Palette>().unwrap().ui;
    let _ = ui.set_query(world, "");
    let epoch = ui.epoch(world).unwrap_or(0);
    world.resource_mut::<Palette>().unwrap().query.clear();
    start_search(world, String::new(), None, epoch);
}

/// Start a search for `query`. A newer call advances the epoch, so a reply
/// from this search is dropped if the field changes again before it returns.
fn start_search(world: &mut World, query: String, mode: Option<char>, epoch: u64) {
    let feed_epoch = world.resource::<Palette>().unwrap().search.advance();
    let tx = world.resource::<Palette>().unwrap().search_tx.clone();
    world.resource_mut::<Palette>().unwrap().message = "Searching…".into();
    // Detach so this search can finish after a newer one has started. The
    // feed epoch drops a stale reply before it is delivered. The palette
    // epoch drops one that still arrives after a newer edit.
    spawn_detached(world, async move {
        sleep(SEARCH_LAG).await;
        if tx.is_cancelled() || tx.epoch() != feed_epoch {
            return;
        }
        let hits = filter_in(&query, mode);
        let _ = tx.send_at(
            feed_epoch,
            Filtered {
                query,
                mode,
                epoch,
                hits,
            },
        );
    })
    .detach();
}

fn paint_status(world: &mut World) {
    let (status, message) = {
        let palette = world.resource::<Palette>().unwrap();
        let message = if palette.asking {
            palette.message.clone()
        } else {
            format!(
                "{} of {}  |  {}",
                palette.hits.len(),
                COMMANDS.len(),
                palette.message
            )
        };
        (palette.status, message)
    };
    set_text(world, status, &message);
}

fn set_text(world: &mut World, entity: Entity, text: &str) {
    if let Some(label) = world.get_mut::<Label>(entity) {
        if label.text != text {
            label.text = text.to_string();
        }
    }
}

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "palette".to_string(),
        size: LogicalSize::new(640.0, 420.0),
        position: Position::Center,
        decorations: false,
        always_on_top: true,
        kind: WindowKind::Panel,
        hide_on_blur: true,
        dismiss: Dismiss::Hide,
        // Menu-bar app: no Dock icon on macOS. Other platforms ignore this.
        accessory: true,
        ..WindowConfig::default()
    });
    app.world_mut()
        .insert_resource(TextFace(Arc::new(LatinFace)));
    app.world_mut().insert_resource(Theme::default());
    app.add_plugin(UiPlugin);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, palette_update);
    // Held across `run`. Dropping it releases the grab.
    let binding = configured_hotkey(std::env::args().skip(1));
    app.world_mut().insert_resource(Summon(binding.clone()));
    let _hotkey = match toggle_window(app.world_mut(), binding.as_str()) {
        Ok(handle) => Some(handle),
        Err(err) => {
            rndr_core::note_unregistered(app.world_mut(), binding.as_str(), err);
            None
        }
    };
    run(app);
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn show_palette_label_is_the_global_hotkey() {
        assert_eq!(
            super::summon_shortcut("Alt+Space", rndr_core::Platform::Windows),
            "Alt+Space"
        );
        assert_eq!(
            super::summon_shortcut("Alt+Space", rndr_core::Platform::X11),
            "Alt+Space"
        );
        assert_eq!(
            super::summon_shortcut("Alt+Space", rndr_core::Platform::Mac),
            "⌥Space"
        );
        assert_eq!(
            super::summon_shortcut("primary+Space", rndr_core::Platform::Windows),
            "Ctrl+Space"
        );
    }

    #[test]
    fn menu_chord_is_a_parseable_accelerator() {
        use rndr_core::Platform;
        assert_eq!(super::menu_chord("Alt+Space", Platform::Mac), "Alt+Space");
        assert_eq!(
            super::menu_chord("Alt+Space", Platform::Windows),
            "Alt+Space"
        );
        assert_eq!(
            super::menu_chord("primary+Space", Platform::Mac),
            "Cmd+Space"
        );
        assert_eq!(
            super::menu_chord("Ctrl+Alt+Space", Platform::X11),
            "Ctrl+Alt+Space"
        );
    }

    #[test]
    fn a_rejected_hotkey_uses_a_short_status_line() {
        use rndr_core::{HotkeyError, HotkeyRejected, World};
        let mut world = World::new();
        world.insert_resource(HotkeyRejected {
            binding: "Alt+Space".to_string(),
            error: HotkeyError::AlreadyTaken,
        });
        assert_eq!(
            super::status_hint(&world),
            "Alt+Space taken. Type to filter. Up and Down move, Enter runs, Esc hides."
        );
        world.insert_resource(HotkeyRejected {
            binding: "Ctrl+Alt+Space".to_string(),
            error: HotkeyError::Unsupported,
        });
        assert_eq!(
            super::rejection_status("Ctrl+Alt+Space", HotkeyError::Unsupported),
            "Ctrl+Alt+Space unavailable"
        );
        assert_eq!(
            super::rejection_status("nope", HotkeyError::InvalidBinding),
            "nope invalid"
        );
        let hint = super::status_hint(&world);
        assert_eq!(
            hint,
            "Ctrl+Alt+Space unavailable. Type to filter. Up and Down move, Enter runs, Esc hides."
        );
        assert!(!hint.contains('\n'));
    }

    #[test]
    fn fuzzy_records_the_matched_letters() {
        let (score, ranges) = fuzzy("sm", "Search memory").unwrap();
        assert_eq!(ranges, vec![(0, 1), (7, 8)]);
        assert_eq!(score, 6);
        let (_, tight) = fuzzy("se", "Search memory").unwrap();
        assert_eq!(tight, vec![(0, 2)]);
        assert!(fuzzy("zzz", "Search memory").is_none());
        assert!(fuzzy("  ", "Search memory").unwrap().1.is_empty());
        let (score, ranges) = fuzzy("é", "Résumé builder").unwrap();
        assert_eq!(ranges, vec![(1, 3)]);
        assert_eq!(score, 1);
        let (score, ranges) = fuzzy("É", "Résumé builder").unwrap();
        assert_eq!(ranges, vec![(1, 3)]);
        assert_eq!(score, 1);
        let (score, ranges) = fuzzy("rés", "Résumé builder").unwrap();
        assert_eq!(ranges, vec![(0, 4)]);
        let (folded, _) = fuzzy("rés", "Restart daemon").unwrap();
        assert!(score < folded, "{score} vs {folded}");
        let hits = super::filter("rés");
        let resume = super::COMMANDS
            .iter()
            .position(|(_, title)| *title == "Résumé builder")
            .unwrap();
        let restart = super::COMMANDS
            .iter()
            .position(|(_, title)| *title == "Restart daemon")
            .unwrap();
        let resume_at = hits.iter().position(|index| *index == resume).unwrap();
        let restart_at = hits.iter().position(|index| *index == restart).unwrap();
        assert!(resume_at < restart_at);
        let (_, cafe) = fuzzy("cafe", "Café").unwrap();
        assert_eq!(cafe, vec![(0, "Café".len())]);
        let (_, resume) = fuzzy("resume", "résumé").unwrap();
        assert_eq!(resume, vec![(0, "résumé".len())]);
        let (_, upper) = fuzzy("CAFE", "café menu").unwrap();
        assert_eq!(upper, vec![(0, "café".len())]);
        let nfd = "Cafe\u{0301}";
        let (_, ranges) = fuzzy("cafe", nfd).unwrap();
        assert_eq!(ranges, vec![(0, nfd.len())]);
        let (_, ranges) = fuzzy("e\u{0301}", "Résumé builder").unwrap();
        assert_eq!(ranges, vec![(1, 3)]);
        let decomposed = "re\u{0301}sume\u{0301}";
        let (_, ranges) = fuzzy("resume", decomposed).unwrap();
        assert_eq!(ranges, vec![(0, decomposed.len())]);
        assert!(fuzzy("cafe", "cat").is_none());
    }

    #[test]
    fn row_icons_are_lucide_glyphs_inset_like_the_text() {
        use rndr_core::{is_icon_scalar, Rect, Schedule, Sim, Theme};

        let mut sim = Sim::new();
        sim.app.world_mut().insert_resource(Theme::default());
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();

        let list = sim.app.world().resource::<super::Palette>().unwrap().list;
        let row = super::VirtualViewHandle { root: list }
            .row(sim.world(), super::ItemId(0))
            .expect("a row")
            .content;
        let row_rect = *sim.app.world().get::<Rect>(row).expect("row rect");
        let glyph = super::command_icon(0).scalar().to_string();
        let icon_entity = icon_label(sim.world(), row, &glyph).expect("icon");
        let icon_rect = *sim.app.world().get::<Rect>(icon_entity).expect("icon rect");
        assert!(glyph.chars().all(is_icon_scalar));
        assert!(
            (icon_rect.x - (row_rect.x + rndr_core::TEXT_ORIGIN_X)).abs() < 1.0,
            "icon x {} row x {}",
            icon_rect.x,
            row_rect.x
        );
        let centered = row_rect.y + (row_rect.h - icon_rect.h) * 0.5;
        assert!(
            (icon_rect.y - centered).abs() < 1.0,
            "icon y {} expected {centered}",
            icon_rect.y
        );
    }

    #[test]
    fn palette_icon_is_a_template_only_on_macos() {
        use rndr_core::Platform;
        let mac = super::palette_icon(Platform::Mac);
        assert!(mac.template);
        let x11 = super::palette_icon(Platform::X11);
        assert!(!x11.template);
        let windows = super::palette_icon(Platform::Windows);
        assert!(!windows.template);
        assert_eq!(opaque_rgb(&mac), Some([0, 0, 0]));
        assert_eq!(opaque_rgb(&x11), Some([255, 255, 255]));
        assert_eq!(opaque_rgb(&windows), Some([255, 255, 255]));
    }

    #[test]
    fn ask_hint_fits_and_the_draft_survives_leaving() {
        use rndr_core::{
            Dismiss, KeyEvent, Label, Modifiers, NamedKey, Sim, WindowConfig, WindowPhase,
        };

        let mut sim = Sim::new();
        sim.app.world_mut().insert_resource(WindowConfig {
            dismiss: Dismiss::Hide,
            ..WindowConfig::default()
        });
        sim.app
            .world_mut()
            .insert_resource(rndr_core::Theme::default());
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app
            .add_system(rndr_core::Schedule::Startup, super::setup);
        sim.app
            .add_system(rndr_core::Schedule::Update, super::palette_update);
        sim.update();

        super::enter_ask(sim.world());
        super::palette_update(sim.world());
        let status = status_text(&sim);
        let platform = rndr_core::platform_of(sim.world());
        let shortcut = sim
            .app
            .world()
            .resource::<rndr_core::Keymap>()
            .unwrap()
            .shortcut("ask", &[], platform);
        assert!(status.contains(&shortcut), "{status}");
        assert!(status.contains("Esc"));
        assert!(!status.contains("Click"));
        assert!(!status.contains(" of "));
        assert!(status.len() < 70, "{status}");

        {
            let input = sim.world().resource_mut::<rndr_core::Input>().unwrap();
            input
                .keys
                .push(KeyEvent::named(NamedKey::Escape, Modifiers::new()));
        }
        sim.update();
        assert!(!sim.app.world().resource::<super::Palette>().unwrap().asking);
        assert!(sim
            .app
            .world()
            .resource::<WindowPhase>()
            .map(|phase| phase.visible)
            .unwrap_or(true));

        super::enter_ask(sim.world());
        let field = sim.app.world().resource::<super::Palette>().unwrap().field;
        sim.app.world_mut().get_mut::<Label>(field).unwrap().text = "keep me".into();
        super::leave_ask(sim.world());
        assert_eq!(
            sim.app.world().resource::<super::Palette>().unwrap().draft,
            "keep me"
        );
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "");
        super::enter_ask(sim.world());
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "keep me");

        super::palette_update(sim.world());
        {
            let input = sim.world().resource_mut::<rndr_core::Input>().unwrap();
            input
                .keys
                .push(KeyEvent::named(NamedKey::Escape, Modifiers::new()));
        }
        sim.update();
        assert!(sim.app.world().resource::<super::Palette>().unwrap().asking);
        assert!(!sim.app.world().resource::<WindowPhase>().unwrap().visible);
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "keep me");
    }

    fn status_text(sim: &rndr_core::Sim) -> String {
        let status = sim.app.world().resource::<super::Palette>().unwrap().status;
        sim.app
            .world()
            .get::<rndr_core::Label>(status)
            .unwrap()
            .text
            .clone()
    }

    #[test]
    fn a_slower_query_does_not_replace_a_newer_one() {
        use std::time::Duration;

        use rndr_core::{events, send, Schedule, Sim, TextChanged, Theme};

        let mut sim = Sim::new();
        sim.app.world_mut().insert_resource(Theme::default());
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);

        sim.update();
        assert_eq!(list_count(&sim), 0, "rows wait for the background feed");

        sim.advance(super::SEARCH_LAG);
        sim.update();
        assert_eq!(list_count(&sim), super::COMMANDS.len() as u32);

        let field = sim.app.world().resource::<super::Palette>().unwrap().field;
        send(
            sim.world(),
            TextChanged {
                entity: field,
                text: "a".into(),
            },
        );
        sim.update();
        // Poll the "a" search so its deadline is armed.
        sim.update();

        sim.advance(Duration::from_millis(1));
        send(
            sim.world(),
            TextChanged {
                entity: field,
                text: "ab".into(),
            },
        );
        sim.update();
        sim.update();

        // The "a" search is due. The "ab" search is still one millisecond out.
        sim.advance(super::SEARCH_LAG - Duration::from_millis(1));
        sim.update();
        assert_eq!(
            list_count(&sim),
            super::COMMANDS.len() as u32,
            "the older query must not replace the full list"
        );
        assert!(events::<super::Filtered>(sim.world()).is_empty());

        sim.advance(Duration::from_millis(1));
        sim.update();
        assert_eq!(list_count(&sim), super::filter("ab").len() as u32);
        assert_ne!(super::filter("a").len(), super::COMMANDS.len());
    }

    #[test]
    fn semantic_palette_search_owns_typing_and_forwards_navigation_activation() {
        use rndr_core::*;
        let mut sim = Sim::new();
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.app
            .world_mut()
            .insert_resource(Viewport::new(720., 620.));
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();
        rndr_paint::frame(&mut sim.app);
        let p = sim.app.world().resource::<super::Palette>().unwrap();
        let (list, field) = (p.list, p.field);
        let h = super::VirtualViewHandle { root: list };
        assert!(sim.app.world().entities_with::<List>().is_empty());
        // Even a pointer focus request synchronously delegates to the search
        // editor. A queued printable key in that same native batch keeps typing.
        let row = h.row(sim.app.world(), super::ItemId(0)).unwrap().root;
        assert!(request_focus(sim.world(), row));
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
        request_focus(sim.world(), field);
        sim.world().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::ArrowDown, Modifiers::default())];
        rndr_paint::frame(&mut sim.app);
        assert_eq!(h.active(sim.app.world()), Some(super::ItemId(1)));
        assert_eq!(h.selected(sim.app.world()), Some(super::ItemId(1)));
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
        // Filtering/reordering retains the command key, never its old index.
        sim.world().resource_mut::<super::Palette>().unwrap().hits = vec![3, 1, 8];
        super::reconcile_commands(sim.world(), list);
        assert_eq!(h.active(sim.app.world()), Some(super::ItemId(1)));
        assert_eq!(h.selected(sim.app.world()), Some(super::ItemId(1)));
        sim.world().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
        rndr_paint::frame(&mut sim.app);
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
        assert!(sim
            .app
            .world()
            .resource::<super::Palette>()
            .unwrap()
            .running
            .is_some());
    }

    #[test]
    fn semantic_palette_pointer_and_typing_in_one_batch_keeps_search_focus() {
        use rndr_core::*;
        let mut sim = Sim::new();
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.app
            .world_mut()
            .insert_resource(Viewport::new(720., 620.));
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();
        rndr_paint::frame(&mut sim.app);
        let p = sim.app.world().resource::<super::Palette>().unwrap();
        let (list, field) = (p.list, p.field);
        let row = super::VirtualViewHandle { root: list }
            .row(sim.app.world(), super::ItemId(3))
            .unwrap()
            .root;
        let r = *sim.app.world().get::<Rect>(row).unwrap();
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer.position = [r.x + 80., r.y + r.h / 2.];
        input.pointer.down = true;
        input.pointer.pressed = true;
        input.keys = vec![KeyEvent::character('s', Modifiers::default())];
        rndr_paint::frame(&mut sim.app);
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
        assert_eq!(
            sim.app.world().resource::<super::Palette>().unwrap().query,
            "s"
        );
        // A stable key's icon must not be reinterpreted as a filtered index.
        sim.world().resource_mut::<super::Palette>().unwrap().hits = vec![3];
        super::reconcile_commands(sim.world(), list);
        rndr_paint::frame(&mut sim.app);
        let row = super::VirtualViewHandle { root: list }
            .row(sim.app.world(), super::ItemId(3))
            .unwrap()
            .content;
        let glyph = super::command_icon(3).scalar().to_string();
        assert!(icon_label(sim.app.world(), row, &glyph).is_some());
    }

    #[test]
    fn semantic_palette_ime_snapshot_does_not_navigate_or_duplicate_commit() {
        use rndr_core::*;
        let mut sim = Sim::new();
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.app
            .world_mut()
            .insert_resource(Viewport::new(720., 620.));
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();
        rndr_paint::frame(&mut sim.app);
        let p = sim.app.world().resource::<super::Palette>().unwrap();
        let (list, field) = (p.list, p.field);
        let h = super::VirtualViewHandle { root: list };
        request_focus(sim.world(), field);
        let active = h.active(sim.app.world());
        {
            let input = sim.world().resource_mut::<Input>().unwrap();
            input.ime_enabled = true;
            input.ime_preedit = "に".into();
            input.keys = vec![
                KeyEvent::named(NamedKey::ArrowDown, Modifiers::default()),
                KeyEvent::named(NamedKey::Enter, Modifiers::default()),
            ];
        }
        rndr_paint::frame(&mut sim.app);
        assert_eq!(h.active(sim.app.world()), active);
        assert!(!sim.app.world().resource::<super::Palette>().unwrap().asking);
        {
            let input = sim.world().resource_mut::<Input>().unwrap();
            input.ime_preedit.clear();
            input.ime_commit = "日本".into();
            input.keys = vec![
                KeyEvent::character('x', Modifiers::default()),
                KeyEvent::named(NamedKey::Enter, Modifiers::default()),
            ];
        }
        rndr_paint::frame(&mut sim.app);
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "日本");
        assert_eq!(
            sim.app.world().resource::<super::Palette>().unwrap().query,
            "日本"
        );
        assert_eq!(h.active(sim.app.world()), active);
        assert!(!sim.app.world().resource::<super::Palette>().unwrap().asking);
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
    }

    #[test]
    fn semantic_palette_hover_and_double_click_ask_preserve_prompt_ownership() {
        use rndr_core::*;
        let mut sim = Sim::new();
        sim.app.add_plugin(rndr_ui::UiPlugin);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.app
            .world_mut()
            .insert_resource(Viewport::new(720., 620.));
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();
        rndr_paint::frame(&mut sim.app);
        let p = sim.app.world().resource::<super::Palette>().unwrap();
        let (list, field) = (p.list, p.field);
        let h = super::VirtualViewHandle { root: list };
        let r = *sim
            .app
            .world()
            .get::<Rect>(h.row(sim.app.world(), super::ItemId(3)).unwrap().root)
            .unwrap();
        sim.world()
            .resource_mut::<Input>()
            .unwrap()
            .pointer
            .position = [r.x + 80., r.y + 8.];
        rndr_paint::frame(&mut sim.app);
        assert_eq!(h.active(sim.app.world()), Some(super::ItemId(3)));
        assert_eq!(h.selected(sim.app.world()), Some(super::ItemId(3)));
        let r = *sim
            .app
            .world()
            .get::<Rect>(h.row(sim.app.world(), super::ItemId(0)).unwrap().root)
            .unwrap();
        for _ in 0..2 {
            {
                let input = sim.world().resource_mut::<Input>().unwrap();
                input.pointer = Pointer {
                    position: [r.x + 80., r.y + 8.],
                    down: true,
                    pressed: true,
                    ..Pointer::default()
                };
            }
            rndr_paint::frame(&mut sim.app);
            sim.advance(std::time::Duration::from_millis(20));
            {
                let input = sim.world().resource_mut::<Input>().unwrap();
                input.pointer.down = false;
                input.pointer.pressed = false;
                input.pointer.released = true;
            }
            rndr_paint::frame(&mut sim.app);
        }
        assert!(sim.app.world().resource::<super::Palette>().unwrap().asking);
        assert_eq!(
            sim.app.world().resource::<Focus>().unwrap().entity,
            Some(field)
        );
        sim.world().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::character('q', Modifiers::default())];
        rndr_paint::frame(&mut sim.app);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        sim.world().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Enter, shift)];
        rndr_paint::frame(&mut sim.app);
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "q\n");
        sim.world().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
        rndr_paint::frame(&mut sim.app);
        assert!(sim.app.world().resource::<super::Palette>().unwrap().asking);
        assert_eq!(sim.app.world().get::<Label>(field).unwrap().text, "");
        assert!(sim
            .app
            .world()
            .resource::<super::Palette>()
            .unwrap()
            .message
            .starts_with("Asked: q"));
    }

    fn list_count(sim: &rndr_core::Sim) -> u32 {
        let list = sim.app.world().resource::<super::Palette>().unwrap().list;
        super::VirtualViewHandle { root: list }.len(sim.app.world()) as u32
    }

    fn icon_label(
        world: &rndr_core::World,
        row: rndr_core::Entity,
        text: &str,
    ) -> Option<rndr_core::Entity> {
        fn walk(
            world: &rndr_core::World,
            entity: rndr_core::Entity,
            text: &str,
            found: &mut Option<rndr_core::Entity>,
        ) {
            if found.is_some() {
                return;
            }
            if world
                .get::<rndr_core::Label>(entity)
                .is_some_and(|label| label.text == text)
            {
                *found = Some(entity);
                return;
            }
            for child in rndr_core::children_of(world, entity) {
                walk(world, child, text, found);
            }
        }
        let mut found = None;
        walk(world, row, text, &mut found);
        found
    }

    fn opaque_rgb(icon: &rndr_core::TrayIcon) -> Option<[u8; 3]> {
        let rndr_core::Image::Rgba { rgba, .. } = &icon.image else {
            return None;
        };
        rgba.chunks(4)
            .find(|pixel| pixel[3] == 255)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
    }
}
