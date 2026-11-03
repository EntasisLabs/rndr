//! Deterministic host slot integration: actual UI controls, no raster proxies.
use rndr_core::*;
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    *,
};

#[test]
fn externally_deleted_roots_restore_fallback_and_copy_never_reads_live_values() {
    let (mut app, view) = setup("Fallback source");
    let slot = mount(&mut app, view, 0, 77);
    let live = child(&mut app, slot);
    app.world_mut().get_mut::<Label>(live).unwrap().text = "PRIVATE LIVE VALUE".into();
    rndr_paint::frame(&mut app);
    let doc = app.world_mut().get_mut::<MarkdownView>(view).unwrap();
    doc.select_all();
    assert_eq!(doc.selected_text(), "Fallback source");
    despawn(app.world_mut(), slot.root);
    assert!(!slot.is_mounted(app.world()));
    assert_eq!(
        slot.detach(app.world_mut()),
        Err(MarkdownSlotError::StaleHandle)
    );
    rndr_paint::frame(&mut app);
    assert!(!app.world().is_alive(live));
    let restored = children_of(app.world(), view)[0];
    assert_eq!(
        app.world().get::<Label>(restored).unwrap().text,
        "Fallback source"
    );
}
fn setup(source: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 300.));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = markdown_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    (app, view)
}
fn mount(app: &mut App, view: Entity, index: usize, id: u64) -> MarkdownSlot {
    let expected = app.world().get::<MarkdownView>(view).unwrap().blocks()[index].clone();
    mount_markdown_slot(
        app.world_mut(),
        view,
        index,
        &expected,
        MarkdownSlotId(id),
        60.,
    )
    .unwrap()
}
fn child(app: &mut App, slot: MarkdownSlot) -> Entity {
    let e = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            ..Layout::column()
        },
        Label::new("Live 1"),
    ));
    set_parent(app.world_mut(), e, slot.root);
    e
}
#[test]
fn streamed_append_and_full_reparse_preserve_root_state_and_order() {
    let (mut app, view) = setup("Header\n\n\nValue\n\n\nTail ");
    let slot = mount(&mut app, view, 1, 5);
    let live = child(&mut app, slot);
    rndr_paint::frame(&mut app);
    let source = "Header\n\n\nValue\n\n\nTail **bold**\n\n```rust\nlet x = 1;\n```\n";
    let existing = app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .source()
        .len();
    for ch in source[existing..].chars() {
        app.world_mut()
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .push_str(&ch.to_string());
        rndr_paint::frame(&mut app);
        assert!(slot.is_mounted(app.world()));
        assert_eq!(children_of(app.world(), view)[1], slot.root);
        assert_eq!(children_of(app.world(), slot.root), vec![live]);
        assert_eq!(
            app.world().get::<MarkdownView>(view).unwrap().blocks(),
            parse_markdown_with_theme(app.world(), view).as_slice()
        );
    }
    app.world_mut().get_mut::<Label>(live).unwrap().text = "Live 42".into();
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_markdown(source);
    rndr_paint::frame(&mut app);
    assert!(slot.is_mounted(app.world()));
    assert_eq!(app.world().get::<Label>(live).unwrap().text, "Live 42");
}
// Use a second ordinary view so theme-dependent spans are compared exactly.
fn parse_markdown_with_theme(world: &World, view: Entity) -> Vec<MarkdownBlock> {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut()
        .insert_resource(*world.resource::<MarkdownTheme>().unwrap());
    let other = markdown_view(
        app.world_mut(),
        world.get::<MarkdownView>(view).unwrap().source(),
    );
    app.update();
    app.world()
        .get::<MarkdownView>(other)
        .unwrap()
        .blocks()
        .to_vec()
}
#[test]
fn prior_shape_replacement_keeps_later_host_and_reading_order() {
    let (mut app, view) = setup("Plain\n\nValue\n\nEnd");
    let slot = mount(&mut app, view, 1, 1);
    let live = child(&mut app, slot);
    rndr_paint::frame(&mut app);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_markdown("---\n\nValue\n\nEnd");
    rndr_paint::frame(&mut app);
    assert!(slot.is_mounted(app.world()));
    assert!(app.world().is_alive(live));
    assert_eq!(children_of(app.world(), view)[1], slot.root);
    let rows = children_of(app.world(), view);
    assert!(
        app.world().get::<Rect>(rows[0]).unwrap().y < app.world().get::<Rect>(rows[1]).unwrap().y
    );
    assert!(
        app.world().get::<Rect>(rows[1]).unwrap().y < app.world().get::<Rect>(rows[2]).unwrap().y
    );
}
#[test]
fn source_replacement_and_removal_cascade_and_reject_stale_handles() {
    for source in ["Changed", "", "# Value", "**Value**", "Inserted\n\nValue"] {
        let (mut app, view) = setup("Value");
        let slot = mount(&mut app, view, 0, 2);
        let live = child(&mut app, slot);
        rndr_paint::frame(&mut app);
        app.world_mut()
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .set_markdown(source);
        assert!(!slot.is_mounted(app.world()));
        rndr_paint::frame(&mut app);
        assert!(!slot.is_mounted(app.world()), "{source}");
        assert!(!app.world().is_alive(slot.root));
        assert!(!app.world().is_alive(live));
        assert_eq!(
            slot.set_height(app.world_mut(), 20.),
            Err(MarkdownSlotError::StaleHandle)
        );
        assert_eq!(
            slot.detach(app.world_mut()),
            Err(MarkdownSlotError::StaleHandle)
        );
    }
}
#[test]
fn detach_restores_markdown_and_old_identity_cannot_be_reused() {
    let (mut app, view) = setup("Value");
    let slot = mount(&mut app, view, 0, 3);
    let live = child(&mut app, slot);
    rndr_paint::frame(&mut app);
    slot.detach(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(live));
    rndr_paint::frame(&mut app);
    let root = children_of(app.world(), view)[0];
    assert_eq!(app.world().get::<Label>(root).unwrap().text, "Value");
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    assert_eq!(
        mount_markdown_slot(app.world_mut(), view, 0, &block, slot.id, 10.),
        Err(MarkdownSlotError::Duplicate)
    );
    let fresh = mount(&mut app, view, 0, 4);
    assert_ne!(fresh.root, slot.root);
    assert_eq!(
        slot.detach(app.world_mut()),
        Err(MarkdownSlotError::StaleHandle)
    );
    assert!(fresh.is_mounted(app.world()));
    despawn(app.world_mut(), view);
    assert!(!app.world().is_alive(fresh.root));
    assert!(!fresh.is_mounted(app.world()));
}
#[test]
fn duplicate_pending_source_stale_expected_and_invalid_height_are_atomic() {
    let (mut app, view) = setup("Same\n\nSame");
    let slot = mount(&mut app, view, 0, 1);
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    let count = app.world().entities_with::<Layout>().len();
    for (index, id, height, error) in [
        (1, 1, 10., MarkdownSlotError::Duplicate),
        (0, 2, 10., MarkdownSlotError::Duplicate),
        (9, 2, 10., MarkdownSlotError::StaleBlock),
        (1, 2, f32::NAN, MarkdownSlotError::InvalidHeight),
        (1, 2, -1., MarkdownSlotError::InvalidHeight),
    ] {
        assert_eq!(
            mount_markdown_slot(
                app.world_mut(),
                view,
                index,
                &block,
                MarkdownSlotId(id),
                height
            ),
            Err(error)
        );
        assert_eq!(app.world().entities_with::<Layout>().len(), count);
    }
    let other = mount(&mut app, view, 1, 2);
    assert_ne!(slot.root, other.root);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .push_str(" changed");
    assert_eq!(
        mount_markdown_slot(app.world_mut(), view, 1, &block, MarkdownSlotId(3), 20.),
        Err(MarkdownSlotError::StaleBlock)
    );
    rndr_paint::frame(&mut app);
    assert!(slot.is_mounted(app.world()));
    assert!(!other.is_mounted(app.world()));
    assert_eq!(
        mount_markdown_slot(app.world_mut(), view, 1, &block, MarkdownSlotId(3), 20.),
        Err(MarkdownSlotError::StaleBlock)
    );
}
#[test]
fn real_button_keyboard_focus_activation_pointer_and_cleanup() {
    let (mut app, view) = setup("Value\n\nAfter");
    let slot = mount(&mut app, view, 0, 9);
    let button = controls::Button::mount(
        app.world_mut(),
        slot.root,
        UiId(42),
        ButtonProps::new("Increment", HostIntent::Command(7)),
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    request_focus(app.world_mut(), button.root);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    rndr_paint::frame(&mut app);
    let requests = take_action_intents(app.world_mut());
    assert_eq!(requests.len(), 1);
    assert_eq!(
        controls::host_intent(app.world(), &requests[0]),
        Some(HostIntent::Command(7))
    );
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(button.root)
    );
    let rect = *app.world().get::<Rect>(button.root).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + 4., rect.y + 4.];
    input.pointer.pressed = true;
    input.pointer.down = true;
    rndr_paint::frame(&mut app);
    assert!(app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .selected_text()
        .is_empty());
    assert_eq!(app.world().get::<Scroll>(view).unwrap().offset, [0., 0.]);
    slot.detach(app.world_mut()).unwrap();
    rndr_paint::frame(&mut app);
    assert_ne!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(button.root)
    );
    assert_eq!(
        try_activate(app.world_mut(), button.root),
        ActivationResult::Rejected
    );
    assert!(take_action_intents(app.world_mut()).is_empty());
}
#[test]
fn host_height_resize_and_scroll_clip_are_live_not_snapshots() {
    let (mut app, view) = setup("Value\n\nAfter");
    let slot = mount(&mut app, view, 0, 1);
    let live = child(&mut app, slot);
    rndr_paint::frame(&mut app);
    let wide = *app.world().get::<Rect>(live).unwrap();
    slot.set_height(app.world_mut(), 500.).unwrap();
    app.world_mut().insert_resource(Viewport::new(200., 150.));
    rndr_paint::frame(&mut app);
    let narrow = *app.world().get::<Rect>(live).unwrap();
    assert!(narrow.w < wide.w);
    assert_eq!(app.world().get::<Rect>(slot.root).unwrap().w, 168.);
    let clip = app.world().get::<Clip>(slot.root).unwrap();
    assert!(clip.h <= 150. && clip.w <= 200.);
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = -400.;
    rndr_paint::frame(&mut app);
    assert!(app.world().get::<Clip>(live).unwrap().h <= 0.);
    assert!(!hit_test(app.world(), narrow.x + 2., narrow.y + 2.).is_some_and(|hit| hit == live));
}
#[test]
fn bridge_is_explicit_complete_and_updates_retained_markdown_and_control() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    assert!(!app.world().has_resource::<Theme>());
    assert!(!app.world().has_resource::<MarkdownTheme>());
    let (mut app, view) = setup("Plain paragraph\n\nValue");
    let plain = children_of(app.world(), view)[0];
    let slot = mount(&mut app, view, 1, 1);
    let button = controls::Button::mount(
        app.world_mut(),
        slot.root,
        UiId(1),
        ButtonProps::new("Theme", HostIntent::Command(1)),
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    let before = app.world().get::<Label>(plain).unwrap().color;
    let old_button = app.world().get::<Label>(button.label).unwrap().color;
    for appearance in [Appearance::Dark, Appearance::Light] {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = UiTheme {
                appearance,
                contrast,
                reading_scale: 1.5,
                ..Default::default()
            };
            let core = theme.core_theme();
            let actual = [
                core.canvas,
                core.tree,
                core.seam,
                core.pane,
                core.title,
                core.body,
                core.status,
                core.child,
                core.empty,
                core.text,
                core.caret,
                core.dirty,
            ];
            let tokens = [
                Token::Canvas,
                Token::Navigation,
                Token::SubtleStroke,
                Token::Content,
                Token::Raised,
                Token::Raised,
                Token::ControlHover,
                Token::ControlHover,
                Token::Content,
                Token::Text,
                Token::Focus,
                Token::WarningText,
            ];
            assert_eq!(actual.to_vec(), tokens.map(|t| theme.color(t)).to_vec());
            let focus = theme.color(Token::Focus);
            assert_eq!(
                core.selection,
                if contrast == Contrast::High {
                    Color::rgba(focus.r, focus.g, focus.b, 0.35)
                } else {
                    theme.color(Token::Selected)
                }
            );
            assert_ne!(core.selection, core.pane);
            apply_theme_bridge(app.world_mut(), theme.clone());
            rndr_paint::frame(&mut app);
            assert!(slot.is_mounted(app.world()));
            assert_eq!(children_of(app.world(), view)[0], plain);
            assert_eq!(
                app.world().get::<Label>(plain).unwrap().color,
                Some(theme.color(Token::Text))
            );
            assert_eq!(app.world().get::<TextStyle>(plain).unwrap().size, 24.);
            assert_eq!(
                *app.world().resource::<MarkdownTheme>().unwrap(),
                theme.markdown_theme()
            );
            assert_eq!(theme_of(app.world()).text, theme.color(Token::Text));
            assert_eq!(app.world().resource::<UiTheme>().unwrap(), &theme);
        }
    }
    let dark = UiTheme {
        appearance: Appearance::Dark,
        ..Default::default()
    };
    apply_theme_bridge(app.world_mut(), dark);
    rndr_paint::frame(&mut app);
    assert_ne!(app.world().get::<Label>(plain).unwrap().color, before);
    assert_ne!(
        app.world().get::<Label>(button.label).unwrap().color,
        old_button
    );
}
