use rndr_markdown::{apply_view, parse, Command, Session};

fn round_trip(src: &str, cmd: Command) -> String {
    let tree = parse(src);
    let applied = apply_view(src, &tree, &cmd).unwrap();
    assert_eq!(applied.tree.concat(), applied.bytes);
    assert_eq!(applied.tree, parse(&applied.bytes));
    applied.bytes
}

#[test]
fn strong_reuses_the_existing_delimiter() {
    let src = "say **a** please\n";
    let start = src.find("**a**").unwrap() + 2;
    let end = start + 1;
    assert_eq!(
        round_trip(src, Command::ToggleStrong { start, end }),
        "say a please\n"
    );
    assert_eq!(
        round_trip("say a please\n", Command::ToggleStrong { start: 4, end: 5 }),
        "say **a** please\n"
    );
}

#[test]
fn code_span_grows_a_backtick_run_for_the_body() {
    let src = "a `b` c\n";
    let start = src.find('b').unwrap();
    assert_eq!(
        round_trip(
            src,
            Command::ToggleCode {
                start,
                end: start + 1
            }
        ),
        "a b c\n"
    );
    assert_eq!(
        round_trip("a`b\n", Command::ToggleCode { start: 0, end: 3 }),
        "``a`b``\n"
    );
    assert_eq!(
        round_trip("`x\n", Command::ToggleCode { start: 0, end: 2 }),
        "`` `x ``\n"
    );
}

#[test]
fn link_wraps_and_unwraps_the_label() {
    let wrapped = round_trip(
        "see docs\n",
        Command::ToggleLink {
            start: 4,
            end: 8,
            destination: "https://example.com".into(),
        },
    );
    assert_eq!(wrapped, "see [docs](https://example.com)\n");
    let start = wrapped.find("docs").unwrap();
    let end = start + 4;
    assert_eq!(
        round_trip(
            &wrapped,
            Command::ToggleLink {
                start,
                end,
                destination: String::new(),
            }
        ),
        "see docs\n"
    );
}

#[test]
fn strike_toggles() {
    assert_eq!(
        round_trip("ab\n", Command::ToggleStrike { start: 0, end: 2 }),
        "~~ab~~\n"
    );
}

#[test]
fn replace_keeps_a_heading_marker() {
    assert_eq!(
        round_trip(
            "Hello\n",
            Command::Replace {
                start: 0,
                end: 0,
                text: "# ".into(),
            }
        ),
        "# Hello\n"
    );
}

#[test]
fn caret_move_ends_a_typing_run_and_redo_restores_it() {
    let mut session = Session::open(".\n");
    session
        .edit(
            Command::Insert {
                at: 0,
                text: "a".into(),
            },
            0,
        )
        .unwrap();
    session.note_caret(1, 1);
    session.move_caret(0, 0).unwrap();
    session
        .edit(
            Command::Insert {
                at: 0,
                text: "b".into(),
            },
            10,
        )
        .unwrap();
    session.note_caret(1, 1);
    assert_eq!(session.bytes(), "ba.\n");
    session.undo(20).unwrap();
    assert_eq!(session.bytes(), "a.\n");
    assert_eq!(session.caret(), 0);
    session.redo(30).unwrap();
    assert_eq!(session.bytes(), "ba.\n");
    assert_eq!(session.caret(), 1);
}
