//! Runnable synthetic application using only the public collection API.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::collections::Item;
use rndr_ui::{
    collections::*,
    controls::{self, ButtonProps, HostIntent},
    *,
};

#[derive(Clone)]
pub struct Gallery {
    pub root: Entity,
    pub columns: Entity,
    pub list_panel: Entity,
    pub tree_panel: Entity,
    pub filter: Entity,
    pub filter_frame: Entity,
    pub filter_label: Entity,
    pub status: Entity,
    pub list: ViewHandle,
    pub tree: ViewHandle,
    pub buttons: Vec<controls::ControlHandle>,
    pub records: Vec<Item>,
    pub disabled: bool,
    pub saved: Vec<Item>,
    pub query: String,
    pub last_event: String,
    pub next_id: u64,
    pub page: usize,
    pub paging: Entity,
    pub page_label: Entity,
}
impl Component for Gallery {}

// Reserve the high half of gallery IDs for app-owned grouping nodes.
const GROUP_BASE: u64 = 1 << 63;
fn metadata(item: &Item) -> String {
    if item.id.0 >= GROUP_BASE {
        "Collection · Application-owned".into()
    } else {
        format!(
            "ID {:04}  ·  {}",
            item.id.0,
            if item.enabled {
                "Application-owned"
            } else {
                "Unavailable"
            }
        )
    }
}
pub fn sample() -> Vec<Item> {
    [
        "Brand refresh",
        "Mobile patterns",
        "Release checklist",
        "Color studies",
        "Typography",
        "Motion principles",
        "Component audit",
        "Research synthesis",
        "Interaction notes",
        "Accessibility review",
        "Product direction",
        "Archive index",
    ]
    .into_iter()
    .enumerate()
    .map(|(i, name)| {
        let mut item = Item::new(i as u64 + 1, name);
        item.enabled = i != 5;
        item
    })
    .collect()
}
pub fn row(world: &mut World, slot: RowSlot, item: &Item) {
    let children = children_of(world, slot.content);
    if children.is_empty() {
        text(world, slot.content, &item.name, Typography::Label).unwrap();
        text(world, slot.content, &metadata(item), Typography::Secondary).unwrap();
    } else {
        world.get_mut::<Label>(children[0]).unwrap().text = item.name.clone();
        world.get_mut::<Label>(children[1]).unwrap().text = metadata(item);
    }
}
fn forest(records: &[Item], query: &str) -> Vec<TreeNode> {
    let mut out = vec![];
    let q = query.to_lowercase();
    for (group, title, parity) in [
        (GROUP_BASE + 1, "Design library", 1),
        (GROUP_BASE + 2, "Product work", 0),
    ] {
        let visible: Vec<_> = records
            .iter()
            .filter(|r| {
                r.id.0 % 2 == parity
                    && (r.name.to_lowercase().contains(&q) || title.to_lowercase().contains(&q))
            })
            .collect();
        if !visible.is_empty() {
            out.push(TreeNode {
                item: Item::new(group, title),
                parent: None,
            });
            out.push(TreeNode {
                item: Item::new(group + 10, "Current projects"),
                parent: Some(ItemId(group)),
            });
            out.extend(visible.into_iter().map(|item| TreeNode {
                item: item.clone(),
                parent: Some(ItemId(group + 10)),
            }));
        }
    }
    out
}
pub fn create(width: f32, height: f32, theme: UiTheme) -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    // UiPlugin owns scrolling without Shell's filesystem/global-selection effects.
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    app.world_mut().insert_resource(theme);
    let w = app.world_mut();
    let root = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.0,
            gap: 12.0,
            ..Layout::column()
        },
        Scroll::default(),
        UiRecipe(UiStyle {
            background: Some(Token::Canvas.into()),
            ..UiStyle::default()
        }),
    ));
    text(w, root, "Data, in good order", Typography::Title).unwrap();
    text(
        w,
        root,
        "Composable collections / stable identity / your data",
        Typography::Secondary,
    )
    .unwrap();
    let search_row = w.spawn((Layout {
        width: Size::Fill,
        gap: 10.0,
        ..Layout::row()
    },));
    set_parent(w, search_row, root);
    let filter_label = text(w, search_row, "Filter", Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(filter_label).unwrap().width = Size::Px(52.0);
    w.insert(
        filter_label,
        LabelBox {
            inset_x: 0.0,
            inset_y: 6.0,
            min_height: 32.0,
        },
    );
    let filter_frame = w.spawn((Layout {
        width: Size::Fill,
        padding: 1.0,
        ..Layout::column()
    },));
    set_parent(w, filter_frame, search_row);
    let filter = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            min_height: 32.0,
            ..Layout::column()
        },
        TextField,
        Focusable,
        Label::new(""),
        Semantics {
            name: Some("Filter both collections".into()),
            description: "Type to filter. Reset clears the query.".into(),
            readonly: false,
        },
        UiRecipe(UiStyle {
            background: Some(Token::Content.into()),
            foreground: Some(Token::Text.into()),
            typography: Some(Typography::Label),
            corner: Some(Corner::Control),
            ..UiStyle::default()
        }),
    ));
    w.insert(filter, ResolvedColor);
    w.insert(
        filter,
        LabelBox {
            inset_x: 8.0,
            inset_y: 6.0,
            min_height: 32.0,
        },
    );
    set_parent(w, filter, filter_frame);
    let reset = controls::Button::mount(
        w,
        search_row,
        UiId(99),
        ButtonProps::new("Reset", HostIntent::Command(99)),
    )
    .unwrap();
    w.get_mut::<Layout>(reset.root).unwrap().width = Size::Px(96.0);
    let mut buttons = vec![reset];
    for labels in [
        [("Add", 1), ("Remove", 2), ("Reverse", 3), ("Empty", 4)],
        [("Theme", 5), ("Dense", 6), ("Disable", 7), ("10K data", 8)],
    ] {
        let bar = w.spawn((Layout {
            width: Size::Fill,
            gap: 8.0,
            ..Layout::row()
        },));
        set_parent(w, bar, root);
        for (label, cmd) in labels {
            buttons.push(
                controls::Button::mount(
                    w,
                    bar,
                    UiId(cmd),
                    ButtonProps::new(label, HostIntent::Command(cmd)),
                )
                .unwrap(),
            );
        }
    }
    let paging = w.spawn((
        Layout {
            width: Size::Fill,
            gap: 8.0,
            ..Layout::row()
        },
        Hidden,
    ));
    set_parent(w, paging, root);
    for (label, cmd) in [("Previous page", 9), ("Next page", 10)] {
        buttons.push(
            controls::Button::mount(
                w,
                paging,
                UiId(cmd),
                ButtonProps::new(label, HostIntent::Command(cmd)),
            )
            .unwrap(),
        );
    }
    let page_label = text(w, paging, "Page 1", Typography::Secondary).unwrap();
    let columns = w.spawn((Layout {
        width: Size::Fill,
        gap: 16.0,
        ..Layout::row()
    },));
    set_parent(w, columns, root);
    let list_panel = panel(
        w,
        columns,
        "01 / ListView",
        "Focus ≠ selection. Enter activates.",
    );
    let tree_panel = panel(
        w,
        columns,
        "02 / TreeView",
        "Left / Right explores the hierarchy.",
    );
    let records = sample();
    let list = ListView::mount(
        w,
        list_panel,
        UiId(1),
        ViewProps::new("Project list"),
        &records,
        row,
    )
    .unwrap();
    let tree = TreeView::mount(
        w,
        tree_panel,
        UiId(1),
        ViewProps::new("Project tree"),
        &forest(&records, ""),
        row,
    )
    .unwrap();
    let status = text(
        w,
        root,
        "Ready · click selects, arrows focus, Space selects, Enter activates",
        Typography::Secondary,
    )
    .unwrap();
    text(
        w,
        root,
        "Retained rows · 128-record application pages · no virtualization claim",
        Typography::Secondary,
    )
    .unwrap();
    w.insert_resource(Gallery {
        root,
        columns,
        list_panel,
        tree_panel,
        filter,
        filter_frame,
        filter_label,
        status,
        list,
        tree,
        buttons,
        records,
        saved: vec![],
        disabled: false,
        query: String::new(),
        last_event: "Ready".into(),
        next_id: 20,
        page: 0,
        paging,
        page_label,
    });
    tree.set_expanded(w, ItemId(GROUP_BASE + 1), true).unwrap();
    tree.set_expanded(w, ItemId(GROUP_BASE + 11), true).unwrap();
    list.select(w, Some(ItemId(1))).unwrap();
    list.focus(w, ItemId(if width < 700.0 { 2 } else { 3 }))
        .unwrap();
    app.add_system(Schedule::Update, update);
    app.add_system(Schedule::PostUpdate, resize);
    app
}
fn panel(w: &mut World, parent: Entity, name: &str, detail: &str) -> Entity {
    let p = w.spawn((
        Layout {
            width: Size::Fill,
            gap: 8.0,
            padding: 12.0,
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::Raised.into()),
            corner: Some(Corner::Group),
            ..UiStyle::default()
        }),
    ));
    set_parent(w, p, parent);
    text(w, p, name, Typography::Heading).unwrap();
    text(w, p, detail, Typography::Secondary).unwrap();
    p
}
pub fn reconcile(w: &mut World) {
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let q = g.query.to_lowercase();
    let matching: Vec<_> = g
        .records
        .iter()
        .filter(|r| r.name.to_lowercase().contains(&q))
        .cloned()
        .collect();
    let pages = matching.len().div_ceil(128).max(1);
    g.page = g.page.min(pages - 1);
    let filtered: Vec<_> = matching.into_iter().skip(g.page * 128).take(128).collect();
    if pages > 1 {
        w.remove::<Hidden>(g.paging);
    } else {
        w.insert(g.paging, Hidden);
    }
    w.get_mut::<Label>(g.page_label).unwrap().text = format!("Page {} / {pages}", g.page + 1);
    let mut props = ViewProps::new("Project list");
    props.enabled = !g.disabled;
    props.empty = if g.records.is_empty() {
        "Your collection is empty. Add a project or restore the sample."
    } else {
        "No matches. Try another filter or reset it."
    }
    .into();
    g.list = ListView::mount(w, g.list_panel, UiId(1), props.clone(), &filtered, row).unwrap();
    props.name = "Project tree".into();
    let nodes = forest(&filtered, "");
    // The tree's two groups + two nested branches count toward the same budget.
    g.tree = TreeView::mount(w, g.tree_panel, UiId(1), props, &nodes, row).unwrap();
    w.insert_resource(g);
}
fn update(w: &mut World) {
    let intents = take_action_intents(w);
    let commands: Vec<_> = intents
        .iter()
        .filter_map(|i| controls::host_intent(w, i))
        .collect();
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let mut dirty = false;
    if events::<TextChanged>(w)
        .iter()
        .any(|e| e.entity == g.filter)
    {
        g.query = w.get::<Label>(g.filter).unwrap().text.clone();
        g.page = 0;
        dirty = true;
    }
    for cmd in commands {
        let HostIntent::Command(cmd) = cmd else {
            continue;
        };
        match cmd {
            1 => {
                if g.records.len() < 10000 {
                    g.records
                        .push(Item::new(g.next_id, format!("New project {}", g.next_id)));
                    g.next_id += 1;
                }
                dirty = true;
            }
            2 => {
                let id = g.list.selected(w).or_else(|| g.tree.selected(w));
                if let Some(id) = id {
                    g.records.retain(|r| r.id != id);
                } else {
                    g.records.pop();
                }
                dirty = true;
            }
            3 => {
                g.records.reverse();
                dirty = true;
            }
            4 => {
                if g.records.is_empty() {
                    g.records = if g.saved.is_empty() {
                        sample()
                    } else {
                        std::mem::take(&mut g.saved)
                    };
                } else {
                    g.saved = std::mem::take(&mut g.records);
                }
                dirty = true;
            }
            5 => {
                let theme = w.resource_mut::<UiTheme>().unwrap();
                theme.appearance = if theme.appearance == Appearance::Light {
                    Appearance::Dark
                } else {
                    Appearance::Light
                };
            }
            6 => {
                let theme = w.resource_mut::<UiTheme>().unwrap();
                theme.density = if theme.density == Density::Compact {
                    Density::Comfortable
                } else {
                    Density::Compact
                };
            }
            7 => {
                g.disabled = !g.disabled;
                dirty = true;
            }
            8 => {
                g.records = (1..=10000)
                    .map(|i| Item::new(i, format!("Dataset project {i:05}")))
                    .collect();
                g.next_id = 10001;
                g.page = 0;
                g.query.clear();
                w.get_mut::<Label>(g.filter).unwrap().text.clear();
                dirty = true;
            }
            9 => {
                g.page = g.page.saturating_sub(1);
                dirty = true;
            }
            10 => {
                g.page += 1;
                dirty = true;
            }
            99 => {
                g.page = 0;
                g.query.clear();
                w.get_mut::<Label>(g.filter).unwrap().text.clear();
                dirty = true;
            }
            _ => {}
        }
    }
    for event in events::<CollectionEvent>(w) {
        let source = if event.view == g.list.root {
            "List"
        } else if event.view == g.tree.root {
            "Tree"
        } else {
            continue;
        };
        g.last_event = format!("{source} · {:?}", event.kind);
    }
    w.get_mut::<Label>(g.status).unwrap().text = format!(
        "{} · {} records · page {}{}",
        g.last_event,
        g.records.len(),
        g.page + 1,
        if g.disabled { " · disabled" } else { "" }
    );
    w.insert_resource(g);
    if dirty {
        reconcile(w);
    }
}
fn resize(w: &mut World) {
    let g = w.resource::<Gallery>().unwrap().clone();
    let port = w.resource::<Viewport>().unwrap();
    let narrow = port.width < 700.0;
    let theme = w.resource::<UiTheme>().unwrap().clone();
    let scale = theme.text_scale.clamp(1.0, 4.0);
    let paging_height = if w.has::<Hidden>(g.paging) {
        0.0
    } else {
        44.0 * scale
    };
    let h = if narrow {
        150.0
    } else {
        (port.height - 430.0 - 120.0 * (scale - 1.0) - paging_height).clamp(160.0, 700.0)
    };
    let focused = w.resource::<Focus>().and_then(|f| f.entity) == Some(g.filter);
    w.insert(
        g.filter_frame,
        UiRecipe(UiStyle {
            background: Some(
                if focused {
                    Token::Focus
                } else if theme.contrast == Contrast::High {
                    Token::StrongStroke
                } else {
                    Token::SubtleStroke
                }
                .into(),
            ),
            corner: Some(Corner::Control),
            ..UiStyle::default()
        }),
    );
    w.get_mut::<Layout>(g.filter_frame).unwrap().padding = if theme.contrast == Contrast::High {
        2.0
    } else {
        1.0
    };
    w.get_mut::<Layout>(g.filter_label).unwrap().width = Size::Px(52.0 * scale);
    w.get_mut::<Layout>(g.root).unwrap().padding = if narrow { 12.0 } else { 20.0 };
    w.get_mut::<Layout>(g.columns).unwrap().axis = if narrow { Axis::Column } else { Axis::Row };
    for view in [g.list, g.tree] {
        w.get_mut::<Layout>(view.root).unwrap().height = Size::Px(h);
    }
}
