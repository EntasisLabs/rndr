//! Seeded random-input soak for every `rndr_ui` component family and every
//! canonical Liquid block.
//!
//! The gallery is a list of scenes. Each scene is mounted on [`rndr_core::Sim`],
//! driven for a fixed number of frames, then torn down through the component's
//! own destroy or unmount path. A failure names the seed and frame.
//!
//! `fuzz_gallery_fixed_seed` is the CI run (a few thousand frames, one seed).
//! The longer soak is ignored:
//!
//! ```text
//! RNDR_FUZZ_SEED=1 RNDR_FUZZ_FRAMES=20000 \
//!   cargo test -p rndr_liquid_ui --test fuzz_gallery -- --ignored fuzz_gallery_soak --exact
//! ```
//!
//! Input mapping is pinned to [`Platform::X11`] so macOS, Windows, and Linux
//! runners share one chord table. Ctrl is the primary modifier.

use std::collections::{BTreeSet, HashSet};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::time::Duration;

use rndr_core::editing::SourceDocuments;
use rndr_core::structured::{AtomicPayload, BlockKind, StructuredDocument};
use rndr_core::structured_native::StructuredDocuments;
use rndr_core::*;
use rndr_liquid::{parse_fence, LiquidDocument, Segment, FENCE_LANGS};
use rndr_liquid_ui::{dispatch, liquid_view, LiquidUiPlugin, NoHost};
use rndr_ui::collections::{
    Item, ItemId, ListView, SelectionMode, TreeNode, TreeView, ViewHandle, ViewProps,
    VirtualViewHandle,
};
use rndr_ui::completion::{apply_completion_popups, CompletionPopup};
use rndr_ui::controls::{
    Button as UiButton, ButtonProps, ButtonVariant, ControlHandle, HostIntent, IconButton,
    ModeHandle, ModeSelector, NavigationMeta, NavigationRow, SourceAction, SourceLocator, Toolbar,
    ToolbarContentWidth, ToolbarHandle,
};
use rndr_ui::data::{
    CellAlignment, Choice, Column, ColumnId, DataTable, GroupId, InspectorProps, ObjectId,
    Property, PropertyGroup, PropertyId, PropertyInspector, PropertyKind, PropertySlot,
    PropertyValue, TableProps,
};
use rndr_ui::diff::{DiffHandle, DiffMarker, DiffProps, DiffRow, DiffView, HunkId};
use rndr_ui::editing::{EditorSurface, EditorSurfaceHandle, EditorSurfaceProps};
use rndr_ui::menu::{open_menu, MenuAt, MenuEntry, MenuHandle, MenuItem};
use rndr_ui::selection::{
    Checkbox, MultiSelect, OptionId, SelectOption, SelectionHandle, SelectionProps, SingleSelect,
};
use rndr_ui::source_tools::{apply_find_bars, FindBar};
use rndr_ui::structured::{StructuredCommand, StructuredSurface, StructuredSurfaceProps};
use rndr_ui::structured_embeds::AtomicPanelHandle;
use rndr_ui::{
    apply_theme_bridge, reading_layout, section_layout, separator, text, window_controls, Callout,
    Card, Feedback, Surface, Typography, UiHandle, UiId, UiPlugin, UiTheme, WorkspaceLayout,
};

const CI_SEED: u64 = 0xC0FF_EE00;
const FRAMES_PER_SCENE: u64 = 150;
const SCENE_COUNT: u64 = 14;
const CI_FRAMES: u64 = FRAMES_PER_SCENE * SCENE_COUNT;
const SOAK_FRAMES: u64 = 20_000;

const CANONICAL_BLOCKS: &[&str] = &[
    "card",
    "carousel",
    "actions",
    "callout",
    "section",
    "block",
    "chips",
    "media",
    "cite",
    "compare",
    "plan",
    "timeline",
    "shortlist",
    "decision",
    "brief",
    "dashboard",
    "chart",
    "report",
    "slides",
    "tabs",
    "steps",
    "recipe",
    "accordion",
    "code",
    "tree",
    "kanban",
    "feed",
];

const SCENES: [SceneId; SCENE_COUNT as usize] = [
    SceneId::Surfaces,
    SceneId::Controls,
    SceneId::Collections,
    SceneId::Virtual,
    SceneId::Table,
    SceneId::Inspector,
    SceneId::Selection,
    SceneId::Diff,
    SceneId::Editor,
    SceneId::Structured,
    SceneId::Menu,
    SceneId::Window,
    SceneId::Mixed,
    SceneId::Liquid,
];

const TEXT_SAMPLES: &[&str] = &[
    "a",
    "Hello",
    " ",
    "line\nnext",
    "e\u{301}",
    "a\u{0301}\u{0327}",
    "ن\u{0651}",
    "日本語",
    "汉字",
    "한국어",
    "مرحبا",
    "שלום",
    "a日本語b",
    "\u{202E}abc",
    "👩\u{200D}💻",
    "🏳️\u{200D}🌈",
    "👍",
    "\u{200D}",
    "\u{200E}ok\u{200F}",
    "e\u{0301}\u{0327}漢",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneId {
    Surfaces,
    Controls,
    Collections,
    Virtual,
    Table,
    Inspector,
    Selection,
    Diff,
    Editor,
    Structured,
    Menu,
    Window,
    Mixed,
    Liquid,
}

impl SceneId {
    fn name(self) -> &'static str {
        match self {
            Self::Surfaces => "surfaces",
            Self::Controls => "controls",
            Self::Collections => "collections",
            Self::Virtual => "virtual-lists",
            Self::Table => "table",
            Self::Inspector => "inspector",
            Self::Selection => "selection",
            Self::Diff => "diff",
            Self::Editor => "editor",
            Self::Structured => "structured",
            Self::Menu => "menu",
            Self::Window => "window-controls",
            Self::Mixed => "mixed",
            Self::Liquid => "liquid",
        }
    }
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn gen_range(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() as usize) % n
        }
    }

    fn chance(&mut self, numer: u64, denom: u64) -> bool {
        denom > 0 && self.next_u64() % denom < numer
    }

    fn f32(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next_u64() >> 11) as f32 / ((1u64 << 53) as f32);
        lo + (hi - lo) * unit
    }
}

struct Here {
    seed: u64,
    frame: u64,
    scene: &'static str,
}

impl Here {
    fn check(&self, cond: bool, message: impl std::fmt::Display) {
        if !cond {
            panic!(
                "fuzz gallery failed seed={:#x} frame={} scene={}: {message}",
                self.seed, self.frame, self.scene
            );
        }
    }
}

enum TrackedList {
    Retained {
        handle: ViewHandle,
        ids: BTreeSet<ItemId>,
    },
    Virtual {
        handle: VirtualViewHandle,
        ids: BTreeSet<ItemId>,
    },
}

struct TrackedSelect {
    handle: SelectionHandle,
    options: Vec<OptionId>,
}

enum SceneBody {
    Surfaces(Vec<UiHandle>),
    Controls {
        controls: Vec<ControlHandle>,
        modes: ModeHandle,
        toolbar: ToolbarHandle,
    },
    Collections(Vec<ViewHandle>),
    Virtual(Vec<VirtualViewHandle>),
    Table(rndr_ui::data::TableHandle),
    Inspector(rndr_ui::data::InspectorHandle),
    Selection(Vec<SelectionHandle>),
    Diff(DiffHandle),
    Editor {
        surface: EditorSurfaceHandle,
        completion: CompletionPopup,
    },
    Structured {
        surface: rndr_ui::structured::StructuredSurfaceHandle,
        panel: AtomicPanelHandle,
    },
    Menu {
        anchor: ControlHandle,
    },
    Window,
    Mixed {
        controls: Vec<ControlHandle>,
        selects: Vec<SelectionHandle>,
        list: ViewHandle,
    },
    Liquid,
}

struct Mounted {
    column: Entity,
    body: SceneBody,
    lists: Vec<TrackedList>,
    selects: Vec<TrackedSelect>,
    menus: Vec<MenuHandle>,
    menu_anchor: Option<Entity>,
}

struct PointerBody {
    position: [f32; 2],
    down: bool,
    button: PointerButton,
    pressed: bool,
    released: bool,
    press_at: Option<[f32; 2]>,
}

struct TextFrame {
    scroll: [f32; 2],
    keys: Vec<KeyEvent>,
    chars: String,
    preedit: String,
    commit: String,
    ime: bool,
    modifiers: Modifiers,
}

struct Harness {
    sim: Sim,
    rng: Rng,
    seed: u64,
    shell: Entity,
    baseline: HashSet<Entity>,
    mounted: Option<Mounted>,
    scene_name: &'static str,
    scene_slot: usize,
    pointer: PointerBody,
    viewport: [f32; 2],
}

#[test]
fn gallery_scenes_cover_ui_families_and_liquid_blocks() {
    let names: Vec<_> = SCENES.iter().copied().map(SceneId::name).collect();
    assert_eq!(
        names,
        [
            "surfaces",
            "controls",
            "collections",
            "virtual-lists",
            "table",
            "inspector",
            "selection",
            "diff",
            "editor",
            "structured",
            "menu",
            "window-controls",
            "mixed",
            "liquid",
        ]
    );
    assert_liquid_catalog();
}

#[test]
fn fuzz_gallery_fixed_seed() {
    run(CI_SEED, CI_FRAMES);
}

#[test]
#[ignore = "longer soak; RNDR_FUZZ_SEED and RNDR_FUZZ_FRAMES override the defaults"]
fn fuzz_gallery_soak() {
    let seed = std::env::var("RNDR_FUZZ_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x50A6_5EED);
    let frames = std::env::var("RNDR_FUZZ_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|frames: &u64| *frames > 0)
        .unwrap_or(SOAK_FRAMES);
    run(seed, frames);
}

fn run(seed: u64, frames: u64) {
    assert_liquid_catalog();
    let mut harness = Harness::new(seed);
    for frame in 0..frames {
        let result = catch_unwind(AssertUnwindSafe(|| harness.step(frame)));
        if let Err(payload) = result {
            eprintln!("rndr fuzz gallery failure seed={seed:#x} frame={frame}");
            resume_unwind(payload);
        }
    }
    let result = catch_unwind(AssertUnwindSafe(|| harness.teardown(frames)));
    if let Err(payload) = result {
        eprintln!("rndr fuzz gallery failure seed={seed:#x} frame={frames} (teardown)");
        resume_unwind(payload);
    }
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut sim = Sim::with_seed(seed);
        sim.app.add_plugin(UiPlugin);
        sim.app.add_plugin(MarkdownPlugin);
        sim.app.add_plugin(LiquidUiPlugin::new(NoHost));
        sim.app
            .add_system(Schedule::Update, apply_find_bars)
            .add_system(Schedule::Update, apply_completion_popups);
        let world = sim.world();
        apply_theme_bridge(world, UiTheme::default());
        world.insert_resource(Platform::X11);
        world.insert_resource(Viewport::new(960.0, 640.0));
        world.insert_resource(WindowChrome {
            leading: 0.0,
            trailing: 0.0,
            draws_controls: true,
            resize_edges: true,
            maximized: false,
            hovered: None,
        });
        world.insert_resource(Clipboard::default());
        world.insert_resource(SourceDocuments::default());
        world.insert_resource(StructuredDocuments::default());
        world.insert_resource(Input::default());
        let shell = world.spawn((Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },));
        sim.update();
        let baseline = census(sim.world());
        Self {
            sim,
            rng: Rng::new(seed),
            seed,
            shell,
            baseline,
            mounted: None,
            scene_name: "shell",
            scene_slot: 0,
            pointer: PointerBody {
                position: [480.0, 320.0],
                down: false,
                button: PointerButton::Left,
                pressed: false,
                released: false,
                press_at: None,
            },
            viewport: [960.0, 640.0],
        }
    }

    fn step(&mut self, frame: u64) {
        if self.mounted.is_none() || (frame > 0 && frame.is_multiple_of(FRAMES_PER_SCENE)) {
            self.teardown(frame);
            let id = SCENES[self.scene_slot % SCENES.len()];
            self.scene_slot += 1;
            self.mount(id, frame);
            self.check(frame);
        }
        self.drive();
        if self.rng.chance(1, 25) {
            self.sim
                .advance(Duration::from_millis(self.rng.gen_range(800) as u64 + 50));
        } else {
            self.sim.advance(Duration::from_millis(16));
        }
        self.sim.update();
        self.check(frame);
    }

    fn mount(&mut self, id: SceneId, frame: u64) {
        self.scene_name = id.name();
        let column = self.column();
        let (body, lists, selects, menu_anchor) = match id {
            SceneId::Surfaces => (self.mount_surfaces(column), vec![], vec![], None),
            SceneId::Controls => (self.mount_controls(column), vec![], vec![], None),
            SceneId::Collections => self.mount_collections(column),
            SceneId::Virtual => self.mount_virtual(column),
            SceneId::Table => self.mount_table(column),
            SceneId::Inspector => (self.mount_inspector(column), vec![], vec![], None),
            SceneId::Selection => self.mount_selection(column),
            SceneId::Diff => self.mount_diff(column),
            SceneId::Editor => (self.mount_editor(column), vec![], vec![], None),
            SceneId::Structured => (self.mount_structured(column, frame), vec![], vec![], None),
            SceneId::Menu => self.mount_menu(column),
            SceneId::Window => (self.mount_window(column), vec![], vec![], None),
            SceneId::Mixed => self.mount_mixed(column),
            SceneId::Liquid => (self.mount_liquid(column), vec![], vec![], None),
        };
        self.mounted = Some(Mounted {
            column,
            body,
            lists,
            selects,
            menus: Vec::new(),
            menu_anchor,
        });
        if menu_anchor.is_some() {
            self.open_fresh_menu();
        }
        self.sim.update();
    }

    fn column(&mut self) -> Entity {
        let shell = self.shell;
        let world = self.sim.world();
        let column = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(560.0),
                padding: 8.0,
                gap: 8.0,
                ..Layout::column()
            },
            Scroll::default(),
        ));
        set_parent(world, column, shell);
        column
    }

    fn mount_surfaces(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let theme = UiTheme::default();
        let workspace = WorkspaceLayout::resolve(640.0, &theme);
        let reading = world.spawn((reading_layout(workspace.available, &theme),));
        set_parent(world, reading, column);
        let section = world.spawn((section_layout(&theme),));
        set_parent(world, section, column);
        let _ = text(
            world,
            reading,
            "Gallery 日本語 / العربية / e\u{301} / 👩🏽‍💻",
            Typography::Title,
        );
        let _ = separator(world, reading);
        let mut handles = Vec::new();
        let card = Card::mount(world, section, UiId(1), "Reading").expect("card");
        let body = text(
            world,
            card.content,
            "Wrapped reading text with a combining mark e\u{301}.",
            Typography::Reading,
        )
        .expect("card text");
        card.adopt(world, body).expect("adopt card");
        handles.push(card);
        for (index, feedback) in [
            Feedback::Info,
            Feedback::Success,
            Feedback::Warning,
            Feedback::Danger,
        ]
        .into_iter()
        .enumerate()
        {
            let callout = Callout::mount(
                world,
                section,
                UiId(index as u64 + 2),
                feedback,
                "Written meaning",
            )
            .expect("callout");
            let note = text(
                world,
                callout.content,
                "Callout body",
                Typography::Secondary,
            )
            .expect("callout text");
            callout.adopt(world, note).expect("adopt callout");
            handles.push(callout);
        }
        for (index, surface) in [
            Surface::Canvas,
            Surface::Navigation,
            Surface::Content,
            Surface::Raised,
            Surface::Overlay,
        ]
        .into_iter()
        .enumerate()
        {
            let handle = surface
                .mount(world, section, UiId(index as u64 + 10))
                .expect("surface");
            let note = text(
                world,
                handle.content,
                &format!("{surface:?}"),
                Typography::Label,
            )
            .expect("surface text");
            handle.adopt(world, note).expect("adopt surface");
            handles.push(handle);
        }
        SceneBody::Surfaces(handles)
    }

    fn mount_controls(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let mut controls = Vec::new();
        for (index, variant) in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Quiet,
            ButtonVariant::Destructive,
        ]
        .into_iter()
        .enumerate()
        {
            let mut props = ButtonProps::new(
                format!("Run {index}"),
                HostIntent::Command(index as u64 + 1),
            );
            props.variant = variant;
            if index == 3 {
                props.description = "Destructive, still explicit".into();
            }
            controls.push(
                UiButton::mount(world, column, UiId(index as u64 + 1), props).expect("button"),
            );
        }
        let mut disabled = ButtonProps::new("Wait", HostIntent::Command(8));
        disabled.enabled = false;
        controls.push(UiButton::mount(world, column, UiId(8), disabled).expect("disabled"));
        let mut readonly = ButtonProps::new("Look", HostIntent::Command(9));
        readonly.readonly = true;
        controls.push(UiButton::mount(world, column, UiId(9), readonly).expect("readonly"));
        controls.push(
            IconButton::mount(
                world,
                column,
                UiId(10),
                "★",
                "Favorite",
                ButtonProps::new("★", HostIntent::Command(10)),
            )
            .expect("icon"),
        );
        controls.push(
            NavigationRow::mount(
                world,
                column,
                UiId(11),
                ButtonProps::new("Library", HostIntent::Command(11)),
                NavigationMeta {
                    selected: true,
                    secondary: Some("12".into()),
                    dirty: true,
                },
            )
            .expect("navigation"),
        );
        controls.push(
            SourceAction::mount(
                world,
                column,
                UiId(12),
                SourceLocator {
                    document: 1,
                    revision: 1,
                    anchor: 4,
                },
                true,
            )
            .expect("source"),
        );
        let toolbar = Toolbar::mount(world, column, UiId(20), 420.0).expect("toolbar");
        for (index, label) in ["Cut", "Copy", "Paste"].into_iter().enumerate() {
            let handle = UiButton::mount(
                world,
                toolbar.root,
                UiId(index as u64 + 1),
                ButtonProps::new(label, HostIntent::Command(20 + index as u64)),
            )
            .expect("toolbar button");
            world.insert(handle.root, ToolbarContentWidth);
            controls.push(handle);
        }
        let modes = ModeSelector::mount(
            world,
            column,
            UiId(30),
            "Document",
            &["Source", "Preview"],
            TabOrientation::Horizontal,
            0,
        )
        .expect("modes");
        let _ = modes.fit(world, 420.0);
        SceneBody::Controls {
            controls,
            modes,
            toolbar,
        }
    }

    fn mount_collections(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let items = sample_items(6);
        let mut range = ViewProps::new("Range list");
        range.selection = SelectionMode::Range;
        let list =
            ListView::mount(world, column, UiId(1), range, &items, compose_row).expect("list");
        scroll_port(world, list.root, 140.0);
        let mut single = ViewProps::new("Tree");
        single.selection = SelectionMode::Single;
        let nodes = vec![
            TreeNode {
                item: Item::new(1, "src"),
                parent: None,
            },
            TreeNode {
                item: Item::new(2, "lib.rs"),
                parent: Some(ItemId(1)),
            },
            TreeNode {
                item: Item::new(3, "main.rs"),
                parent: Some(ItemId(1)),
            },
            TreeNode {
                item: Item::new(4, "notes"),
                parent: None,
            },
        ];
        let tree =
            TreeView::mount(world, column, UiId(2), single, &nodes, compose_row).expect("tree");
        scroll_port(world, tree.root, 160.0);
        let ids = |items: &[Item]| items.iter().map(|item| item.id).collect();
        let lists = vec![
            TrackedList::Retained {
                handle: list,
                ids: ids(&items),
            },
            TrackedList::Retained {
                handle: tree,
                ids: [1, 2, 3, 4].into_iter().map(ItemId).collect(),
            },
        ];
        (
            SceneBody::Collections(vec![list, tree]),
            lists,
            Vec::new(),
            None,
        )
    }

    fn mount_virtual(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let uniform_items = sample_items(24);
        let mut multiple = ViewProps::new("Virtual");
        multiple.selection = SelectionMode::Multiple;
        let uniform = ListView::mount_virtual(
            world,
            column,
            UiId(1),
            multiple,
            &uniform_items,
            22.0,
            compose_row,
        )
        .expect("virtual");
        scroll_port(world, uniform.root, 120.0);
        let variable_items = sample_items(8);
        let heights = [18.0, 28.0, 40.0, 22.0, 36.0, 24.0, 30.0, 20.0];
        let variable = ListView::mount_variable(
            world,
            column,
            UiId(2),
            ViewProps::new("Variable"),
            &variable_items,
            &heights,
            compose_row,
        )
        .expect("variable");
        scroll_port(world, variable.root, 140.0);
        let lists = vec![
            TrackedList::Virtual {
                handle: uniform,
                ids: uniform_items.iter().map(|item| item.id).collect(),
            },
            TrackedList::Virtual {
                handle: variable,
                ids: variable_items.iter().map(|item| item.id).collect(),
            },
        ];
        (
            SceneBody::Virtual(vec![uniform, variable]),
            lists,
            Vec::new(),
            None,
        )
    }

    fn mount_table(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let columns = vec![
            Column {
                id: ColumnId(1),
                label: "Name".into(),
                width: 160.0,
                alignment: CellAlignment::Start,
                sortable: true,
            },
            Column {
                id: ColumnId(2),
                label: "Note".into(),
                width: 180.0,
                alignment: CellAlignment::Start,
                sortable: false,
            },
        ];
        let rows = sample_items(5);
        let handle = DataTable::mount(
            world,
            column,
            UiId(1),
            TableProps::new("Records"),
            &columns,
            &rows,
            compose_cell,
        )
        .expect("table");
        scroll_port(world, handle.rows.root, 160.0);
        let lists = vec![TrackedList::Retained {
            handle: handle.rows,
            ids: rows.iter().map(|item| item.id).collect(),
        }];
        (SceneBody::Table(handle), lists, Vec::new(), None)
    }

    fn mount_inspector(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let groups = vec![
            PropertyGroup {
                id: GroupId(1),
                label: "General".into(),
            },
            PropertyGroup {
                id: GroupId(2),
                label: "Style".into(),
            },
        ];
        let fields = vec![
            property(
                1,
                1,
                "Title",
                PropertyKind::Text { max_chars: 80 },
                PropertyValue::Text("Hello".into()),
            ),
            property(
                2,
                1,
                "Count",
                PropertyKind::Number {
                    min: 0.0,
                    max: 100.0,
                },
                PropertyValue::Number(4.0),
            ),
            property(
                3,
                1,
                "Open",
                PropertyKind::Boolean,
                PropertyValue::Boolean(true),
            ),
            property(
                4,
                2,
                "Ink",
                PropertyKind::Choice(vec![
                    Choice {
                        id: 1,
                        label: "Red".into(),
                    },
                    Choice {
                        id: 2,
                        label: "Blue".into(),
                    },
                ]),
                PropertyValue::Choice(1),
            ),
            property(
                5,
                2,
                "Tags",
                PropertyKind::MultiChoice(vec![
                    Choice {
                        id: 1,
                        label: "Draft".into(),
                    },
                    Choice {
                        id: 2,
                        label: "Live".into(),
                    },
                ]),
                PropertyValue::Choices(vec![1]),
            ),
            property(
                6,
                2,
                "Extra",
                PropertyKind::Custom("note".into()),
                PropertyValue::Custom("custom".into()),
            ),
        ];
        let handle = PropertyInspector::mount(
            world,
            column,
            UiId(1),
            InspectorProps::new("Inspector", ObjectId(1)),
            &groups,
            &fields,
            compose_custom,
        )
        .expect("inspector");
        SceneBody::Inspector(handle)
    }

    fn mount_selection(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let props = SelectionProps::new("Ready");
        let check = Checkbox::mount(world, column, UiId(1), props.clone(), false).expect("check");
        let options = sample_options();
        let single = SingleSelect::mount(
            world,
            column,
            UiId(2),
            SelectionProps::new("Ink"),
            &options,
            Some(OptionId(1)),
        )
        .expect("single");
        let multi = MultiSelect::mount(
            world,
            column,
            UiId(3),
            SelectionProps::new("Tags"),
            &options,
            &[OptionId(2)],
        )
        .expect("multi");
        let ids: Vec<_> = options.iter().map(|option| option.id).collect();
        let selects = vec![
            TrackedSelect {
                handle: single,
                options: ids.clone(),
            },
            TrackedSelect {
                handle: multi,
                options: ids,
            },
        ];
        (
            SceneBody::Selection(vec![check, single, multi]),
            Vec::new(),
            selects,
            None,
        )
    }

    fn mount_diff(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let rows = vec![
            diff_row(1, 1, DiffMarker::Header, "notes.rs"),
            diff_row(2, 1, DiffMarker::Unchanged, "fn main() {"),
            diff_row(3, 1, DiffMarker::Removed, "- old"),
            diff_row(4, 1, DiffMarker::Added, "+ 日本語"),
            diff_row(5, 2, DiffMarker::Header, "more"),
            diff_row(6, 2, DiffMarker::Added, "+ e\u{301}"),
        ];
        let handle = DiffView::mount(world, column, UiId(1), DiffProps::new("Changes"), &rows)
            .expect("diff");
        scroll_port(world, handle.root, 160.0);
        let lists = vec![TrackedList::Virtual {
            handle: VirtualViewHandle { root: handle.root },
            ids: rows.iter().map(|row| row.id).collect(),
        }];
        (SceneBody::Diff(handle), lists, Vec::new(), None)
    }

    fn mount_editor(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let document = world.resource_mut::<SourceDocuments>().unwrap().open(
            "fn main() {\n    // 日本語\n    let greeting = \"مرحبا\";\n    let mark = \"e\u{301}\";\n}\n",
        );
        let surface = EditorSurface::mount(
            world,
            column,
            UiId(1),
            EditorSurfaceProps::new(document, "Source"),
        )
        .expect("editor");
        let _find = FindBar::mount(world, column, surface.editor).expect("find");
        let completion = CompletionPopup::mount(world, surface.editor).expect("completion");
        SceneBody::Editor {
            surface,
            completion,
        }
    }

    fn mount_structured(&mut self, column: Entity, frame: u64) -> SceneBody {
        let world = self.sim.world();
        let document = world
            .resource_mut::<StructuredDocuments>()
            .unwrap()
            .insert(StructuredDocument::new("before\nafter\n"));
        let surface = StructuredSurface::mount(
            world,
            column,
            UiId(1),
            StructuredSurfaceProps::new(document, "Document"),
        )
        .unwrap_or_else(|error| {
            panic!(
                "fuzz gallery failed seed={:#x} frame={frame} scene=structured: mount {error:?}",
                self.seed
            )
        });
        surface
            .command(
                world,
                StructuredCommand::Atomic(AtomicPayload {
                    kind: "embedded".into(),
                    fallback: "A retained draft".into(),
                    attributes: Default::default(),
                }),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "fuzz gallery failed seed={:#x} frame={frame} scene=structured: atomic {error:?}",
                    self.seed
                )
            });
        let block = world
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .blocks()
            .iter()
            .find(|block| block.kind == BlockKind::Atomic)
            .unwrap()
            .id;
        let panel = AtomicPanelHandle::mount(world, column, UiId(2), surface.editor, block)
            .unwrap_or_else(|error| {
                panic!(
                    "fuzz gallery failed seed={:#x} frame={frame} scene=structured: panel {error:?}",
                    self.seed
                )
            });
        SceneBody::Structured { surface, panel }
    }

    fn mount_menu(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let anchor = UiButton::mount(
            world,
            column,
            UiId(1),
            ButtonProps::new("Menu", HostIntent::Command(1)),
        )
        .expect("menu anchor");
        (
            SceneBody::Menu { anchor },
            Vec::new(),
            Vec::new(),
            Some(anchor.root),
        )
    }

    fn mount_window(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let band = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(36.0),
                ..Layout::row()
            },
            WindowDrag,
        ));
        set_parent(world, band, column);
        let _row = window_controls(world, band);
        SceneBody::Window
    }

    fn mount_mixed(
        &mut self,
        column: Entity,
    ) -> (
        SceneBody,
        Vec<TrackedList>,
        Vec<TrackedSelect>,
        Option<Entity>,
    ) {
        let world = self.sim.world();
        let button = UiButton::mount(
            world,
            column,
            UiId(1),
            ButtonProps::new("Apply", HostIntent::Command(1)),
        )
        .expect("mixed button");
        let field = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(32.0),
                ..Layout::column()
            },
            Label::new("draft"),
            TextField,
            Focusable,
            CapturesKeys,
        ));
        set_parent(world, field, column);
        let options = sample_options();
        let select = SingleSelect::mount(
            world,
            column,
            UiId(2),
            SelectionProps::new("Mode"),
            &options,
            None,
        )
        .expect("mixed select");
        let items = sample_items(4);
        let list = ListView::mount(
            world,
            column,
            UiId(3),
            ViewProps::new("Nearby"),
            &items,
            compose_row,
        )
        .expect("mixed list");
        scroll_port(world, list.root, 100.0);
        let lists = vec![TrackedList::Retained {
            handle: list,
            ids: items.iter().map(|item| item.id).collect(),
        }];
        let selects = vec![TrackedSelect {
            handle: select,
            options: options.iter().map(|option| option.id).collect(),
        }];
        (
            SceneBody::Mixed {
                controls: vec![button],
                selects: vec![select],
                list,
            },
            lists,
            selects,
            None,
        )
    }

    fn mount_liquid(&mut self, column: Entity) -> SceneBody {
        let world = self.sim.world();
        let source = gallery_source();
        let view = liquid_view(world, &source);
        set_parent(world, view, column);
        world
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .set_follow(false);
        SceneBody::Liquid
    }

    fn drive(&mut self) {
        self.pointer.pressed = false;
        self.pointer.released = false;
        let action = self.rng.gen_range(12);
        let mut scroll = [0.0, 0.0];
        let mut keys = Vec::new();
        let mut chars = String::new();
        let mut preedit = String::new();
        let mut commit = String::new();
        let mut ime = false;
        let mut modifiers = Modifiers::default();
        if self.rng.chance(1, 3) {
            modifiers.shift = true;
        }
        if self.rng.chance(1, 5) {
            modifiers.alt = true;
        }
        match action {
            0 => self.move_pointer(),
            1 => self.press_pointer(),
            2 => self.release_pointer(),
            3 => {
                self.pointer.down = true;
                self.move_pointer();
            }
            4 => {
                scroll = [self.rng.f32(-80.0, 80.0), self.rng.f32(-140.0, 140.0)];
            }
            5 => keys.push(self.named_key(modifiers)),
            6 => {
                modifiers = Modifiers::default();
                let sample = TEXT_SAMPLES[self.rng.gen_range(TEXT_SAMPLES.len())];
                if self.rng.chance(1, 2) {
                    chars.push_str(sample);
                } else {
                    commit.push_str(sample);
                    ime = true;
                }
            }
            7 => {
                modifiers.ctrl = true;
                self.sim.world().resource_mut::<Clipboard>().unwrap().text =
                    TEXT_SAMPLES.join("\n");
                keys.push(KeyEvent::character('v', modifiers));
            }
            8 => {
                ime = true;
                let sample = TEXT_SAMPLES[self.rng.gen_range(TEXT_SAMPLES.len())];
                if self.rng.chance(1, 2) {
                    preedit.push_str(sample);
                } else {
                    preedit.push_str(sample);
                    commit.push_str(sample);
                }
            }
            9 => self.resize_viewport(),
            10 => {
                self.pointer.button = PointerButton::Middle;
                self.press_pointer();
                self.sim
                    .world()
                    .resource_mut::<Clipboard>()
                    .unwrap()
                    .primary = "primary مرحبا".into();
            }
            _ => self.open_fresh_menu(),
        }
        if action != 10 && action != 1 && self.rng.chance(1, 17) {
            self.pointer.button = match self.rng.gen_range(4) {
                0 => PointerButton::Right,
                1 => PointerButton::Middle,
                2 => PointerButton::Other(8),
                _ => PointerButton::Left,
            };
        }
        self.write_input(TextFrame {
            scroll,
            keys,
            chars,
            preedit,
            commit,
            ime,
            modifiers,
        });
    }

    fn move_pointer(&mut self) {
        let [width, height] = self.viewport;
        self.pointer.position = [
            self.rng.f32(-32.0, width + 32.0),
            self.rng.f32(-32.0, height + 32.0),
        ];
    }

    fn press_pointer(&mut self) {
        self.move_pointer();
        self.pointer.down = true;
        self.pointer.pressed = true;
        self.pointer.press_at = Some(self.pointer.position);
        if self.rng.chance(2, 3) {
            self.pointer.button = PointerButton::Left;
        }
    }

    fn release_pointer(&mut self) {
        if self.rng.chance(1, 4) {
            self.move_pointer();
        }
        self.pointer.pressed = false;
        self.pointer.released = self.pointer.down;
        self.pointer.down = false;
    }

    fn named_key(&mut self, modifiers: Modifiers) -> KeyEvent {
        let named = [
            NamedKey::Tab,
            NamedKey::ArrowUp,
            NamedKey::ArrowDown,
            NamedKey::ArrowLeft,
            NamedKey::ArrowRight,
            NamedKey::Enter,
            NamedKey::Escape,
            NamedKey::Space,
            NamedKey::Backspace,
            NamedKey::Delete,
            NamedKey::Home,
            NamedKey::End,
            NamedKey::PageUp,
            NamedKey::PageDown,
        ];
        let mut event = if self.rng.chance(1, 5) {
            let ch = ['a', 'z', ' ', '1', '/'][self.rng.gen_range(5)];
            KeyEvent::character(ch, modifiers)
        } else {
            KeyEvent::named(named[self.rng.gen_range(named.len())], modifiers)
        };
        if self.rng.chance(1, 8) {
            event.released = true;
        }
        if self.rng.chance(1, 11) {
            event.repeat = true;
        }
        event
    }

    fn resize_viewport(&mut self) {
        let width = [240.0, 480.0, 800.0, 960.0, 1200.0][self.rng.gen_range(5)];
        let height = [180.0, 360.0, 640.0, 900.0][self.rng.gen_range(4)];
        self.viewport = [width, height];
        let world = self.sim.world();
        if let Some(viewport) = world.resource_mut::<Viewport>() {
            viewport.width = width;
            viewport.height = height;
            if self.rng.chance(1, 4) {
                viewport.insets = Insets {
                    top: 8.0,
                    right: 4.0,
                    bottom: 8.0,
                    left: 4.0,
                };
            } else {
                viewport.insets = Insets::default();
            }
        }
    }

    fn open_fresh_menu(&mut self) {
        let (anchor, previous) = {
            let Some(mounted) = self.mounted.as_mut() else {
                return;
            };
            (mounted.menu_anchor, std::mem::take(&mut mounted.menus))
        };
        let Some(anchor) = anchor else {
            return;
        };
        let world = self.sim.world();
        for menu in previous {
            menu.close(world);
        }
        if let Ok(menu) = open_menu(world, anchor, MenuAt::Below, &menu_entries()) {
            self.mounted.as_mut().unwrap().menus.push(menu);
        }
    }

    fn write_input(&mut self, frame: TextFrame) {
        let pointer = &self.pointer;
        let mut input = Input {
            pointer: Pointer {
                id: MOUSE_ID,
                position: pointer.position,
                pressed: pointer.pressed,
                down: pointer.down,
                released: pointer.released,
                button: pointer.button,
                press_at: pointer.press_at,
            },
            scroll: frame.scroll,
            shift: frame.modifiers.shift,
            ctrl: frame.modifiers.ctrl,
            alt: frame.modifiers.alt,
            super_key: frame.modifiers.super_key,
            ime_enabled: frame.ime,
            ime_preedit: frame.preedit,
            ime_commit: frame.commit,
            ime_cursor: None,
            ..Input::default()
        };
        if !input.ime_preedit.is_empty() {
            let end = input.ime_preedit.len();
            input.ime_cursor = Some((end, end));
        }
        for event in frame.keys {
            input.ctrl |= event.modifiers.ctrl;
            input.shift |= event.modifiers.shift;
            input.alt |= event.modifiers.alt;
            input.super_key |= event.modifiers.super_key;
            input.push_key(event, Platform::X11);
        }
        if !frame.chars.is_empty() && input.keys.is_empty() {
            input.chars.push_str(&frame.chars);
        }
        self.sim.world().insert_resource(input);
    }

    fn check(&mut self, frame: u64) {
        let here = Here {
            seed: self.seed,
            frame,
            scene: self.scene_name,
        };
        let world = self.sim.world();
        if let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) {
            here.check(world.is_alive(focus), format!("focus {focus:?} is dead"));
            here.check(
                access_details(world, focus).focus,
                format!("focus {focus:?} is not focusable"),
            );
        }
        if let Some(selected) = world
            .resource::<Selection>()
            .and_then(|selection| selection.entity)
        {
            here.check(
                world.is_alive(selected),
                format!("selection {selected:?} is dead"),
            );
        }
        if let Some(entity) = world.resource::<ScrollDrag>().and_then(|drag| drag.entity) {
            here.check(
                world.is_alive(entity),
                format!("scroll drag {entity:?} is dead"),
            );
        }
        if let Some(entity) = world.resource::<Drag>().and_then(|drag| drag.entity) {
            here.check(world.is_alive(entity), format!("drag {entity:?} is dead"));
        }
        if let Some(hovered) = world
            .resource::<WindowChrome>()
            .and_then(|chrome| chrome.hovered)
        {
            here.check(
                world.is_alive(hovered),
                format!("window hover {hovered:?} is dead"),
            );
        }
        if let Some(queue) = world.resource::<ActionIntents>() {
            for intent in queue.pending() {
                here.check(
                    world.is_alive(intent.entity),
                    format!("action intent {:?} is dead", intent.entity),
                );
            }
        }
        for entity in world.entities_with::<List>() {
            let list = world.get::<List>(entity).unwrap();
            if let Some(index) = list.selected {
                here.check(
                    index < list.count,
                    format!("list {entity:?} selected {index} of {}", list.count),
                );
            }
            if let Some(index) = list.clicked {
                here.check(
                    index < list.count,
                    format!("list {entity:?} clicked {index} of {}", list.count),
                );
            }
        }
        for group in world.entities_with::<Tabs>() {
            let selected = world.get::<Tabs>(group).unwrap().selected;
            let live = children_of(world, group).into_iter().any(|child| {
                world
                    .get::<Tab>(child)
                    .is_some_and(|tab| tab.id == selected)
            });
            here.check(
                live,
                format!("tabs {group:?} selected {selected} is not a live tab"),
            );
        }
        for entity in world.entities_with::<Rect>() {
            let rect = *world.get::<Rect>(entity).unwrap();
            here.check(
                rect.x.is_finite()
                    && rect.y.is_finite()
                    && rect.w.is_finite()
                    && rect.h.is_finite()
                    && rect.w >= 0.0
                    && rect.h >= 0.0,
                format!("non-finite rect {rect:?} on {entity:?}"),
            );
        }
        for entity in world.entities_with::<Clip>() {
            let clip = *world.get::<Clip>(entity).unwrap();
            here.check(
                clip.x.is_finite()
                    && clip.y.is_finite()
                    && clip.w.is_finite()
                    && clip.h.is_finite()
                    && clip.w >= 0.0
                    && clip.h >= 0.0,
                format!("non-finite clip {clip:?} on {entity:?}"),
            );
        }
        if let Some(mounted) = self.mounted.as_ref() {
            for list in &mounted.lists {
                match list {
                    TrackedList::Retained { handle, ids } => {
                        check_item(&here, ids, handle.selected(world), "selected");
                        check_item(&here, ids, handle.focused(world), "focused");
                        check_item(&here, ids, handle.range_anchor(world), "anchor");
                        for id in handle.selection(world) {
                            check_item(&here, ids, Some(id), "selection");
                        }
                        if let Some(id) = handle.selected(world) {
                            if let Some(row) = handle.row(world, id) {
                                here.check(
                                    world.is_alive(row.root) && world.is_alive(row.content),
                                    format!("selected row {id:?} is dead"),
                                );
                            }
                        }
                    }
                    TrackedList::Virtual { handle, ids } => {
                        check_item(&here, ids, handle.selected(world), "selected");
                        check_item(&here, ids, handle.focused(world), "focused");
                        check_item(&here, ids, handle.active(world), "active");
                        check_item(&here, ids, handle.range_anchor(world), "anchor");
                        for id in handle.selection(world) {
                            check_item(&here, ids, Some(id), "selection");
                        }
                        if let Some(id) = handle.active(world).or_else(|| handle.focused(world)) {
                            if let Some(row) = handle.row(world, id) {
                                here.check(
                                    world.is_alive(row.root) && world.is_alive(row.content),
                                    format!("active row {id:?} is dead"),
                                );
                            }
                        }
                    }
                }
            }
            for select in &mounted.selects {
                if let Some(popup) = select.handle.popup(world) {
                    here.check(
                        world.is_alive(popup),
                        format!("select popup {popup:?} is dead"),
                    );
                }
                for id in &select.options {
                    match select.handle.option(world, *id) {
                        Some(entity) => here.check(
                            world.is_alive(entity),
                            format!("option {id:?} entity {entity:?} is dead"),
                        ),
                        None => here.check(false, format!("option {id:?} has no entity")),
                    }
                }
            }
            for menu in &mounted.menus {
                if menu.is_open(world) {
                    here.check(
                        world.is_alive(menu.root),
                        format!("open menu {:?} is dead", menu.root),
                    );
                    for id in 1..=4 {
                        if let Some(entity) = menu.item(world, id) {
                            here.check(
                                world.is_alive(entity),
                                format!("menu item {id} {entity:?} is dead"),
                            );
                        }
                    }
                } else {
                    here.check(
                        !world.is_alive(menu.root),
                        format!("closed menu {:?} is still alive", menu.root),
                    );
                }
            }
            match &mounted.body {
                SceneBody::Editor {
                    surface,
                    completion,
                } => {
                    here.check(
                        world.is_alive(surface.root) && world.is_alive(surface.editor),
                        "editor surface died during the scene",
                    );
                    here.check(
                        world.is_alive(completion.root),
                        "completion popup died while its editor was mounted",
                    );
                }
                SceneBody::Structured { surface, panel } => {
                    here.check(
                        world.is_alive(surface.root) && world.is_alive(surface.editor),
                        "structured document surface died during the scene",
                    );
                    // Escape unmounts a focused atomic panel. A live panel still
                    // has to keep its child editor.
                    if world.is_alive(panel.root) {
                        here.check(
                            world.is_alive(panel.editor),
                            format!(
                                "atomic editor {:?} died while the panel was mounted",
                                panel.editor
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
        let world = self.sim.world();
        for request in take_action_intents(world) {
            let _ = dispatch(world, &request);
        }
    }

    fn teardown(&mut self, frame: u64) {
        let Some(mounted) = self.mounted.take() else {
            return;
        };
        self.scene_name = "teardown";
        let world = self.sim.world();
        for menu in mounted.menus {
            menu.close(world);
        }
        match mounted.body {
            SceneBody::Surfaces(handles) => {
                for handle in handles {
                    handle.destroy(world).expect("destroy surface");
                }
            }
            SceneBody::Controls {
                controls,
                modes,
                toolbar,
            } => {
                for control in controls {
                    control.destroy(world).expect("destroy control");
                }
                modes.destroy(world).expect("destroy modes");
                toolbar.destroy(world).expect("destroy toolbar");
            }
            SceneBody::Collections(handles) => {
                for handle in handles {
                    handle.unmount(world).expect("unmount collection");
                }
            }
            SceneBody::Virtual(handles) => {
                for handle in handles {
                    handle.unmount(world).expect("unmount virtual");
                }
            }
            SceneBody::Table(handle) => {
                handle.unmount(world).expect("unmount table");
            }
            SceneBody::Inspector(handle) => {
                handle.unmount(world).expect("unmount inspector");
            }
            SceneBody::Selection(handles) => {
                for handle in handles {
                    handle.unmount(world).expect("unmount selection");
                }
            }
            SceneBody::Diff(handle) => {
                VirtualViewHandle { root: handle.root }
                    .unmount(world)
                    .expect("unmount diff");
            }
            SceneBody::Editor { surface, .. } => {
                // The completion popup is detached. `apply_completion_popups`
                // drops it on the next frame once the editor is gone.
                surface.destroy(world).expect("destroy editor");
            }
            SceneBody::Structured { surface, panel } => {
                panel.unmount(world);
                surface.destroy(world).expect("destroy structured");
            }
            SceneBody::Menu { anchor } => {
                anchor.destroy(world).expect("destroy menu anchor");
            }
            SceneBody::Window | SceneBody::Liquid => {}
            SceneBody::Mixed {
                controls,
                selects,
                list,
            } => {
                for control in controls {
                    control.destroy(world).expect("destroy mixed control");
                }
                for select in selects {
                    select.unmount(world).expect("unmount mixed select");
                }
                list.unmount(world).expect("unmount mixed list");
            }
        }
        self.sim.update();
        let world = self.sim.world();
        if world.is_alive(mounted.column) {
            let mut owned = Vec::new();
            subtree(world, mounted.column, &mut owned);
            forget_interaction_targets(world, &owned);
            despawn(world, mounted.column);
        }
        self.sim.update();
        let here = Here {
            seed: self.seed,
            frame,
            scene: "teardown",
        };
        let leaked: Vec<_> = census(self.sim.world())
            .into_iter()
            .filter(|entity| !self.baseline.contains(entity))
            .collect();
        here.check(
            leaked.is_empty(),
            format!(
                "leaked entities after unmount: {}",
                leaked
                    .iter()
                    .map(|entity| format!("{}:{}", entity.index(), entity.generation()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        self.check(frame);
    }
}

fn check_item(here: &Here, ids: &BTreeSet<ItemId>, id: Option<ItemId>, label: &str) {
    if let Some(id) = id {
        here.check(
            ids.contains(&id),
            format!("{label} id {id:?} is not a live item"),
        );
    }
}

fn sample_items(count: u64) -> Vec<Item> {
    (1..=count)
        .map(|id| {
            let mut item = Item::new(id, format!("Item {id}"));
            if id == count {
                item.enabled = false;
            }
            item
        })
        .collect()
}

fn sample_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new(1, "Red"),
        SelectOption::new(2, "Blue"),
        SelectOption {
            id: OptionId(3),
            label: "Muted".into(),
            enabled: false,
        },
    ]
}

fn property(
    id: u64,
    group: u64,
    label: &str,
    kind: PropertyKind,
    value: PropertyValue,
) -> Property {
    Property {
        id: PropertyId(id),
        group: GroupId(group),
        label: label.into(),
        description: String::new(),
        kind,
        value,
        enabled: true,
        readonly: false,
        validation: None,
        validate: None,
    }
}

fn diff_row(id: u64, hunk: u64, marker: DiffMarker, text: &str) -> DiffRow {
    DiffRow {
        id: ItemId(id),
        hunk: HunkId(hunk),
        text: text.into(),
        marker,
        height: 22.0,
    }
}

fn compose_row(world: &mut World, slot: rndr_ui::collections::RowSlot, item: &Item) {
    let _ = text(world, slot.content, &item.name, Typography::Label);
}

fn compose_cell(world: &mut World, slot: rndr_ui::data::CellSlot, item: &Item, column: &Column) {
    let _ = text(
        world,
        slot.content,
        &format!("{} {}", column.label, item.name),
        Typography::Label,
    );
}

fn compose_custom(world: &mut World, slot: PropertySlot, property: &Property) {
    let _ = text(world, slot.editor, &property.label, Typography::Secondary);
}

fn menu_entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::Item(MenuItem::new(1, "Copy").shortcut("Ctrl+C")),
        MenuEntry::Separator,
        MenuEntry::Group("Edit".into()),
        MenuEntry::Item(MenuItem::new(2, "Paste")),
        MenuEntry::Item(MenuItem::new(4, "Disabled").enabled(false)),
        MenuEntry::submenu("More", vec![MenuEntry::Item(MenuItem::new(3, "Emoji 👍"))]),
    ]
}

fn scroll_port(world: &mut World, entity: Entity, height: f32) {
    if let Some(layout) = world.get_mut::<Layout>(entity) {
        layout.width = Size::Fill;
        layout.height = Size::Px(height);
    }
}

fn subtree(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    out.push(entity);
    for child in children_of(world, entity) {
        subtree(world, child, out);
    }
}

fn census(world: &World) -> HashSet<Entity> {
    let mut entities = HashSet::new();
    for entity in world
        .entities_with::<Layout>()
        .into_iter()
        .chain(world.entities_with::<Label>())
        .chain(world.entities_with::<Button>())
        .chain(world.entities_with::<TextField>())
        .chain(world.entities_with::<Editor>())
        .chain(world.entities_with::<Scroll>())
        .chain(world.entities_with::<List>())
        .chain(world.entities_with::<Tab>())
        .chain(world.entities_with::<Focusable>())
        .chain(world.entities_with::<WindowButton>())
        .chain(world.entities_with::<Parent>())
    {
        entities.insert(entity);
    }
    entities
}

fn assert_liquid_catalog() {
    for lang in FENCE_LANGS {
        let parsed = parse_fence(lang, body_for(lang))
            .unwrap_or_else(|| panic!("fence {lang} did not parse"));
        assert_eq!(parsed.kind(), canonical_kind(lang), "{lang}");
    }
    let source = gallery_source();
    let document = LiquidDocument::parse(&source);
    let mut kinds = BTreeSet::new();
    for segment in document.segments() {
        if let Segment::Block(index) = segment {
            kinds.insert(document.block(index).unwrap().kind());
        }
    }
    assert_eq!(kinds, CANONICAL_BLOCKS.iter().copied().collect());
}

fn canonical_kind(lang: &str) -> &'static str {
    match lang {
        "action_row" => "actions",
        "chip_group" => "chips",
        "procedure" => "recipe",
        other => match other {
            "card" => "card",
            "carousel" => "carousel",
            "actions" => "actions",
            "callout" => "callout",
            "section" => "section",
            "block" => "block",
            "chips" => "chips",
            "media" => "media",
            "cite" => "cite",
            "compare" => "compare",
            "plan" => "plan",
            "timeline" => "timeline",
            "shortlist" => "shortlist",
            "decision" => "decision",
            "brief" => "brief",
            "dashboard" => "dashboard",
            "chart" => "chart",
            "report" => "report",
            "slides" => "slides",
            "tabs" => "tabs",
            "steps" => "steps",
            "recipe" => "recipe",
            "accordion" => "accordion",
            "code" => "code",
            "tree" => "tree",
            "kanban" => "kanban",
            "feed" => "feed",
            _ => "unknown",
        },
    }
}

fn gallery_source() -> String {
    let mut source = String::from("Gallery\n\n日本語 / العربية / e\u{301} / 👩🏽‍💻 / שלום\n\n");
    for lang in CANONICAL_BLOCKS {
        source.push_str("```");
        source.push_str(lang);
        source.push('\n');
        source.push_str(body_for(lang));
        source.push_str("\n```\n\n");
    }
    source
}

fn body_for(lang: &str) -> &'static str {
    match lang {
        "card" => "title: Card\nbody: Hello 日本語",
        "carousel" => "---\ntitle: Alpha\n---\ntitle: Beta",
        "actions" | "action_row" => "Approve | approve\nRequest changes | changes",
        "callout" => "tone: tip\ntitle: Note\nbody: Host-owned",
        "section" => "title: Section\nbody: Reading",
        "block" => "font: mono\nsize: lg\nalign: center\n---\nHello styled",
        "chips" | "chip_group" => "Alpha\nBeta",
        "media" => "src: pic.png\nalt: Picture\ncaption: Caption",
        "cite" => "title: Quote\nquote: مرحبا",
        "compare" => "| | Local | Cloud |\n| --- | --- | --- |\n| Setup | cargo | export |",
        "plan" => "title: Plan\n\n---\nlabel: Design\nbody: Sketch\n---\nlabel: Build\nbody: Code",
        "timeline" => {
            "title: Timeline\n\n---\nlabel: Start\nts: 2024-01-01\n---\nlabel: Ship\nts: 2024-06-01"
        }
        "shortlist" => "title: Shortlist\n\n---\nlabel: One\nsummary: First\n---\nlabel: Two\nsummary: Second",
        "decision" => {
            "title: Choose\n\n---\nlabel: Ship\npros: sooner\ncons: risk\n---\nlabel: Wait\npros: safer\ncons: late"
        }
        "brief" => "title: Brief\n\n---\n## Context\nHello 日本語",
        "dashboard" => {
            "title: Health\ncolumns: 2\n\n---\nlabel: Uptime\nvalue: 99.9%\n---\nlabel: Latency\nvalue: 12\nunit: ms"
        }
        "chart" => {
            "type: bar\ntitle: Users\n| Month | Users |\n| --- | --- |\n| Jan | 3 |\n| Feb | 4 |"
        }
        "report" => "title: Report\n\nA paragraph with e\u{301} and עברית.",
        "slides" => "title: Deck\n\n---\nlabel: One\nbody: Slide one\n---\nlabel: Two\nbody: Slide two",
        "tabs" => "title: Paths\n\n---\nlabel: Local\nbody: cargo run\n---\nlabel: Cloud\nbody: export",
        "steps" => "title: Build\n\n---\nlabel: Mix\nbody: Stir\n---\nlabel: Bake\nbody: Heat",
        "recipe" | "procedure" => {
            "title: Soup\n\n---\nlabel: Simmer\nbody: Stir\nduration: 5s\n---\nlabel: Serve\nbody: Bowl"
        }
        "accordion" => "title: FAQ\n\n---\nlabel: Parser\nbody: Headless\nopen: true\n---\nlabel: Host\nbody: Feeds",
        "code" => "lang: rust\ntitle: main.rs\n---\nfn main() {\n    let n = 1;\n}\n",
        "tree" => "title: Files\nsrc/\n  lib.rs\n  main.rs",
        "kanban" => "## Todo\n- [ ] Canvas\n## Doing\n- [ ] Liquid\n## Done\n- [x] Charts",
        "feed" => "id: releases\ndatatype: md\ntitle: Releases",
        other => panic!("no liquid body for {other}"),
    }
}
