//! Headless command palette. Not a screen-reader or native-keyboard certificate.
use rndr_core::*;
use rndr_ui::collections::{ItemId, VirtualViewHandle};
use rndr_ui::palette::*;
use rndr_ui::{UiId, UiPlugin};

fn open_palette(commands: &[PaletteCommand]) -> (Sim, Entity, PaletteHandle) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(800.0, 600.0));
    let anchor = sim.world().spawn((
        Layout {
            width: Size::Px(560.0),
            height: Size::Px(32.0),
            ..Layout::column()
        },
        Focusable,
        Button::new("Search"),
    ));
    let mut props = PaletteProps::new("Commands");
    props.list_height = 400.0;
    props.row_height = 32.0;
    let palette = CommandPalette::mount(sim.world(), anchor, UiId(7), props).unwrap();
    palette.set_commands(sim.world(), commands).unwrap();
    assert!(palette.open(sim.world()));
    sim.update();
    (sim, anchor, palette)
}

fn commands() -> Vec<PaletteCommand> {
    let mut search = PaletteCommand::new(1, "Search memory");
    search.icon = Some(Icon::Search);
    search.shortcut = "Ctrl+K".into();
    search.group = Some("Memory".into());
    let mut resume = PaletteCommand::new(2, "Résumé builder");
    resume.group = Some("Memory".into());
    let mut settings = PaletteCommand::new(3, "Open settings");
    settings.icon = Some(Icon::Settings);
    settings.shortcut = "Ctrl+,".into();
    settings.group = Some("Session".into());
    let mut quit = PaletteCommand::new(4, "Quit");
    quit.enabled = false;
    quit.group = Some("Session".into());
    vec![search, resume, settings, quit]
}

fn press(sim: &mut Sim, key: NamedKey) {
    sim.world().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(key, Modifiers::default())];
    sim.update();
}

fn type_text(sim: &mut Sim, text: &str) {
    sim.world().resource_mut::<Input>().unwrap().keys = text
        .chars()
        .map(|ch| KeyEvent::character(ch, Modifiers::default()))
        .collect();
    sim.update();
}

fn row_label(world: &World, row: Entity, text: &str) -> Option<Entity> {
    fn walk(world: &World, entity: Entity, text: &str, found: &mut Option<Entity>) {
        if found.is_some() {
            return;
        }
        if world
            .get::<Label>(entity)
            .is_some_and(|label| label.text == text)
        {
            *found = Some(entity);
            return;
        }
        for child in children_of(world, entity) {
            walk(world, child, text, found);
        }
    }
    let mut found = None;
    walk(world, row, text, &mut found);
    found
}

#[test]
fn typing_filters_highlights_and_keeps_field_focus() {
    let (mut sim, _, palette) = open_palette(&commands());
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(palette.field)
    );
    type_text(&mut sim, "se");
    sim.update();
    let query = palette.query(sim.app.world()).unwrap();
    assert_eq!(query.filter, "se");
    assert_eq!(query.mode, None);
    let handle = VirtualViewHandle { root: palette.list };
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(1)));
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(palette.field)
    );
    let row = handle.row(sim.app.world(), ItemId(1)).unwrap();
    let title = row_label(sim.app.world(), row.content, "Search memory").unwrap();
    let spans = sim.app.world().get::<Spans>(title).unwrap();
    assert_eq!(spans.runs.len(), 1);
    // Memory stays one section: Search, then Résumé. Settings is the other
    // section. Quit does not match.
    assert_eq!(handle.len(sim.app.world()), 5);
    assert!(handle.row(sim.app.world(), ItemId(2)).is_some());
    assert!(handle.row(sim.app.world(), ItemId(3)).is_some());
    assert!(handle.row(sim.app.world(), ItemId(4)).is_none());
}

#[test]
fn arrows_page_home_end_and_enter_activate_without_moving_focus() {
    let plain: Vec<_> = (0..8)
        .map(|id| PaletteCommand::new(id, format!("Command {id}")))
        .collect();
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(800.0, 600.0));
    let anchor = sim.world().spawn((
        Layout {
            width: Size::Px(560.0),
            height: Size::Px(28.0),
            ..Layout::column()
        },
        Focusable,
        Button::new("Go"),
    ));
    let mut props = PaletteProps::new("Commands");
    props.list_height = 96.0;
    props.row_height = 32.0;
    let palette = CommandPalette::mount(sim.world(), anchor, UiId(3), props).unwrap();
    palette.set_commands(sim.world(), &plain).unwrap();
    assert!(palette.open(sim.world()));
    sim.update();
    let handle = VirtualViewHandle { root: palette.list };
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(0)));
    press(&mut sim, NamedKey::ArrowDown);
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(1)));
    assert_eq!(handle.selected(sim.app.world()), Some(ItemId(1)));
    press(&mut sim, NamedKey::PageDown);
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(4)));
    press(&mut sim, NamedKey::End);
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(7)));
    press(&mut sim, NamedKey::Home);
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(0)));
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(palette.field)
    );
    press(&mut sim, NamedKey::Enter);
    let activated: Vec<_> = events::<PaletteActivated>(sim.app.world())
        .iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(activated, vec![ItemId(0)]);
    assert!(palette.is_open(sim.app.world()));
}

#[test]
fn escape_returns_focus_unless_the_host_defers_it() {
    let (mut sim, anchor, palette) = open_palette(&commands());
    palette.set_defer_escape(sim.world(), true).unwrap();
    press(&mut sim, NamedKey::Escape);
    assert!(palette.is_open(sim.app.world()));
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(palette.field)
    );
    assert!(events::<PaletteDismissed>(sim.app.world()).is_empty());
    palette.set_defer_escape(sim.world(), false).unwrap();
    press(&mut sim, NamedKey::Escape);
    assert!(!palette.is_open(sim.app.world()));
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(anchor)
    );
    assert_eq!(
        events::<PaletteDismissed>(sim.app.world())
            .iter()
            .map(|event| event.focus)
            .collect::<Vec<_>>(),
        vec![Some(anchor)]
    );
}

#[test]
fn prefix_mode_sections_icons_and_shortcuts() {
    let (mut sim, _, palette) = open_palette(&commands());
    palette
        .set_modes(
            sim.world(),
            &[PaletteMode::new('>', "Actions", "Filter actions")],
        )
        .unwrap();
    let mut settings = commands()[2].clone();
    settings.modes = vec!['>'];
    let mut catalog = commands();
    catalog[2] = settings;
    // Settings leaves the default mode once it is tagged for `>` only.
    palette.set_commands(sim.world(), &catalog).unwrap();
    sim.update();
    let handle = VirtualViewHandle { root: palette.list };
    assert!(handle.row(sim.app.world(), ItemId(3)).is_none());
    press(&mut sim, NamedKey::ArrowDown);
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(2)));
    assert!(is_section(ItemId(1 << 63)));
    let memory = handle
        .row(sim.app.world(), ItemId(1 << 63))
        .expect("memory section");
    assert!(row_label(sim.app.world(), memory.content, "Memory").is_some());
    let search = handle.row(sim.app.world(), ItemId(1)).unwrap();
    assert!(row_label(
        sim.app.world(),
        search.content,
        &Icon::Search.scalar().to_string()
    )
    .is_some());
    assert!(row_label(sim.app.world(), search.content, "Ctrl+K").is_some());
    palette.set_query(sim.world(), ">set").unwrap();
    sim.update();
    assert_eq!(palette.query(sim.app.world()).unwrap().mode, Some('>'));
    assert_eq!(palette.query(sim.app.world()).unwrap().filter, "set");
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(3)));
    assert!(handle.row(sim.app.world(), ItemId(1)).is_none());
}

#[test]
fn empty_and_async_epochs() {
    let (mut sim, _, palette) = open_palette(&commands());
    type_text(&mut sim, "zzz");
    sim.update();
    assert_eq!(
        VirtualViewHandle { root: palette.list }.len(sim.app.world()),
        0
    );
    assert_eq!(
        sim.app.world().get::<Label>(palette.status).unwrap().text,
        "No matches"
    );
    palette.set_async(sim.world(), true).unwrap();
    assert!(palette.is_loading(sim.app.world()));
    let epoch = palette.epoch(sim.app.world()).unwrap();
    assert!(!palette
        .resolve(sim.world(), epoch.wrapping_sub(1), &commands())
        .unwrap());
    assert!(palette.is_loading(sim.app.world()));
    assert!(palette.resolve(sim.world(), epoch, &commands()).unwrap());
    assert!(!palette.is_loading(sim.app.world()));
    assert!(VirtualViewHandle { root: palette.list }.len(sim.app.world()) >= 4);
    let current = palette.set_query(sim.world(), "mem").unwrap();
    assert!(palette.is_loading(sim.app.world()));
    assert!(!palette
        .resolve(sim.world(), epoch, &commands()[..1])
        .unwrap());
    assert!(VirtualViewHandle { root: palette.list }.len(sim.app.world()) >= 4);
    let narrowed = vec![commands()[0].clone()];
    assert!(palette
        .resolve(sim.world(), current.epoch, &narrowed)
        .unwrap());
    assert_eq!(
        VirtualViewHandle { root: palette.list }.active(sim.app.world()),
        Some(ItemId(1))
    );
    assert!(!palette.is_loading(sim.app.world()));
}

#[test]
fn combobox_listbox_and_active_descendant() {
    let (sim, _, palette) = open_palette(&commands());
    let world = sim.app.world();
    let field = node(world, palette.field);
    assert_eq!(field.role, AccessRole::ComboBox);
    assert_eq!(field.expanded, Some(true));
    assert_eq!(field.name, "Commands");
    let details = access_details(world, palette.field);
    assert_eq!(details.controls, Some(access_id(palette.list)));
    let row = VirtualViewHandle { root: palette.list }
        .row(world, ItemId(1))
        .unwrap()
        .root;
    assert_eq!(details.active_descendant, Some(access_id(row)));
    assert_eq!(node(world, palette.list).role, AccessRole::ListBox);
    assert_eq!(node(world, row).role, AccessRole::Option);
    assert!(world.has::<LiveRegion>(palette.status));
    assert!(world
        .get::<AccessMark>(palette.status)
        .is_some_and(|mark| mark.0 == AccessRole::Status));
}

#[test]
fn host_matcher_replaces_fuzzy() {
    fn prefix(query: &str, title: &str) -> Option<PaletteMatch> {
        if query.is_empty() || title.starts_with(query) {
            Some(PaletteMatch {
                score: 0,
                ranges: if query.is_empty() {
                    Vec::new()
                } else {
                    vec![(0, query.len())]
                },
            })
        } else {
            None
        }
    }
    let (mut sim, _, palette) = open_palette(&commands());
    palette.set_matcher(sim.world(), prefix).unwrap();
    palette.set_query(sim.world(), "Open").unwrap();
    sim.update();
    let handle = VirtualViewHandle { root: palette.list };
    assert_eq!(handle.active(sim.app.world()), Some(ItemId(3)));
    assert!(handle.row(sim.app.world(), ItemId(1)).is_none());
}

#[test]
fn ime_enter_does_not_activate() {
    let (mut sim, _, palette) = open_palette(&commands());
    let active = palette.active(sim.app.world());
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.ime_enabled = true;
        input.ime_preedit = "に".into();
        input.keys = vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    }
    sim.update();
    assert_eq!(palette.active(sim.app.world()), active);
    assert!(events::<PaletteActivated>(sim.app.world()).is_empty());
    assert_eq!(
        sim.app.world().resource::<Focus>().unwrap().entity,
        Some(palette.field)
    );
}

fn node(world: &World, entity: Entity) -> AccessNode {
    let id = access_id(entity);
    access_tree(world)
        .nodes
        .into_iter()
        .find(|candidate| candidate.id == id)
        .expect("access node")
}

#[test]
fn identity_is_the_popup() {
    let (sim, _, palette) = open_palette(&commands());
    assert_eq!(
        rndr_ui::identity(sim.app.world(), palette.root),
        Some(UiId(7))
    );
    assert_eq!(rndr_ui::identity(sim.app.world(), palette.field), None);
}
