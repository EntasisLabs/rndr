use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};

const A: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: First\n```\n";

fn pump(app: &mut App, frames: usize) {
    for _ in 0..frames {
        rndr_paint::frame(app);
    }
}

fn controls(world: &World, view: Entity, text: &str) -> Vec<Entity> {
    world
        .entities_with::<Label>()
        .into_iter()
        .filter(|e| {
            if world.get::<Label>(*e).unwrap().text != text {
                return false;
            }
            let mut node = *e;
            while let Some(parent) = world.get::<Parent>(node) {
                if parent.entity == view {
                    return true;
                }
                if world.has::<LiquidView>(parent.entity) {
                    return false;
                }
                node = parent.entity;
            }
            false
        })
        .collect()
}

fn reset_case(surface_reset: bool, keyed: bool, released: bool) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let source = format!(
        "{A}\n{A}\n{}",
        (0..80)
            .map(|i| format!("Paragraph {i}.\n\n"))
            .collect::<String>()
    );
    let view = liquid_view(app.world_mut(), &source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    let surface = liquid_surface_handle(app.world(), view).unwrap();
    if keyed {
        set_liquid_source_keyed(
            app.world_mut(),
            surface,
            LiquidSourceRevision::ZERO,
            &source,
            &LiquidKeyMap {
                keys: vec![Some("one".into()), Some("two".into())],
            },
        )
        .unwrap();
    }
    let handles: Vec<_> = (0..2)
        .map(|i| liquid_component_handle(app.world(), surface, i).unwrap())
        .collect();
    for h in &handles {
        put_liquid_component_state_for(
            app.world_mut(),
            *h,
            0,
            LiquidComponentState::Accordion { open: vec![0] },
        )
        .unwrap();
    }
    pump(&mut app, 2);
    assert_eq!(
        app.world().get::<LiquidView>(view).unwrap().mounted(),
        vec![0, 1]
    );
    assert_eq!(controls(app.world(), view, "− One").len(), 2);
    let mut root = controls(app.world(), view, "− One")[0];
    while app.world().get::<Parent>(root).unwrap().entity != view {
        root = app.world().get::<Parent>(root).unwrap().entity;
    }
    let independent = liquid_view(app.world_mut(), A);
    let independent_surface = liquid_surface_handle(app.world(), independent).unwrap();
    let independent_handle = liquid_component_handle(app.world(), independent_surface, 0).unwrap();
    let independent_record = put_liquid_component_state_for(
        app.world_mut(),
        independent_handle,
        0,
        LiquidComponentState::Accordion { open: vec![0] },
    )
    .unwrap();
    set_parent(app.world_mut(), independent, root);
    let attachment = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), attachment, root);
    if released {
        let bottom = (app.world().get::<Rect>(view).unwrap().h
            - app.world().get::<Scroll>(view).unwrap().content[1])
            .min(0.0);
        app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = bottom;
        pump(&mut app, 2);
        assert!(app
            .world()
            .get::<LiquidView>(view)
            .unwrap()
            .mounted()
            .is_empty());
    }
    let revision = liquid_source_revision(app.world(), surface).unwrap();
    let before = liquid_component_states(app.world(), view);
    let records: Vec<_> = handles
        .iter()
        .map(|h| {
            liquid_component_state_for(app.world(), *h)
                .unwrap()
                .unwrap()
        })
        .collect();
    let mounted = app.world().get::<LiquidView>(view).unwrap().mounted();
    let markdown = app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .source()
        .to_owned();
    // Failed planning must not consume a mounted or saved native binding.
    assert!(matches!(
        reset_liquid_block(app.world_mut(), handles[0], records[0].revision + 1),
        Err(LiquidF2Error::Revision { .. })
    ));
    if keyed {
        assert!(matches!(
            reset_liquid_surface(app.world_mut(), surface, LiquidSourceRevision::ZERO),
            Err(LiquidF2Error::Revision { .. })
        ));
    }
    assert_eq!(liquid_component_states(app.world(), view), before);
    assert_eq!(
        liquid_source_revision(app.world(), surface).unwrap(),
        revision
    );
    assert_eq!(
        app.world().get::<LiquidView>(view).unwrap().mounted(),
        mounted
    );
    assert_eq!(
        app.world().get::<MarkdownView>(view).unwrap().source(),
        markdown
    );
    let current_surface = if surface_reset {
        let fresh = reset_liquid_surface(app.world_mut(), surface, revision).unwrap();
        assert_ne!(fresh, surface);
        assert!(liquid_source_revision(app.world(), surface).is_err());
        assert_eq!(
            liquid_source_revision(app.world(), fresh).unwrap(),
            LiquidSourceRevision::ZERO
        );
        fresh
    } else {
        let fresh = reset_liquid_block(app.world_mut(), handles[0], records[0].revision).unwrap();
        assert_ne!(fresh, handles[0]);
        assert_ne!(
            liquid_source_revision(app.world(), surface).unwrap(),
            revision
        );
        assert_eq!(
            liquid_component_handle(app.world(), surface, 1).unwrap(),
            handles[1]
        );
        assert_eq!(
            liquid_component_state_for(app.world(), handles[1]).unwrap(),
            Some(records[1].clone())
        );
        surface
    };
    assert!(liquid_component_state_for(app.world(), handles[0]).is_err());
    assert!(put_liquid_component_state_for(
        app.world_mut(),
        handles[0],
        0,
        LiquidComponentState::Accordion { open: vec![0] }
    )
    .is_err());
    let fresh = liquid_component_handle(app.world(), current_surface, 0).unwrap();
    assert_ne!(fresh, handles[0]);
    assert_eq!(
        liquid_component_state_for(app.world(), fresh).unwrap(),
        None
    );
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
    pump(&mut app, 12);
    assert_eq!(
        app.world().get::<LiquidView>(view).unwrap().mounted(),
        vec![0, 1]
    );
    assert_eq!(
        controls(app.world(), view, "+ One").len(),
        if surface_reset { 2 } else { 1 }
    );
    assert_eq!(
        controls(app.world(), view, "− One").len(),
        if surface_reset { 0 } else { 1 }
    );
    assert!(app.world().is_alive(attachment));
    assert!(app.world().has::<LiquidView>(independent));
    assert_eq!(
        liquid_surface_handle(app.world(), independent).unwrap(),
        independent_surface
    );
    assert_eq!(
        liquid_component_state_for(app.world(), independent_handle).unwrap(),
        Some(independent_record)
    );
    if !surface_reset {
        assert_eq!(
            liquid_component_state_for(app.world(), handles[1]).unwrap(),
            Some(records[1].clone())
        );
    }
    // Fresh authority really writes the new rendered control, never the retired one.
    let expected = liquid_component_state_for(app.world(), fresh)
        .unwrap()
        .map_or(0, |r| r.revision);
    put_liquid_component_state_for(
        app.world_mut(),
        fresh,
        expected,
        LiquidComponentState::Accordion { open: vec![0] },
    )
    .unwrap();
    pump(&mut app, 2);
    assert_eq!(
        controls(app.world(), view, "+ One").len(),
        usize::from(surface_reset)
    );
}

#[test]
fn offscreen_block_reset_unkeyed() {
    reset_case(false, false, true);
}
#[test]
fn offscreen_block_reset_keyed() {
    reset_case(false, true, true);
}
#[test]
fn offscreen_surface_reset_unkeyed() {
    reset_case(true, false, true);
}
#[test]
fn offscreen_surface_reset_keyed() {
    reset_case(true, true, true);
}
#[test]
fn mounted_block_reset_unkeyed() {
    reset_case(false, false, false);
}
#[test]
fn mounted_block_reset_keyed() {
    reset_case(false, true, false);
}
#[test]
fn mounted_surface_reset_unkeyed() {
    reset_case(true, false, false);
}
#[test]
fn mounted_surface_reset_keyed() {
    reset_case(true, true, false);
}
