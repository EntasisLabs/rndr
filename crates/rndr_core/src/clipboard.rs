use crate::action::{Binding, Chord, Fired, COPY, CUT, PASTE};

/// Write `copied` into the clipboard and clear a legacy copy/cut/paste [`Fired`]
/// so this frame does not apply the same edit twice.
pub(crate) fn note_clipboard(world: &mut World, copied: Option<&str>) {
    if let Some(copied) = copied {
        if let Some(clipboard) = world.resource_mut::<Clipboard>() {
            clipboard.text = copied.to_string();
            clipboard.dirty = true;
        } else {
            world.insert_resource(Clipboard {
                text: copied.to_string(),
                dirty: true,
            });
        }
    }
    if let Some(fired) = world.resource_mut::<Fired>() {
        if matches!(fired.name.as_deref(), Some(COPY | CUT | PASTE)) {
            fired.name = None;
        }
    }
}
use crate::caret::Caret;
use crate::field::TextField;
use crate::input::{Focus, Key};
use crate::style::Label;
use crate::world::World;

/// In-memory clipboard. Core never calls the operating system.
/// `dirty` is set when this frame wrote `text`, so a platform can push it out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clipboard {
    pub text: String,
    pub dirty: bool,
}

pub fn copy_binding() -> Binding {
    Binding::new(COPY, Chord::new(Key::Copy)).consume()
}

pub fn cut_binding() -> Binding {
    Binding::new(CUT, Chord::new(Key::Cut)).consume()
}

pub fn paste_binding() -> Binding {
    Binding::new(PASTE, Chord::new(Key::Paste)).consume()
}

/// Copy, cut, or paste the focused [`TextField`] selection from [`Fired`].
pub fn apply_clipboard(world: &mut World) {
    let name = world
        .resource::<Fired>()
        .and_then(|fired| fired.name.clone());
    let Some(name) = name else {
        return;
    };
    let entity = world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .filter(|entity| world.has::<TextField>(*entity));
    let Some(entity) = entity else {
        return;
    };
    let mut text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (index, anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let Some(edit) = edit_clipboard(&mut text, index, anchor, &name, &clip) else {
        return;
    };
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = edit.index;
        caret.anchor = edit.anchor;
    }
    if let Some(copied) = edit.copied {
        if let Some(clipboard) = world.resource_mut::<Clipboard>() {
            clipboard.text = copied;
            clipboard.dirty = true;
        }
    }
}

struct ClipEdit {
    index: usize,
    anchor: usize,
    copied: Option<String>,
}

fn edit_clipboard(
    text: &mut String,
    index: usize,
    anchor: usize,
    name: &str,
    clip: &str,
) -> Option<ClipEdit> {
    let start = anchor.min(index).min(text.len());
    let end = anchor.max(index).min(text.len());
    match name {
        COPY => Some(ClipEdit {
            index,
            anchor,
            copied: Some(text[start..end].to_string()),
        }),
        CUT => {
            let copied = text[start..end].to_string();
            text.replace_range(start..end, "");
            Some(ClipEdit {
                index: start,
                anchor: start,
                copied: Some(copied),
            })
        }
        PASTE => {
            text.replace_range(start..end, clip);
            let next = start + clip.len();
            Some(ClipEdit {
                index: next,
                anchor: next,
                copied: None,
            })
        }
        _ => None,
    }
}
