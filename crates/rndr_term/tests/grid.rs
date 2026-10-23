use rndr_core::{
    App, Color, FontStyle, Input, KeyCode, KeyEvent, Modifiers, MonoCache, MonoGrid, MonoUnderline,
    NamedKey, Platform, PointerButton, TextStyle, Viewport, FONT_WEIGHT_REGULAR,
};
use rndr_term::{
    feed_terminal, open_terminal, take_terminal_bytes, TerminalExited, TerminalOptions,
    TerminalPlugin,
};

fn font() -> TextStyle {
    TextStyle {
        family: "bitmap".into(),
        size: 8.0,
        weight: FONT_WEIGHT_REGULAR,
        style: FontStyle::Normal,
    }
}

fn harness(cols: u16, rows: u16) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(TerminalPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(f32::from(cols) * 8.0, f32::from(rows) * 8.0));
    let entity = app.world_mut().spawn(Color::rgb(0.0, 0.0, 0.0));
    let mut options = TerminalOptions::offline();
    options.cols = cols;
    options.rows = rows;
    options.cell = Some((8.0, 8.0));
    options.font = Some(font());
    open_terminal(app.world_mut(), entity, options).unwrap();
    app.update();
    (app, entity)
}

fn cell(app: &App, entity: rndr_core::Entity, col: usize, row: usize) -> rndr_core::MonoCell {
    let grid = app.world().get::<MonoGrid>(entity).unwrap();
    grid.cells[row * grid.cols as usize + col]
}

#[test]
fn sgr_colors_styles_and_unchanged_rows_are_not_reshaped() {
    let (mut app, entity) = harness(20, 4);
    assert!(feed_terminal(
        app.world_mut(),
        entity,
        b"\x1b[31mR\x1b[0m\x1b[1mB\x1b[0m\x1b[4mU\x1b[0m\x1b[7mI\x1b[0m\x1b[38;2;1;2;3mT\x1b[0m\x1b[38;5;196mC"
    ));
    app.update();

    let red = cell(&app, entity, 0, 0);
    assert!(red.fg.r > 0.7 && red.fg.g < 0.05, "{:?}", red.fg);
    assert_eq!(red.ch, 'R');

    let bold = cell(&app, entity, 1, 0);
    assert!(bold.style.bold);
    assert_eq!(bold.ch, 'B');

    let under = cell(&app, entity, 2, 0);
    assert_eq!(under.style.underline, MonoUnderline::Single);
    assert_eq!(under.ch, 'U');

    let inverse = cell(&app, entity, 3, 0);
    assert!(inverse.bg.r > inverse.fg.r, "inverse {:?}", inverse);
    assert_eq!(inverse.ch, 'I');

    let truecolor = cell(&app, entity, 4, 0);
    assert!(
        (truecolor.fg.r - 1.0 / 255.0).abs() < 0.01,
        "{:?}",
        truecolor.fg
    );
    assert!((truecolor.fg.g - 2.0 / 255.0).abs() < 0.01);
    assert!((truecolor.fg.b - 3.0 / 255.0).abs() < 0.01);

    let indexed = cell(&app, entity, 5, 0);
    assert!(
        indexed.fg.r > 0.9 && indexed.fg.g < 0.05,
        "{:?}",
        indexed.fg
    );

    let shaped = app.world().get::<MonoCache>(entity).unwrap().shaped;
    let gens = app.world().get::<MonoGrid>(entity).unwrap().row_gen.clone();
    app.update();
    assert_eq!(app.world().get::<MonoCache>(entity).unwrap().shaped, shaped);
    assert_eq!(app.world().get::<MonoGrid>(entity).unwrap().row_gen, gens);

    feed_terminal(app.world_mut(), entity, b"\r\nZ");
    app.update();
    let grid = app.world().get::<MonoGrid>(entity).unwrap();
    assert_eq!(grid.row_gen[0], gens[0], "the first line was reshaped");
    assert_ne!(grid.row_gen[1], gens[1]);
    assert_eq!(cell(&app, entity, 0, 1).ch, 'Z');
    assert!(app.world().get::<MonoCache>(entity).unwrap().shaped > shaped);
}

#[test]
fn keys_paste_and_mouse_reports_are_written() {
    let (mut app, entity) = harness(20, 4);
    let platform = Platform::host();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.push_key(KeyEvent::character('a', Modifiers::default()), platform);
        input.push_key(
            KeyEvent::named(NamedKey::Enter, Modifiers::default()),
            platform,
        );
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        input.push_key(
            KeyEvent::character('c', ctrl).with_code(KeyCode::Letter('c')),
            platform,
        );
    }
    app.update();
    assert_eq!(
        take_terminal_bytes(app.world_mut(), entity).unwrap(),
        b"a\r\x03"
    );

    feed_terminal(app.world_mut(), entity, b"\x1b[?2004h");
    app.world_mut()
        .resource_mut::<rndr_core::Clipboard>()
        .unwrap()
        .text = "hi".into();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.push_key(KeyEvent::character('v', copy_mods(platform)), platform);
    }
    app.update();
    assert_eq!(
        take_terminal_bytes(app.world_mut(), entity).unwrap(),
        b"\x1b[200~hi\x1b[201~"
    );

    feed_terminal(app.world_mut(), entity, b"\x1b[?1000h\x1b[?1006h");
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [2.0, 2.0];
        input.pointer.pressed = true;
        input.pointer.button = PointerButton::Left;
    }
    app.update();
    assert_eq!(
        take_terminal_bytes(app.world_mut(), entity).unwrap(),
        b"\x1b[<0;1;1M"
    );

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.ime_preedit = "x".into();
        input.push_key(KeyEvent::character('a', Modifiers::default()), platform);
    }
    app.update();
    assert!(
        take_terminal_bytes(app.world_mut(), entity)
            .unwrap()
            .is_empty(),
        "composition must not forward the raw key"
    );
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit.clear();
        input.ime_commit = "ok".into();
    }
    app.update();
    assert_eq!(take_terminal_bytes(app.world_mut(), entity).unwrap(), b"ok");
}

#[test]
fn wheel_scrolls_and_shift_page_moves_the_viewport() {
    let (mut app, entity) = harness(8, 4);
    let mut bytes = Vec::new();
    for n in 0..12u8 {
        bytes.extend(format!("line {n}\r\n").into_bytes());
    }
    feed_terminal(app.world_mut(), entity, &bytes);
    app.update();
    let before = row_text(&app, entity, 0);
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [2.0, 2.0];
        input.scroll = [0.0, 24.0];
    }
    app.update();
    let after = row_text(&app, entity, 0);
    assert_ne!(before, after, "wheel should scroll the viewport");

    let platform = Platform::host();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.scroll = [0.0, 0.0];
        input.push_key(
            KeyEvent::named(
                NamedKey::PageDown,
                Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            ),
            platform,
        );
    }
    app.update();
    assert!(take_terminal_bytes(app.world_mut(), entity)
        .unwrap()
        .is_empty());
}

#[test]
fn resize_follows_the_rect() {
    let (mut app, entity) = harness(10, 4);
    app.world_mut()
        .insert_resource(Viewport::new(8.0 * 16.0, 8.0 * 6.0));
    app.update();
    app.update();
    let grid = app.world().get::<MonoGrid>(entity).unwrap();
    assert_eq!((grid.cols, grid.rows), (16, 6));
}

#[test]
fn copy_selection_sets_the_clipboard() {
    let (mut app, entity) = harness(12, 3);
    feed_terminal(app.world_mut(), entity, b"Hi");
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [1.0, 1.0];
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.button = PointerButton::Left;
    }
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.down = true;
        input.pointer.position = [12.0, 1.0];
    }
    app.update();
    let platform = Platform::host();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.down = false;
        input.pointer.released = true;
        input.push_key(KeyEvent::character('c', copy_mods(platform)), platform);
    }
    app.update();
    let text = app
        .world()
        .resource::<rndr_core::Clipboard>()
        .unwrap()
        .text
        .clone();
    assert!(text.contains('H'), "{text}");
}

#[test]
fn tabs_marks_curly_underline_and_blocks_survive_the_grid() {
    let (mut app, entity) = harness(20, 4);
    let bytes =
        "tab:\tA\r\ne\u{0301} a\u{0308}\r\n\x1b[4:3mC\x1b[0m\x1b[4;58;5;196mU\x1b[0m\u{2588}";
    feed_terminal(app.world_mut(), entity, bytes.as_bytes());
    app.update();

    assert!(
        row_text(&app, entity, 0).chars().all(|ch| ch != '\t'),
        "a tab cell must be blank, got {:?}",
        row_text(&app, entity, 0)
    );
    assert_eq!(cell(&app, entity, 8, 0).ch, 'A');

    let acute = cell(&app, entity, 0, 1);
    assert_eq!(acute.ch, 'e');
    assert_eq!(acute.marks[0], '\u{0301}');
    let diaeresis = cell(&app, entity, 2, 1);
    assert_eq!(diaeresis.ch, 'a');
    assert_eq!(diaeresis.marks[0], '\u{0308}');

    let curly = cell(&app, entity, 0, 2);
    assert_eq!(curly.ch, 'C');
    assert_eq!(curly.style.underline, MonoUnderline::Curly);
    let colored = cell(&app, entity, 1, 2);
    assert_eq!(colored.ch, 'U');
    assert_eq!(colored.style.underline, MonoUnderline::Single);
    assert!(
        colored.underline_color.r > 0.9 && colored.underline_color.g < 0.05,
        "{:?}",
        colored.underline_color
    );
    let block = cell(&app, entity, 2, 2);
    assert_eq!(block.ch, '\u{2588}');
    let cache = app.world().get::<MonoCache>(entity).unwrap();
    assert!(
        cache.rows[2].backgrounds.iter().any(|run| {
            (run.x - 16.0).abs() < 1.0
                && run.w >= 7.0
                && run.h >= 7.0
                && (run.color.r - block.fg.r).abs() < 0.05
        }),
        "full block should fill its cell, runs {:?}",
        cache.rows[2].backgrounds
    );
    assert!(
        cache.rows[2]
            .glyphs
            .iter()
            .all(|glyph| glyph.ch != '\u{2588}'),
        "the block glyph would leave seams"
    );
    assert!(
        cache.rows[2].strokes.len() >= 3,
        "curly underline should be a wave, strokes {:?}",
        cache.rows[2].strokes
    );
}

#[test]
fn a_press_and_release_in_one_frame_selects_the_span() {
    let (mut app, entity) = harness(12, 3);
    feed_terminal(app.world_mut(), entity, b"abcdefghij");
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.press_at = Some([1.0, 1.0]);
        input.pointer.position = [40.0, 1.0];
        input.pointer.pressed = true;
        input.pointer.released = true;
        input.pointer.down = false;
        input.pointer.button = PointerButton::Left;
    }
    app.update();
    let grid = app.world().get::<MonoGrid>(entity).unwrap();
    let band = grid.selection.first().expect("selection");
    assert_eq!(band.row, 0);
    assert!(
        band.end.saturating_sub(band.start) >= 2,
        "same-frame drag selected {:?}",
        grid.selection
    );
    let primary = {
        let clip = app.world().resource::<rndr_core::Clipboard>().unwrap();
        assert!(clip.primary_dirty, "selection should publish the primary");
        assert!(clip.primary.contains('a'), "{}", clip.primary);
        clip.primary.clone()
    };

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.released = false;
        input.pointer.press_at = None;
        input.pointer.button = PointerButton::Middle;
        input.pointer.pressed = true;
        input.pointer.position = [1.0, 1.0];
    }
    app.update();
    let written = take_terminal_bytes(app.world_mut(), entity).unwrap();
    let text = std::str::from_utf8(&written).unwrap_or("");
    assert!(
        text.contains(primary.as_str()) || text.contains('a'),
        "middle click should paste the primary {primary:?}, wrote {written:?}"
    );
}

fn row_text(app: &App, entity: rndr_core::Entity, row: usize) -> String {
    let grid = app.world().get::<MonoGrid>(entity).unwrap();
    let start = row * grid.cols as usize;
    grid.cells[start..start + grid.cols as usize]
        .iter()
        .map(|cell| cell.ch)
        .collect()
}

#[test]
fn child_exit_is_an_event_and_close_returns() {
    let mut app = App::new();
    app.add_plugin(TerminalPlugin);
    let entity = app.world_mut().spawn(Color::rgb(0.0, 0.0, 0.0));
    let mut options = TerminalOptions::program(shell());
    options.args = shell_exit_args();
    options.cell = Some((8.0, 8.0));
    options.cols = 40;
    options.rows = 8;
    options.font = Some(font());
    open_terminal(app.world_mut(), entity, options).unwrap();
    let start = std::time::Instant::now();
    loop {
        app.update();
        if let Some(event) = rndr_core::events::<TerminalExited>(app.world()).first() {
            assert_eq!(event.entity, entity);
            assert_eq!(event.code, Some(7));
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("child did not exit");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let sleeper = app.world_mut().spawn(Color::rgb(0.0, 0.0, 0.0));
    let mut options = TerminalOptions::program(sleep_program());
    options.args = sleep_args();
    options.cell = Some((8.0, 8.0));
    options.font = Some(font());
    open_terminal(app.world_mut(), sleeper, options).unwrap();
    let start = std::time::Instant::now();
    rndr_term::close_terminal(app.world_mut(), sleeper);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "close_terminal did not return"
    );
    assert!(!app.world().has::<MonoGrid>(sleeper));
}

fn copy_mods(platform: Platform) -> Modifiers {
    if platform.primary_is_super() {
        Modifiers {
            super_key: true,
            ..Modifiers::default()
        }
    } else {
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        }
    }
}

fn shell() -> &'static str {
    if cfg!(windows) {
        "cmd"
    } else {
        "sh"
    }
}

fn shell_exit_args() -> Vec<String> {
    if cfg!(windows) {
        vec!["/C".into(), "exit 7".into()]
    } else {
        vec!["-c".into(), "printf hi; exit 7".into()]
    }
}

fn sleep_program() -> &'static str {
    if cfg!(windows) {
        "ping"
    } else {
        "sleep"
    }
}

fn sleep_args() -> Vec<String> {
    if cfg!(windows) {
        vec!["-n".into(), "30".into(), "127.0.0.1".into()]
    } else {
        vec!["30".into()]
    }
}
