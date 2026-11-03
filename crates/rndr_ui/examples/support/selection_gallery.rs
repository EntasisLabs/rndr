//! Synthetic host for independently reusable controls and explicit inspector commits.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    data::*,
    selection::*,
    *,
};
#[derive(Clone)]
pub struct Gallery {
    pub root: Entity,
    pub body: Entity,
    pub standalone: Entity,
    pub inspector_parent: Entity,
    pub status: Entity,
    pub checkbox: Option<SelectionHandle>,
    pub single: Option<SelectionHandle>,
    pub multi: Option<SelectionHandle>,
    pub inspector: Option<InspectorHandle>,
    pub checked: bool,
    pub selected: Option<OptionId>,
    pub selected_many: Vec<OptionId>,
    pub options: Vec<SelectOption>,
    pub committed: Vec<Property>,
    pub disabled: bool,
    pub readonly: bool,
    pub last: String,
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
            gap: 10.,
            padding: 12.,
            ..Layout::column()
        },
    );
    w.insert(
        e,
        UiStyle {
            background: Some(Token::Raised.into()),
            corner: Some(Corner::Group),
            ..Default::default()
        },
    );
    text(w, e, title, Typography::Heading).unwrap();
    e
}
fn property(id: u64, label: &str, kind: PropertyKind, value: PropertyValue) -> Property {
    Property {
        id: PropertyId(id),
        group: GroupId(1),
        label: label.into(),
        description: "Changes are drafts until explicit Commit".into(),
        kind,
        value,
        enabled: true,
        readonly: false,
        validate: None,
        validation: None,
    }
}
fn require_channel(v: &PropertyValue) -> Result<(), String> {
    if matches!(v,PropertyValue::Choices(ids) if ids.is_empty()) {
        Err("Choose at least one channel before Commit".into())
    } else {
        Ok(())
    }
}
pub fn create(width: f32, height: f32, theme: UiTheme, flags: &[String]) -> App {
    let has = |s: &str| flags.iter().any(|a| a == s);
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(width, height));
    a.world_mut().insert_resource(theme);
    let w = a.world_mut();
    let root = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 16.,
            gap: 12.,
            ..Layout::column()
        },
        Scroll::default(),
        UiStyle {
            background: Some(Token::Canvas.into()),
            ..Default::default()
        },
    ));
    text(w, root, "Choices, with ownership", Typography::Title).unwrap();
    text(
        w,
        root,
        "Checkbox / single select / multi select — synthetic host-owned settings",
        Typography::Secondary,
    )
    .unwrap();
    text(w,root,"Tab to focus; Enter/Space opens or toggles. Arrows highlight options, not selection. Escape closes; Tab closes and continues. Outside press closes without click-through. Multi-select applies requests immediately; inspector still requires Commit.",Typography::Secondary).unwrap();
    let commands = node(
        w,
        root,
        Layout {
            width: Size::Fill,
            gap: 8.,
            ..Layout::row()
        },
    );
    for (id, label) in [(1, "Reorder options"), (2, "Remove last"), (3, "Reset")] {
        controls::Button::mount(
            w,
            commands,
            UiId(id),
            ButtonProps::new(label, HostIntent::Command(id)),
        )
        .unwrap();
    }
    let status = text(w, root, "No changes requested", Typography::Secondary).unwrap();
    let body = node(
        w,
        root,
        Layout {
            axis: if width < 700. {
                Axis::Column
            } else {
                Axis::Row
            },
            width: Size::Fill,
            gap: 16.,
            ..Layout::column()
        },
    );
    let standalone = panel(w, body, "Standalone / application state");
    let inspector_parent = panel(w, body, "PropertyInspector / explicit commit");
    let mut options = vec![
        SelectOption::new(1, "Design"),
        SelectOption::new(2, "Archived (disabled option)"),
        SelectOption::new(3, "Engineering"),
        SelectOption::new(4, "Research"),
    ];
    options[1].enabled = false;
    if has("--many") {
        options.extend(
            (5..85)
                .map(|id| SelectOption::new(id, format!("Synthetic team {id} — stable identity"))),
        );
    }
    if has("--empty") {
        options.clear();
    }
    let choices = vec![
        Choice {
            id: 1,
            label: "Draft".into(),
        },
        Choice {
            id: 2,
            label: "In review".into(),
        },
        Choice {
            id: 3,
            label: "Ready".into(),
        },
    ];
    let mut committed = vec![
        property(
            1,
            "Notifications",
            PropertyKind::Boolean,
            PropertyValue::Boolean(true),
        ),
        property(
            2,
            "Workflow phase",
            PropertyKind::Choice(choices.clone()),
            PropertyValue::Choice(1),
        ),
        property(
            3,
            "Delivery channels",
            PropertyKind::MultiChoice(choices),
            PropertyValue::Choices(vec![1]),
        ),
        property(
            4,
            "Review interval (1–30)",
            PropertyKind::Number { min: 1., max: 30. },
            PropertyValue::Number(7.),
        ),
    ];
    committed[2].validate = Some(require_channel);
    let readonly = has("--readonly");
    let disabled = has("--disabled");
    for p in &mut committed {
        p.readonly = readonly;
    }
    w.insert_resource(Gallery {
        root,
        body,
        standalone,
        inspector_parent,
        status,
        checkbox: None,
        single: None,
        multi: None,
        inspector: None,
        checked: true,
        selected: Some(OptionId(1)),
        selected_many: vec![OptionId(1), OptionId(4)],
        options,
        committed,
        disabled,
        readonly,
        last: "No changes requested".into(),
    });
    reconcile(w);
    // Explicit access examples stay independently focusable/observable, regardless of preset.
    let mut ro = SelectionProps::new("Read-only preference");
    ro.readonly = true;
    Checkbox::mount(w, standalone, UiId(20), ro, true).unwrap();
    let mut dis = SelectionProps::new("Disabled preference");
    dis.enabled = false;
    Checkbox::mount(w, standalone, UiId(21), dis, false).unwrap();
    let mut ro = SelectionProps::new("Read-only team");
    ro.readonly = true;
    SingleSelect::mount(
        w,
        standalone,
        UiId(22),
        ro,
        &[SelectOption::new(9, "Visible but not editable")],
        Some(OptionId(9)),
    )
    .unwrap();
    MultiSelect::mount(
        w,
        standalone,
        UiId(23),
        SelectionProps::new("Empty choices"),
        &[],
        &[],
    )
    .unwrap();
    if has("--invalid") {
        let h = w.resource::<Gallery>().unwrap().inspector.unwrap();
        h.set_draft(w, PropertyId(3), "").unwrap();
        let e = h.slot(w, PropertyId(3)).unwrap().editor;
        request_focus(w, e);
    }
    a.add_system(Schedule::Update, update);
    a
}
pub fn reconcile(w: &mut World) {
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let props = |label: &str| {
        let mut p = SelectionProps::new(label);
        p.enabled = !g.disabled;
        p.readonly = g.readonly;
        p
    };
    g.checkbox = Some(
        Checkbox::mount(
            w,
            g.standalone,
            UiId(1),
            props("Receive updates"),
            g.checked,
        )
        .unwrap(),
    );
    g.single = Some(
        SingleSelect::mount(
            w,
            g.standalone,
            UiId(2),
            props("Primary team"),
            &g.options,
            g.selected,
        )
        .unwrap(),
    );
    g.multi = Some(
        MultiSelect::mount(
            w,
            g.standalone,
            UiId(3),
            props("Collaborating teams"),
            &g.options,
            &g.selected_many,
        )
        .unwrap(),
    );
    let mut ip = InspectorProps::new("Synthetic preferences", ObjectId(1));
    ip.enabled = !g.disabled;
    g.inspector = Some(
        PropertyInspector::mount(
            w,
            g.inspector_parent,
            UiId(50),
            ip,
            &[PropertyGroup {
                id: GroupId(1),
                label: "Draft settings".into(),
            }],
            &g.committed,
            |_, _, _| {},
        )
        .unwrap(),
    );
    w.get_mut::<Layout>(g.inspector.unwrap().root)
        .unwrap()
        .height = Size::Px(650.);
    w.get_mut::<Label>(g.status).unwrap().text = g.last.clone();
    w.insert_resource(g);
}
fn update(w: &mut World) {
    let g = w.resource::<Gallery>().unwrap();
    let body = g.body;
    let width = w
        .resource::<Viewport>()
        .map(|v| v.content_width())
        .unwrap_or(1180.);
    let scale = w.resource::<UiTheme>().unwrap().text_scale;
    w.get_mut::<Layout>(body).unwrap().axis = if width < 700. * scale {
        Axis::Column
    } else {
        Axis::Row
    };
    let changes = events::<SelectionChange>(w).to_vec();
    let commits = events::<PropertyEvent>(w).to_vec();
    let intents = take_action_intents(w);
    let mut changed = false;
    let mut g = w.resource::<Gallery>().unwrap().clone();
    for c in changes {
        if !w.is_alive(c.control) {
            continue;
        }
        if Some(c.control) == g.checkbox.map(|h| h.root) {
            if let SelectionValue::Checked(v) = c.value {
                g.checked = v;
                changed = true;
            }
        } else if Some(c.control) == g.single.map(|h| h.root) {
            if let SelectionValue::Selected(ids) = c.value {
                g.selected = ids.first().copied();
                changed = true;
            }
        } else if Some(c.control) == g.multi.map(|h| h.root) {
            if let SelectionValue::Selected(ids) = c.value {
                g.selected_many = ids;
                changed = true;
            }
        }
        if changed {
            g.last = "Standalone request accepted by synthetic host (inspector records unchanged)"
                .into();
        }
    }
    for e in commits {
        if let PropertyEventKind::CommitRequested { after: value, .. } = e.kind {
            if let Some(p) = g.committed.iter_mut().find(|p| p.id == e.property) {
                p.value = value;
                g.last = format!("Explicit Commit accepted for {}", p.label);
                changed = true;
            }
        }
    }
    for i in intents {
        match i.name.as_str() {
            "rndr-ui:Command(1)" => {
                g.options.reverse();
                g.last = "Options reordered: stable IDs and selection retained".into();
                changed = true;
            }
            "rndr-ui:Command(2)" => {
                g.options.pop();
                g.last = "Last option removed: stale IDs ignored; model remains host-owned".into();
                changed = true;
            }
            "rndr-ui:Command(3)" => {
                g.checked = true;
                g.selected = Some(OptionId(1));
                g.selected_many = vec![];
                g.last = "Standalone values reset by host".into();
                changed = true;
            }
            _ => {}
        }
    }
    if changed {
        w.insert_resource(g);
        reconcile(w);
    }
}
