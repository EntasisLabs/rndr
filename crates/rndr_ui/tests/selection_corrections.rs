//! Focused F1/F2 correction coverage; original independent probes stay unchanged.
use rndr_core::*;
use rndr_ui::{data::*, UiId, UiPlugin};
fn setup() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(600., 600.));
    let r = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    (a, r)
}
fn property(ids: &[u64]) -> Property {
    Property {
        id: PropertyId(1),
        group: GroupId(1),
        label: "Channels".into(),
        description: "Delivery routes".into(),
        kind: PropertyKind::MultiChoice(
            (1..=3)
                .map(|id| Choice {
                    id,
                    label: ["One", "Two", "Three"][id as usize - 1].into(),
                })
                .collect(),
        ),
        value: PropertyValue::Choices(ids.into()),
        enabled: true,
        readonly: false,
        validate: None,
        validation: None,
    }
}
fn mount(a: &mut App, r: Entity, p: &Property) -> InspectorHandle {
    let h = PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(7),
        InspectorProps::new("Form", ObjectId(1)),
        &[PropertyGroup {
            id: GroupId(1),
            label: "Settings".into(),
        }],
        std::slice::from_ref(p),
        |_, _, _| {},
    )
    .unwrap();
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    h
}
fn cancelled(a: &mut App) -> bool {
    rndr_paint::frame(a);
    events::<PropertyEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, PropertyEventKind::Cancelled(_)))
}
fn node(a: &App, h: InspectorHandle) -> AccessNode {
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    access_tree(a.world())
        .nodes
        .into_iter()
        .find(|n| n.id == access_id(e))
        .unwrap()
}
fn name(a: &App, h: InspectorHandle, summary: &str, open: bool) {
    let n = node(a, h);
    assert!(
        n.name.contains("Channels") && n.name.contains(summary),
        "{n:?}"
    );
    assert_eq!(n.role, AccessRole::ComboBox);
    assert_eq!(n.expanded, Some(open));
}
#[test]
fn summary_tracks_empty_draft_invalid_and_open_closed() {
    let (mut a, r) = setup();
    let h = mount(&mut a, r, &property(&[2]));
    rndr_paint::frame(&mut a);
    name(&a, h, "Two", false);
    h.set_draft(a.world_mut(), PropertyId(1), "").unwrap();
    rndr_paint::frame(&mut a);
    name(&a, h, "None selected", false);
    h.set_draft(a.world_mut(), PropertyId(1), "3,1").unwrap();
    rndr_paint::frame(&mut a);
    name(&a, h, "One, Three", false);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    rndr_paint::frame(&mut a);
    name(&a, h, "One, Three", true);
    let tree = access_tree(a.world());
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|n| n.role == AccessRole::Option && n.selected)
            .count(),
        2
    );
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
    rndr_paint::frame(&mut a);
    name(&a, h, "One, Three", false);
    h.set_draft(a.world_mut(), PropertyId(1), "99").unwrap();
    rndr_paint::frame(&mut a);
    name(&a, h, "Invalid selection: 99", false);
    assert!(h.draft(a.world(), PropertyId(1)).unwrap().error.is_some());
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
}
#[test]
fn disabled_readonly_and_validator_rejected_summary_stays_truthful() {
    for (enabled, readonly) in [(false, false), (true, true)] {
        let (mut a, r) = setup();
        let mut p = property(&[2]);
        p.enabled = enabled;
        p.readonly = readonly;
        let h = mount(&mut a, r, &p);
        rndr_paint::frame(&mut a);
        name(&a, h, "Two", false);
        let n = node(&a, h);
        assert_eq!(n.enabled, enabled);
        let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
        assert_eq!(access_details(a.world(), e).readonly, readonly);
        assert!(h.set_draft(a.world_mut(), PropertyId(1), "1").is_err());
    }
    let (mut a, r) = setup();
    let mut p = property(&[2]);
    p.validate = Some(|v| {
        if *v == PropertyValue::Choices(vec![1]) {
            Err("One rejected".into())
        } else {
            Ok(())
        }
    });
    let h = mount(&mut a, r, &p);
    h.set_draft(a.world_mut(), PropertyId(1), "1").unwrap();
    rndr_paint::frame(&mut a);
    name(&a, h, "One", false);
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().error.as_deref(),
        Some("One rejected")
    );
    assert!(access_tree(a.world())
        .nodes
        .iter()
        .any(|n| n.name.contains("Invalid: One rejected")));
}
#[test]
fn reordered_same_set_is_clean_for_cancel_reconcile_and_unmount() {
    let (mut a, r) = setup();
    let mut p = property(&[2, 1]);
    let h = mount(&mut a, r, &p);
    h.set_draft(a.world_mut(), PropertyId(1), "1,2").unwrap();
    h.cancel(a.world_mut(), PropertyId(1), CancelReason::Escape)
        .unwrap();
    assert!(!cancelled(&mut a));
    h.set_draft(a.world_mut(), PropertyId(1), "1,2").unwrap();
    p.value = PropertyValue::Choices(vec![1, 2]);
    mount(&mut a, r, &p);
    assert!(!cancelled(&mut a));
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    h.unmount(a.world_mut()).unwrap();
    assert!(!cancelled(&mut a));
    assert_eq!(p.value, PropertyValue::Choices(vec![1, 2]));
}
#[test]
fn reordered_host_reconcile_preserves_dirty_draft_without_ack() {
    let (mut a, r) = setup();
    let mut p = property(&[2, 1]);
    let h = mount(&mut a, r, &p);
    h.set_draft(a.world_mut(), PropertyId(1), "3").unwrap();
    p.value = PropertyValue::Choices(vec![1, 2]);
    mount(&mut a, r, &p);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "3");
    assert!(!cancelled(&mut a));
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    rndr_paint::frame(&mut a);
    let es = events::<PropertyEvent>(a.world());
    assert!(es.iter().any(|e| matches!(&e.kind,PropertyEventKind::CommitRequested { before: PropertyValue::Choices(b), after: PropertyValue::Choices(v) } if b==&vec![1,2] && v==&vec![3])));
}
#[test]
fn delayed_reordered_ack_preserves_later_draft_and_drains_pending() {
    let (mut a, r) = setup();
    let mut p = property(&[2]);
    let h = mount(&mut a, r, &p);
    h.set_draft(a.world_mut(), PropertyId(1), "2,1").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    // Pending duplicate suppression uses set equality, independent of raw order.
    h.set_draft(a.world_mut(), PropertyId(1), "1,2").unwrap();
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    h.set_draft(a.world_mut(), PropertyId(1), "3,2").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    h.set_draft(a.world_mut(), PropertyId(1), "1").unwrap();
    p.value = PropertyValue::Choices(vec![2, 1]);
    mount(&mut a, r, &p);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "1");
    assert!(!cancelled(&mut a));
    p.value = PropertyValue::Choices(vec![3, 2]);
    mount(&mut a, r, &p);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "1");
    assert!(!cancelled(&mut a));
    // Returning to a previously requested set is a new request: both acks drained.
    h.set_draft(a.world_mut(), PropertyId(1), "2,1").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    assert_eq!(p.value, PropertyValue::Choices(vec![3, 2]));
}
#[test]
fn ack_matching_current_reordered_draft_clears_dirty_without_rewriting_host() {
    let (mut a, r) = setup();
    let mut p = property(&[3]);
    let h = mount(&mut a, r, &p);
    h.set_draft(a.world_mut(), PropertyId(1), "1,2").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    p.value = PropertyValue::Choices(vec![2, 1]);
    mount(&mut a, r, &p);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "1,2");
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    h.cancel(a.world_mut(), PropertyId(1), CancelReason::Escape)
        .unwrap();
    assert!(!cancelled(&mut a));
    assert_eq!(p.value, PropertyValue::Choices(vec![2, 1]));
}
