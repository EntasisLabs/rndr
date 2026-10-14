use rndr_core::{edit_string, Key};

#[test]
fn arrows_and_backspace_step_by_scalar() {
    let mut text = "é".to_string();
    let (index, anchor) = edit_string(&mut text, 2, 2, Some(Key::Left), false, "", false);
    assert_eq!((index, anchor), (0, 0));
    assert_eq!(text, "é");

    let (index, _) = edit_string(&mut text, 0, 0, Some(Key::Right), false, "", false);
    assert_eq!(index, 2);
    assert_eq!(text, "é");

    let (index, _) = edit_string(&mut text, 2, 2, Some(Key::Backspace), false, "", false);
    assert_eq!(text, "");
    assert_eq!(index, 0);
}

#[test]
fn backspace_inside_a_scalar_removes_the_whole_scalar() {
    let mut text = "é".to_string();
    let (index, _) = edit_string(&mut text, 1, 1, Some(Key::Backspace), false, "", false);
    assert_eq!(text, "");
    assert_eq!(index, 0);
}

#[test]
fn ascii_caret_still_moves_one_byte() {
    let mut text = "ab".to_string();
    let (index, _) = edit_string(&mut text, 2, 2, Some(Key::Left), false, "", false);
    assert_eq!(index, 1);
    let (index, _) = edit_string(
        &mut text,
        index,
        index,
        Some(Key::Backspace),
        false,
        "",
        false,
    );
    assert_eq!(text, "b");
    assert_eq!(index, 0);
}
