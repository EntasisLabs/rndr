//! Command palette.
//!
//! A search field over a virtual list of commands. Typing emits `TextChanged`
//! during Input. Update starts a background feed that waits, then filters.
//! A reply stamped with an older epoch is dropped, so a slow result cannot
//! replace a newer query. The list's standard keymap actions move the selection
//! and Enter activates the row. Each row's shortcut comes from `Keymap::shortcut`
//! and is drawn in its own dim label color. Letters the filter matched are
//! highlighted. Running a command spawns a task whose handle lives in ordinary
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
    context_stack, events, feed, platform_of, set_parent, sleep, spawn, spawn_detached, theme_of,
    toggle_on_click, App, Axis, CapturesKeys, Caret, Chrome, Color, Context, Dismiss, DismissHold,
    Entity, Event, Focus, HotkeyBinding, HotkeyRejected, ImageView, Item, KeyBinding, Keymap,
    KeymapFired, Keystroke, Label, Layout, List, ListActivated, ListKeys, ListRows, LogicalSize,
    Menu, NamedKey, Placeholder, Position, Prompt, Radius, Schedule, Size, Spans, Submitted,
    TaskHandle, TextChanged, TextFace, TextField, TextOverflow, TextSpan, Theme, TrayConfig,
    TrayIcon, TrayItemId, TrayMenu, World, TEXT_ORIGIN_X,
};
use rndr_paint::LatinFace;
use rndr_render::{configured_hotkey, load_rgba, run, toggle_window};
use unicode_normalization::UnicodeNormalization;

const CONTEXT: &str = "Palette";
const ROW_H: f32 = 32.0;
const ICON_PX: f32 = 16.0;
const ICON_RADIUS: f32 = 4.0;
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
    hits: Vec<usize>,
}

impl Event for Filtered {}

/// A command finished running.
struct Ran {
    title: &'static str,
}

impl Event for Ran {}

struct Palette {
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
    load_row_icons(world);
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
            axis: Axis::Column,
            gap: 10.0,
            padding: 16.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        theme.canvas,
        Context::new(CONTEXT),
    ));

    let field = world.spawn((
        Layout {
            min_height: 36.0,
            ..text_layout(Size::Fill, Size::Hug)
        },
        theme.title,
        Radius(theme.radius),
        Label::new(""),
        TextField,
        CapturesKeys,
    ));
    set_parent(world, field, root);

    let list = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        theme.pane,
        Radius(theme.radius),
        List::new(0, ROW_H),
        ListKeys,
        ListRows(build_row),
    ));
    set_parent(world, list, root);

    let status = world.spawn((
        Layout {
            min_height: 36.0,
            ..text_layout(Size::Fill, Size::Px(36.0))
        },
        theme.status,
        Radius(theme.radius),
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    set_parent(world, status, root);

    world.insert_resource(Focus {
        entity: Some(field),
    });
    let show_item = install_tray(world);
    let (search, search_tx) = feed::open(world, rndr_core::FeedConfig::latest());
    world.insert_resource(Palette {
        field,
        list,
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
    start_search(world, String::new());
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

fn load_row_icons(world: &mut World) {
    for index in 0..4u32 {
        let _ = load_rgba(world, u64::from(index), 16, 16, row_icon(index));
    }
}

fn row_icon(index: u32) -> Vec<u8> {
    let colors = [
        [232, 84, 70, 255],
        [70, 160, 232, 255],
        [232, 180, 60, 255],
        [90, 190, 120, 255],
    ];
    let color = colors[index as usize % 4];
    let mut pixels = vec![0u8; 16 * 16 * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        *pixel = color;
    }
    pixels
}

fn row_icon_entity(world: &World, row: Entity) -> Option<Entity> {
    for child in rndr_core::children_of(world, row) {
        if world.has::<ImageView>(child) {
            return Some(child);
        }
        for grandchild in rndr_core::children_of(world, child) {
            if world.has::<ImageView>(grandchild) {
                return Some(grandchild);
            }
        }
    }
    None
}

fn ensure_row_icon(world: &mut World, row: Entity, index: u32) {
    let image = u64::from(index % 4);
    if let Some(icon) = row_icon_entity(world, row) {
        if let Some(view) = world.get_mut::<ImageView>(icon) {
            view.image = image;
            view.radius = ICON_RADIUS;
        }
        return;
    }
    // A lead spacer matches the text inset. Row padding would also shrink the
    // row vertically and clip the label.
    let lead = world.spawn((Layout {
        axis: Axis::Row,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(TEXT_ORIGIN_X + ICON_PX),
        height: Size::Fill,
        min_width: 0.0,
        min_height: 0.0,
    },));
    let spacer = world.spawn((Layout {
        width: Size::Px(TEXT_ORIGIN_X),
        height: Size::Px(1.0),
        ..Layout::row()
    },));
    let icon = world.spawn((
        Layout {
            width: Size::Px(ICON_PX),
            height: Size::Px(ICON_PX),
            ..Layout::row()
        },
        ImageView::new(image).with_radius(ICON_RADIUS),
    ));
    set_parent(world, spacer, lead);
    set_parent(world, icon, lead);
    set_parent(world, lead, row);
}

fn build_row(world: &mut World, row: Entity, index: u32) {
    let selected = world
        .get::<rndr_core::ListItem>(row)
        .is_some_and(|item| item.selected);
    let Some((title, action)) = command_at(world, index) else {
        return;
    };
    let platform = platform_of(world);
    let stack = context_stack(world, row);
    let shortcut = world
        .resource::<Keymap>()
        .map(|keymap| keymap.shortcut(action, &stack, platform))
        .unwrap_or_default();
    let theme = theme_of(world);
    let selected_color = Color::rgb(0.24, 0.40, 0.62);
    world.insert(row, if selected { selected_color } else { theme.pane });
    world.insert(row, Radius(6.0));
    if let Some(layout) = world.get_mut::<Layout>(row) {
        layout.axis = Axis::Row;
        layout.gap = 12.0;
        layout.padding = 0.0;
    }
    if let Some(label) = world.get_mut::<Label>(row) {
        label.text.clear();
    }
    ensure_row_icon(world, row, index);
    let (name, keys) = row_parts(world, row);
    set_text(world, name, title);
    set_text(world, keys, &shortcut);
    let dim = Color::rgb(0.62, 0.66, 0.74);
    if let Some(label) = world.get_mut::<Label>(keys) {
        label.color = Some(dim);
    }
    world.remove::<Spans>(keys);
    let query = world
        .resource::<Palette>()
        .map(|palette| palette.query.clone())
        .unwrap_or_default();
    let ranges = fuzzy(&query, title)
        .map(|(_, ranges)| ranges)
        .unwrap_or_default();
    if ranges.is_empty() {
        world.remove::<Spans>(name);
    } else {
        let text = theme.text;
        let mark = Color::rgba(0.95, 0.78, 0.25, 0.70);
        world.insert(
            name,
            Spans {
                runs: ranges
                    .into_iter()
                    .map(|(start, end)| TextSpan::new(start, end, text).highlight(mark))
                    .collect(),
            },
        );
    }
}

fn command_at(world: &World, index: u32) -> Option<(&'static str, &'static str)> {
    let palette = world.resource::<Palette>()?;
    let hit = palette.hits.get(index as usize).copied()?;
    Some((COMMANDS[hit].1, COMMANDS[hit].0))
}

fn row_parts(world: &mut World, row: Entity) -> (Entity, Entity) {
    let clear = Color::rgba(0.0, 0.0, 0.0, 0.0);
    let mut labels: Vec<Entity> = rndr_core::children_of(world, row)
        .into_iter()
        .filter(|child| world.has::<Label>(*child))
        .collect();
    while labels.len() < 2 {
        let width = if labels.is_empty() {
            Size::Fill
        } else {
            Size::Hug
        };
        let child = world.spawn((
            Layout {
                width,
                height: Size::Fill,
                ..Layout::row()
            },
            clear,
            Label::new(""),
        ));
        set_parent(world, child, row);
        labels.push(child);
    }
    if let Some(layout) = world.get_mut::<Layout>(labels[0]) {
        layout.width = Size::Fill;
        layout.height = Size::Fill;
    }
    if let Some(layout) = world.get_mut::<Layout>(labels[1]) {
        layout.width = Size::Hug;
        layout.height = Size::Fill;
    }
    (labels[0], labels[1])
}

/// One base character in `title`, plus any combining marks that follow it.
struct Atom {
    start: usize,
    end: usize,
    bases: Vec<char>,
}

/// One folded base from the query, tied back to the grapheme it came from.
struct Needle {
    base: char,
    piece: usize,
}

/// Cost of matching a base only after folding an accent away.
///
/// Larger than a position gap inside a title, so an exact accent match
/// sorts ahead of a folded one. Case differences are not a penalty.
const ACCENT_PENALTY: i32 = 1_000;

/// Subsequence match. Lower score is better: tight, early matches win.
/// An exact accent match scores above the same letters with accents folded
/// off. Ranges are byte spans in `title` for the letters that hit, merged
/// when they sit next to each other. Matching ignores case and diacritics,
/// and a hit covers the original character including a following combining
/// mark.
fn fuzzy(query: &str, title: &str) -> Option<(i32, Vec<(usize, usize)>)> {
    let query = query.trim();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let (pieces, needles) = query_needles(query);
    if needles.is_empty() {
        return Some((0, Vec::new()));
    }
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    let mut ranges = Vec::new();
    let mut needle = 0usize;
    for atom in atoms(title) {
        if needle >= needles.len() {
            break;
        }
        let grapheme = &title[atom.start..atom.end];
        let mut used = false;
        let mut inexact = false;
        for base in &atom.bases {
            if needle >= needles.len() {
                break;
            }
            if *base == needles[needle].base {
                let at = atom.start;
                score += match last {
                    Some(prev) => (at as i32 - prev as i32 - 1).max(0),
                    None => at as i32,
                };
                last = Some(at);
                if !same_grapheme(&pieces[needles[needle].piece], grapheme) {
                    inexact = true;
                }
                needle += 1;
                used = true;
            }
        }
        if !used {
            continue;
        }
        if inexact {
            score += ACCENT_PENALTY;
        }
        if let Some((_, end)) = ranges.last_mut() {
            if *end == atom.start {
                *end = atom.end;
                continue;
            }
        }
        ranges.push((atom.start, atom.end));
    }
    (needle == needles.len()).then_some((score, ranges))
}

fn query_needles(query: &str) -> (Vec<String>, Vec<Needle>) {
    let mut pieces = Vec::new();
    let mut needles = Vec::new();
    for atom in atoms(query) {
        let grapheme = &query[atom.start..atom.end];
        if grapheme.chars().all(char::is_whitespace) {
            continue;
        }
        let piece = pieces.len();
        pieces.push(grapheme.to_string());
        for base in atom.bases {
            needles.push(Needle { base, piece });
        }
    }
    (pieces, needles)
}

fn same_grapheme(query: &str, title: &str) -> bool {
    fold_grapheme(query) == fold_grapheme(title)
}

fn fold_grapheme(text: &str) -> String {
    text.nfd().flat_map(char::to_lowercase).collect()
}

fn atoms(text: &str) -> Vec<Atom> {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if is_mark(ch) {
            if let Some(prev) = atoms.last_mut() {
                prev.end = start + ch.len_utf8();
            }
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some(&(at, next)) = chars.peek() {
            if !is_mark(next) {
                break;
            }
            chars.next();
            end = at + next.len_utf8();
        }
        atoms.push(Atom {
            start,
            end,
            bases: fold_bases(ch),
        });
    }
    atoms
}

fn fold_bases(ch: char) -> Vec<char> {
    let lower: String = ch.to_lowercase().collect();
    lower.nfd().filter(|ch| !is_mark(*ch)).collect()
}

fn is_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{3099}'..='\u{309A}'
    )
}

fn filter(query: &str) -> Vec<usize> {
    let mut hits: Vec<(i32, usize)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(index, (_, title))| fuzzy(query, title).map(|(score, _)| (score, index)))
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
    let filtered: Vec<(String, Vec<usize>)> = events::<Filtered>(world)
        .iter()
        .map(|event| (event.query.clone(), event.hits.clone()))
        .collect();
    let ran: Vec<&'static str> = events::<Ran>(world)
        .iter()
        .map(|event| event.title)
        .collect();
    let activated: Vec<u32> = events::<ListActivated>(world)
        .iter()
        .map(|event| event.index)
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
            world.resource_mut::<Palette>().unwrap().query = text.clone();
            start_search(world, text);
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

    for (query, hits) in filtered {
        let (current, asking) = {
            let palette = world.resource::<Palette>().unwrap();
            (palette.query.clone(), palette.asking)
        };
        if asking || query != current {
            continue;
        }
        let list = world.resource::<Palette>().unwrap().list;
        let running = world.resource::<Palette>().unwrap().running.is_some();
        world.resource_mut::<Palette>().unwrap().hits = hits.clone();
        if !running {
            let hint = status_hint(world);
            world.resource_mut::<Palette>().unwrap().message = hint;
        }
        if let Some(list) = world.get_mut::<List>(list) {
            list.count = hits.len() as u32;
            list.selected = if hits.is_empty() { None } else { Some(0) };
            list.offset = 0.0;
        }
    }

    for title in ran {
        let palette = world.resource_mut::<Palette>().unwrap();
        palette.message = format!("Ran: {title}");
        palette.running = None;
    }

    for index in activated {
        let hit = world
            .resource::<Palette>()
            .and_then(|palette| palette.hits.get(index as usize).copied());
        if let Some(hit) = hit {
            if COMMANDS[hit].0 == "ask" {
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
    world.remove::<ListKeys>(list);
    let hint = ask_hint(world);
    let palette = world.resource_mut::<Palette>().unwrap();
    palette.asking = true;
    palette.query.clear();
    palette.hits = (0..COMMANDS.len()).collect();
    palette.message = hint;
    if let Some(list) = world.get_mut::<List>(list) {
        list.count = COMMANDS.len() as u32;
        list.selected = Some(0);
        list.offset = 0.0;
    }
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
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text.clear();
        label.overflow = TextOverflow::Visible;
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        caret.index = 0;
        caret.anchor = 0;
    }
    world.insert(list, ListKeys);
    let hint = status_hint(world);
    let palette = world.resource_mut::<Palette>().unwrap();
    palette.asking = false;
    palette.draft = draft;
    palette.query.clear();
    palette.hits = (0..COMMANDS.len()).collect();
    palette.message = hint;
    if let Some(list) = world.get_mut::<List>(list) {
        list.count = COMMANDS.len() as u32;
        list.selected = Some(0);
        list.offset = 0.0;
    }
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
    world.resource_mut::<Palette>().unwrap().query.clear();
    start_search(world, String::new());
}

/// Start a search for `query`. A newer call advances the epoch, so a reply
/// from this search is dropped if the field changes again before it returns.
fn start_search(world: &mut World, query: String) {
    let epoch = world.resource::<Palette>().unwrap().search.advance();
    let tx = world.resource::<Palette>().unwrap().search_tx.clone();
    world.resource_mut::<Palette>().unwrap().message = "Searching…".into();
    // Detach so this search can finish after a newer one has started. The
    // epoch, not the task handle, decides which reply is applied.
    spawn_detached(world, async move {
        sleep(SEARCH_LAG).await;
        if tx.is_cancelled() || tx.epoch() != epoch {
            return;
        }
        let hits = filter(&query);
        let _ = tx.send_at(epoch, Filtered { query, hits });
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
    app.add_plugin(Chrome);
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
    fn row_icons_are_solid_and_inset_like_the_text() {
        let icon = super::row_icon(0);
        let edge = &icon[0..4];
        let inner = &icon[(8 * 16 + 8) * 4..][..4];
        assert_eq!(edge, &[232, 84, 70, 255]);
        assert_eq!(edge, inner);

        use rndr_core::{Rect, Schedule, Sim, Theme};

        let mut sim = Sim::new();
        sim.app.world_mut().insert_resource(Theme::default());
        sim.app.add_plugin(rndr_core::Chrome);
        sim.app.add_system(Schedule::Startup, super::setup);
        sim.app.add_system(Schedule::Update, super::palette_update);
        sim.update();
        sim.advance(super::SEARCH_LAG);
        sim.update();

        let list = sim.app.world().resource::<super::Palette>().unwrap().list;
        let row = rndr_core::children_of(sim.world(), list)
            .into_iter()
            .find(|child| sim.app.world().has::<rndr_core::ListItem>(*child))
            .expect("a row");
        let row_rect = *sim.app.world().get::<Rect>(row).expect("row rect");
        let icon_entity = super::row_icon_entity(sim.world(), row).expect("icon");
        let icon_rect = *sim.app.world().get::<Rect>(icon_entity).expect("icon rect");
        let view = *sim
            .app
            .world()
            .get::<rndr_core::ImageView>(icon_entity)
            .unwrap();
        assert!((view.radius - super::ICON_RADIUS).abs() < 1.0e-4);
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
        sim.app.add_plugin(rndr_core::Chrome);
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
        sim.app.add_plugin(rndr_core::Chrome);
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

    fn list_count(sim: &rndr_core::Sim) -> u32 {
        let list = sim.app.world().resource::<super::Palette>().unwrap().list;
        sim.app.world().get::<rndr_core::List>(list).unwrap().count
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
