use rndr_core::*;
use rndr_ui::*;

pub fn scene(theme: UiTheme, width: f32) -> (App, UiHandle, Entity) {
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(width, 6000.0));
    app.world_mut().insert_resource(theme.clone());
    app.add_plugin(UiPlugin).add_plugin(UiPlugin);
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(width),
            ..Layout::column()
        },
        UiStyle {
            background: Some(Token::Canvas.into()),
            ..UiStyle::default()
        },
    ));
    let reading = app.world_mut().spawn((reading_layout(width, &theme),));
    set_parent(app.world_mut(), reading, root);
    text(
        app.world_mut(),
        reading,
        "rndr UI — passive A0 gallery",
        Typography::Title,
    )
    .unwrap();
    text(
        app.world_mut(),
        reading,
        "A0 foundations only. No operating controls; A2 waits for independent A1 review.",
        Typography::Secondary,
    )
    .unwrap();
    separator(app.world_mut(), reading).unwrap();
    let card = Card::mount(app.world_mut(), reading, UiId(1), "A quiet reading surface").unwrap();
    let body = text(app.world_mut(), card.content, "Long content wraps without fixed text heights. Theme, density and text scaling preserve this content slot and its identity. Explicit app colors remain authoritative.", Typography::Reading).unwrap();
    card.adopt(app.world_mut(), body).unwrap();
    for (index, feedback) in [
        Feedback::Info,
        Feedback::Success,
        Feedback::Warning,
        Feedback::Danger,
    ]
    .into_iter()
    .enumerate()
    {
        let callout = Callout::mount(
            app.world_mut(),
            reading,
            UiId(index as u64 + 2),
            feedback,
            "Meaning is written, not color alone",
        )
        .unwrap();
        let body = text(app.world_mut(), callout.content, "Passive message content. Wrapping and expanded text stay in document flow; nothing here performs an action.", Typography::Reading).unwrap();
        callout.adopt(app.world_mut(), body).unwrap();
    }
    for (index, surface) in [
        Surface::Canvas,
        Surface::Navigation,
        Surface::Content,
        Surface::Raised,
        Surface::Overlay,
    ]
    .into_iter()
    .enumerate()
    {
        let handle = surface
            .mount(app.world_mut(), reading, UiId(index as u64 + 10))
            .unwrap();
        let body = text(
            app.world_mut(),
            handle.content,
            &format!("{surface:?} surface — opaque semantic plane"),
            Typography::Label,
        )
        .unwrap();
        handle.adopt(app.world_mut(), body).unwrap();
    }
    for role in [Typography::Heading, Typography::Reading, Typography::Code] {
        text(
            app.world_mut(),
            reading,
            &format!("{role:?}: typography uses the installed engine and licensed fallback."),
            role,
        )
        .unwrap();
    }
    text(
        app.world_mut(),
        reading,
        "Localization fixture: 日本語 / العربية / e\u{301} / 👩🏽‍💻",
        Typography::Reading,
    )
    .unwrap();
    // Full-document headless capture. Use the existing core layout result,
    // not a fixed viewport that would silently omit lower gallery states.
    app.update();
    let height = app.world().get::<Rect>(root).unwrap().h;
    app.world_mut().resource_mut::<Viewport>().unwrap().height = height + 32.0;
    (app, card, body)
}
