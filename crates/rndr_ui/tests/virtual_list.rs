use rndr_core::*;
use rndr_ui::{
    collections::{
        CollectionEvent, CollectionEventKind, Item as RowItem, ItemId, ListView, RowSlot,
        ViewProps, VirtualViewHandle,
    },
    UiId, UiPlugin,
};
use std::time::Instant;
#[derive(Default)]
struct Builds(usize);
fn compose(w: &mut World, slot: RowSlot, item: &RowItem) {
    w.resource_mut::<Builds>().unwrap().0 += 1;
    if let Some(e) = children_of(w, slot.content).first().copied() {
        w.get_mut::<Label>(e).unwrap().text = item.name.clone();
    } else {
        rndr_ui::text(w, slot.content, &item.name, rndr_ui::Typography::Label).unwrap();
    }
}
fn items(n: u64) -> Vec<RowItem> {
    (0..n)
        .map(|id| RowItem::new(id, format!("Record {id}")))
        .collect()
}
fn setup(n: u64) -> (App, Entity, VirtualViewHandle) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(400., 320.));
    a.world_mut().insert_resource(Builds::default());
    let p = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let h = ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &items(n),
        32.,
        compose,
    )
    .unwrap();
    a.world_mut().get_mut::<Layout>(h.root).unwrap().height = Size::Px(320.);
    rndr_paint::frame(&mut a);
    (a, p, h)
}
fn key(a: &mut App, k: NamedKey) {
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(k, Modifiers::default())];
    rndr_paint::frame(a);
}
fn click(a: &mut App, pos: [f32; 2]) {
    {
        let i = a.world_mut().resource_mut::<Input>().unwrap();
        i.pointer.position = pos;
        i.pointer.down = true;
        i.pointer.released = false;
        i.pointer.pressed = true;
    }
    rndr_paint::frame(a);
    {
        let i = a.world_mut().resource_mut::<Input>().unwrap();
        i.pointer.down = false;
        i.pointer.pressed = false;
        i.pointer.released = true;
    }
    rndr_paint::frame(a);
}
#[test]
fn hundred_thousand_rows_have_bounded_entities_callbacks_and_no_steady_entity_allocations() {
    let t = Instant::now();
    let (mut a, _, h) = setup(100_000);
    let mount = t.elapsed();
    assert_eq!(h.len(a.world()), 100_000);
    let count = a.world().entities_with::<Layout>().len();
    assert!(count < 50, "{count}");
    let identities = a.world().entities_with::<Layout>();
    a.world_mut().resource_mut::<Builds>().unwrap().0 = 0;
    let t = Instant::now();
    for _ in 0..20 {
        rndr_paint::frame(&mut a);
    }
    let steady = t.elapsed();
    let steady_callbacks = a.world().resource::<Builds>().unwrap().0;
    assert_eq!(identities, a.world().entities_with::<Layout>());
    assert!(a.world().resource::<Builds>().unwrap().0 <= 20 * 40);
    h.select(a.world_mut(), Some(ItemId(50_000))).unwrap();
    for i in [1000, 20_000, 50_000, 99_999, 0] {
        h.reveal(a.world_mut(), ItemId(i)).unwrap();
        rndr_paint::frame(&mut a);
        assert!(a.world().entities_with::<Layout>().len() <= count + 3);
        assert_eq!(h.selected(a.world()), Some(ItemId(50_000)));
    }
    eprintln!("100000 metadata: mount+frame={mount:?}; 20 steady headless paint frames={steady:?}; realized layouts={count}; steady_callbacks={steady_callbacks}");
}
#[test]
fn keys_survive_filter_reorder_replace_and_recycling() {
    let (mut a, p, h) = setup(2000);
    h.select(a.world_mut(), Some(ItemId(3))).unwrap();
    h.focus(a.world_mut(), ItemId(4)).unwrap();
    rndr_paint::frame(&mut a);
    let entity = h.row(a.world(), ItemId(4)).unwrap().root;
    let mut data = items(2000);
    data.reverse();
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &data,
        32.,
        compose,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    assert_eq!(h.focused(a.world()), Some(ItemId(4)));
    assert_eq!(h.selected(a.world()), Some(ItemId(3)));
    // Entity is allowed to recycle, but input/model identity is never the pool slot.
    let _ = entity;
    data.retain(|i| i.id.0 < 10);
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &data,
        32.,
        compose,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    assert_eq!(h.focused(a.world()), Some(ItemId(4)));
    assert_eq!(h.selected(a.world()), Some(ItemId(3)));
    h.reveal(a.world_mut(), ItemId(9)).unwrap();
    key(&mut a, NamedKey::Enter);
    assert!(events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| e.kind == CollectionEventKind::Activated(ItemId(4))));
}
#[test]
fn removed_disabled_and_empty_metadata_reconcile_without_stale_activation() {
    let (mut a, p, h) = setup(100);
    h.focus(a.world_mut(), ItemId(3)).unwrap();
    h.select(a.world_mut(), Some(ItemId(3))).unwrap();
    let mut data = items(100);
    data[3].enabled = false;
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &data,
        32.,
        compose,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    assert_ne!(h.focused(a.world()), Some(ItemId(3)));
    assert_eq!(h.selected(a.world()), None);
    assert!(h.focus(a.world_mut(), ItemId(3)).is_err());
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &[],
        32.,
        compose,
    )
    .unwrap();
    key(&mut a, NamedKey::ArrowUp);
    key(&mut a, NamedKey::Enter);
    assert!(h.is_empty(a.world()));
    assert!(!events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, CollectionEventKind::Activated(_))));
}
#[test]
fn selection_and_activation_remain_independent() {
    let (mut a, _, h) = setup(100);
    h.focus(a.world_mut(), ItemId(1)).unwrap();
    key(&mut a, NamedKey::ArrowDown);
    assert_eq!(h.focused(a.world()), Some(ItemId(2)));
    assert_eq!(h.selected(a.world()), None);
    key(&mut a, NamedKey::Enter);
    assert_eq!(h.selected(a.world()), None);
    assert!(events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| e.kind == CollectionEventKind::Activated(ItemId(2))));
    key(&mut a, NamedKey::Space);
    let mut up = KeyEvent::named(NamedKey::Space, Modifiers::default());
    up.released = true;
    a.world_mut().resource_mut::<Input>().unwrap().keys = vec![up];
    rndr_paint::frame(&mut a);
    assert_eq!(h.selected(a.world()), Some(ItemId(2)));
}
#[test]
fn scroll_then_click_uses_new_geometry_and_not_recycled_item() {
    let (mut a, _, h) = setup(1000);
    {
        let input = a.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [100., 16.];
        input.scroll = [0., -320.];
        input.pointer.pressed = true;
        input.pointer.down = true;
    }
    rndr_paint::frame(&mut a);
    assert_eq!(
        h.focused(a.world()),
        Some(ItemId(10)),
        "press must hit new geometry before release"
    );
    assert_eq!(h.selected(a.world()), None);
    {
        let input = a.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.released = true;
        input.pointer.down = false;
        input.scroll = [0., 0.];
    }
    rndr_paint::frame(&mut a);
    assert_eq!(h.selected(a.world()), Some(ItemId(10)));
}
#[test]
fn keyboard_reveal_does_not_undo_later_deliberate_scroll() {
    let (mut a, _, h) = setup(1000);
    h.focus(a.world_mut(), ItemId(0)).unwrap();
    key(&mut a, NamedKey::End);
    assert_eq!(h.focused(a.world()), Some(ItemId(999)));
    let r = *a
        .world()
        .get::<Rect>(h.row(a.world(), ItemId(999)).unwrap().root)
        .unwrap();
    assert!(r.y >= 0. && r.y + r.h <= 320.1, "{r:?}");
    {
        let i = a.world_mut().resource_mut::<Input>().unwrap();
        i.pointer.position = [100., 50.];
        i.scroll = [0., 320.];
    }
    rndr_paint::frame(&mut a);
    let offset = a.world().get::<Scroll>(h.root).unwrap().offset;
    for _ in 0..3 {
        rndr_paint::frame(&mut a);
    }
    assert_eq!(a.world().get::<Scroll>(h.root).unwrap().offset, offset);
    assert_eq!(h.focused(a.world()), Some(ItemId(999)));
    key(&mut a, NamedKey::ArrowUp);
    assert_eq!(h.focused(a.world()), Some(ItemId(998)));
    assert!(h.row(a.world(), ItemId(998)).is_some());
}
fn nested(w: &mut World, slot: RowSlot, item: &RowItem) {
    use rndr_ui::{
        controls::{Button as UiButton, ButtonProps, HostIntent},
        selection::{Checkbox, SelectionProps},
    };
    w.get_mut::<Layout>(slot.content).unwrap().axis = Axis::Row;
    UiButton::mount(
        w,
        slot.content,
        UiId(1),
        ButtonProps::new("Action", HostIntent::Command(item.id.0)),
    )
    .unwrap();
    Checkbox::mount(
        w,
        slot.content,
        UiId(2),
        SelectionProps::new("Enabled"),
        false,
    )
    .unwrap();
}
#[test]
fn nested_button_checkbox_pointer_keyboard_and_recycle_do_not_bubble() {
    let (mut a, p, h) = setup(1000);
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &items(1000),
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    let slot = h.row(a.world(), ItemId(0)).unwrap();
    let controls = children_of(a.world(), slot.content);
    for c in controls {
        let r = *a.world().get::<Rect>(c).unwrap();
        click(&mut a, [r.x + 8., r.y + 8.]);
        assert_eq!(h.selected(a.world()), None);
        assert!(!events::<CollectionEvent>(a.world())
            .iter()
            .any(|e| matches!(e.kind, CollectionEventKind::Activated(_))));
        request_focus(a.world_mut(), c);
        key(&mut a, NamedKey::Enter);
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(c));
        assert_eq!(h.selected(a.world()), None);
        assert!(!events::<CollectionEvent>(a.world())
            .iter()
            .any(|e| matches!(e.kind, CollectionEventKind::Activated(_))));
    }
    let old_children = children_of(a.world(), slot.content);
    h.reveal(a.world_mut(), ItemId(900)).unwrap();
    rndr_paint::frame(&mut a);
    assert!(old_children.iter().all(|e| !a.world().is_alive(*e)));
    assert_eq!(h.selected(a.world()), None);
}
#[test]
fn invalid_input_is_atomic_and_keys_conflict_with_retained_family() {
    let (mut a, p, h) = setup(100);
    let n = a.world().entities_with::<Layout>().len();
    for stride in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(ListView::mount_virtual(
            a.world_mut(),
            p,
            UiId(1),
            ViewProps::new("Records"),
            &items(100),
            stride,
            compose
        )
        .is_err());
    }
    let duplicate = [RowItem::new(1, "A"), RowItem::new(1, "B")];
    assert!(ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &duplicate,
        32.,
        compose
    )
    .is_err());
    assert!(ListView::mount(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &[],
        |_, _, _| {}
    )
    .is_err());
    assert_eq!(n, a.world().entities_with::<Layout>().len());
    assert_eq!(h.len(a.world()), 100);
}

#[test]
fn armed_row_is_cancelled_before_pool_slot_is_rebound() {
    let (mut a, _, h) = setup(1000);
    h.focus(a.world_mut(), ItemId(0)).unwrap();
    key(&mut a, NamedKey::Space);
    h.reveal(a.world_mut(), ItemId(900)).unwrap();
    let mut up = KeyEvent::named(NamedKey::Space, Modifiers::default());
    up.released = true;
    a.world_mut().resource_mut::<Input>().unwrap().keys = vec![up];
    rndr_paint::frame(&mut a);
    assert_eq!(h.selected(a.world()), None);
    // Unmount also retires nested input and every owned entity.
    let root = h.root;
    h.unmount(a.world_mut()).unwrap();
    assert!(!a.world().is_alive(root));
    assert!(h.select(a.world_mut(), Some(ItemId(1))).is_err());
}
#[test]
fn palette_policy_is_opt_in_and_nested_controls_are_excluded() {
    let (mut a, _, h) = setup(1000);
    a.world_mut().insert(
        h.root,
        rndr_ui::collections::VirtualListPolicy {
            hover_navigation: true,
            selection_follows_navigation: true,
            double_click_activate: true,
            ..Default::default()
        },
    );
    h.focus(a.world_mut(), ItemId(0)).unwrap();
    key(&mut a, NamedKey::ArrowDown);
    assert_eq!(h.selected(a.world()), Some(ItemId(1)));
    click(&mut a, [100., 80.]);
    click(&mut a, [100., 80.]);
    assert_eq!(h.selected(a.world()), Some(ItemId(2)));
    assert!(events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| e.kind == CollectionEventKind::Activated(ItemId(2))));
    let p = a.world().get::<Parent>(h.root).unwrap().entity;
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &items(1000),
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    let control = children_of(a.world(), h.row(a.world(), ItemId(0)).unwrap().content)[0];
    let r = *a.world().get::<Rect>(control).unwrap();
    click(&mut a, [r.x + 8., r.y + 8.]);
    click(&mut a, [r.x + 8., r.y + 8.]);
    assert_eq!(h.selected(a.world()), Some(ItemId(2)));
    assert!(!events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, CollectionEventKind::Activated(_))));
}
#[test]
fn nested_focus_survives_same_key_replacement_and_disabled_ancestry_blocks_it() {
    let (mut a, p, h) = setup(100);
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &items(100),
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    let c = children_of(a.world(), h.row(a.world(), ItemId(2)).unwrap().content)[0];
    assert!(request_focus(a.world_mut(), c));
    let mut data = items(100);
    data[2].name = "Replacement label".into();
    data.swap(2, 3);
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &data,
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(c));
    assert_eq!(h.focused(a.world()), Some(ItemId(2)));
    data[3].enabled = false;
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &data,
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    assert!(!access_details(a.world(), c).activate);
    assert_ne!(a.world().resource::<Focus>().unwrap().entity, Some(c));
}
#[test]
fn explicit_external_field_forwarding_preserves_focus_and_ime_and_selection_contract() {
    let (mut a, p, h) = setup(1000);
    let f = a
        .world_mut()
        .spawn((Layout::column(), TextField, CapturesKeys, Focusable));
    set_parent(a.world_mut(), f, p);
    request_focus(a.world_mut(), f);
    assert!(h.forward_key(
        a.world_mut(),
        &KeyEvent::named(NamedKey::End, Modifiers::default())
    ));
    assert_eq!(h.active(a.world()), Some(ItemId(999)));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(f));
    assert_eq!(h.selected(a.world()), None);
    a.world_mut().resource_mut::<Input>().unwrap().ime_enabled = true;
    h.forward_key(
        a.world_mut(),
        &KeyEvent::named(NamedKey::Home, Modifiers::default()),
    );
    assert_eq!(h.active(a.world()), Some(ItemId(999)));
}

#[test]
fn external_focus_owner_is_synchronous_and_never_inherited_by_nested_controls() {
    let (mut a, p, h) = setup(1000);
    let field = a
        .world_mut()
        .spawn((Layout::column(), TextField, CapturesKeys, Focusable));
    set_parent(a.world_mut(), field, p);
    a.world_mut().insert(
        h.root,
        rndr_ui::collections::VirtualListPolicy {
            focus_owner: Some(field),
            ..Default::default()
        },
    );
    let row = h.row(a.world(), ItemId(2)).unwrap().root;
    assert!(request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(field));
    assert_eq!(h.active(a.world()), Some(ItemId(2)));
    ListView::mount_virtual(
        a.world_mut(),
        p,
        UiId(1),
        ViewProps::new("Records"),
        &items(1000),
        48.,
        nested,
    )
    .unwrap();
    rndr_paint::frame(&mut a);
    let c = children_of(a.world(), h.row(a.world(), ItemId(2)).unwrap().content)[0];
    assert!(request_focus(a.world_mut(), c));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(c));
    assert_eq!(h.focused(a.world()), Some(ItemId(2)));
    assert_eq!(try_activate(a.world_mut(), c), ActivationResult::Accepted);
    assert_eq!(h.selected(a.world()), None);
    a.world_mut().insert(field, ControlDisabled);
    assert!(request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(row));
}
