use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
#[path = "../examples/support/operating.rs"]
mod operating;
fn fixture(width: f32) -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(width, 1000.0));
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(width),
        ..Layout::column()
    },));
    (a, root)
}
fn key(a: &mut App, n: NamedKey) {
    a.world_mut().insert_resource(Input::default());
    a.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(KeyEvent::named(n, Modifiers::new()), Platform::Mac);
    a.update();
    a.world_mut().insert_resource(Input::default());
}
fn pointer(a: &mut App, e: Entity, down: bool, inside: bool) {
    let r = *a.world().get::<Rect>(e).unwrap();
    let p = &mut a.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position = if inside {
        [r.x + 12.0, r.y + 12.0]
    } else {
        [r.x + r.w + 20.0, r.y + r.h + 20.0]
    };
    p.pressed = down;
    p.released = !down;
    p.down = down;
    p.press_at = Some([r.x + 12.0, r.y + 12.0]);
    a.update();
    a.world_mut().insert_resource(Input::default());
}
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x - 0.02
        && inner.y >= outer.y - 0.02
        && inner.x + inner.w <= outer.x + outer.w + 0.02
        && inner.y + inner.h <= outer.y + outer.h + 0.02
}
#[test]
fn controls_share_canonical_pointer_keyboard_at_and_fifo_without_fanout() {
    let (mut a, root) = fixture(320.0);
    let h = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Run", HostIntent::Command(9)),
    )
    .unwrap();
    a.update();
    pointer(&mut a, h.root, true, true);
    assert!(interaction_pressed(a.world(), h.root));
    assert!(take_action_intents(a.world_mut()).is_empty());
    pointer(&mut a, h.root, false, true);
    key(&mut a, NamedKey::Enter);
    assert_eq!(
        try_apply_access(a.world_mut(), AccessCommand::Activate(h.root)),
        AccessCommandResult::Activation(ActivationResult::Accepted)
    );
    a.update();
    let q = take_action_intents(a.world_mut());
    assert_eq!(q.len(), 3);
    assert!(q
        .iter()
        .all(|r| host_intent(a.world(), r) == Some(HostIntent::Command(9))));
    assert!(a.world().resource::<Fired>().unwrap().name.is_none());
    for _ in 0..64 {
        assert_eq!(
            try_activate(a.world_mut(), h.root),
            ActivationResult::Accepted
        );
    }
    assert_eq!(
        try_activate(a.world_mut(), h.root),
        ActivationResult::Overflow
    );
    assert_eq!(a.world().resource::<ActionIntents>().unwrap().overflow, 1);
    assert_eq!(take_action_intents(a.world_mut()).len(), 64);
    let legacy = a
        .world_mut()
        .spawn((Layout::column(), rndr_core::Button::new("old")));
    set_parent(a.world_mut(), legacy, root);
    request_focus(a.world_mut(), legacy);
    key(&mut a, NamedKey::Enter);
    assert_eq!(
        a.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("old")
    );
    assert!(take_action_intents(a.world_mut()).is_empty());
}
#[test]
fn cancellation_disabled_readonly_and_action_update_do_not_commit_stale_gestures() {
    let (mut a, root) = fixture(320.0);
    let mut props = ButtonProps::new("Run", HostIntent::Command(1));
    let h = controls::Button::mount(a.world_mut(), root, UiId(1), props.clone()).unwrap();
    a.update();
    pointer(&mut a, h.root, true, true);
    pointer(&mut a, h.root, false, false);
    assert!(take_action_intents(a.world_mut()).is_empty());
    pointer(&mut a, h.root, true, true);
    cancel_activation(a.world_mut());
    pointer(&mut a, h.root, false, true);
    assert!(take_action_intents(a.world_mut()).is_empty());
    pointer(&mut a, h.root, true, true);
    props.intent = HostIntent::Command(2);
    controls::Button::mount(a.world_mut(), root, UiId(1), props.clone()).unwrap();
    pointer(&mut a, h.root, false, true);
    assert!(take_action_intents(a.world_mut()).is_empty());
    for (enabled, readonly, focus, activate) in [
        (false, false, false, false),
        (true, true, true, false),
        (true, false, true, true),
    ] {
        props.enabled = enabled;
        props.readonly = readonly;
        controls::Button::mount(a.world_mut(), root, UiId(1), props.clone()).unwrap();
        a.update();
        let d = access_details(a.world(), h.root);
        assert_eq!((d.focus, d.activate), (focus, activate));
        let node = access_tree(a.world())
            .nodes
            .into_iter()
            .find(|n| n.id == access_id(h.root))
            .unwrap();
        assert_eq!(node.enabled, enabled);
        assert_eq!(
            try_activate(a.world_mut(), h.root) == ActivationResult::Accepted,
            activate
        );
        take_action_intents(a.world_mut());
    }
}
#[test]
fn lifecycle_keys_stale_generations_and_borrowed_state_survive() {
    let (mut a, root) = fixture(320.0);
    let p = ButtonProps::new("Before", HostIntent::Command(1));
    let h = controls::Button::mount(a.world_mut(), root, UiId(1), p).unwrap();
    a.update();
    request_focus(a.world_mut(), h.root);
    let h2 = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("After", HostIntent::Command(1)),
    )
    .unwrap();
    assert_eq!(h, h2);
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(h.root));
    assert_eq!(
        Card::mount(a.world_mut(), root, UiId(1), "conflict"),
        Err(UiError::IdentityConflict)
    );
    let card = Card::mount(a.world_mut(), root, UiId(2), "Passive").unwrap();
    assert_eq!(
        controls::Button::mount(
            a.world_mut(),
            root,
            UiId(2),
            ButtonProps::new("No", HostIntent::Command(1))
        ),
        Err(UiError::IdentityConflict)
    );
    assert!(!access_details(a.world(), card.root).activate);
    let borrowed = a
        .world_mut()
        .spawn((Layout::column(), Label::new("caller"), Scroll::default()));
    set_parent(a.world_mut(), borrowed, h.label);
    try_activate(a.world_mut(), h.root);
    h.destroy(a.world_mut()).unwrap();
    assert!(a.world().is_alive(borrowed));
    assert!(a.world().get::<Parent>(borrowed).is_none());
    assert!(take_action_intents(a.world_mut()).is_empty());
    let new = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("New", HostIntent::Command(1)),
    )
    .unwrap();
    assert_ne!(new.root, h.root);
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
}
#[test]
fn malformed_owned_slots_fail_before_update_or_destroy() {
    let (mut a, root) = fixture(320.0);
    let h = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("Original", HostIntent::Command(1)),
    )
    .unwrap();
    a.world_mut().remove::<Label>(h.label);
    assert_eq!(
        controls::Button::mount(
            a.world_mut(),
            root,
            UiId(1),
            ButtonProps::new("Changed", HostIntent::Command(2))
        ),
        Err(UiError::Stale)
    );
    assert_eq!(h.destroy(a.world_mut()), Err(UiError::Stale));
    assert_eq!(
        a.world().get::<rndr_core::Button>(h.root).unwrap().action,
        "rndr-ui:Command(1)"
    );
    assert!(IconButton::mount(
        a.world_mut(),
        root,
        UiId(2),
        "+",
        "",
        ButtonProps::new("+", HostIntent::Command(1))
    )
    .is_err());
}
#[test]
fn manual_modes_have_roving_entry_real_relations_and_full_inactive_exclusion() {
    let (mut a, root) = fixture(320.0);
    let h = ModeSelector::mount(
        a.world_mut(),
        root,
        UiId(1),
        "Mode",
        &["Source", "Preview"],
        TabOrientation::Horizontal,
        0,
    )
    .unwrap();
    let mut descendants = vec![];
    for panel in &h.panels {
        let b = controls::Button::mount(
            a.world_mut(),
            *panel,
            UiId(1),
            ButtonProps::new("Panel action", HostIntent::Command(7)),
        )
        .unwrap();
        descendants.push(b);
    }
    a.update();
    key(&mut a, NamedKey::Tab);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(h.tabs[0].root)
    );
    key(&mut a, NamedKey::ArrowRight);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(h.tabs[1].root)
    );
    assert_eq!(a.world().get::<Tabs>(h.group).unwrap().selected, 0);
    assert!(!request_focus(a.world_mut(), descendants[1].root));
    assert_eq!(
        try_activate(a.world_mut(), descendants[1].root),
        ActivationResult::Rejected
    );
    assert!(
        a.world().get::<Rect>(descendants[1].root).is_none(),
        "inactive panel is excluded from layout, not just painted over"
    );
    let r = *a.world().get::<Rect>(descendants[0].root).unwrap();
    assert_ne!(
        hit_test(a.world(), r.x + 12.0, r.y + 12.0),
        Some(descendants[1].root)
    );
    key(&mut a, NamedKey::Space);
    assert_eq!(a.world().get::<Tabs>(h.group).unwrap().selected, 0);
    let e = KeyEvent::named(NamedKey::Space, Modifiers::new()).released();
    a.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(e, Platform::Mac);
    a.update();
    a.world_mut().insert_resource(Input::default());
    assert_eq!(a.world().get::<Tabs>(h.group).unwrap().selected, 1);
    assert!(take_action_intents(a.world_mut()).is_empty());
    let tree = access_tree(a.world());
    assert!(!tree
        .nodes
        .iter()
        .any(|n| n.id == access_id(descendants[0].root)));
    assert!(tree
        .nodes
        .iter()
        .any(|n| n.id == access_id(descendants[1].root)));
    assert_eq!(
        access_details(a.world(), h.tabs[1].root).controls,
        Some(access_id(h.panels[1]))
    );
    assert_eq!(
        access_details(a.world(), h.panels[1]).labelled_by,
        Some(access_id(h.tabs[1].root))
    );
    let copy = ModeSelector::mount(
        a.world_mut(),
        root,
        UiId(1),
        "Renamed",
        &["Source label", "Preview label"],
        TabOrientation::Vertical,
        0,
    )
    .unwrap();
    assert_eq!(h, copy);
    assert_eq!(a.world().get::<Tabs>(h.group).unwrap().selected, 1);
    h.set_available(a.world_mut(), 1, true, true).unwrap();
    assert!(!access_details(a.world(), h.tabs[1].root).activate);
    assert!(access_details(a.world(), h.tabs[1].root).focus);
    h.set_available(a.world_mut(), 1, false, false).unwrap();
    assert!(!access_details(a.world(), h.tabs[1].root).focus);
    let borrowed = a.world_mut().spawn((Layout::column(), Label::new("keep")));
    set_parent(a.world_mut(), borrowed, h.panels[1]);
    h.destroy(a.world_mut()).unwrap();
    assert!(a.world().is_alive(borrowed));
    assert!(a.world().get::<Parent>(borrowed).is_none());
}
#[test]
fn source_is_visible_named_keyboard_reachable_and_host_validates_locator() {
    let mut a = operating::create(800.0, 600.0, UiTheme::default(), false);
    let d = a.world().resource::<operating::Demo>().unwrap().clone();
    assert!(a
        .world()
        .get::<Label>(d.source.label)
        .unwrap()
        .text
        .contains("Show source"));
    request_focus(a.world_mut(), d.source.root);
    key(&mut a, NamedKey::Enter);
    assert_eq!(a.world().resource::<operating::Demo>().unwrap().reveals, 1);
    a.world_mut()
        .resource_mut::<operating::Demo>()
        .unwrap()
        .locator
        .revision += 1;
    key(&mut a, NamedKey::Enter);
    let d = a.world().resource::<operating::Demo>().unwrap();
    assert_eq!((d.reveals, d.rejected), (1, 1));
}
#[test]
fn host_validation_rejects_queued_payload_replacement_and_stale_generation() {
    let (mut a, root) = fixture(320.0);
    let l = SourceLocator {
        document: 1,
        revision: 2,
        anchor: 3,
    };
    let h = SourceAction::mount(a.world_mut(), root, UiId(1), l, true).unwrap();
    a.update();
    try_activate(a.world_mut(), h.root);
    let q = take_action_intents(a.world_mut()).pop().unwrap();
    assert_eq!(
        host_intent(a.world(), &q),
        Some(HostIntent::RevealSource(l))
    );
    SourceAction::mount(
        a.world_mut(),
        root,
        UiId(1),
        SourceLocator { revision: 4, ..l },
        true,
    )
    .unwrap();
    assert_eq!(host_intent(a.world(), &q), None);
    h.destroy(a.world_mut()).unwrap();
    SourceAction::mount(a.world_mut(), root, UiId(1), l, true).unwrap();
    assert_eq!(host_intent(a.world(), &q), None);
}
#[test]
fn actual_gallery_matrix_has_real_glyphs_wrap_targets_and_narrow_reflow() {
    // 1280/4=320 logical-point 400%-equivalent reflow, distinct from 200% type.
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            for density in [Density::Comfortable, Density::Compact] {
                for scale in [1.0, 2.0] {
                    for width in [320.0, 1000.0] {
                        let theme = UiTheme {
                            appearance,
                            contrast,
                            density,
                            text_scale: scale,
                            reading_scale: scale,
                            ..UiTheme::default()
                        };
                        let mut a = operating::create(width, 30000.0, theme.clone(), true);
                        let d = a.world().resource::<operating::Demo>().unwrap().clone();
                        request_focus(a.world_mut(), d.nav[0].root);
                        let f = rndr_paint::frame(&mut a);
                        assert!(f.quads.iter().any(|q| q.uv != rndr_paint::solid_uv()));
                        assert_eq!(
                            a.world().get::<Layout>(d.toolbar.root).unwrap().axis,
                            Axis::Column
                        );
                        for h in d
                            .commands
                            .iter()
                            .chain(d.nav.iter())
                            .chain(d.selector.tabs.iter())
                            .chain([d.source].iter())
                        {
                            let r = *a.world().get::<Rect>(h.root).unwrap();
                            let lr = *a.world().get::<Rect>(h.label).unwrap();
                            let t = a.world().get::<TextLayout>(h.label).unwrap();
                            assert!(
                                r.w >= theme.target_min() - 0.01
                                    && r.h >= theme.target_min() - 0.01
                            );
                            assert!(
                                r.x >= 0.0 && r.x + r.w <= width + 0.02,
                                "horizontal overflow {r:?} at {width}/{scale}"
                            );
                            assert!(contains(r, lr), "label outside target");
                            assert!(t.height <= lr.h + 0.02);
                            assert!(!t.glyphs.is_empty());
                            let expected: String = a
                                .world()
                                .get::<Label>(h.label)
                                .unwrap()
                                .text
                                .chars()
                                .filter(|c| c.is_ascii_graphic())
                                .collect();
                            let actual: String = t
                                .glyphs
                                .iter()
                                .map(|g| g.ch)
                                .filter(|c| c.is_ascii_graphic())
                                .collect();
                            assert_eq!(
                                actual, expected,
                                "complete ASCII glyph coverage, not just World text"
                            );
                            assert!(f.quads.iter().any(|q|q.uv!=rndr_paint::solid_uv()&&contains(lr,q.rect)),"no actual glyph in target");
                        }
                        let nav = d.nav[0];
                        let r = *a.world().get::<Rect>(nav.root).unwrap();
                        assert!(f
                            .quads
                            .iter()
                            .any(|q| q.rect == r && q.color == theme.color(Token::Focus)));
                        assert!(a
                            .world()
                            .get::<Label>(nav.label)
                            .unwrap()
                            .text
                            .starts_with("| "));
                        let n = access_tree(a.world())
                            .nodes
                            .into_iter()
                            .find(|n| n.id == access_id(nav.root))
                            .unwrap();
                        assert!(n.focused && n.selected);
                        assert!(n.name.contains("unsaved"));
                    }
                }
            }
        }
    }
}
#[test]
fn toolbar_wide_row_narrow_stack_and_destroy_never_hides_actions() {
    let (mut a, root) = fixture(1000.0);
    let t = Toolbar::mount(a.world_mut(), root, UiId(1), 1000.0).unwrap();
    let mut controls = vec![];
    for i in 0..3 {
        controls.push(
            controls::Button::mount(
                a.world_mut(),
                t.root,
                UiId(i),
                ButtonProps::new("Go", HostIntent::Command(i)),
            )
            .unwrap(),
        );
    }
    a.update();
    assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis, Axis::Row);
    Toolbar::mount(a.world_mut(), root, UiId(1), 160.0).unwrap();
    a.update();
    assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis, Axis::Column);
    for h in &controls {
        assert!(interaction_visible(a.world(), h.root));
        assert!(access_details(a.world(), h.root).activate);
    }
    t.destroy(a.world_mut()).unwrap();
    for h in controls {
        h.validate(a.world()).unwrap();
        assert!(a.world().get::<Parent>(h.root).is_none());
    }
}
#[test]
fn mode_and_theme_updates_preserve_editor_content_focus_selection_history_and_scroll() {
    let mut a = operating::create(800.0, 600.0, UiTheme::default(), false);
    let d = a.world().resource::<operating::Demo>().unwrap().clone();
    request_focus(a.world_mut(), d.editor);
    a.world_mut().insert_resource(Platform::Windows);
    let before = a.world().get::<Buffer>(d.editor).unwrap().text();
    a.world_mut().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::character('!', Modifiers::new()),
        Platform::Windows,
    );
    a.update();
    a.world_mut().insert_resource(Input::default());
    let source = a.world().get::<Buffer>(d.editor).unwrap().text();
    assert_ne!(source, before);
    {
        let b = a.world_mut().get_mut::<Buffer>(d.editor).unwrap();
        b.anchor = 1;
        b.caret = 3;
    }
    let caret = (3, 1);
    a.world_mut().get_mut::<Scroll>(d.nested).unwrap().offset = [0.0, -20.0];
    let h = ModeSelector::mount(
        a.world_mut(),
        d.root,
        UiId(30),
        "Mode",
        &["Source", "Preview"],
        TabOrientation::Horizontal,
        1,
    )
    .unwrap();
    assert_eq!(h, d.selector);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(d.editor)
    );
    assert_eq!(a.world().get::<Tabs>(h.group).unwrap().selected, 0);
    a.world_mut().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        contrast: Contrast::High,
        text_scale: 2.0,
        ..UiTheme::default()
    });
    a.update();
    assert_eq!(a.world().get::<Buffer>(d.editor).unwrap().text(), source);
    assert_eq!(
        a.world().get::<Scroll>(d.nested).unwrap().offset,
        [0.0, -20.0]
    );
    request_focus(a.world_mut(), h.tabs[1].root);
    key(&mut a, NamedKey::Enter);
    request_focus(a.world_mut(), h.tabs[0].root);
    key(&mut a, NamedKey::Enter);
    assert_eq!(a.world().get::<Buffer>(d.editor).unwrap().text(), source);
    let after = a.world().get::<Buffer>(d.editor).unwrap();
    assert_eq!((after.caret, after.anchor), caret);
    request_focus(a.world_mut(), d.editor);
    let mut mods = Modifiers::new();
    mods.ctrl = true;
    for (ch, expected) in [('z', before), ('y', source)] {
        a.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .push_key(KeyEvent::character(ch, mods), Platform::Windows);
        a.update();
        a.world_mut().insert_resource(Input::default());
        assert_eq!(
            a.world().get::<Buffer>(d.editor).unwrap().text(),
            expected,
            "same history survives mode and theme changes"
        );
    }
}
#[test]
fn nested_scroll_paints_real_clip_and_focus_reveal_without_rebuilding() {
    let mut a = operating::create(320.0, 240.0, UiTheme::default(), true);
    let d = a.world().resource::<operating::Demo>().unwrap().clone();
    request_focus(a.world_mut(), d.source.root);
    let f = rndr_paint::frame(&mut a);
    assert!(a.world().get::<Scroll>(d.root).unwrap().offset[1] < 0.0);
    let r = *a.world().get::<Rect>(d.source.root).unwrap();
    assert!(r.y >= 0.0 && r.y + r.h <= 240.1);
    assert!(f
        .quads
        .iter()
        .any(|q| q.uv != rndr_paint::solid_uv() && q.clip.h <= 240.0));
}
