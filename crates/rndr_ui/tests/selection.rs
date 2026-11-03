use rndr_core::*;
use rndr_ui::{controls, data::*, selection::*, UiId, UiPlugin};
fn setup() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(600., 500.));
    let r = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: 12.,
        gap: 8.,
        ..Layout::column()
    },));
    (a, r)
}
fn frame(a: &mut App) {
    rndr_paint::frame(a);
}
fn k(k: NamedKey) -> KeyEvent {
    KeyEvent::named(k, Modifiers::default())
}
fn batch(a: &mut App, keys: Vec<KeyEvent>) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = keys;
    frame(a);
}
fn options() -> Vec<SelectOption> {
    let mut o = vec![
        SelectOption::new(10, "Alpha"),
        SelectOption::new(20, "Unavailable"),
        SelectOption::new(30, "Gamma"),
    ];
    o[1].enabled = false;
    o
}
fn changes(a: &App) -> Vec<SelectionValue> {
    events::<SelectionChange>(a.world())
        .iter()
        .map(|e| e.value.clone())
        .collect()
}
fn point(a: &App, e: Entity) -> [f32; 2] {
    let r = *a.world().get::<Rect>(e).unwrap();
    [r.x + r.w / 2., r.y + r.h / 2.]
}
fn pointer(a: &mut App, p: [f32; 2], down: bool) {
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.position = p;
    i.pointer.press_at = Some(p);
    i.pointer.down = down;
    i.pointer.pressed = down;
    i.pointer.released = !down;
    frame(a);
}
#[test]
fn checkbox_requests_do_not_mutate_acknowledged_state_and_space_requires_release() {
    let (mut a, r) = setup();
    let h = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Enable"),
        false,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(&mut a, vec![k(NamedKey::Enter), k(NamedKey::Enter)]);
    assert_eq!(
        changes(&a),
        vec![
            SelectionValue::Checked(true),
            SelectionValue::Checked(false)
        ]
    );
    assert_eq!(
        a.world().get::<SelectionAccess>(h.root).unwrap().checked,
        Some(false)
    );
    batch(&mut a, vec![k(NamedKey::Space)]);
    assert!(changes(&a).is_empty());
    let mut up = k(NamedKey::Space);
    up.released = true;
    batch(&mut a, vec![up]);
    assert_eq!(changes(&a), vec![SelectionValue::Checked(true)]);
    Checkbox::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Enable"),
        true,
    )
    .unwrap();
    assert_eq!(
        a.world().get::<SelectionAccess>(h.root).unwrap().checked,
        Some(true)
    );
}
#[test]
fn single_queued_open_navigate_choose_keeps_highlight_separate_from_selection() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Phase"),
        &options(),
        Some(OptionId(10)),
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(&mut a, vec![k(NamedKey::Enter), k(NamedKey::ArrowDown)]);
    let gamma = h.option(a.world(), OptionId(30)).unwrap();
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(gamma));
    assert!(changes(&a).is_empty());
    assert!(!a.world().get::<SelectedState>(gamma).unwrap().0);
    batch(&mut a, vec![k(NamedKey::Enter)]);
    assert_eq!(
        changes(&a),
        vec![SelectionValue::Selected(vec![OptionId(30)])]
    );
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
    assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
}
#[test]
fn multi_toggles_stay_open_and_escape_closes_without_rollback() {
    let (mut a, r) = setup();
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Tags"),
        &options(),
        &[],
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(
        &mut a,
        vec![
            k(NamedKey::Enter),
            k(NamedKey::Enter),
            k(NamedKey::ArrowDown),
            k(NamedKey::Enter),
            k(NamedKey::Enter),
        ],
    );
    assert_eq!(
        changes(&a),
        vec![
            SelectionValue::Selected(vec![OptionId(10)]),
            SelectionValue::Selected(vec![OptionId(10), OptionId(30)]),
            SelectionValue::Selected(vec![OptionId(10)])
        ]
    );
    let popup = h.popup(a.world()).unwrap();
    assert!(!a.world().has::<Hidden>(popup));
    assert!(a.world().has::<Modal>(popup));
    batch(&mut a, vec![k(NamedKey::Escape)]);
    assert!(a.world().has::<Hidden>(popup));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
    assert!(changes(&a).is_empty());
}
#[test]
fn tab_closes_and_walks_from_trigger_not_into_other_popup_rows() {
    let (mut a, r) = setup();
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Tags"),
        &options(),
        &[],
    )
    .unwrap();
    let next = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(2),
        SelectionProps::new("Next"),
        false,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(
        &mut a,
        vec![k(NamedKey::Enter), k(NamedKey::Tab), k(NamedKey::Enter)],
    );
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(next.root)
    );
    assert_eq!(changes(&a), vec![SelectionValue::Checked(true)]);
    assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
}
#[test]
fn outside_press_release_never_clicks_through_and_at_background_is_blocked() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Phase"),
        &options(),
        None,
    )
    .unwrap();
    let spacer = a.world_mut().spawn((Layout {
        height: Size::Px(260.),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), spacer, r);
    let other = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(2),
        SelectionProps::new("Behind"),
        false,
    )
    .unwrap();
    frame(&mut a);
    let p = point(&a, other.root);
    request_focus(a.world_mut(), h.root);
    batch(&mut a, vec![k(NamedKey::Enter)]);
    assert_eq!(
        try_apply_access(a.world_mut(), AccessCommand::Activate(other.root)),
        AccessCommandResult::Rejected
    );
    pointer(&mut a, p, true);
    pointer(&mut a, p, false);
    assert!(changes(&a).is_empty());
    assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
    pointer(&mut a, p, true);
    pointer(&mut a, p, false);
    assert_eq!(changes(&a), vec![SelectionValue::Checked(true)]);
}
#[test]
fn pointer_option_release_selects_and_does_not_toggle_on_press() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Phase"),
        &options(),
        None,
    )
    .unwrap();
    frame(&mut a);
    let p = point(&a, h.root);
    pointer(&mut a, p, true);
    pointer(&mut a, p, false);
    let option = h.option(a.world(), OptionId(30)).unwrap();
    let p = point(&a, option);
    pointer(&mut a, p, true);
    assert!(changes(&a).is_empty());
    pointer(&mut a, p, false);
    assert_eq!(
        changes(&a),
        vec![SelectionValue::Selected(vec![OptionId(30)])]
    );
}
#[test]
fn reorder_remove_reconcile_retains_ids_and_moves_stale_highlight() {
    let (mut a, r) = setup();
    let o = options();
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Tags"),
        &o,
        &[OptionId(10)],
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(&mut a, vec![k(NamedKey::Enter), k(NamedKey::ArrowDown)]);
    let old = h.option(a.world(), OptionId(10)).unwrap();
    let reordered = vec![o[2].clone(), o[0].clone()];
    MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Tags"),
        &reordered,
        &[OptionId(10)],
    )
    .unwrap();
    assert_eq!(h.option(a.world(), OptionId(10)), Some(old));
    let popup = h.popup(a.world()).unwrap();
    assert_eq!(
        children_of(a.world(), popup),
        vec![h.option(a.world(), OptionId(30)).unwrap(), old]
    );
    MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Tags"),
        &o[..1],
        &[OptionId(30)],
    )
    .unwrap();
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(old));
    batch(&mut a, vec![k(NamedKey::Enter)]);
    assert_eq!(
        changes(&a),
        vec![SelectionValue::Selected(vec![OptionId(10)])]
    );
}
#[test]
fn empty_all_disabled_and_semantics_are_truthful() {
    let (mut a, r) = setup();
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Empty tags"),
        &[],
        &[],
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(
        &mut a,
        vec![
            k(NamedKey::Enter),
            k(NamedKey::ArrowDown),
            k(NamedKey::Enter),
        ],
    );
    let p = h.popup(a.world()).unwrap();
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(p));
    assert!(changes(&a).is_empty());
    let tree = access_tree(a.world());
    assert!(tree
        .nodes
        .iter()
        .any(|n| n.role == AccessRole::ComboBox && n.expanded == Some(true)));
    assert!(a.world().get::<SelectionAccess>(p).unwrap().multiselect);
    let mut o = options();
    for x in &mut o {
        x.enabled = false;
    }
    MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Empty tags"),
        &o,
        &[],
    )
    .unwrap();
    let e = h.option(a.world(), OptionId(10)).unwrap();
    assert_eq!(
        try_apply_access(a.world_mut(), AccessCommand::Activate(e)),
        AccessCommandResult::Rejected
    );
}
#[test]
fn ancestor_disabled_readonly_hidden_and_window_blur_close_scopes() {
    for policy in 0..4 {
        let (mut a, r) = setup();
        let h = MultiSelect::mount(
            a.world_mut(),
            r,
            UiId(1),
            SelectionProps::new("Tags"),
            &options(),
            &[],
        )
        .unwrap();
        frame(&mut a);
        request_focus(a.world_mut(), h.root);
        batch(&mut a, vec![k(NamedKey::Enter)]);
        match policy {
            0 => a.world_mut().insert(r, ControlDisabled),
            1 => a.world_mut().insert(
                r,
                Semantics {
                    readonly: true,
                    ..Default::default()
                },
            ),
            2 => a.world_mut().insert(r, Hidden),
            _ => send(a.world_mut(), WindowFocus::Blurred),
        };
        frame(&mut a);
        assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
        assert!(changes(&a).is_empty());
        if policy < 3 {
            assert!(!access_details(a.world(), h.root).activate);
        }
    }
}
#[test]
fn repeated_synthetic_modified_and_ime_edges_do_not_act() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Phase"),
        &options(),
        None,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    for mode in 0..4 {
        let mut event = k(NamedKey::Enter);
        match mode {
            0 => event.repeat = true,
            1 => event.synthetic = true,
            2 => event.modifiers.alt = true,
            _ => a.world_mut().resource_mut::<Input>().unwrap().ime_enabled = true,
        };
        batch(&mut a, vec![event]);
        assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
        assert!(changes(&a).is_empty());
    }
}
#[test]
fn placement_many_rows_constrained_scroll_and_dead_anchor_cleanup() {
    let (mut a, r) = setup();
    a.world_mut().insert_resource(Viewport::new(210., 170.));
    let o: Vec<_> = (0..100)
        .map(|i| SelectOption::new(i, format!("Long option {i}")))
        .collect();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Many"),
        &o,
        None,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    batch(&mut a, vec![k(NamedKey::Enter), k(NamedKey::End)]);
    frame(&mut a);
    let p = h.popup(a.world()).unwrap();
    let rect = *a.world().get::<Rect>(p).unwrap();
    assert!(rect.x >= 0. && rect.y >= 0. && rect.x + rect.w <= 210.1 && rect.y + rect.h <= 170.1);
    assert!(a.world().get::<Scroll>(p).unwrap().content[1] > rect.h);
    despawn(a.world_mut(), r);
    frame(&mut a);
    frame(&mut a);
    assert!(!a.world().is_alive(p));
    let rect = popup_rect(
        Rect {
            x: 180.,
            y: 145.,
            w: 50.,
            h: 20.,
        },
        Viewport::new(200., 180.),
        260.,
        120.,
    );
    assert!(rect.y < 145. && rect.x + rect.w <= 200. && rect.h <= 141.);
}
fn inspector(a: &mut App, r: Entity, values: &[Property]) -> InspectorHandle {
    PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(99),
        InspectorProps::new("Form", ObjectId(1)),
        &[PropertyGroup {
            id: GroupId(1),
            label: "Settings".into(),
        }],
        values,
        |_, _, _| {},
    )
    .unwrap()
}
fn property(id: u64, kind: PropertyKind, value: PropertyValue) -> Property {
    Property {
        id: PropertyId(id),
        group: GroupId(1),
        label: format!("Property {id}"),
        description: String::new(),
        kind,
        value,
        enabled: true,
        readonly: false,
        validate: None,
        validation: None,
    }
}
#[test]
fn inspector_multi_popup_focus_boundary_draft_validation_commit_and_delayed_ack() {
    let (mut a, r) = setup();
    let choices = vec![
        Choice {
            id: 10,
            label: "Alpha".into(),
        },
        Choice {
            id: 30,
            label: "Gamma".into(),
        },
    ];
    let mut values = vec![
        property(
            1,
            PropertyKind::MultiChoice(choices),
            PropertyValue::Choices(vec![]),
        ),
        property(2, PropertyKind::Boolean, PropertyValue::Boolean(false)),
    ];
    let h = inspector(&mut a, r, &values);
    frame(&mut a);
    let slot = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), slot.editor);
    batch(
        &mut a,
        vec![
            k(NamedKey::Enter),
            k(NamedKey::Enter),
            k(NamedKey::ArrowDown),
            k(NamedKey::Enter),
        ],
    );
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![10, 30]))
    );
    assert!(!events::<PropertyEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, PropertyEventKind::CommitRequested { .. })));
    batch(&mut a, vec![k(NamedKey::Tab)]);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(slot.commit)
    );
    batch(&mut a, vec![k(NamedKey::Enter)]);
    assert!(events::<PropertyEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, PropertyEventKind::CommitRequested { .. })));
    request_focus(a.world_mut(), slot.editor);
    batch(
        &mut a,
        vec![k(NamedKey::Enter), k(NamedKey::Enter), k(NamedKey::Escape)],
    );
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![30]))
    );
    values[0].value = PropertyValue::Choices(vec![10, 30]);
    inspector(&mut a, r, &values);
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![30]))
    );
    batch(
        &mut a,
        vec![k(NamedKey::Tab), k(NamedKey::Tab), k(NamedKey::Tab)],
    );
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![10, 30]))
    );
}
#[test]
fn duplicate_and_oversize_options_fail_atomically_and_key_conflicts_rejected() {
    let (mut a, r) = setup();
    let h = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Check"),
        false,
    )
    .unwrap();
    assert!(SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Bad"),
        &options(),
        None
    )
    .is_err());
    let mut o = options();
    o.push(o[0].clone());
    assert!(MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(2),
        SelectionProps::new("Bad"),
        &o,
        &[]
    )
    .is_err());
    assert!(a.world().is_alive(h.root));
    let o: Vec<_> = (0..257).map(|i| SelectOption::new(i, "X")).collect();
    assert!(MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(2),
        SelectionProps::new("Bad"),
        &o,
        &[]
    )
    .is_err());
    let _ = controls::HostIntent::Command(0);
}
