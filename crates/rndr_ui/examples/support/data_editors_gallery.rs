//! Joint synthetic model host: no files, database, reflection or hidden persistence.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::{
    collections::{CollectionEvent, CollectionEventKind, Item, ItemId},
    controls::{self, ButtonProps, HostIntent},
    data::*,
    *,
};
#[derive(Clone)]
pub struct Record {
    pub id: u64,
    pub title: String,
    pub progress: f64,
    pub enabled: bool,
    pub phase: u64,
}
#[derive(Clone)]
pub struct Gallery {
    pub root: Entity,
    pub body: Entity,
    pub table_parent: Entity,
    pub inspector_parent: Entity,
    pub status: Entity,
    pub table: Option<TableHandle>,
    pub inspector: Option<InspectorHandle>,
    pub records: Vec<Record>,
    pub selected: Option<u64>,
    pub sort: Option<SortState>,
    pub disabled: bool,
    pub readonly: bool,
    pub filtered: bool,
    pub width: f32,
    pub last: String,
}
pub fn sample() -> Vec<Record> {
    [
        "Brand refresh",
        "Mobile patterns",
        "Release checklist",
        "Color studies",
        "Typography",
        "Motion principles",
        "Component audit",
        "Research synthesis",
    ]
    .iter()
    .enumerate()
    .map(|(n, s)| Record {
        id: n as u64 + 1,
        title: s.to_string(),
        progress: (n * 12) as f64,
        enabled: n % 2 == 0,
        phase: n as u64 % 3 + 1,
    })
    .collect()
}
fn node(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn panel(w: &mut World, p: Entity, title: &str) -> Entity {
    let e = node(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.,
            padding: 12.,
            ..Layout::column()
        },
    );
    w.insert(
        e,
        UiRecipe(UiStyle {
            background: Some(Token::Raised.into()),
            corner: Some(Corner::Group),
            ..UiStyle::default()
        }),
    );
    text(w, e, title, Typography::Heading).unwrap();
    e
}
pub fn create(width: f32, height: f32, theme: UiTheme) -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    app.world_mut().insert_resource(theme);
    let w = app.world_mut();
    let root = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 16.,
            gap: 10.,
            ..Layout::column()
        },
        Scroll::default(),
        UiRecipe(UiStyle {
            background: Some(Token::Canvas.into()),
            ..UiStyle::default()
        }),
    ));
    text(w, root, "Data, with intent", Typography::Title).unwrap();
    text(
        w,
        root,
        "Reusable table + property inspector / synthetic, application-owned data",
        Typography::Secondary,
    )
    .unwrap();
    for commands in [
        vec![
            (1, "Reset"),
            (2, "Empty"),
            (3, "Remove selected"),
            (4, "Filter"),
            (5, "Resize name"),
        ],
        vec![
            (6, "Read only"),
            (7, "Disable"),
            (8, "Light / dark"),
            (9, "Density"),
            (10, "Text scale"),
        ],
    ] {
        let row = node(
            w,
            root,
            Layout {
                width: Size::Fill,
                gap: 6.,
                ..Layout::row()
            },
        );
        for (cmd, title) in commands {
            controls::Button::mount(
                w,
                row,
                UiId(cmd),
                ButtonProps::new(title, HostIntent::Command(cmd)),
            )
            .unwrap();
        }
    }
    let status = text(
        w,
        root,
        "Choose a row with Space or click; Enter activates independently.",
        Typography::Secondary,
    )
    .unwrap();
    let body = node(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 14.,
            ..Layout::row()
        },
    );
    let table_parent = panel(w, body, "Projects");
    let inspector_parent = panel(w, body, "Properties");
    w.insert_resource(Gallery {
        root,
        body,
        table_parent,
        inspector_parent,
        status,
        table: None,
        inspector: None,
        records: sample(),
        selected: Some(1),
        sort: None,
        disabled: false,
        readonly: false,
        filtered: false,
        width: 240.,
        last: "Drafts are local. Commit requests update this synthetic model only.".into(),
    });
    reconcile(w);
    app.add_system(Schedule::Update, update);
    app.add_system(Schedule::PostUpdate, resize);
    app
}
fn props(r: &Record, readonly: bool) -> Vec<Property> {
    [
        (
            "Name",
            "Text · at most 48 characters",
            PropertyKind::Text { max_chars: 48 },
            PropertyValue::Text(r.title.clone()),
            GroupId(1),
        ),
        (
            "Progress",
            "Number · finite, 0 to 100",
            PropertyKind::Number { min: 0., max: 100. },
            PropertyValue::Number(r.progress),
            GroupId(1),
        ),
        (
            "Enabled",
            "Boolean · activate to change draft",
            PropertyKind::Boolean,
            PropertyValue::Boolean(r.enabled),
            GroupId(2),
        ),
        (
            "Phase",
            "Choice · activate to cycle draft",
            PropertyKind::Choice(vec![
                Choice {
                    id: 1,
                    label: "Plan".into(),
                },
                Choice {
                    id: 2,
                    label: "Build".into(),
                },
                Choice {
                    id: 3,
                    label: "Review".into(),
                },
            ]),
            PropertyValue::Choice(r.phase),
            GroupId(2),
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, (label, desc, kind, value, group))| Property {
        id: PropertyId(n as u64 + 1),
        group,
        label: label.into(),
        description: desc.into(),
        kind,
        value,
        enabled: true,
        readonly,
        validation: None,
        validate: None,
    })
    .collect()
}
pub fn reconcile(w: &mut World) {
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let mut records = g.records.clone();
    if g.filtered {
        records.retain(|r| r.enabled);
    }
    if let Some(sort) = g.sort {
        records.sort_by(|a, b| {
            let cmp = if sort.column == ColumnId(1) {
                a.title.cmp(&b.title)
            } else {
                a.progress.total_cmp(&b.progress)
            };
            let cmp = cmp.then(a.id.cmp(&b.id));
            if sort.direction == SortDirection::Ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });
    }
    if g.selected
        .is_some_and(|id| !records.iter().any(|r| r.id == id))
    {
        g.selected = None;
    }
    let rows: Vec<_> = records
        .iter()
        .map(|r| {
            Item::new(
                r.id,
                format!(
                    "{}, {} percent, {}",
                    r.title,
                    r.progress,
                    if r.enabled { "enabled" } else { "not enabled" }
                ),
            )
        })
        .collect();
    let columns = vec![
        Column {
            id: ColumnId(1),
            label: "Project".into(),
            width: g.width,
            alignment: CellAlignment::Start,
            sortable: true,
        },
        Column {
            id: ColumnId(2),
            label: "Progress".into(),
            width: 130.,
            alignment: CellAlignment::End,
            sortable: true,
        },
        Column {
            id: ColumnId(3),
            label: "Enabled".into(),
            width: 110.,
            alignment: CellAlignment::Center,
            sortable: false,
        },
    ];
    let mut tp = TableProps::new("Synthetic projects");
    tp.enabled = !g.disabled;
    tp.sort = g.sort;
    tp.body_height = 320.;
    tp.empty = if g.filtered {
        "No matching records. Reset or change filter."
    } else {
        "No records. Reset restores the synthetic sample."
    }
    .into();
    let table = DataTable::mount(
        w,
        g.table_parent,
        UiId(1),
        tp,
        &columns,
        &rows,
        |w, slot, item, col| {
            let r = records.iter().find(|r| r.id == item.id.0).unwrap();
            let value = match col.id.0 {
                1 => r.title.clone(),
                2 => format!("{} %", r.progress),
                _ => {
                    if r.enabled {
                        "Yes".into()
                    } else {
                        "No".into()
                    }
                }
            };
            let kids = children_of(w, slot.content);
            if kids.is_empty() {
                text(w, slot.content, &value, Typography::Label).unwrap();
            } else {
                w.get_mut::<Label>(kids[0]).unwrap().text = value;
            }
        },
    )
    .unwrap();
    // The canonical collection rejects mutation while disabled. A disabled
    // remount already retains its previous selection (or clears removed rows).
    if !g.disabled {
        table.rows.select(w, g.selected.map(ItemId)).unwrap();
    }
    g.table = Some(table);
    let fields = g
        .selected
        .and_then(|id| g.records.iter().find(|r| r.id == id))
        .map(|r| props(r, g.readonly))
        .unwrap_or_default();
    let mut ip = InspectorProps::new(
        g.selected
            .map(|id| format!("Properties for project {id}"))
            .unwrap_or("No selection".into()),
        ObjectId(g.selected.unwrap_or(u64::MAX)),
    );
    ip.enabled = !g.disabled;
    ip.empty = "Select a project to inspect its properties.".into();
    g.inspector = Some(
        PropertyInspector::mount(
            w,
            g.inspector_parent,
            UiId(1),
            ip,
            &[
                PropertyGroup {
                    id: GroupId(1),
                    label: "Identity & progress".into(),
                },
                PropertyGroup {
                    id: GroupId(2),
                    label: "Workflow".into(),
                },
            ],
            &fields,
            |_, _, _| {},
        )
        .unwrap(),
    );
    w.get_mut::<Label>(g.status).unwrap().text = format!(
        "{} records · selected {:?} · {}",
        records.len(),
        g.selected,
        g.last
    );
    w.insert_resource(g);
}
fn update(w: &mut World) {
    let commands: Vec<_> = take_action_intents(w)
        .iter()
        .filter_map(|i| controls::host_intent(w, i))
        .collect();
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let mut dirty = false;
    for event in events::<SortRequested>(w) {
        if Some(event.table) == g.table.map(|h| h.root) {
            g.sort = Some(event.sort);
            g.last = "Host applied the requested stable sort.".into();
            dirty = true;
        }
    }
    for event in events::<CollectionEvent>(w) {
        if Some(event.view) != g.table.map(|h| h.rows.root) {
            continue;
        }
        match event.kind {
            CollectionEventKind::Selected(id) => {
                g.selected = id.map(|id| id.0);
                g.last = "Selection switched; any old draft is explicitly cancelled.".into();
                dirty = true;
            }
            CollectionEventKind::Activated(id) => {
                g.last = format!("Activated row {} (selection unchanged)", id.0);
                dirty = true;
            }
            _ => {}
        }
    }
    for event in events::<PropertyEvent>(w) {
        if Some(event.inspector) != g.inspector.map(|h| h.root) {
            continue;
        }
        match &event.kind {
            PropertyEventKind::CommitRequested { before: _, after } => {
                if let Some(r) = g.records.iter_mut().find(|r| r.id == event.object.0) {
                    match (event.property.0, after) {
                        (1, PropertyValue::Text(v)) => r.title = v.clone(),
                        (2, PropertyValue::Number(v)) => r.progress = *v,
                        (3, PropertyValue::Boolean(v)) => r.enabled = *v,
                        (4, PropertyValue::Choice(v)) => r.phase = *v,
                        _ => {}
                    }
                    g.last = format!(
                        "Committed project {} property {} to synthetic model.",
                        r.id, event.property.0
                    );
                    dirty = true;
                }
            }
            PropertyEventKind::Cancelled(reason) => {
                g.last = format!("Draft cancelled: {reason:?}");
                dirty = true;
            }
            PropertyEventKind::DraftChanged(_) => {}
        }
    }
    for intent in commands {
        let HostIntent::Command(cmd) = intent else {
            continue;
        };
        match cmd {
            1 => {
                g.records = sample();
                g.selected = Some(1);
                g.filtered = false;
                g.disabled = false;
                g.readonly = false;
            }
            2 => {
                g.records.clear();
                g.selected = None;
            }
            3 => {
                if let Some(id) = g.selected {
                    g.records.retain(|r| r.id != id);
                    g.selected = None;
                }
            }
            4 => g.filtered = !g.filtered,
            5 => g.width = if g.width < 300. { 360. } else { 200. },
            6 => g.readonly = !g.readonly,
            7 => g.disabled = !g.disabled,
            8 => {
                let t = w.resource_mut::<UiTheme>().unwrap();
                t.appearance = if t.appearance == Appearance::Light {
                    Appearance::Dark
                } else {
                    Appearance::Light
                };
            }
            9 => {
                let t = w.resource_mut::<UiTheme>().unwrap();
                t.density = if t.density == Density::Compact {
                    Density::Comfortable
                } else {
                    Density::Compact
                };
            }
            10 => {
                let t = w.resource_mut::<UiTheme>().unwrap();
                t.text_scale = if t.text_scale < 1.5 { 2. } else { 1. };
                t.reading_scale = t.text_scale;
            }
            _ => continue,
        }
        dirty = true;
    }
    w.insert_resource(g);
    if dirty {
        reconcile(w);
    }
}
fn resize(w: &mut World) {
    let Some(g) = w.resource::<Gallery>().cloned() else {
        return;
    };
    let v = *w.resource::<Viewport>().unwrap();
    let narrow = v.width < 820.;
    let scaled = w.resource::<UiTheme>().is_some_and(|t| t.text_scale > 1.5);
    w.insert(
        g.body,
        Layout {
            width: Size::Fill,
            height: if narrow || scaled {
                Size::Hug
            } else {
                Size::Fill
            },
            gap: 14.,
            ..if narrow || scaled {
                Layout::column()
            } else {
                Layout::row()
            }
        },
    );
    w.get_mut::<Layout>(g.table_parent).unwrap().height = if narrow || scaled {
        Size::Px(if scaled { 550. } else { 440. })
    } else {
        Size::Fill
    };
    w.get_mut::<Layout>(g.inspector_parent).unwrap().height = if narrow || scaled {
        Size::Px(680.)
    } else {
        Size::Fill
    };
    let children = children_of(w, g.root);
    let stack_actions = scaled || v.width < 620.;
    for row in children.into_iter().skip(2).take(2) {
        w.insert(
            row,
            Layout {
                width: Size::Fill,
                gap: 6.,
                ..if stack_actions {
                    Layout::column()
                } else {
                    Layout::row()
                }
            },
        );
    }
}
