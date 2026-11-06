# IME and complex scripts

The composition fold is shared by the field, the prompt, and the editor.
While a preedit is showing, character keys and Home, End, Page Up, and Page
Down are dropped unless Ctrl, Alt, or Super is held. A commit is inserted
once. Clearing the preedit without a commit leaves the text alone.
`editing` refuses source edits while a composition is active. A structured
document captures a range with `begin_composition`, drops it with
`cancel_composition`, and inserts the commit once.

`crates/rndr_core/tests/complex_script_ime.rs` scripts that path for
`にほん`, `汉字`, and `مرحبا` on the field, the prompt, the editor, an
attached source session, and a structured document. Inspector drafts and
structured editor chrome use those same field and document paths. The test
does not open an OS input method.

`reading_layout` sizes the column as half an em times the character count,
plus padding. That estimate does not change for CJK or Arabic. It is not a
measured advance, and it is a poor approximation for both. The check is in
`crates/rndr_ui/tests/reading_measure.rs`.

## Open manual matrix

These cells were not run on a macOS or Windows host, or against a live
input method. Do not mark them passed from the headless cases above.

| Cell | macOS | Windows |
| --- | --- | --- |
| Japanese preedit, commit, cancel | open | open |
| Chinese preedit, commit, cancel | open | open |
| Korean preedit, commit, cancel | open | open |
| Arabic caret and bidi | open | open |
| Candidate window beside the caret | open | open |
| Physical DPI and the caret rectangle | open | open |
| Field, prompt, editor, inspector, source session, structured editor | open | open |

Linux Wayland and X11 input methods are open as well. Headless shaping with
the builtin bitmap engine does not certify glyph outlines.
