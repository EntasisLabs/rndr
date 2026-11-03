//! Large uniform-height collection with genuinely embedded operating controls.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::{
    collections::{Item as RowItem, *},
    controls::{self, ButtonProps, HostIntent},
    selection::{Checkbox, SelectionChange, SelectionProps, SelectionValue},
    *,
};
use std::{collections::BTreeSet, sync::Arc};
#[derive(Clone)]
pub struct Gallery {
    pub parent: Entity,
    pub list: VirtualViewHandle,
    pub records: Arc<Vec<RowItem>>,
    pub checked: Arc<BTreeSet<ItemId>>,
    pub even: bool,
    pub status: Entity,
    pub last_event: String,
}
fn render(w: &mut World, slot: RowSlot, item: &RowItem) {
    w.get_mut::<Layout>(slot.content).unwrap().axis = Axis::Row;
    w.get_mut::<Layout>(slot.content).unwrap().gap = 8.;
    let label = children_of(w, slot.content)
        .into_iter()
        .find(|e| w.has::<Label>(*e));
    if let Some(e) = label {
        w.get_mut::<Label>(e).unwrap().text = item.name.clone();
    } else {
        let e = text(w, slot.content, &item.name, Typography::Label).unwrap();
        w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    }
    controls::Button::mount(
        w,
        slot.content,
        UiId(1),
        ButtonProps::new("Inspect", HostIntent::Command(10)),
    )
    .unwrap();
    let checked = w
        .resource::<Gallery>()
        .is_some_and(|g| g.checked.contains(&item.id));
    Checkbox::mount(
        w,
        slot.content,
        UiId(2),
        SelectionProps::new("Marked"),
        checked,
    )
    .unwrap();
}
pub fn create(n: u64, width: f32, height: f32, theme: UiTheme) -> App {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let w = a.world_mut();
    w.insert_resource(theme);
    w.insert_resource(Viewport::new(width, height));
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 8.,
        padding: 16.,
        ..Layout::column()
    },));
    text(
        w,
        root,
        "Virtual semantic list · 100,000 stable keys",
        Typography::Title,
    )
    .unwrap();
    text(w, root, "Arrows focus; Space/click selects; Enter activates. Nested controls never bubble. Row-local widget state is ephemeral.", Typography::Secondary).unwrap();
    let tools = w.spawn((Layout {
        width: Size::Fill,
        gap: 6.,
        ..Layout::row()
    },));
    set_parent(w, tools, root);
    for (i, name) in [
        "Filter even",
        "Reverse",
        "Replace labels",
        "Jump last",
        "Disable first",
    ]
    .iter()
    .enumerate()
    {
        controls::Button::mount(
            w,
            tools,
            UiId(i as u64),
            ButtonProps::new(*name, HostIntent::Command(i as u64)),
        )
        .unwrap();
    }
    let records: Vec<_> = (0..n)
        .map(|id| RowItem::new(id, format!("Record {id:06}")))
        .collect();
    let stride = 56. * w.resource::<UiTheme>().unwrap().text_scale.max(1.);
    let list = ListView::mount_virtual(
        w,
        root,
        UiId(20),
        ViewProps::new("Records"),
        &records,
        stride,
        render,
    )
    .unwrap();
    w.get_mut::<Layout>(list.root).unwrap().height = Size::Fill;
    let status = text(w, root, "Ready", Typography::Secondary).unwrap();
    w.insert_resource(Gallery {
        parent: root,
        list,
        records: Arc::new(records),
        checked: Arc::new(BTreeSet::new()),
        even: false,
        status,
        last_event: "Ready".into(),
    });
    a.add_system(Schedule::Update, update);
    a
}
pub fn update(w: &mut World) {
    let mut g = w.resource::<Gallery>().unwrap().clone();
    let mut dirty = false;
    for request in take_action_intents(w) {
        let Some(HostIntent::Command(id)) = controls::host_intent(w, &request) else {
            continue;
        };
        match id {
            0 => {
                g.even = !g.even;
                dirty = true;
            }
            1 => {
                Arc::make_mut(&mut g.records).reverse();
                dirty = true;
            }
            2 => {
                for i in Arc::make_mut(&mut g.records) {
                    i.name = format!("Replacement {}", i.id.0);
                }
                dirty = true;
            }
            3 => {
                if let Some(id) = g
                    .records
                    .iter()
                    .rfind(|i| i.enabled && (!g.even || i.id.0 % 2 == 0))
                    .map(|i| i.id)
                {
                    let _ = g.list.focus(w, id);
                }
            }
            4 => {
                if let Some(i) = Arc::make_mut(&mut g.records).first_mut() {
                    i.enabled = !i.enabled;
                    dirty = true;
                }
            }
            10 => {
                g.last_event = format!(
                    "Embedded inspect {:?}; parent selection {:?}",
                    g.list.item_for_entity(w, request.entity),
                    g.list.selected(w)
                )
            }
            _ => (),
        }
    }
    for request in events::<SelectionChange>(w) {
        if let (Some(id), SelectionValue::Checked(value)) =
            (g.list.item_for_entity(w, request.control), &request.value)
        {
            if *value {
                Arc::make_mut(&mut g.checked).insert(id);
            } else {
                Arc::make_mut(&mut g.checked).remove(&id);
            }
            g.last_event = format!(
                "Marked {id:?}: {value}; parent selection {:?}",
                g.list.selected(w)
            );
        }
    }
    for event in events::<CollectionEvent>(w) {
        if event.view == g.list.root {
            g.last_event = format!("{:?}", event.kind);
        }
    }
    if dirty {
        let records: Vec<_> = g
            .records
            .iter()
            .filter(|i| !g.even || i.id.0 % 2 == 0)
            .cloned()
            .collect();
        let stride = 56. * w.resource::<UiTheme>().unwrap().text_scale.max(1.);
        ListView::mount_virtual(
            w,
            g.parent,
            UiId(20),
            ViewProps::new("Records"),
            &records,
            stride,
            render,
        )
        .unwrap();
    }
    w.get_mut::<Label>(g.status).unwrap().text = format!(
        "{} logical rows · selection {:?} · focus {:?} · {}",
        g.list.len(w),
        g.list.selected(w),
        g.list.focused(w),
        g.last_event
    );
    w.insert_resource(g);
}
