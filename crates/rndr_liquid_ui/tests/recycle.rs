//! Offscreen Liquid blocks are released, then restored with their state.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};

fn boot(source: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    (app, view)
}

fn prose(n: usize) -> String {
    (0..n)
        .map(|i| format!("Filler paragraph {i} alpha beta gamma delta.\n\n"))
        .collect()
}

fn layout_entities(app: &App) -> usize {
    app.world().entities_with::<Layout>().len()
}

fn has(app: &App, view: Entity, text: &str) -> bool {
    let mut all = Vec::new();
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        for child in children_of(world, entity) {
            out.push(child);
            walk(world, child, out);
        }
    }
    walk(app.world(), view, &mut all);
    all.iter().any(|entity| {
        app.world()
            .get::<Label>(*entity)
            .is_some_and(|l| l.text == text)
    })
}

fn button(world: &World, root: Entity, label: &str) -> Entity {
    let mut all = Vec::new();
    fn walk(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        for child in children_of(world, entity) {
            out.push(child);
            walk(world, child, out);
        }
    }
    walk(world, root, &mut all);
    all.into_iter()
        .find(|entity| world.get::<Label>(*entity).is_some_and(|l| l.text == label))
        .unwrap_or_else(|| panic!("no {label}"))
}

fn press(app: &mut App, root: Entity, label: &str) {
    let mut at = button(app.world(), root, label);
    while try_activate(app.world_mut(), at) != ActivationResult::Accepted {
        at = app.world().get::<Parent>(at).unwrap().entity;
    }
    for request in take_action_intents(app.world_mut()) {
        dispatch(app.world_mut(), &request);
    }
    rndr_paint::frame(app);
}

fn scroll_to(app: &mut App, view: Entity, offset: f32) {
    app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = offset;
    rndr_paint::frame(app);
}

#[test]
fn mounted_entities_stay_bounded_as_blocks_grow() {
    let small = {
        let source: String = (0..20)
            .map(|i| format!("```callout\ntitle: Tip {i}\nbody: Body {i}.\n```\n\n"))
            .collect();
        let (app, view) = boot(&source);
        (
            layout_entities(&app),
            app.world().get::<LiquidView>(view).unwrap().mounted().len(),
        )
    };
    let large = {
        let source: String = (0..400)
            .map(|i| format!("```callout\ntitle: Tip {i}\nbody: Body {i}.\n```\n\n"))
            .collect();
        let (app, view) = boot(&source);
        (
            layout_entities(&app),
            app.world().get::<LiquidView>(view).unwrap().mounted().len(),
        )
    };
    assert!(large.1 < 40, "mounted {} of 400 blocks", large.1);
    assert!(
        large.0 < small.0.saturating_mul(3).max(400),
        "layout entities grew from {} at 20 blocks to {} at 400",
        small.0,
        large.0
    );
}

#[test]
fn released_block_keeps_its_height_and_restores_reader_state() {
    let source = format!(
        "```accordion\ntitle: FAQ\n\n---\nlabel: One\nbody: First body\n---\nlabel: Two\nbody: Second body\n```\n\n\
```tabs\n---\nlabel: Alpha\nbody: Alpha body.\n---\nlabel: Beta\nbody: Beta body.\n```\n\n{}",
        prose(80)
    );
    let (mut app, view) = boot(&source);
    assert!(has(&app, view, "+ One"));
    press(&mut app, view, "+ One");
    assert!(has(&app, view, "First body"));
    press(&mut app, view, "Beta");
    let selected = tab_selected(app.world(), view);
    assert_eq!(selected, Some(1), "tab selection {selected:?}");
    let content = app.world().get::<Scroll>(view).unwrap().content[1];
    let bottom = {
        let scroll = app.world().get::<Scroll>(view).unwrap();
        let rect = app.world().get::<Rect>(view).unwrap();
        (rect.h - scroll.content[1]).min(0.0)
    };
    scroll_to(&mut app, view, bottom);
    assert!(
        !has(&app, view, "First body"),
        "the accordion left the window"
    );
    let parked = app.world().get::<Scroll>(view).unwrap().content[1];
    assert!(
        (parked - content).abs() < 0.5,
        "extent changed from {content} to {parked}"
    );
    scroll_to(&mut app, view, 0.0);
    rndr_paint::frame(&mut app);
    assert!(has(&app, view, "− One"), "accordion stayed open");
    assert!(has(&app, view, "First body"), "accordion body came back");
    assert_eq!(tab_selected(app.world(), view), Some(1));
    let back = app.world().get::<Scroll>(view).unwrap().content[1];
    assert!((back - content).abs() < 0.5, "extent changed on return");
}

fn tab_selected(world: &World, root: Entity) -> Option<u64> {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(tabs) = world.get::<Tabs>(entity) {
            return Some(tabs.selected);
        }
        stack.extend(children_of(world, entity));
    }
    None
}

#[test]
fn streaming_appends_keep_state_through_a_release() {
    let (mut app, view) = boot("Intro.\n\n");
    push_liquid_str(
        app.world_mut(),
        view,
        "```accordion\ntitle: FAQ\n\n---\nlabel: One\nbody: First body\n---\nlabel: Two\nbody: Second body\n```\n\n",
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    press(&mut app, view, "+ One");
    assert!(has(&app, view, "First body"));
    push_liquid_str(app.world_mut(), view, &prose(80));
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let bottom = {
        let scroll = app.world().get::<Scroll>(view).unwrap();
        let rect = app.world().get::<Rect>(view).unwrap();
        (rect.h - scroll.content[1]).min(0.0)
    };
    scroll_to(&mut app, view, bottom);
    assert!(
        !has(&app, view, "First body"),
        "the accordion left the window"
    );
    push_liquid_str(
        app.world_mut(),
        view,
        "```callout\ntitle: Later\nbody: Appended while away.\n```\n\n",
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    scroll_to(&mut app, view, 0.0);
    rndr_paint::frame(&mut app);
    assert!(
        has(&app, view, "− One"),
        "accordion stayed open across the append"
    );
    assert!(has(&app, view, "First body"), "accordion body came back");
    push_liquid_str(app.world_mut(), view, &prose(40));
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(
        has(&app, view, "First body"),
        "a later append kept the open accordion"
    );
    let mounted = app.world().get::<LiquidView>(view).unwrap().mounted().len();
    let blocks = app
        .world()
        .get::<MarkdownView>(view)
        .unwrap()
        .blocks()
        .len();
    assert!(blocks > 80, "document stayed short: {blocks} blocks");
    assert!(mounted < 20, "mounted {mounted} blocks after scrolling");
    assert!(
        layout_entities(&app) < 400,
        "layout entities grew to {}",
        layout_entities(&app)
    );
}

#[test]
fn a_markdown_host_without_the_policy_stays_mounted_offscreen() {
    let mut source = String::new();
    for index in 0..80 {
        if index > 0 {
            source.push_str("\n\n");
        }
        source.push_str(&format!("Plain block {index} alpha."));
    }
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 180.0));
    let view = markdown_view(app.world_mut(), &source);
    app.update();
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    let slot = mount_markdown_slot(app.world_mut(), view, 0, &block, MarkdownSlotId(3)).unwrap();
    let root = slot.root;
    app.update();
    assert!(
        !app.world()
            .get::<MarkdownView>(view)
            .unwrap()
            .recycle_host_slots(),
        "recycling stays off unless a view asks for it"
    );
    assert!(slot.is_mounted(app.world()));
    assert!(app.world().is_alive(root));
    assert!(!children_of(app.world(), view).contains(&root));
}
