//! Synthetic, reusable executable/test fixture. No markdown executes host code.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    *,
};

pub const TAIL: &str = "Tokens arrive through core's **streaming parser**. The value and buttons above are retained entities, not images.\n\n```rust\nlet answer = 42;\n```\n\nResize narrow, scroll, then Tab between controls. Enter/Space activate.\n\n- Core still owns parsing\n- Hosts own trusted UI and state\n- Theme changes are explicit\n\n";
#[derive(Clone)]
pub struct Gallery {
    pub root: Entity,
    pub view: Entity,
    pub value_slot: MarkdownSlot,
    pub controls_slot: MarkdownSlot,
    pub value: Entity,
    pub increment: controls::ControlHandle,
    pub theme_button: controls::ControlHandle,
    pub count: u64,
    pub ticks: u64,
    pub cursor: usize,
}
#[derive(Clone)]
pub struct Tick;
impl Event for Tick {}

pub fn create(width: f32, height: f32, theme: UiTheme) -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    apply_theme_bridge(app.world_mut(), theme);
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        UiStyle {
            background: Some(Token::Canvas.into()),
            ..Default::default()
        },
    ));
    let view = markdown_view(
        app.world_mut(),
        "# Markdown host slots\n\nLive value\n\nControls\n\n## Streaming\n\n",
    );
    set_parent(app.world_mut(), view, root);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app); // Bind to last-synced ordinary blocks, not syntax.
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[1].clone();
    let value_slot =
        mount_markdown_slot(app.world_mut(), view, 1, &block, MarkdownSlotId(1)).unwrap();
    let value = text(
        app.world_mut(),
        value_slot.root,
        "Live value: 0 · streamed ticks: 0",
        Typography::Reading,
    )
    .unwrap();
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[2].clone();
    let controls_slot =
        mount_markdown_slot(app.world_mut(), view, 2, &block, MarkdownSlotId(2)).unwrap();
    let increment = controls::Button::mount(
        app.world_mut(),
        controls_slot.root,
        UiId(10),
        ButtonProps::new("Increment live value", HostIntent::Command(1)),
    )
    .unwrap();
    let theme_button = controls::Button::mount(
        app.world_mut(),
        controls_slot.root,
        UiId(11),
        ButtonProps::new("Toggle light / dark", HostIntent::Command(2)),
    )
    .unwrap();
    app.world_mut().insert_resource(Gallery {
        root,
        view,
        value_slot,
        controls_slot,
        value,
        increment,
        theme_button,
        count: 0,
        ticks: 0,
        cursor: 0,
    });
    app.add_system(Schedule::Update, update);
    rndr_paint::frame(&mut app);
    app
}

/// One deterministic streaming step; native entry point drives it with timers.
pub fn tick(world: &mut World) {
    let (view, start, end) = {
        let g = world.resource_mut::<Gallery>().unwrap();
        g.ticks += 1;
        let start = g.cursor;
        let end = TAIL[start..]
            .char_indices()
            .nth(3)
            .map(|(i, _)| start + i)
            .unwrap_or(TAIL.len());
        g.cursor = end;
        (g.view, start, end)
    };
    world
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .push_str(&TAIL[start..end]);
    publish(world);
}
fn publish(world: &mut World) {
    let g = world.resource::<Gallery>().unwrap();
    let (value, count, ticks) = (g.value, g.count, g.ticks);
    world.get_mut::<Label>(value).unwrap().text =
        format!("Live value: {count} · streamed ticks: {ticks}");
}
fn update(world: &mut World) {
    for request in take_action_intents(world) {
        match controls::host_intent(world, &request) {
            Some(HostIntent::Command(1)) => world.resource_mut::<Gallery>().unwrap().count += 1,
            Some(HostIntent::Command(2)) => {
                let mut theme = world.resource::<UiTheme>().unwrap().clone();
                theme.appearance = if theme.appearance == Appearance::Light {
                    Appearance::Dark
                } else {
                    Appearance::Light
                };
                apply_theme_bridge(world, theme);
            }
            _ => {}
        }
    }
    let ticks = events::<Tick>(world).len();
    for _ in 0..ticks {
        tick(world);
    }
    publish(world);
}
