//! Command palette.
//!
//! A search field over a virtual list of commands. Typing emits `TextChanged`
//! during Input; Update debounces a subsequence filter on the background
//! executor. The list's standard keymap actions move the selection and Enter
//! activates the row. Each row's shortcut comes from `Keymap::shortcut` and is
//! drawn in its own dim label color. Letters the filter matched are highlighted.
//! Running a command spawns a task whose handle lives in ordinary app state.
//!
//! The window is frameless, centered, and always on top. Escape and blur hide
//! it and leave the process running. The clear chord is still registered;
//! window dismiss claims bare Escape first.
//!
//! ```text
//! cargo run -p palette
//! ```

use std::sync::Arc;
use std::time::Duration;

use rndr_core::{
    context_stack, debounce, events, platform_of, set_parent, sleep, spawn, theme_of, App, Axis,
    CapturesKeys, Caret, Chrome, Color, Context, Dismiss, Entity, Event, Focus, KeyBinding, Keymap,
    KeymapFired, Keystroke, Label, Layout, List, ListActivated, ListKeys, ListRows, LogicalSize,
    NamedKey, Position, Radius, Schedule, Size, Spans, TaskHandle, TextChanged, TextFace,
    TextField, TextSpan, Theme, WindowConfig, WindowKind, World,
};
use rndr_paint::LatinFace;
use rndr_render::run;

const CONTEXT: &str = "Palette";
const ROW_H: f32 = 32.0;
const DEBOUNCE: Duration = Duration::from_millis(40);
const RUN_DELAY: Duration = Duration::from_millis(350);
const CLEAR: &str = "palette.clear";

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
    ("summarize", "Summarize session"),
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
    ("quick_note", "Quick note"),
    ("todo", "Add todo to memory"),
    ("calendar", "What is next on my calendar"),
    ("focus_mode", "Toggle focus mode"),
    ("quit", "Quit Medousa"),
];

/// Filter result produced on the background executor.
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
    query: String,
    hits: Vec<usize>,
    message: String,
    /// Dropping this cancels a run that is still in flight.
    running: Option<TaskHandle>,
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
        List::new(COMMANDS.len() as u32, ROW_H),
        ListKeys,
        ListRows(build_row),
    ));
    set_parent(world, list, root);

    let status = world.spawn((
        Layout {
            min_height: 32.0,
            ..text_layout(Size::Fill, Size::Hug)
        },
        theme.status,
        Radius(theme.radius),
        Label::new(""),
    ));
    set_parent(world, status, root);

    world.insert_resource(Focus {
        entity: Some(field),
    });
    world.insert_resource(Palette {
        field,
        list,
        status,
        query: String::new(),
        hits: (0..COMMANDS.len()).collect(),
        message: "Type to filter. Up and Down move, Enter runs, Esc clears.".into(),
        running: None,
    });
    paint_status(world);
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

/// Subsequence match. Lower score is better: tight, early matches win.
/// Ranges are byte spans in `title` for the letters that hit, merged when
/// they sit next to each other.
fn fuzzy(query: &str, title: &str) -> Option<(i32, Vec<(usize, usize)>)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    let mut ranges = Vec::new();
    let mut chars = title.char_indices();
    for q in query.chars().filter(|c| !c.is_whitespace()) {
        let (at, ch) = chars
            .by_ref()
            .find(|(_, ch)| ch.to_ascii_lowercase() == q)?;
        score += match last {
            Some(prev) => (at - prev - 1) as i32,
            None => at as i32,
        };
        last = Some(at);
        let end = at + ch.len_utf8();
        if let Some((_, prev_end)) = ranges.last_mut() {
            if *prev_end == at {
                *prev_end = end;
                continue;
            }
        }
        ranges.push((at, end));
    }
    Some((score, ranges))
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
    let changed: Vec<(Entity, String)> = events::<TextChanged>(world)
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
        let field = world.resource::<Palette>().unwrap().field;
        if entity == field {
            world.resource_mut::<Palette>().unwrap().query = text.clone();
            debounce(world, "filter", DEBOUNCE, move || async move {
                let hits = filter(&text);
                Filtered { query: text, hits }
            });
        }
    }

    for (query, hits) in filtered {
        let current = world.resource::<Palette>().unwrap().query.clone();
        if query != current {
            continue;
        }
        let list = world.resource::<Palette>().unwrap().list;
        world.resource_mut::<Palette>().unwrap().hits = hits.clone();
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
            run_command(world, hit);
        }
    }

    for action in actions {
        if action == CLEAR {
            clear_query(world);
        } else if let Some(index) = COMMANDS.iter().position(|(id, _)| *id == action) {
            run_command(world, index);
        }
    }

    paint_status(world);
}

fn clear_query(world: &mut World) {
    let field = world.resource::<Palette>().unwrap().field;
    if let Some(label) = world.get_mut::<Label>(field) {
        label.text.clear();
    }
    if let Some(caret) = world.get_mut::<Caret>(field) {
        caret.index = 0;
        caret.anchor = 0;
    }
    world.resource_mut::<Palette>().unwrap().query.clear();
    let text = String::new();
    debounce(world, "filter", DEBOUNCE, move || async move {
        let hits = filter(&text);
        Filtered { query: text, hits }
    });
}

fn paint_status(world: &mut World) {
    let (status, message) = {
        let palette = world.resource::<Palette>().unwrap();
        let message = format!(
            "{} of {}  |  {}",
            palette.hits.len(),
            COMMANDS.len(),
            palette.message
        );
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
        ..WindowConfig::default()
    });
    app.world_mut()
        .insert_resource(TextFace(Arc::new(LatinFace)));
    app.world_mut().insert_resource(Theme::default());
    app.add_plugin(Chrome);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, palette_update);
    run(app);
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_records_the_matched_letters() {
        let (score, ranges) = fuzzy("sm", "Search memory").unwrap();
        assert_eq!(ranges, vec![(0, 1), (7, 8)]);
        assert_eq!(score, 6);
        let (_, tight) = fuzzy("se", "Search memory").unwrap();
        assert_eq!(tight, vec![(0, 2)]);
        assert!(fuzzy("zzz", "Search memory").is_none());
        assert!(fuzzy("  ", "Search memory").unwrap().1.is_empty());
    }
}
