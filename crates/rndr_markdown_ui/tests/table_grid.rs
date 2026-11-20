//! Grid editing. Bytes stay the concrete tree; commands are the table splices.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rndr_core::*;
use rndr_markdown::{recover, Command, Session};
use rndr_markdown_ui::*;
use rndr_ui::UiId;

const TABLE: &str = "| alpha | beta |\n| :--- | ---: |\n| one | two |\n";

fn boot(src: &str) -> (App, MarkdownSurfaceHandle, MarkdownId) {
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    let id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(src);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(720.0),
        height: Size::Px(480.0),
        ..Layout::column()
    },));
    let handle = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(9),
        MarkdownSurfaceProps::new(id, "Notes"),
    )
    .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    app.update();
    (app, handle, id)
}

fn bytes(app: &App, id: MarkdownId) -> String {
    app.world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .bytes()
        .to_string()
}

fn caret(app: &App, id: MarkdownId) -> usize {
    app.world()
        .resource::<MarkdownDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .caret()
}

fn move_to(app: &mut App, id: MarkdownId, at: usize) {
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .move_caret(at, at)
        .unwrap();
}

fn press(app: &mut App, event: KeyEvent) {
    let mut input = Input::default();
    input.push_key(event, Platform::X11);
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.update();
}

fn labels(app: &App) -> Vec<String> {
    let world = app.world();
    world
        .entities_with::<Label>()
        .into_iter()
        .filter_map(|entity| world.get::<Label>(entity).map(|label| label.text.clone()))
        .filter(|text| !text.is_empty())
        .collect()
}

#[test]
fn typing_in_a_cell_is_byte_exact_and_undoes() {
    let (mut app, handle, id) = boot(TABLE);
    let at = TABLE.find("alpha").unwrap() + "alpha".len();
    move_to(&mut app, id, at);
    press(&mut app, KeyEvent::character('X', Modifiers::default()));
    let next = bytes(&app, id);
    assert_eq!(next, "| alphaX | beta |\n| :--- | ---: |\n| one | two |\n");
    let tree = rndr_markdown::parse(&next);
    assert_eq!(tree.concat(), next);
    handle
        .command(app.world_mut(), MarkdownCommand::Undo)
        .unwrap();
    assert_eq!(bytes(&app, id), TABLE);
    handle
        .command(app.world_mut(), MarkdownCommand::Redo)
        .unwrap();
    assert_eq!(
        bytes(&app, id),
        "| alphaX | beta |\n| :--- | ---: |\n| one | two |\n"
    );
}

#[test]
fn a_pipe_in_a_cell_is_escaped_and_the_rest_of_the_table_stays() {
    let (mut app, _, id) = boot(TABLE);
    let at = TABLE.find("beta").unwrap() + "beta".len();
    move_to(&mut app, id, at);
    press(&mut app, KeyEvent::character('|', Modifiers::default()));
    assert_eq!(
        bytes(&app, id),
        "| alpha | beta\\| |\n| :--- | ---: |\n| one | two |\n"
    );
}

#[test]
fn arrows_tab_and_enter_move_between_cells() {
    let (mut app, _, id) = boot(TABLE);
    let alpha = TABLE.find("alpha").unwrap();
    move_to(&mut app, id, alpha);
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    // End of "alpha". The next arrow crosses into "beta".
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    let beta = TABLE.find("beta").unwrap();
    assert_eq!(caret(&app, id), beta, "right edge enters the next cell");
    press(
        &mut app,
        KeyEvent::named(
            NamedKey::Tab,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ),
    );
    assert_eq!(caret(&app, id), alpha + "alpha".len());
    move_to(&mut app, id, alpha);
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowDown, Modifiers::default()),
    );
    let one = TABLE.find("one").unwrap();
    assert_eq!(caret(&app, id), one, "down stays in the column");
    move_to(&mut app, id, alpha);
    press(
        &mut app,
        KeyEvent::named(NamedKey::Enter, Modifiers::default()),
    );
    assert_eq!(caret(&app, id), one);
    assert_eq!(bytes(&app, id), TABLE, "enter does not split the table");
    press(
        &mut app,
        KeyEvent::named(NamedKey::Tab, Modifiers::default()),
    );
    let two = TABLE.find("two").unwrap();
    assert_eq!(caret(&app, id), two);
}

#[test]
fn tab_in_the_last_cell_appends_a_row_through_the_command() {
    let (mut app, _, id) = boot(TABLE);
    let end = TABLE.find("two").unwrap() + "two".len();
    move_to(&mut app, id, end);
    press(
        &mut app,
        KeyEvent::named(NamedKey::Tab, Modifiers::default()),
    );
    let next = bytes(&app, id);
    let expected = {
        let tree = rndr_markdown::parse(TABLE);
        rndr_markdown::apply_view(TABLE, &tree, &Command::InsertTableRow { at: 0, index: 1 })
            .unwrap()
            .bytes
    };
    assert_eq!(next, expected);
    assert!(next.starts_with(TABLE));
    assert!(next.len() > TABLE.len());
    let tree = rndr_markdown::parse(&next);
    assert_eq!(tree.concat(), next);
}

#[test]
fn backspace_stays_inside_the_cell() {
    let (mut app, _, id) = boot(TABLE);
    let alpha = TABLE.find("alpha").unwrap();
    move_to(&mut app, id, alpha);
    press(
        &mut app,
        KeyEvent::named(NamedKey::Backspace, Modifiers::default()),
    );
    assert_eq!(
        bytes(&app, id),
        TABLE,
        "backspace at the start keeps the pipe"
    );
    move_to(&mut app, id, alpha + 1);
    press(
        &mut app,
        KeyEvent::named(NamedKey::Backspace, Modifiers::default()),
    );
    assert_eq!(
        bytes(&app, id),
        "| lpha | beta |\n| :--- | ---: |\n| one | two |\n"
    );
}

#[test]
fn journal_replays_the_cell_splice() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rndr-grid-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("note.md");
    let journal = dir.join("note.md.journal");
    fs::write(&doc, TABLE).unwrap();
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    let id = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open_file(&doc, &journal)
        .unwrap();
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(720.0),
        height: Size::Px(480.0),
        ..Layout::column()
    },));
    let handle = MarkdownSurface::mount(
        app.world_mut(),
        parent,
        UiId(11),
        MarkdownSurfaceProps::new(id, "Notes"),
    )
    .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(handle.root);
    app.update();
    let at = TABLE.find("alpha").unwrap() + "alpha".len();
    move_to(&mut app, id, at);
    press(&mut app, KeyEvent::character('Q', Modifiers::default()));
    let edited = bytes(&app, id);
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .flush()
        .unwrap();
    let stored = fs::read(&journal).unwrap();
    let (start, end) = first_frame_range(&stored);
    assert!(
        end > start && end - start < 16,
        "cell splice {start}..{end}, not a whole-table rewrite"
    );
    assert_ne!((start, end), (0, TABLE.len() as u64));
    let recovered = recover(&doc, &journal).unwrap();
    assert_eq!(recovered, edited.as_bytes());
    let again = Session::open_file(&doc, &journal).unwrap();
    assert_eq!(again.bytes(), edited);
    let _ = fs::remove_dir_all(&dir);
}

fn first_frame_range(journal: &[u8]) -> (u64, u64) {
    let body = 60 + 8;
    let start = u64::from_le_bytes(journal[body + 8..body + 16].try_into().unwrap());
    let end = u64::from_le_bytes(journal[body + 16..body + 24].try_into().unwrap());
    (start, end)
}

#[test]
fn inline_marks_follow_the_caret_and_pipes_show_only_on_the_pipe() {
    let src = "| **bold** | x |\n| --- | --- |\n| [go](https://e.test) | y |\n";
    let (mut app, _, id) = boot(src);
    move_to(&mut app, id, src.find("x").unwrap());
    app.update();
    let shown = labels(&app);
    assert!(
        shown.iter().any(|text| text == "bold"),
        "markers hidden beside the caret: {shown:?}"
    );
    assert!(
        shown.iter().any(|text| text == "go"),
        "link label is the link text: {shown:?}"
    );
    assert!(
        shown.iter().all(|text| !text.contains("https://")),
        "destination stays hidden: {shown:?}"
    );
    let star = src.find("**bold**").unwrap() + 2;
    move_to(&mut app, id, star);
    app.update();
    let shown = labels(&app);
    assert!(
        shown.iter().any(|text| text.contains("**")),
        "delimiters show when the caret is in the span: {shown:?}"
    );
    move_to(&mut app, id, 0);
    app.update();
    let shown = labels(&app);
    assert!(
        shown.iter().any(|text| text.contains('|')),
        "a caret on the opening pipe reveals that row: {shown:?}"
    );
}

#[test]
fn the_context_menu_inserts_a_column_and_sets_alignment() {
    let (mut app, _, id) = boot(TABLE);
    move_to(&mut app, id, TABLE.find("alpha").unwrap());
    app.update();
    app.update();
    let cell = {
        let world = app.world();
        world
            .entities_with::<Label>()
            .into_iter()
            .find(|entity| {
                world
                    .get::<Label>(*entity)
                    .is_some_and(|label| label.text == "alpha")
            })
            .expect("header cell")
    };
    let rect = *app.world().get::<Rect>(cell).unwrap();
    let at = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
    let mut input = Input::default();
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Right;
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.update();
    // First enabled item is "Insert row below" on the header. One Down reaches
    // "Insert column left".
    press(
        &mut app,
        KeyEvent::named(NamedKey::ArrowDown, Modifiers::default()),
    );
    press(
        &mut app,
        KeyEvent::named(NamedKey::Enter, Modifiers::default()),
    );
    let with_column = bytes(&app, id);
    let expected = {
        let tree = rndr_markdown::parse(TABLE);
        rndr_markdown::apply_view(
            TABLE,
            &tree,
            &Command::InsertTableColumn { at: 0, index: 0 },
        )
        .unwrap()
        .bytes
    };
    assert_eq!(with_column, expected, "column insert is the table command");

    move_to(&mut app, id, with_column.find("alpha").unwrap());
    app.update();
    app.update();
    let cell = {
        let world = app.world();
        world
            .entities_with::<Label>()
            .into_iter()
            .find(|entity| {
                world
                    .get::<Label>(*entity)
                    .is_some_and(|label| label.text == "alpha")
            })
            .expect("header cell after insert")
    };
    let rect = *app.world().get::<Rect>(cell).unwrap();
    let at = [rect.x + 4.0, rect.y + rect.h * 0.5];
    let mut input = Input::default();
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Right;
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    *app.world_mut().resource_mut::<Input>().unwrap() = input;
    app.update();
    for _ in 0..6 {
        press(
            &mut app,
            KeyEvent::named(NamedKey::ArrowDown, Modifiers::default()),
        );
    }
    press(
        &mut app,
        KeyEvent::named(NamedKey::Enter, Modifiers::default()),
    );
    let aligned = bytes(&app, id);
    let expected = {
        let tree = rndr_markdown::parse(&with_column);
        rndr_markdown::apply_view(
            &with_column,
            &tree,
            &Command::SetTableAlignment {
                at: 0,
                index: 1,
                alignment: rndr_markdown::Alignment::Right,
            },
        )
        .unwrap()
        .bytes
    };
    assert_eq!(aligned, expected);
    let tree = rndr_markdown::parse(&aligned);
    assert_eq!(tree.concat(), aligned);
}
