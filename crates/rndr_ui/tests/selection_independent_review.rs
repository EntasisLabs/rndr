//! Independent review probes; expected-contract assertions intentionally expose defects.
use rndr_core::*;
use rndr_ui::{data::*, selection::*, UiId, UiPlugin};
fn setup() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(400., 400.));
    let r = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 8.,
        ..Layout::column()
    },));
    (a, r)
}
fn frame(a: &mut App) {
    rndr_paint::frame(a);
}
fn key(a: &mut App, keys: Vec<KeyEvent>) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = keys;
    frame(a);
}
fn k(n: NamedKey) -> KeyEvent {
    KeyEvent::named(n, Modifiers::default())
}
fn options() -> Vec<SelectOption> {
    (1..=40)
        .map(|i| SelectOption::new(i, format!("Option {i}")))
        .collect()
}
#[test]
fn initial_selected_last_row_is_visible_after_open_and_settle() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Team"),
        &options(),
        Some(OptionId(40)),
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    key(&mut a, vec![k(NamedKey::Enter)]);
    frame(&mut a);
    let e = h.option(a.world(), OptionId(40)).unwrap();
    let clip = a.world().get::<Clip>(e).unwrap();
    assert!(
        clip.h > 0.,
        "initial selected row remains invisible: {:?}, {:?}",
        a.world().get::<Rect>(e),
        clip
    );
}
#[test]
fn end_navigation_reveals_last_row() {
    let (mut a, r) = setup();
    let h = SingleSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Team"),
        &options(),
        None,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    key(&mut a, vec![k(NamedKey::Enter), k(NamedKey::End)]);
    frame(&mut a);
    assert!(
        a.world()
            .get::<Clip>(h.option(a.world(), OptionId(40)).unwrap())
            .unwrap()
            .h
            > 0.
    );
}
#[test]
fn checkbox_ime_space_release_never_emits() {
    let (mut a, r) = setup();
    let h = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Check"),
        false,
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    key(&mut a, vec![k(NamedKey::Space)]);
    a.world_mut().resource_mut::<Input>().unwrap().ime_enabled = true;
    let mut up = k(NamedKey::Space);
    up.released = true;
    key(&mut a, vec![up]);
    assert!(events::<SelectionChange>(a.world()).is_empty());
}
#[test]
fn readonly_invalidation_swallows_matching_outside_release() {
    let (mut a, r) = setup();
    let owner = a.world_mut().spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    set_parent(a.world_mut(), owner, r);
    let h = MultiSelect::mount(
        a.world_mut(),
        owner,
        UiId(1),
        SelectionProps::new("Teams"),
        &options()[..2],
        &[],
    )
    .unwrap();
    let other = Checkbox::mount(
        a.world_mut(),
        r,
        UiId(2),
        SelectionProps::new("Behind"),
        false,
    )
    .unwrap();
    frame(&mut a);
    let rc = *a.world().get::<Rect>(other.root).unwrap();
    let p = [rc.x + rc.w / 2., rc.y + rc.h / 2.];
    request_focus(a.world_mut(), h.root);
    key(&mut a, vec![k(NamedKey::Enter)]);
    a.world_mut().insert(
        owner,
        Semantics {
            name: Some("Teams owner".into()),
            readonly: true,
            ..Default::default()
        },
    );
    {
        let i = a.world_mut().resource_mut::<Input>().unwrap();
        i.pointer.position = p;
        i.pointer.press_at = Some(p);
        i.pointer.pressed = true;
        i.pointer.down = true;
    }
    frame(&mut a);
    {
        let i = a.world_mut().resource_mut::<Input>().unwrap();
        i.pointer.position = p;
        i.pointer.released = true;
        i.pointer.down = false;
    }
    frame(&mut a);
    assert!(events::<SelectionChange>(a.world()).is_empty());
    assert!(a.world().has::<Hidden>(h.popup(a.world()).unwrap()));
}
fn inspector(a: &mut App, r: Entity, value: Vec<u64>) -> InspectorHandle {
    PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(7),
        InspectorProps::new("Form", ObjectId(1)),
        &[PropertyGroup {
            id: GroupId(1),
            label: "Settings".into(),
        }],
        &[Property {
            id: PropertyId(1),
            group: GroupId(1),
            label: "Channels".into(),
            description: String::new(),
            kind: PropertyKind::MultiChoice(vec![
                Choice {
                    id: 1,
                    label: "One".into(),
                },
                Choice {
                    id: 2,
                    label: "Two".into(),
                },
            ]),
            value: PropertyValue::Choices(value),
            enabled: true,
            readonly: false,
            validate: None,
            validation: None,
        }],
        |_, _, _| {},
    )
    .unwrap()
}
#[test]
fn inspector_popup_blur_cancels_dirty_draft() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, vec![]);
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    key(&mut a, vec![k(NamedKey::Enter), k(NamedKey::Enter)]);
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![1]))
    );
    send(a.world_mut(), WindowFocus::Blurred);
    frame(&mut a);
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Choices(vec![]))
    );
    assert!(!events::<PropertyEvent>(a.world())
        .iter()
        .any(|e| matches!(e.kind, PropertyEventKind::CommitRequested { .. })));
}
#[test]
fn multi_inspector_accessible_name_includes_current_value() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, vec![2]);
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    let tree = access_tree(a.world());
    let n = tree.nodes.iter().find(|n| n.id == access_id(e)).unwrap();
    assert!(
        n.name.contains("Two") || n.value.contains("Two"),
        "multi trigger name/value drop selected value: {:?}",
        n
    );
}
#[test]
fn noncanonical_host_multi_value_does_not_request_spurious_commit() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, vec![2, 1]);
    frame(&mut a);
    assert!(
        !h.commit(a.world_mut(), PropertyId(1)).unwrap(),
        "unchanged set must not request a commit solely due to vector ordering"
    );
}
#[test]
fn keyboard_roundtrip_same_set_does_not_request_spurious_commit() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, vec![2, 1]);
    frame(&mut a);
    let slot = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), slot.editor);
    // Open, toggle first option off and back on, close, Tab, explicitly Commit.
    key(
        &mut a,
        vec![
            k(NamedKey::Enter),
            k(NamedKey::Enter),
            k(NamedKey::Enter),
            k(NamedKey::Escape),
            k(NamedKey::Tab),
            k(NamedKey::Enter),
        ],
    );
    let commits: Vec<_> = events::<PropertyEvent>(a.world())
        .iter()
        .filter(|e| matches!(e.kind, PropertyEventKind::CommitRequested { .. }))
        .collect();
    assert!(
        commits.is_empty(),
        "same selected set produced unnecessary host mutation request: {:?}",
        commits
    );
}
#[test]
fn removed_armed_option_cannot_retarget_its_space_release() {
    let (mut a, r) = setup();
    let h = MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Teams"),
        &options()[..2],
        &[],
    )
    .unwrap();
    frame(&mut a);
    request_focus(a.world_mut(), h.root);
    key(&mut a, vec![k(NamedKey::Enter), k(NamedKey::Space)]);
    MultiSelect::mount(
        a.world_mut(),
        r,
        UiId(1),
        SelectionProps::new("Teams"),
        &options()[1..2],
        &[],
    )
    .unwrap();
    let mut up = k(NamedKey::Space);
    up.released = true;
    key(&mut a, vec![up]);
    assert!(events::<SelectionChange>(a.world()).is_empty());
}
