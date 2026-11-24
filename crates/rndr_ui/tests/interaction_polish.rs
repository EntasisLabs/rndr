//! Hover, press, focus, truncation, sash, and radius polish.
use std::time::Duration;

use rndr_core::*;
use rndr_ui::controls::{Button, ButtonProps, HostIntent};
use rndr_ui::split::{SplitSash, SplitSashProps};
use rndr_ui::*;

fn host(width: f32, height: f32) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(width),
        height: Size::Px(height),
        ..Layout::column()
    },));
    (app, root)
}

fn face_of(world: &World, label: Entity) -> Entity {
    world.get::<Parent>(label).unwrap().entity
}

#[test]
fn narrow_labels_ellipsis_and_grow_a_tooltip() {
    let (mut app, root) = host(160.0, 80.0);
    let button = Button::mount(
        app.world_mut(),
        root,
        UiId(1),
        ButtonProps::new(
            "Truncated label that does not fit this control",
            HostIntent::Command(1),
        ),
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(button.root)
        .unwrap()
        .width = Size::Px(72.0);
    app.update();
    app.update();
    let label = app.world().get::<Label>(button.label).unwrap();
    assert_eq!(label.overflow, TextOverflow::Ellipsis);
    let layout = app.world().get::<TextLayout>(button.label).unwrap();
    let rect = app.world().get::<Rect>(button.label).unwrap();
    assert_eq!(layout.lines.len(), 1);
    assert!(layout.width <= rect.w + 0.5);
    let engine = text_engine(app.world());
    let style = style_for(app.world(), button.label);
    let natural = engine
        .measure(&label.text, &style, &[], f32::INFINITY)
        .width;
    assert!(natural > layout.width + 4.0);
    assert!(
        app.world()
            .get::<DescribedBy>(button.root)
            .is_some_and(|described| !described.0.is_empty()),
        "a truncated label explains itself"
    );

    let (mut wide, parent) = host(640.0, 80.0);
    let short = Button::mount(
        wide.world_mut(),
        parent,
        UiId(2),
        ButtonProps::new("Fit", HostIntent::Command(2)),
    )
    .unwrap();
    wide.update();
    wide.update();
    assert!(wide.world().get::<DescribedBy>(short.root).is_none());
}

#[test]
fn hover_eases_press_differs_and_reduced_motion_snaps() {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(InteractionMotion::default());
    sim.world().insert_resource(Viewport::new(240.0, 80.0));
    let root = sim.world().spawn((Layout {
        width: Size::Px(240.0),
        height: Size::Px(80.0),
        ..Layout::column()
    },));
    let button = Button::mount(
        sim.world(),
        root,
        UiId(3),
        ButtonProps::new("Hover", HostIntent::Command(3)),
    )
    .unwrap();
    sim.update();
    let face = face_of(sim.world(), button.label);
    let idle = *sim.world().get::<Color>(face).unwrap();
    let theme = sim.world().resource::<UiTheme>().cloned().unwrap();
    let hover = theme.color(Token::ControlHover);
    let pressed = theme.color(Token::ControlPressed);
    assert_ne!(hover, pressed);
    assert_ne!(idle, hover);

    let rect = *sim.world().get::<Rect>(button.root).unwrap();
    let at = [rect.x + 12.0, rect.y + 12.0];
    sim.advance(Duration::from_millis(40));
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer.position = at;
    }
    sim.update();
    let mid = *sim.world().get::<Color>(face).unwrap();
    assert_ne!(mid, idle);
    assert_ne!(mid, hover);

    sim.advance(Duration::from_millis(200));
    sim.update();
    assert_eq!(*sim.world().get::<Color>(face).unwrap(), hover);

    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.press_at = Some(at);
    }
    sim.update();
    assert_eq!(*sim.world().get::<Color>(face).unwrap(), pressed);

    let mut reduced = Sim::new();
    reduced.app.add_plugin(UiPlugin);
    reduced
        .world()
        .insert_resource(InteractionMotion::default());
    reduced.world().insert_resource(Viewport::new(240.0, 80.0));
    reduced.world().insert_resource(SystemAppearance {
        reduced_motion: true,
        ..SystemAppearance::default()
    });
    let parent = reduced.world().spawn((Layout {
        width: Size::Px(240.0),
        height: Size::Px(80.0),
        ..Layout::column()
    },));
    let quiet = Button::mount(
        reduced.world(),
        parent,
        UiId(4),
        ButtonProps::new("Still", HostIntent::Command(4)),
    )
    .unwrap();
    reduced.update();
    let bounds = *reduced.world().get::<Rect>(quiet.root).unwrap();
    reduced.advance(Duration::from_millis(40));
    {
        let input = reduced.world().resource_mut::<Input>().unwrap();
        input.pointer.position = [bounds.x + 12.0, bounds.y + 12.0];
    }
    reduced.update();
    let quiet_face = face_of(reduced.world(), quiet.label);
    assert_eq!(*reduced.world().get::<Color>(quiet_face).unwrap(), hover);
}

#[test]
fn sash_line_is_thinner_than_the_hit_target_and_hover_uses_the_resize_cursor() {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(400.0, 160.0));
    let root = sim.world().spawn((Layout {
        width: Size::Px(400.0),
        height: Size::Px(160.0),
        ..Layout::column()
    },));
    let split = SplitSash::mount(
        sim.world(),
        root,
        UiId(5),
        SplitSashProps::new("Panes"),
        0.5,
    )
    .unwrap();
    sim.update();
    let sash = *sim.world().get::<Rect>(split.sash).unwrap();
    let line = children_of(sim.world(), split.sash)
        .into_iter()
        .find(|child| {
            sim.world()
                .get::<Rect>(*child)
                .is_some_and(|rect| rect.w < sash.w)
        })
        .expect("highlight line");
    let line_rect = *sim.world().get::<Rect>(line).unwrap();
    assert!(
        line_rect.w + 1.0 < sash.w,
        "line {line_rect:?} sash {sash:?}"
    );
    assert!((line_rect.x - (sash.x + (sash.w - line_rect.w) * 0.5)).abs() < 1.0);

    let edge = [sash.x + 1.0, sash.y + sash.h * 0.5];
    assert!(edge[0] < line_rect.x);
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer.position = edge;
    }
    sim.update();
    assert_eq!(
        sim.world().resource::<CursorStyle>().copied(),
        Some(CursorStyle::ResizeColumn)
    );
    let theme = sim.world().resource::<UiTheme>().cloned().unwrap();
    assert_eq!(
        *sim.world().get::<Color>(line).unwrap(),
        theme.color(Token::Text)
    );
}

#[test]
fn focus_ring_and_radii_follow_the_theme_and_clip_children() {
    let (mut app, root) = host(320.0, 200.0);
    let theme = app.world().resource::<UiTheme>().cloned().unwrap();
    assert_eq!(theme.radius(RadiusRole::Overlay), Corner::Overlay.points());
    assert_eq!(theme.radius(RadiusRole::Overlay), 10.0);
    assert_eq!(theme.radius(RadiusRole::Control), Corner::Control.points());
    assert_eq!(theme.radius(RadiusRole::Card), Corner::Group.points());
    assert_eq!(theme.radius(RadiusRole::Chip), Corner::Compact.points());
    assert_eq!(theme.interaction_duration(), Duration::from_millis(120));

    let card = Card::mount(app.world_mut(), root, UiId(6), "Card").unwrap();
    app.update();
    assert_eq!(
        app.world().get::<Radius>(card.root).map(|radius| radius.0),
        Some(theme.radius(RadiusRole::Card))
    );
    assert!(app.world().get::<ClipRounded>(card.root).is_some());

    app.world_mut().insert_resource(RadiusScale {
        card: 14.0,
        ..RadiusScale::default()
    });
    app.update();
    assert_eq!(app.world().get::<Radius>(card.root).unwrap().0, 14.0);

    let button = Button::mount(
        app.world_mut(),
        root,
        UiId(7),
        ButtonProps::new("Focus", HostIntent::Command(7)),
    )
    .unwrap();
    assert!(request_focus(app.world_mut(), button.root));
    app.update();
    let ring = *app.world().get::<Color>(button.root).unwrap();
    assert_eq!(ring, theme.color(Token::Focus));
    assert!(app.world().get::<ClipRounded>(button.root).is_some());
}
