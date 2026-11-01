//! Shared operating gallery: synthetic host only, no files/parser/app adoption.
#![allow(dead_code)]
use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
#[derive(Clone)]
pub struct Demo {
    pub root: Entity,
    pub toolbar: ToolbarHandle,
    pub commands: Vec<ControlHandle>,
    pub nav: [ControlHandle; 2],
    pub selector: ModeHandle,
    pub source: ControlHandle,
    pub card: UiHandle,
    pub callout: UiHandle,
    pub editor: Entity,
    pub status: Entity,
    pub nested: Entity,
    pub reading: Entity,
    pub accepted: u64,
    pub reveals: u64,
    pub rejected: u64,
    pub locator: SourceLocator,
    pub selection: usize,
}

pub fn create(width: f32, height: f32, theme: UiTheme, long: bool) -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    app.world_mut().insert_resource(theme);
    let w = app.world_mut();
    let root = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 12.0,
            gap: 8.0,
            ..Layout::column()
        },
        Scroll::default(),
        UiRecipe(UiStyle {
            background: Some(Token::Canvas.into()),
            ..UiStyle::default()
        }),
    ));
    text(
        w,
        root,
        "rndr A2 operating gallery (synthetic host)",
        Typography::Heading,
    )
    .unwrap();
    text(w, root, "Tab/Shift-Tab navigate. Enter or Space activates. Mode arrows focus only. No real documents, saves or deletion.", Typography::Secondary).unwrap();
    let toolbar = Toolbar::mount(w, root, UiId(1), (width - 24.0).max(32.0)).unwrap();
    w.insert(
        toolbar.root,
        Semantics {
            name: Some("Gallery command toolbar".into()),
            ..Semantics::default()
        },
    );
    let mut commands = vec![];
    for (i, label, variant) in [
        (0, "Add", ButtonVariant::Primary),
        (1, "Secondary", ButtonVariant::Secondary),
        (2, "Quiet", ButtonVariant::Quiet),
        (3, "Reset demo count", ButtonVariant::Destructive),
    ] {
        let label = if long {
            format!("{label} with a deliberately very long wrapped command label")
        } else {
            label.into()
        };
        let mut p = ButtonProps::new(label, HostIntent::Command(i));
        p.variant = variant;
        commands.push(controls::Button::mount(w, toolbar.root, UiId(10 + i), p).unwrap());
    }
    commands.push(
        IconButton::mount(
            w,
            toolbar.root,
            UiId(14),
            "+",
            "Add one synthetic item",
            ButtonProps::new("+", HostIntent::Command(0)),
        )
        .unwrap(),
    );
    for command in &commands {
        w.insert(command.root, ToolbarContentWidth);
    }
    let nav = [
        NavigationRow::mount(
            w,
            root,
            UiId(20),
            ButtonProps::new("Draft note", HostIntent::Command(20)),
            NavigationMeta {
                selected: true,
                secondary: Some("Demo folder".into()),
                dirty: true,
            },
        )
        .unwrap(),
        NavigationRow::mount(
            w,
            root,
            UiId(21),
            ButtonProps::new("Reference note", HostIntent::Command(21)),
            NavigationMeta {
                secondary: Some("Read-only fixture".into()),
                ..NavigationMeta::default()
            },
        )
        .unwrap(),
    ];
    let selector = ModeSelector::mount(
        w,
        root,
        UiId(30),
        "Document mode",
        &["Source", "Preview"],
        TabOrientation::Horizontal,
        0,
    )
    .unwrap();
    let editor = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(108.0),
            ..Layout::column()
        },
        Editor::default(),
        CapturesKeys,
        Label::new(""),
        Buffer::from_str("Synthetic editable source. Type here; Undo/Redo remain core-owned."),
        Semantics {
            name: Some("Synthetic source editor".into()),
            ..Semantics::default()
        },
    ));
    w.insert(editor, UiRecipe(UiStyle::editor()));
    set_parent(w, editor, selector.panels[0]);
    let reading = Surface::Content
        .mount(w, selector.panels[1], UiId(1))
        .unwrap();
    let reading_theme = w.resource::<UiTheme>().unwrap().clone();
    w.insert(
        reading.root,
        reading_layout((width - 24.0).max(32.0), &reading_theme),
    );
    text(w, reading.content, "Preview is inert content. Switching mode preserves the same source field, selection and history. Nothing parses or executes the text.", Typography::Reading).unwrap();
    let card = Card::mount(w, root, UiId(40), "Content card").unwrap();
    text(
        w,
        card.content,
        "Borrowed content and actions have explicit slots. The card itself is not clickable.",
        Typography::Reading,
    )
    .unwrap();
    let locator = SourceLocator {
        document: 7,
        revision: 3,
        anchor: 11,
    };
    let source = SourceAction::mount(w, card.content, UiId(1), locator, true).unwrap();
    let callout = Callout::mount(
        w,
        root,
        UiId(41),
        Feedback::Warning,
        "Synthetic unsaved state",
    )
    .unwrap();
    text(
        w,
        callout.content,
        "Warnings stay visible. This is not a recovery or saving implementation.",
        Typography::Reading,
    )
    .unwrap();
    let nested = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(100.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(w, nested, card.content);
    for _ in 0..5 {
        text(w, nested, "Nested scroll content: keyboard-accessible source action stays labelled, never hover-only.", Typography::Reading).unwrap();
    }
    let status = text(
        w,
        root,
        "Requests: 0; source reveals: 0",
        Typography::Secondary,
    )
    .unwrap();
    w.insert_resource(Demo {
        root,
        toolbar,
        commands,
        nav,
        selector,
        source,
        card,
        callout,
        editor,
        status,
        nested,
        reading: reading.root,
        accepted: 0,
        reveals: 0,
        rejected: 0,
        locator,
        selection: 0,
    });
    app.add_system(Schedule::Update, consume);
    app.add_system(Schedule::Update, resize);
    app.update();
    app
}
fn resize(w: &mut World) {
    let Some(d) = w.resource::<Demo>().cloned() else {
        return;
    };
    let width = w.resource::<Viewport>().unwrap().content_width();
    Toolbar::mount(w, d.root, UiId(1), (width - 24.0).max(32.0)).unwrap();
    let theme = w.resource::<UiTheme>().unwrap().clone();
    w.insert(d.reading, reading_layout((width - 24.0).max(32.0), &theme));
}
/// Normal host test double: drains only the reviewed queue, retains multiplicity,
/// validates current UI payload plus independent host document/revision/anchor.
/// Overflow remains observable on ActionIntents; no dedup or alternate producer.
pub fn consume(w: &mut World) {
    let Some(mut d) = w.resource::<Demo>().cloned() else {
        return;
    };
    for request in take_action_intents(w) {
        match host_intent(w, &request) {
            Some(HostIntent::Command(n @ 0..=3)) => {
                d.accepted += 1;
                if n == 3 {
                    d.accepted = 0;
                }
            }
            Some(HostIntent::Command(n @ 20..=21)) => {
                d.selection = (n - 20) as usize;
                d.accepted += 1;
            }
            Some(HostIntent::RevealSource(locator)) if locator == d.locator => {
                d.reveals += 1;
            }
            _ => d.rejected += 1,
        }
    }
    for (i, label) in ["Draft note", "Reference note"].into_iter().enumerate() {
        let meta = NavigationMeta {
            selected: d.selection == i,
            secondary: Some(
                if i == 0 {
                    "Demo folder"
                } else {
                    "Read-only fixture"
                }
                .into(),
            ),
            dirty: i == 0,
        };
        NavigationRow::mount(
            w,
            d.root,
            UiId(20 + i as u64),
            ButtonProps::new(label, HostIntent::Command(20 + i as u64)),
            meta,
        )
        .unwrap();
    }
    w.get_mut::<Label>(d.status).unwrap().text = format!(
        "Requests: {}; source reveals: {}; rejected: {}",
        d.accepted, d.reveals, d.rejected
    );
    w.insert_resource(d);
}
