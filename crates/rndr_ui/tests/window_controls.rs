use rndr_core::*;
use rndr_ui::{window_controls, UiPlugin};

fn boot(platform: Platform) -> (App, Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(platform);
    app.world_mut().insert_resource(WindowConfig {
        titlebar: Some(Titlebar::new(40.0)),
        ..WindowConfig::default()
    });
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400.0, 300.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let band = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.0),
            ..Layout::row()
        },
        Align {
            justify: Justify::End,
            items: Items::Stretch,
        },
    ));
    app.world_mut().insert(band, WindowDrag);
    set_parent(app.world_mut(), band, root);
    let row = window_controls(app.world_mut(), band);
    app.update();
    (app, row)
}

fn buttons(app: &App) -> Vec<(WindowButton, Rect)> {
    let mut out: Vec<_> = app
        .world()
        .entities_with::<WindowButton>()
        .into_iter()
        .map(|e| {
            (
                *app.world().get::<WindowButton>(e).unwrap(),
                *app.world().get::<Rect>(e).unwrap(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    out
}

#[test]
fn controls_sit_at_the_trailing_edge_and_hover_turns_close_red() {
    let (mut app, row) = boot(Platform::Windows);
    assert!(!app.world().has::<Hidden>(row));
    let placed = buttons(&app);
    assert_eq!(
        placed.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
        [
            WindowButton::Minimize,
            WindowButton::Maximize,
            WindowButton::Close
        ]
    );
    let close = placed[2].1;
    assert_eq!((close.x + close.w, close.h), (400.0, 40.0));

    let at = [close.x + close.w / 2.0, close.y + close.h / 2.0];
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = at;
    app.update();
    let lit = app
        .world()
        .resource::<WindowChrome>()
        .unwrap()
        .hovered
        .unwrap();
    let fill = *app.world().get::<Color>(lit).unwrap();
    assert!(fill.r > 0.6 && fill.g < 0.3 && fill.a == 1.0, "{fill:?}");

    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert_eq!(take_window_actions(app.world_mut()), [WindowAction::Close]);
}

#[test]
fn macos_hides_the_drawn_controls() {
    let (app, row) = boot(Platform::Mac);
    assert!(app.world().has::<Hidden>(row));
}
