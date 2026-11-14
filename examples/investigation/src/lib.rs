//! Investigation console on the existing window material.
//!
//! The window is transparent and requests [`Backdrop::Window`](rndr_core::Backdrop):
//! Mica on Windows 11, and a behind-window `NSVisualEffectView` on macOS.
//! Linux and the browser ignore `Backdrop`. The panes are dark translucent
//! fills over that material. `BackdropBlur` blurs pixels the app has already
//! drawn; it does not blur the desktop, and this example does not use it.
//!
//! Investigations, closed cases, and agents are virtual lists. Groups are a
//! [`VirtualTree`](rndr_ui::tree::VirtualTree). The panes are nested
//! [`SplitSash`](rndr_ui::split::SplitSash)es. Case and Audit Log are a
//! [`ModeSelector`](rndr_ui::controls::ModeSelector).
//!
//! ```text
//! cargo run -p investigation
//! cargo run -p investigation -- --export console.png
//! cargo run -p investigation --features observe -- --observe
//! ```
//!
//! `--export` composites the headless straight-alpha surface over a synthetic
//! desktop. A native `--observe` capture is the surface submitted to the
//! window server. Neither image includes the compositor's behind-window blur.
//!
//! # Visual gaps
//!
//! - Real behind-window blur is macOS vibrancy and Windows 11 Mica / Acrylic /
//!   Tabbed. Linux and the browser draw the translucent fills over whatever
//!   the transparent surface shows, which is often the clear color.
//! - Virtual list rows can be translucent. `VirtualTree` paints opaque row
//!   colors itself, so group rows do not show the material.
//! - The sash hit target is 8 points. A 1-point line is drawn in that slot.
//! - Outer shadows on a full-bleed window are clipped by the window. macOS
//!   still supplies the native window shadow.
//! - Quiet areas do not animate. Text uses the desktop shaper when the runner
//!   or `--export` installs Parley, and the builtin atlas otherwise.
//! - Headless and native surface copies do not show the desktop through the
//!   window. The export preview blurs a stand-in wallpaper in the tool that
//!   writes the PNG.

use rndr_core::*;
use rndr_ui::collections::{Item, ItemId, ListView, RowSlot, ViewProps, VirtualViewHandle};
use rndr_ui::menu::{open_menu, MenuAt, MenuEntry, MenuItem, MenuPicked};
use rndr_ui::split::{SplitSash, SplitSashProps};
use rndr_ui::tree::{NodeId, TreeHandle, TreeNode, TreeProps, VirtualTree};
use rndr_ui::{Appearance, UiColor, UiId, UiPlugin, UiStyle, UiTheme};

/// Window width in logical points.
pub const WIDTH: f32 = 1280.0;
/// Window height in logical points.
pub const HEIGHT: f32 = 800.0;
const BAND: f32 = 52.0;

const GLASS_SIDE: Color = Color::rgba(0.07, 0.08, 0.10, 0.78);
const GLASS_CENTER: Color = Color::rgba(0.11, 0.12, 0.14, 0.60);
const GLASS_RIGHT: Color = Color::rgba(0.08, 0.09, 0.11, 0.72);
const GLASS_SASH: Color = Color::rgba(0.12, 0.13, 0.15, 0.72);
const HAIRLINE: Color = Color::rgba(1.0, 1.0, 1.0, 0.14);
const INK: Color = Color::rgba(0.93, 0.94, 0.96, 1.0);
const MUTED: Color = Color::rgba(0.64, 0.66, 0.72, 1.0);
const FAINT: Color = Color::rgba(0.50, 0.53, 0.60, 1.0);
const CLEAR: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
const SELECTED_ROW: Color = Color::rgba(1.0, 1.0, 1.0, 0.12);
const CHIP: Color = Color::rgba(1.0, 1.0, 1.0, 0.08);
const FIELD: Color = Color::rgba(0.0, 0.0, 0.0, 0.28);
const CODE_INK: Color = Color::rgba(0.90, 0.91, 0.94, 1.0);
const CODE_BG: Color = Color::rgba(1.0, 1.0, 1.0, 0.10);
const ACCENT: Color = Color::rgba(0.62, 0.78, 1.0, 1.0);
const RED: Color = Color::rgba(0.96, 0.42, 0.38, 1.0);
const USER_BUBBLE: Color = Color::rgba(0.23, 0.38, 0.66, 0.92);
const GREEN_BG: Color = Color::rgba(0.12, 0.36, 0.26, 0.95);
const GREEN_INK: Color = Color::rgba(0.62, 0.92, 0.74, 1.0);
const RED_BG: Color = Color::rgba(0.55, 0.16, 0.16, 0.95);

const OPEN: &[CaseRow] = &[
    CaseRow {
        id: 63,
        code: "INV-0063",
        title: "Web shell on SRV-APP-02",
        status: "Contained",
        time: "2h",
        level: 4,
    },
    CaseRow {
        id: 61,
        code: "INV-0061",
        title: "Suspicious PowerShell on FIN-LAP-14",
        status: "Triaging",
        time: "5h",
        level: 3,
    },
    CaseRow {
        id: 58,
        code: "INV-0058",
        title: "Phishing wave: invoice-q3",
        status: "Open",
        time: "1d",
        level: 2,
    },
    CaseRow {
        id: 54,
        code: "INV-0054",
        title: "Unusual MFA prompt, NYC",
        status: "Contained",
        time: "2d",
        level: 1,
    },
    CaseRow {
        id: 49,
        code: "INV-0049",
        title: "Beacon to 185.22.40.11",
        status: "Open",
        time: "3d",
        level: 3,
    },
];

const CLOSED: &[CaseRow] = &[
    CaseRow {
        id: 42,
        code: "INV-0042",
        title: "Ransomware note on FS-02",
        status: "Contained",
        time: "1w",
        level: 4,
    },
    CaseRow {
        id: 38,
        code: "INV-0038",
        title: "Expired cert on edge proxy",
        status: "Closed",
        time: "2w",
        level: 1,
    },
];

const AGENTS: &[Agent] = &[
    Agent {
        id: 1,
        name: "Mara Chen",
        status: "Watching SRV-APP-02",
        paused: false,
        ink: Color::rgba(0.35, 0.55, 0.95, 1.0),
    },
    Agent {
        id: 2,
        name: "Nia Okonkwo",
        status: "Paused: usage limit reached",
        paused: true,
        ink: Color::rgba(0.75, 0.28, 0.28, 1.0),
    },
    Agent {
        id: 3,
        name: "Jonah Hale",
        status: "Coordinating containment",
        paused: false,
        ink: Color::rgba(0.28, 0.62, 0.52, 1.0),
    },
    Agent {
        id: 4,
        name: "Priya Shah",
        status: "Idle",
        paused: false,
        ink: Color::rgba(0.55, 0.42, 0.82, 1.0),
    },
];

#[derive(Clone, Copy)]
struct CaseRow {
    id: u64,
    code: &'static str,
    title: &'static str,
    status: &'static str,
    time: &'static str,
    /// 1 = low … 4 = critical. Three bars are drawn; 4 lights all of them.
    level: u8,
}

#[derive(Clone, Copy)]
struct Agent {
    id: u64,
    name: &'static str,
    status: &'static str,
    paused: bool,
    ink: Color,
}

#[derive(Clone, Copy)]
struct BuiltCase {
    code: Entity,
    title: Entity,
    status: Entity,
    time: Entity,
    bars: [Entity; 3],
}
impl Component for BuiltCase {}

#[derive(Clone, Copy)]
struct BuiltAgent {
    name: Entity,
    status: Entity,
}
impl Component for BuiltAgent {}

/// Mounted pieces. The headless test reads the panels and the open list.
/// The other handles stay so a later frame can restyle them.
#[derive(Clone)]
#[allow(dead_code)]
struct Console {
    sidebar: Entity,
    center: Entity,
    right: Entity,
    outer_sash: Entity,
    inner_sash: Entity,
    outer_line: Entity,
    inner_line: Entity,
    lights: Entity,
    spacer: Entity,
    status_button: Entity,
    status_label: Entity,
    status_pill: Entity,
    status_fill: Entity,
    case_panel: Entity,
    audit_panel: Entity,
    open_list: VirtualViewHandle,
    closed_list: VirtualViewHandle,
    agents: VirtualViewHandle,
    groups: TreeHandle,
    status: String,
}

/// Build the console into `app` and install the systems that keep it current.
///
/// Call this before [`rndr_render::run`](rndr_render::run). The window config
/// is transparent, with a custom titlebar and [`Backdrop::Window`].
pub fn install(app: &mut App) {
    app.world_mut().insert_resource(WindowConfig {
        title: "Investigation".into(),
        size: LogicalSize::new(WIDTH, HEIGHT),
        min_size: Some(LogicalSize::new(960.0, 640.0)),
        titlebar: Some(Titlebar::new(BAND).traffic_lights(14.0, 19.0)),
        transparent: true,
        backdrop: Backdrop::Window,
        ..WindowConfig::default()
    });
    app.world_mut().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        density: rndr_ui::Density::Compact,
        ..UiTheme::default()
    });
    app.world_mut()
        .insert_resource(Viewport::new(WIDTH, HEIGHT));
    app.add_plugin(UiPlugin);

    let world = app.world_mut();
    let root = spawn(
        world,
        None,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    world.insert(root, ClipRounded::radius(14.0));
    world.insert(root, Corners::all(14.0));
    world.insert(root, BorderSides::uniform(HAIRLINE, 1.0));

    let mut side = SplitSashProps::new("Sidebar and case");
    side.min_leading = 220.0;
    side.min_trailing = 520.0;
    let mut main = SplitSashProps::new("Conversation and audit");
    main.min_leading = 340.0;
    main.min_trailing = 240.0;
    // At 1280: sidebar ~300, audit ~280, center the rest. Minimums fit that width.
    let outer = SplitSash::mount(world, root, UiId(1), side, 0.24).unwrap();
    let inner = SplitSash::mount(world, outer.trailing, UiId(2), main, 0.70).unwrap();
    paint_glass(world, outer.leading, GLASS_SIDE);
    paint_glass(world, inner.leading, GLASS_CENTER);
    paint_glass(world, inner.trailing, GLASS_RIGHT);

    let outer_line = hairline_in_sash(world, outer.sash);
    let inner_line = hairline_in_sash(world, inner.sash);
    let (spacer, lights) = sidebar(world, outer.leading);
    let (status_button, status_label, status_pill, status_fill) = center(world, inner.leading);
    let (case_panel, audit_panel) = audit(world, inner.trailing);
    let slots = *world.resource::<SidebarSlots>().unwrap();
    let open_list = mount_cases(
        world,
        slots.open,
        "Investigations",
        OPEN,
        60.0,
        UiId(10),
        investigation_row,
    );
    let closed_list = mount_cases(
        world,
        slots.closed,
        "Closed cases",
        CLOSED,
        60.0,
        UiId(11),
        investigation_row,
    );
    let groups = mount_groups(world, slots.groups);
    let agents = mount_agents(world, slots.agents);
    world.insert_resource(Console {
        sidebar: outer.leading,
        center: inner.leading,
        right: inner.trailing,
        outer_sash: outer.sash,
        inner_sash: inner.sash,
        outer_line,
        inner_line,
        lights,
        spacer,
        status_button,
        status_label,
        status_pill,
        status_fill,
        case_panel,
        audit_panel,
        open_list,
        closed_list,
        agents,
        groups,
        status: "Contained".into(),
    });
    app.add_system(Schedule::Update, on_status);
    app.add_system(Schedule::Layout, polish);
    app.add_system(Schedule::Layout, hold_panes);
}

fn paint_glass(world: &mut World, entity: Entity, color: Color) {
    world.insert(entity, color);
}

fn hairline_in_sash(world: &mut World, sash: Entity) -> Entity {
    world.insert(
        sash,
        Align {
            justify: Justify::Center,
            items: Items::Center,
        },
    );
    spawn(
        world,
        Some(sash),
        Layout {
            width: Size::Px(1.0),
            height: Size::Fill,
            ..Layout::column()
        },
    )
}

fn sidebar(world: &mut World, pane: Entity) -> (Entity, Entity) {
    if let Some(layout) = world.get_mut::<Layout>(pane) {
        layout.gap = 0.0;
        layout.padding = 0.0;
    }
    let header = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            height: Size::Px(BAND),
            gap: 8.0,
            padding: 8.0,
            ..Layout::row()
        },
    );
    world.insert(
        header,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    world.insert(header, WindowDrag);
    let spacer = spawn(
        world,
        Some(header),
        Layout {
            width: Size::Px(12.0),
            height: Size::Px(1.0),
            ..Layout::row()
        },
    );
    let lights = traffic_lights(world, header);
    let _fill = flex(world, header);
    icon_button(world, header, "new", "New investigation", Icon::Plus);
    icon_button(world, header, "sidebar", "Hide sidebar", Icon::PanelLeft);
    rule(world, pane);
    let body = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 4.0,
            padding: 8.0,
            ..Layout::column()
        },
    );
    search_field(world, body);
    let slots = SidebarSlots {
        // Six visible group rows at 26 points (Endpoints and its three
        // members, then Identity and Network). Investigations scroll, so
        // that list gives up the height Network was missing.
        open: labeled_slot(world, body, "INVESTIGATIONS", 134.0),
        closed: labeled_slot(world, body, "CLOSED", 124.0),
        groups: labeled_slot(world, body, "GROUPS", 164.0),
        agents: labeled_slot(world, body, "AGENTS", 188.0),
    };
    world.insert_resource(slots);
    (spacer, lights)
}

#[derive(Clone, Copy)]
struct SidebarSlots {
    open: Entity,
    closed: Entity,
    groups: Entity,
    agents: Entity,
}

fn labeled_slot(world: &mut World, parent: Entity, title: &str, height: f32) -> Entity {
    section(world, parent, title);
    spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Px(height),
            ..Layout::column()
        },
    )
}

fn center(world: &mut World, pane: Entity) -> (Entity, Entity, Entity, Entity) {
    if let Some(layout) = world.get_mut::<Layout>(pane) {
        layout.gap = 0.0;
        layout.padding = 0.0;
    }
    let toolbar = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            height: Size::Px(BAND - 1.0),
            gap: 8.0,
            padding: 12.0,
            ..Layout::row()
        },
    );
    world.insert(
        toolbar,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    world.insert(toolbar, WindowDrag);
    icon_button(world, toolbar, "edit", "Edit case", Icon::PenLine);
    let (status_button, status_label) = status_button(world, toolbar);
    let _fill = flex(world, toolbar);
    icon_button(world, toolbar, "share", "Share", Icon::Share2);
    icon_button(
        world,
        toolbar,
        "panel",
        "Toggle audit panel",
        Icon::PanelLeft,
    );
    if let Some(icon) = world.entities_with::<Button>().into_iter().find(|entity| {
        world
            .get::<Button>(*entity)
            .is_some_and(|b| b.action == "panel")
    }) {
        flip_icon(world, icon);
    }
    rule(world, pane);
    let (pill, fill) = header(world, pane);
    let chat = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 16.0,
            padding: 16.0,
            ..Layout::column()
        },
    );
    world.insert(chat, Scroll::default());
    conversation(world, chat);
    rule(world, pane);
    composer(world, pane);
    (status_button, status_label, pill, fill)
}

fn header(world: &mut World, pane: Entity) -> (Entity, Entity) {
    let block = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            gap: 8.0,
            padding: 16.0,
            ..Layout::column()
        },
    );
    let tags = row(world, block, 8.0);
    text(world, tags, "INV-0063", 12.0, 600, MUTED, Size::Hug);
    let (pill, fill) = pill(world, tags, "Contained", GREEN_BG, GREEN_INK);
    pill_static(world, tags, "Critical", RED_BG, Color::rgb(1.0, 0.90, 0.90));
    text(
        world,
        block,
        "Web shell on SRV-APP-02",
        22.0,
        650,
        INK,
        Size::Fill,
    );
    paragraph(
        world,
        block,
        "IIS worker on the finance app host is serving a web shell. Containment holds the host online.",
        13.0,
        400,
        MUTED,
    );
    let meta = row(world, block, 14.0);
    world.insert(meta, Wrap { line_gap: 6.0 });
    meta_pair(world, meta, "Owner", "Jonah Hale");
    meta_pair(world, meta, "Lead", "Mara Chen");
    meta_pair(world, meta, "Source", "EDR");
    meta_pair(world, meta, "Opened", "08:41");
    let chips = row(world, block, 8.0);
    world.insert(chips, Wrap { line_gap: 6.0 });
    for agent in AGENTS {
        chip(world, chips, agent);
    }
    (pill, fill)
}

fn conversation(world: &mut World, parent: Entity) {
    let user = "The IIS worker is spawning w3wp.exe and still serving help.aspx. @Mara Chen can you pull the process tree?";
    user_bubble(
        world,
        parent,
        user,
        vec![
            code_span(user, "w3wp.exe"),
            code_span(user, "help.aspx"),
            mention_span(user, "@Mara Chen"),
        ],
    );
    agent_message(
        world,
        parent,
        &AGENTS[0],
        "Endpoints and EDR",
        "09:12",
        "Confirmed. w3wp.exe pid 4188 loaded a module that is not on the image baseline. Isolating SRV-APP-02.",
        &["w3wp.exe", "4188", "SRV-APP-02"],
    );
    let lead = "Status is now Contained. Keep the host online for evidence, and do not reboot until the image is copied.";
    agent_message(
        world,
        parent,
        &AGENTS[2],
        "Lead",
        "09:14",
        lead,
        &["Contained"],
    );
}

fn audit(world: &mut World, pane: Entity) -> (Entity, Entity) {
    if let Some(layout) = world.get_mut::<Layout>(pane) {
        layout.padding = 12.0;
        layout.gap = 10.0;
    }
    let (case_panel, audit_panel) = segmented(world, pane);
    case_body(world, case_panel);
    audit_body(world, audit_panel);
    (case_panel, audit_panel)
}

fn segmented(world: &mut World, pane: Entity) -> (Entity, Entity) {
    let track = spawn_colored(
        world,
        pane,
        Layout {
            width: Size::Fill,
            height: Size::Px(30.0),
            gap: 2.0,
            padding: 2.0,
            ..Layout::row()
        },
        CHIP,
    );
    world.insert(track, Radius(8.0));
    world.insert(track, WindowDrag);
    segment(world, track, "case", "Case", false);
    segment(world, track, "audit", "Audit Log", true);
    let case_panel = panel(world, pane);
    let audit_panel = panel(world, pane);
    world.insert(case_panel, Hidden);
    (case_panel, audit_panel)
}

fn segment(world: &mut World, parent: Entity, action: &str, label: &str, selected: bool) {
    let button = spawn_colored(
        world,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
        if selected {
            Color::rgba(1.0, 1.0, 1.0, 0.14)
        } else {
            CLEAR
        },
    );
    world.insert(button, Radius(6.0));
    world.insert(button, Align::center());
    world.insert(button, Button::new(action));
    world.insert(button, Focusable);
    world.insert(button, TabStop(true));
    world.insert(
        button,
        Semantics {
            name: Some(label.into()),
            ..Semantics::default()
        },
    );
    text(world, button, label, 12.0, 600, INK, Size::Hug);
}

fn panel(world: &mut World, parent: Entity) -> Entity {
    spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.0,
            ..Layout::column()
        },
    )
}

fn case_body(world: &mut World, panel: Entity) {
    section(world, panel, "CASE");
    text(
        world,
        panel,
        "Severity  Critical",
        13.0,
        400,
        INK,
        Size::Fill,
    );
    text(world, panel, "Host  SRV-APP-02", 13.0, 400, INK, Size::Fill);
    paragraph(
        world,
        panel,
        "Vector  IIS web shell via help.aspx",
        13.0,
        400,
        INK,
    );
    paragraph(
        world,
        panel,
        "Recommendation  Keep the host isolated, image memory, and rotate the app pool identity.",
        13.0,
        400,
        MUTED,
    );
}

fn audit_body(world: &mut World, panel: Entity) {
    section(world, panel, "TODAY");
    let rows = [
        Moment {
            time: "09:14",
            icon: Icon::Shield,
            title: "Status Changed",
            detail: "Open to Contained",
            actor: "Jonah Hale",
        },
        Moment {
            time: "09:11",
            icon: Icon::Search,
            title: "Finding",
            detail: "Web shell in help.aspx",
            actor: "Mara Chen",
        },
        Moment {
            time: "09:08",
            icon: Icon::FileCode,
            title: "Evidence",
            detail: "Process tree for w3wp.exe",
            actor: "Mara Chen",
        },
        Moment {
            time: "09:02",
            icon: Icon::Cpu,
            title: "Entity",
            detail: "Host SRV-APP-02 added",
            actor: "system",
        },
        Moment {
            time: "08:41",
            icon: Icon::Flag,
            title: "Opened",
            detail: "Investigation INV-0063",
            actor: "Jonah Hale",
        },
    ];
    for (index, moment) in rows.iter().enumerate() {
        timeline(world, panel, moment, index + 1 == rows.len());
    }
}

struct Moment {
    time: &'static str,
    icon: Icon,
    title: &'static str,
    detail: &'static str,
    actor: &'static str,
}

fn timeline(world: &mut World, parent: Entity, moment: &Moment, last: bool) {
    let line = row(world, parent, 8.0);
    world.insert(
        line,
        Align {
            justify: Justify::Start,
            items: Items::Start,
        },
    );
    text(world, line, moment.time, 11.0, 500, FAINT, Size::Px(44.0));
    let rail = spawn(
        world,
        Some(line),
        Layout {
            width: Size::Px(16.0),
            gap: 2.0,
            ..Layout::column()
        },
    );
    world.insert(rail, Align::center());
    let dot = spawn(
        world,
        Some(rail),
        Layout {
            width: Size::Px(18.0),
            height: Size::Px(18.0),
            ..Layout::row()
        },
    );
    world.insert(dot, Align::center());
    let icon = glyph(world, dot, moment.icon, 14.0, ACCENT);
    if let Some(style) = world.get_mut::<TextStyle>(icon) {
        style.line_height = Some(1.0);
    }
    if !last {
        spawn_colored(
            world,
            rail,
            Layout {
                width: Size::Px(1.0),
                height: Size::Px(28.0),
                ..Layout::column()
            },
            HAIRLINE,
        );
    }
    let body = spawn(
        world,
        Some(line),
        Layout {
            width: Size::Fill,
            gap: 2.0,
            ..Layout::column()
        },
    );
    text(world, body, moment.title, 13.0, 650, INK, Size::Fill);
    paragraph(world, body, moment.detail, 12.0, 400, MUTED);
    text(world, line, moment.actor, 11.0, 400, FAINT, Size::Hug);
}

fn mount_cases(
    world: &mut World,
    parent: Entity,
    name: &str,
    rows: &[CaseRow],
    height: f32,
    id: UiId,
    compose: fn(&mut World, RowSlot, &Item),
) -> VirtualViewHandle {
    let items: Vec<Item> = rows
        .iter()
        .map(|row| {
            Item::new(
                row.id,
                format!("{}, {}, {}", row.code, row.title, row.status),
            )
        })
        .collect();
    let handle = ListView::mount_virtual(
        world,
        parent,
        id,
        ViewProps::new(name),
        &items,
        height,
        compose,
    )
    .unwrap();
    clear_viewport(world, handle.root);
    size_list(world, handle.root, Size::Fill);
    if rows.iter().any(|row| row.id == 63) {
        handle.select(world, Some(ItemId(63))).unwrap();
    }
    handle
}

fn mount_agents(world: &mut World, parent: Entity) -> VirtualViewHandle {
    let items: Vec<Item> = AGENTS
        .iter()
        .map(|agent| Item::new(agent.id, format!("{}, {}", agent.name, agent.status)))
        .collect();
    let handle = ListView::mount_virtual(
        world,
        parent,
        UiId(12),
        ViewProps::new("Agents"),
        &items,
        46.0,
        agent_row,
    )
    .unwrap();
    clear_viewport(world, handle.root);
    size_list(world, handle.root, Size::Fill);
    handle
}

fn mount_groups(world: &mut World, parent: Entity) -> TreeHandle {
    let nodes = [
        TreeNode::new(100, "Endpoints")
            .icon(Icon::Users)
            .badge("3")
            .expandable(true),
        TreeNode::new(101, "A. Chen")
            .child(NodeId(100))
            .icon(Icon::User),
        TreeNode::new(102, "M. Okonkwo")
            .child(NodeId(100))
            .icon(Icon::User),
        TreeNode::new(103, "R. Shah")
            .child(NodeId(100))
            .icon(Icon::User),
        TreeNode::new(200, "Identity")
            .icon(Icon::Users)
            .badge("2")
            .expandable(true),
        TreeNode::new(201, "L. Berg")
            .child(NodeId(200))
            .icon(Icon::User),
        TreeNode::new(202, "S. Ito")
            .child(NodeId(200))
            .icon(Icon::User),
        TreeNode::new(300, "Network")
            .icon(Icon::Users)
            .badge("1")
            .expandable(true),
        TreeNode::new(301, "P. Alvarez")
            .child(NodeId(300))
            .icon(Icon::User),
    ];
    let tree = VirtualTree::mount(
        world,
        parent,
        UiId(13),
        TreeProps::new("Groups").row_height(26.0).translucent(),
        &nodes,
        &[NodeId(100)],
    )
    .unwrap();
    world.insert(
        tree.root,
        UiStyle {
            background: Some(UiColor::Explicit(CLEAR)),
            ..UiStyle::default()
        },
    );
    size_list(world, tree.root, Size::Fill);
    tree
}

fn size_list(world: &mut World, entity: Entity, height: Size) {
    if let Some(layout) = world.get_mut::<Layout>(entity) {
        layout.width = Size::Fill;
        layout.height = height;
    }
}

fn clear_viewport(world: &mut World, entity: Entity) {
    world.insert(
        entity,
        UiStyle {
            background: Some(UiColor::Explicit(CLEAR)),
            ..UiStyle::default()
        },
    );
}

fn investigation_row(world: &mut World, slot: RowSlot, item: &Item) {
    let Some(row) = OPEN
        .iter()
        .chain(CLOSED.iter())
        .find(|row| row.id == item.id.0)
        .copied()
    else {
        return;
    };
    prepare_slot(world, slot);
    let built = if let Some(built) = world.get::<BuiltCase>(slot.content).copied() {
        built
    } else {
        let built = build_case(world, slot.content);
        world.insert(slot.content, built);
        built
    };
    set_text(world, built.code, row.code);
    set_text(world, built.title, row.title);
    set_text(world, built.status, row.status);
    set_text(world, built.time, row.time);
    let color = severity_color(row.level);
    for (index, bar) in built.bars.iter().enumerate() {
        let lit = (index as u8) < row.level.min(3);
        world.insert(
            *bar,
            if lit {
                color
            } else {
                Color::rgba(1.0, 1.0, 1.0, 0.12)
            },
        );
    }
}

fn build_case(world: &mut World, parent: Entity) -> BuiltCase {
    let bars_row = spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Hug,
            height: Size::Px(16.0),
            gap: 2.0,
            ..Layout::row()
        },
    );
    world.insert(
        bars_row,
        Align {
            justify: Justify::Start,
            items: Items::End,
        },
    );
    let bars = [5.0, 9.0, 13.0].map(|height| {
        spawn_colored(
            world,
            bars_row,
            Layout {
                width: Size::Px(3.0),
                height: Size::Px(height),
                ..Layout::column()
            },
            CLEAR,
        )
    });
    let column = spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            gap: 1.0,
            ..Layout::column()
        },
    );
    let top = row(world, column, 8.0);
    world.insert(
        top,
        Align {
            justify: Justify::SpaceBetween,
            items: Items::Center,
        },
    );
    let code = text(world, top, "", 11.0, 600, FAINT, Size::Hug);
    let time = text(world, top, "", 11.0, 400, FAINT, Size::Hug);
    let title = text(world, column, "", 13.0, 600, INK, Size::Fill);
    if let Some(label) = world.get_mut::<Label>(title) {
        label.overflow = TextOverflow::Ellipsis;
    }
    let status = text(world, column, "", 11.0, 400, MUTED, Size::Fill);
    for entity in [code, title, status, time] {
        if let Some(style) = world.get_mut::<TextStyle>(entity) {
            style.line_height = Some(1.15);
        }
    }
    BuiltCase {
        code,
        title,
        status,
        time,
        bars,
    }
}

fn agent_row(world: &mut World, slot: RowSlot, item: &Item) {
    let Some(agent) = AGENTS.iter().find(|agent| agent.id == item.id.0).copied() else {
        return;
    };
    prepare_slot(world, slot);
    let built = if let Some(built) = world.get::<BuiltAgent>(slot.content).copied() {
        built
    } else {
        let disc = spawn_colored(
            world,
            slot.content,
            Layout {
                width: Size::Px(28.0),
                height: Size::Px(28.0),
                ..Layout::row()
            },
            agent.ink,
        );
        world.insert(disc, Radius(14.0));
        world.insert(disc, Align::center());
        let initial = agent.name.chars().next().unwrap_or('?').to_string();
        text(
            world,
            disc,
            &initial,
            12.0,
            650,
            Color::rgb(1.0, 1.0, 1.0),
            Size::Hug,
        );
        let column = spawn(
            world,
            Some(slot.content),
            Layout {
                width: Size::Fill,
                gap: 1.0,
                ..Layout::column()
            },
        );
        let name = text(world, column, "", 13.0, 600, INK, Size::Fill);
        let status = text(world, column, "", 11.0, 400, MUTED, Size::Fill);
        let built = BuiltAgent { name, status };
        world.insert(slot.content, built);
        built
    };
    set_text(world, built.name, agent.name);
    set_text(world, built.status, agent.status);
    if let Some(label) = world.get_mut::<Label>(built.status) {
        label.color = Some(if agent.paused { RED } else { MUTED });
    }
}

fn prepare_slot(world: &mut World, slot: RowSlot) {
    if let Some(layout) = world.get_mut::<Layout>(slot.content) {
        *layout = Layout {
            axis: Axis::Row,
            gap: 8.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        };
    }
    world.insert(
        slot.content,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    // Clear of the list scrollbar, which otherwise clips the trailing time.
    world.insert(
        slot.content,
        Padding {
            top: 6.0,
            right: 10.0,
            bottom: 4.0,
            left: 2.0,
        },
    );
    let selected = world
        .get::<SelectedState>(slot.root)
        .is_some_and(|state| state.0);
    world.insert(
        slot.root,
        UiStyle {
            background: Some(UiColor::Explicit(if selected {
                SELECTED_ROW
            } else {
                CLEAR
            })),
            ..UiStyle::default()
        },
    );
}

fn user_bubble(world: &mut World, parent: Entity, body: &str, spans: Vec<TextSpan>) {
    let line = row(world, parent, 0.0);
    world.insert(
        line,
        Align {
            justify: Justify::End,
            items: Items::Start,
        },
    );
    let bubble = spawn_colored(
        world,
        line,
        Layout {
            width: Size::Px(460.0),
            padding: 10.0,
            ..Layout::column()
        },
        USER_BUBBLE,
    );
    world.insert(bubble, Radius(12.0));
    rich(
        world,
        bubble,
        body,
        spans,
        13.0,
        Color::rgb(0.96, 0.97, 1.0),
    );
}

fn agent_message(
    world: &mut World,
    parent: Entity,
    agent: &Agent,
    role: &str,
    time: &str,
    body: &str,
    marks: &[&str],
) {
    let line = row(world, parent, 10.0);
    world.insert(
        line,
        Align {
            justify: Justify::Start,
            items: Items::Start,
        },
    );
    let disc = spawn_colored(
        world,
        line,
        Layout {
            width: Size::Px(28.0),
            height: Size::Px(28.0),
            ..Layout::row()
        },
        agent.ink,
    );
    world.insert(disc, Radius(14.0));
    world.insert(disc, Align::center());
    let initial = agent.name.chars().next().unwrap_or('?').to_string();
    text(
        world,
        disc,
        &initial,
        12.0,
        650,
        Color::rgb(1.0, 1.0, 1.0),
        Size::Hug,
    );
    let column = spawn(
        world,
        Some(line),
        Layout {
            width: Size::Fill,
            gap: 4.0,
            ..Layout::column()
        },
    );
    let who = row(world, column, 8.0);
    text(world, who, agent.name, 13.0, 650, INK, Size::Hug);
    pill_static(world, who, role, CHIP, ACCENT);
    let _fill = flex(world, who);
    text(world, who, time, 11.0, 400, FAINT, Size::Hug);
    let spans = marks
        .iter()
        .map(|mark| {
            if *mark == "Contained" {
                bold_span(body, mark)
            } else {
                code_span(body, mark)
            }
        })
        .collect();
    rich(world, column, body, spans, 13.0, INK);
}

fn composer(world: &mut World, pane: Entity) {
    let bar = spawn(
        world,
        Some(pane),
        Layout {
            width: Size::Fill,
            gap: 8.0,
            padding: 12.0,
            ..Layout::row()
        },
    );
    world.insert(
        bar,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    icon_button(world, bar, "attach", "Add", Icon::Plus);
    field(world, bar, "Message the team", "Message the team");
    icon_button(world, bar, "mic", "Dictate", Icon::Mic);
    icon_button(world, bar, "send", "Send", Icon::Send);
}

fn search_field(world: &mut World, parent: Entity) {
    let shell = spawn_colored(
        world,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Px(32.0),
            gap: 6.0,
            padding: 8.0,
            ..Layout::row()
        },
        FIELD,
    );
    world.insert(shell, Radius(8.0));
    world.insert(
        shell,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    glyph(world, shell, Icon::Search, 13.0, FAINT);
    field(world, shell, "Search Conversations", "Search Conversations");
}

fn field(world: &mut World, parent: Entity, placeholder: &str, name: &str) -> Entity {
    let entity = spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Px(24.0),
            min_height: 24.0,
            ..Layout::column()
        },
    );
    world.insert(entity, Label::new("").with_color(INK));
    world.insert(entity, LabelBox::CONTENT);
    world.insert(
        entity,
        TextStyle {
            size: 13.0,
            family: "Inter".into(),
            ..TextStyle::default()
        },
    );
    world.insert(entity, TextField);
    world.insert(entity, CapturesKeys);
    world.insert(entity, Placeholder::new(placeholder));
    world.insert(
        entity,
        Semantics {
            name: Some(name.into()),
            ..Semantics::default()
        },
    );
    entity
}

fn status_button(world: &mut World, parent: Entity) -> (Entity, Entity) {
    let button = spawn_colored(
        world,
        parent,
        Layout {
            height: Size::Px(28.0),
            gap: 6.0,
            padding: 8.0,
            ..Layout::row()
        },
        CHIP,
    );
    world.insert(button, Radius(8.0));
    world.insert(button, Align::center());
    world.insert(button, Button::new("status"));
    world.insert(button, Focusable);
    world.insert(button, TabStop(true));
    world.insert(
        button,
        Semantics {
            name: Some("Status, Contained".into()),
            ..Semantics::default()
        },
    );
    half_circle(world, button);
    let label = text(world, button, "Contained", 13.0, 600, INK, Size::Hug);
    glyph(world, button, Icon::ChevronDown, 12.0, MUTED);
    (button, label)
}

fn half_circle(world: &mut World, parent: Entity) {
    let disc = spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Px(14.0),
            height: Size::Px(14.0),
            ..Layout::row()
        },
    );
    world.insert(
        disc,
        Border {
            color: ACCENT,
            width: 1.5,
        },
    );
    world.insert(disc, ClipRounded::radius(7.0));
    spawn_colored(
        world,
        disc,
        Layout {
            width: Size::Px(7.0),
            height: Size::Px(14.0),
            ..Layout::column()
        },
        ACCENT,
    );
}

fn icon_button(world: &mut World, parent: Entity, action: &str, name: &str, icon: Icon) -> Entity {
    let button = spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Px(28.0),
            height: Size::Px(28.0),
            ..Layout::row()
        },
    );
    world.insert(button, Radius(7.0));
    world.insert(button, Align::center());
    world.insert(button, Button::new(action));
    world.insert(button, Focusable);
    world.insert(button, TabStop(true));
    world.insert(
        button,
        Semantics {
            name: Some(name.into()),
            ..Semantics::default()
        },
    );
    glyph(world, button, icon, 15.0, INK);
    button
}

fn flip_icon(world: &mut World, button: Entity) {
    if let Some(icon) = children_of(world, button).into_iter().next() {
        world.insert(
            icon,
            Transform {
                scale: [-1.0, 1.0],
                ..Transform::IDENTITY
            },
        );
    }
}

fn traffic_lights(world: &mut World, parent: Entity) -> Entity {
    let row = spawn(
        world,
        Some(parent),
        Layout {
            gap: 6.0,
            ..Layout::row()
        },
    );
    world.insert(row, Align::center());
    for (button, color, name) in [
        (WindowButton::Close, Color::rgb(1.0, 0.373, 0.341), "Close"),
        (
            WindowButton::Minimize,
            Color::rgb(0.996, 0.737, 0.180),
            "Minimize",
        ),
        (
            WindowButton::Maximize,
            Color::rgb(0.157, 0.784, 0.251),
            "Zoom",
        ),
    ] {
        let dot = spawn_colored(
            world,
            row,
            Layout {
                width: Size::Px(12.0),
                height: Size::Px(12.0),
                ..Layout::row()
            },
            color,
        );
        world.insert(dot, Radius(6.0));
        world.insert(dot, button);
        world.insert(
            dot,
            Semantics {
                name: Some(name.into()),
                ..Semantics::default()
            },
        );
    }
    row
}

fn chip(world: &mut World, parent: Entity, agent: &Agent) {
    let chip = spawn_colored(
        world,
        parent,
        Layout {
            height: Size::Px(24.0),
            gap: 6.0,
            padding: 4.0,
            ..Layout::row()
        },
        CHIP,
    );
    world.insert(chip, Radius(12.0));
    world.insert(chip, Align::center());
    let disc = spawn_colored(
        world,
        chip,
        Layout {
            width: Size::Px(16.0),
            height: Size::Px(16.0),
            ..Layout::row()
        },
        agent.ink,
    );
    world.insert(disc, Radius(8.0));
    world.insert(disc, Align::center());
    let initial = agent.name.chars().next().unwrap_or('?').to_string();
    text(
        world,
        disc,
        &initial,
        9.0,
        650,
        Color::rgb(1.0, 1.0, 1.0),
        Size::Hug,
    );
    text(world, chip, agent.name, 11.0, 500, INK, Size::Hug);
}

fn pill(world: &mut World, parent: Entity, label: &str, bg: Color, fg: Color) -> (Entity, Entity) {
    let chip = spawn_colored(
        world,
        parent,
        Layout {
            height: Size::Px(20.0),
            padding: 6.0,
            ..Layout::row()
        },
        bg,
    );
    world.insert(chip, Radius(10.0));
    world.insert(chip, Align::center());
    let text = text(world, chip, label, 11.0, 650, fg, Size::Hug);
    (text, chip)
}

fn pill_static(world: &mut World, parent: Entity, label: &str, bg: Color, fg: Color) {
    let _ = pill(world, parent, label, bg, fg);
}

fn meta_pair(world: &mut World, parent: Entity, key: &str, value: &str) {
    let pair = row(world, parent, 4.0);
    text(world, pair, key, 11.0, 400, FAINT, Size::Hug);
    text(world, pair, value, 11.0, 600, INK, Size::Hug);
}

fn section(world: &mut World, parent: Entity, label: &str) {
    let entity = text(world, parent, label, 11.0, 650, FAINT, Size::Fill);
    if let Some(style) = world.get_mut::<TextStyle>(entity) {
        style.letter_spacing = 0.6;
    }
}

fn rule(world: &mut World, parent: Entity) {
    spawn_colored(
        world,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Px(1.0),
            ..Layout::column()
        },
        HAIRLINE,
    );
}

fn row(world: &mut World, parent: Entity, gap: f32) -> Entity {
    spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            gap,
            ..Layout::row()
        },
    )
}

fn flex(world: &mut World, parent: Entity) -> Entity {
    spawn(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Px(1.0),
            ..Layout::row()
        },
    )
}

fn text(
    world: &mut World,
    parent: Entity,
    value: &str,
    size: f32,
    weight: u16,
    color: Color,
    width: Size,
) -> Entity {
    let entity = spawn(
        world,
        Some(parent),
        Layout {
            width,
            ..Layout::column()
        },
    );
    world.insert(
        entity,
        Label::new(value)
            .with_color(color)
            .overflow(TextOverflow::Visible),
    );
    world.insert(entity, LabelBox::CONTENT);
    world.insert(
        entity,
        TextStyle {
            size,
            weight,
            family: "Inter".into(),
            line_height: Some(1.3),
            ..TextStyle::default()
        },
    );
    entity
}

fn paragraph(
    world: &mut World,
    parent: Entity,
    value: &str,
    size: f32,
    weight: u16,
    color: Color,
) -> Entity {
    let entity = text(world, parent, value, size, weight, color, Size::Fill);
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.overflow = TextOverflow::Wrap;
    }
    entity
}

fn rich(
    world: &mut World,
    parent: Entity,
    value: &str,
    spans: Vec<TextSpan>,
    size: f32,
    color: Color,
) {
    let entity = paragraph(world, parent, value, size, 400, color);
    world.insert(entity, Spans { runs: spans });
}

fn glyph(world: &mut World, parent: Entity, icon: Icon, size: f32, color: Color) -> Entity {
    text(
        world,
        parent,
        &icon.scalar().to_string(),
        size,
        400,
        color,
        Size::Hug,
    )
}

fn code_span(text: &str, needle: &str) -> TextSpan {
    let start = text.find(needle).unwrap_or(0);
    TextSpan::new(start, start + needle.len(), CODE_INK)
        .highlight(CODE_BG)
        .family("monospace")
}

fn bold_span(text: &str, needle: &str) -> TextSpan {
    let start = text.find(needle).unwrap_or(0);
    TextSpan::new(start, start + needle.len(), INK).weight(700)
}

fn mention_span(text: &str, needle: &str) -> TextSpan {
    let start = text.find(needle).unwrap_or(0);
    TextSpan::new(start, start + needle.len(), ACCENT).weight(650)
}

fn severity_color(level: u8) -> Color {
    match level {
        4 => Color::rgb(0.95, 0.34, 0.31),
        3 => Color::rgb(0.95, 0.55, 0.24),
        2 => Color::rgb(0.90, 0.76, 0.28),
        _ => Color::rgb(0.55, 0.58, 0.64),
    }
}

fn set_text(world: &mut World, entity: Entity, value: &str) {
    if let Some(label) = world.get_mut::<Label>(entity) {
        if label.text != value {
            label.text = value.to_string();
        }
    }
}

fn spawn(world: &mut World, parent: Option<Entity>, layout: Layout) -> Entity {
    let entity = world.spawn((layout,));
    if let Some(parent) = parent {
        set_parent(world, entity, parent);
    }
    entity
}

fn spawn_colored(world: &mut World, parent: Entity, layout: Layout, color: Color) -> Entity {
    let entity = spawn(world, Some(parent), layout);
    world.insert(entity, color);
    entity
}

fn on_status(world: &mut World) {
    let fired = world
        .resource::<Fired>()
        .and_then(|fired| fired.name.clone());
    if fired.as_deref() == Some("case") || fired.as_deref() == Some("audit") {
        show_segment(world, fired.as_deref() == Some("audit"));
    }
    let opened = fired.as_deref() == Some("status");
    if opened {
        let (anchor, current) = {
            let console = world.resource::<Console>().unwrap();
            (console.status_button, console.status.clone())
        };
        let entries = ["Contained", "Triaging", "Open", "Closed"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                MenuEntry::Item(MenuItem::new(index as u64 + 1, label).checked(label == current))
            })
            .collect::<Vec<_>>();
        let _ = open_menu(world, anchor, MenuAt::Below, &entries);
    }
    let picks: Vec<u64> = events::<MenuPicked>(world)
        .iter()
        .map(|pick| pick.item)
        .collect();
    for item in picks {
        let Some(label) = item
            .checked_sub(1)
            .and_then(|index| ["Contained", "Triaging", "Open", "Closed"].get(index as usize))
        else {
            continue;
        };
        apply_status(world, label);
    }
}

fn apply_status(world: &mut World, status: &str) {
    let (label, pill, fill, button) = {
        let console = world.resource::<Console>().unwrap();
        (
            console.status_label,
            console.status_pill,
            console.status_fill,
            console.status_button,
        )
    };
    set_text(world, label, status);
    set_text(world, pill, status);
    let (bg, fg) = match status {
        "Contained" => (GREEN_BG, GREEN_INK),
        "Triaging" => (
            Color::rgba(0.45, 0.32, 0.10, 0.95),
            Color::rgb(1.0, 0.86, 0.55),
        ),
        "Open" => (Color::rgba(0.16, 0.28, 0.48, 0.95), ACCENT),
        _ => (CHIP, MUTED),
    };
    world.insert(fill, bg);
    if let Some(text) = world.get_mut::<Label>(pill) {
        text.color = Some(fg);
    }
    if let Some(semantics) = world.get_mut::<Semantics>(button) {
        semantics.name = Some(format!("Status, {status}"));
    }
    if let Some(console) = world.resource_mut::<Console>() {
        console.status = status.to_string();
    }
}

fn show_segment(world: &mut World, audit: bool) {
    let Some(console) = world.resource::<Console>().cloned() else {
        return;
    };
    if audit {
        world.insert(console.case_panel, Hidden);
        world.remove::<Hidden>(console.audit_panel);
    } else {
        world.insert(console.audit_panel, Hidden);
        world.remove::<Hidden>(console.case_panel);
    }
    paint_segment(world, "case", !audit);
    paint_segment(world, "audit", audit);
}

fn paint_segment(world: &mut World, action: &str, selected: bool) {
    let Some(button) = world.entities_with::<Button>().into_iter().find(|entity| {
        world
            .get::<Button>(*entity)
            .is_some_and(|b| b.action == action)
    }) else {
        return;
    };
    world.insert(
        button,
        if selected {
            Color::rgba(1.0, 1.0, 1.0, 0.14)
        } else {
            CLEAR
        },
    );
}

fn hold_panes(world: &mut World) {
    let width = world
        .resource::<Viewport>()
        .map(|viewport| viewport.width)
        .unwrap_or(0.0);
    let Some(console) = world.resource::<Console>().cloned() else {
        return;
    };
    let right = world
        .get::<Rect>(console.right)
        .map(|rect| rect.w)
        .unwrap_or(0.0);
    if width < 1100.0 || right >= 220.0 {
        return;
    }
    let Some(split_root) = world
        .get::<Parent>(console.sidebar)
        .map(|parent| parent.entity)
    else {
        return;
    };
    let Some(window) = world.get::<Parent>(split_root).map(|parent| parent.entity) else {
        return;
    };
    let mut side = SplitSashProps::new("Sidebar and case");
    side.min_leading = 220.0;
    side.min_trailing = 520.0;
    let mut main = SplitSashProps::new("Conversation and audit");
    main.min_leading = 340.0;
    main.min_trailing = 240.0;
    if let Ok(outer) = SplitSash::mount(world, window, UiId(1), side, 0.24) {
        let _ = SplitSash::mount(world, outer.trailing, UiId(2), main, 0.70);
    }
}

fn polish(world: &mut World) {
    let Some(console) = world.resource::<Console>().cloned() else {
        return;
    };
    world.insert(console.sidebar, GLASS_SIDE);
    world.insert(console.center, GLASS_CENTER);
    world.insert(console.right, GLASS_RIGHT);
    world.insert(console.outer_sash, GLASS_SASH);
    world.insert(console.inner_sash, GLASS_SASH);
    world.insert(console.outer_line, HAIRLINE);
    world.insert(console.inner_line, HAIRLINE);
    let chrome = world
        .resource::<WindowChrome>()
        .copied()
        .unwrap_or_default();
    if chrome.draws_controls {
        world.remove::<Hidden>(console.lights);
    } else {
        world.insert(console.lights, Hidden);
    }
    let width = if chrome.draws_controls {
        4.0
    } else {
        (chrome.leading + 8.0).max(68.0)
    };
    if let Some(layout) = world.get_mut::<Layout>(console.spacer) {
        layout.width = Size::Px(width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_mounts_the_sample_case() {
        let mut app = App::new();
        install(&mut app);
        for _ in 0..4 {
            app.update();
        }
        let world = app.world();
        let config = world.resource::<WindowConfig>().unwrap();
        assert!(config.transparent);
        assert_eq!(config.backdrop, Backdrop::Window);
        assert!(config.titlebar.is_some());
        let labels: Vec<String> = world
            .entities_with::<Label>()
            .into_iter()
            .map(|entity| world.get::<Label>(entity).unwrap().text.clone())
            .collect();
        let joined = labels.join("\n");
        for needle in [
            "INV-0063",
            "Web shell on SRV-APP-02",
            "help.aspx",
            "w3wp.exe",
            "Audit Log",
            "Status Changed",
            "Paused: usage limit reached",
            "Endpoints",
            "Network",
        ] {
            assert!(joined.contains(needle), "missing {needle}\n{joined}");
        }
        let hints: Vec<String> = world
            .entities_with::<Placeholder>()
            .into_iter()
            .map(|entity| world.get::<Placeholder>(entity).unwrap().text.clone())
            .collect();
        assert!(hints.iter().any(|hint| hint == "Search Conversations"));
        assert!(hints.iter().any(|hint| hint == "Message the team"));
        assert!(world.entities_with::<Splitter>().len() >= 2);
        let console = world.resource::<Console>().unwrap();
        assert!(world.has::<Hidden>(console.case_panel));
        assert!(!world.has::<Hidden>(console.audit_panel));
        assert_eq!(console.open_list.selected(world), Some(ItemId(63)));
        let side = world.get::<Rect>(console.sidebar).unwrap();
        let center = world.get::<Rect>(console.center).unwrap();
        let right = world.get::<Rect>(console.right).unwrap();
        assert!(side.w > 240.0 && side.w < 380.0, "sidebar width {}", side.w);
        assert!(
            right.w > 240.0,
            "audit pane width {} (center {})",
            right.w,
            center.w
        );
        assert!(
            center.w < right.w * 3.0,
            "center {} is too wide beside audit {}",
            center.w,
            right.w
        );
        let network = label_rect(world, "Network");
        let agents = label_rect(world, "AGENTS");
        assert!(
            network.y + network.h <= agents.y + 0.5,
            "Network {:?} is clipped by AGENTS {:?}",
            network,
            agents
        );
        assert!(
            network.y + network.h <= side.y + side.h,
            "Network {:?} falls outside the sidebar {:?}",
            network,
            side
        );
        let agents_list = world.get::<Rect>(console.agents.root).unwrap();
        assert!(
            agents_list.y + agents_list.h <= side.y + side.h + 0.5,
            "agents {:?} fall outside the sidebar {:?}",
            agents_list,
            side
        );
    }

    fn label_rect(world: &rndr_core::World, text: &str) -> Rect {
        let entity = world
            .entities_with::<Label>()
            .into_iter()
            .find(|entity| world.get::<Label>(*entity).unwrap().text == text)
            .unwrap_or_else(|| panic!("missing label {text}"));
        world
            .get::<Rect>(entity)
            .copied()
            .unwrap_or_else(|| panic!("{text} has no rect"))
    }
}
