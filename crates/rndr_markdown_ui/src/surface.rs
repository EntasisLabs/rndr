//! Retained markdown surface.
//!
//! The session in [`crate::MarkdownDocuments`] owns the bytes, the tree, the
//! undo stack, and the journal. This surface is a lens: it paints blocks from
//! the tree, and every keystroke becomes a [`rndr_markdown::Command`]. It does
//! not call `attach_source`. [`crate::editing`](rndr_ui::editing) remains the
//! source-session mount.

use rndr_core::*;
use rndr_markdown::{Command, EditError, Session};
use rndr_ui::{UiError, UiId};

use crate::access::build_overlay;
use crate::document::MarkdownId;
use crate::intent;
use crate::project::{self, block_index, DisplayBlock, DisplayKind, Palette};

const OVERSCAN: f32 = 240.0;
const GAP: f32 = 8.0;

struct Node {
    id: UiId,
    document: MarkdownId,
    name: String,
    description: String,
    readonly: bool,
    blocks: Vec<DisplayBlock>,
    seen: u64,
    slots: Vec<Entity>,
    top: Entity,
    bottom: Entity,
    dragging: bool,
}

impl Component for Node {}

#[derive(Clone, Copy)]
struct Slot {
    start: usize,
    end: usize,
}

impl Component for Slot {}

/// Properties. The document id does not change for an existing mount.
#[derive(Clone, Debug)]
pub struct MarkdownSurfaceProps {
    /// Session opened in [`crate::MarkdownDocuments`].
    pub document: MarkdownId,
    /// Required accessible name. It is not the document text.
    pub name: String,
    /// Supplementary help.
    pub description: String,
    /// The surface refuses edits. The session itself has no readonly flag;
    /// this is the view's policy.
    pub readonly: bool,
}

impl MarkdownSurfaceProps {
    /// An editable surface with a persistent name.
    pub fn new(document: MarkdownId, name: impl Into<String>) -> Self {
        Self {
            document,
            name: name.into(),
            description: String::new(),
            readonly: false,
        }
    }
}

/// Why a mount or command was refused. The session is unchanged on failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownSurfaceError {
    /// Parent, name, or identity was not usable.
    Ui(UiError),
    /// The session refused the splice.
    Edit(EditError),
    /// The document id is not open.
    Missing,
}

impl From<UiError> for MarkdownSurfaceError {
    fn from(error: UiError) -> Self {
        Self::Ui(error)
    }
}

impl From<EditError> for MarkdownSurfaceError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

/// Layout root. The root is the focus target; block labels are paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownSurfaceHandle {
    /// Scrollport that owns the blocks.
    pub root: Entity,
}

/// Toolbar commands. Each one compiles to a session splice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownCommand {
    /// Toggle `**`.
    Strong,
    /// Toggle `*` or the delimiter already in the file.
    Emphasis,
    /// Toggle inline code.
    Code,
    /// Toggle `~~`.
    Strike,
    /// Toggle a link. `destination` is used when the selection is not a link yet.
    Link(String),
    /// Set the block to an ATX heading. Level 0 removes the marker.
    Heading(u8),
    /// Toggle a bullet item.
    Bullet,
    /// Toggle an ordered item.
    Ordered,
    /// Toggle a block quote prefix.
    Quote,
    /// Wrap the block in a fenced code block.
    CodeFence,
    /// Replace the block with a new Liquid fence of `kind`.
    Liquid {
        /// Canonical fence language.
        kind: String,
    },
    /// Replace a Liquid value span.
    SetField {
        /// Start of the value.
        start: usize,
        /// End of the value.
        end: usize,
        /// New value bytes. Quotes around the span stay.
        value: String,
    },
    /// Session undo.
    Undo,
    /// Session redo.
    Redo,
}

/// Mount and update entry point.
pub struct MarkdownSurface;

impl MarkdownSurface {
    /// Mount or update under a parent-scoped [`UiId`].
    ///
    /// The caret starts at the beginning of a newly mounted document. An
    /// update keeps the session caret, the scroll offset, and the history.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: MarkdownSurfaceProps,
    ) -> Result<MarkdownSurfaceHandle, MarkdownSurfaceError> {
        if !world.is_alive(parent) || !world.has::<Layout>(parent) {
            return Err(UiError::Stale.into());
        }
        if props.name.trim().is_empty() {
            return Err(UiError::InvalidProps.into());
        }
        if world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(props.document))
            .is_none()
        {
            return Err(MarkdownSurfaceError::Missing);
        }
        let existing = children_of(world, parent)
            .into_iter()
            .find(|entity| world.get::<Node>(*entity).is_some_and(|node| node.id == id));
        if let Some(root) = existing {
            let document = world.get::<Node>(root).map(|node| node.document);
            if document != Some(props.document) {
                return Err(UiError::IdentityConflict.into());
            }
            if let Some(node) = world.get_mut::<Node>(root) {
                node.name.clone_from(&props.name);
                node.description.clone_from(&props.description);
                node.readonly = props.readonly;
            }
            world.insert(
                root,
                Semantics {
                    name: Some(props.name),
                    description: props.description,
                    readonly: props.readonly,
                },
            );
            return Ok(MarkdownSurfaceHandle { root });
        }
        let root = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            Scroll::default(),
            Focusable,
            CapturesKeys,
            AccessMark(AccessRole::Group),
            ViewportRows(realize),
        ));
        let top = spacer(world, root);
        let bottom = spacer(world, root);
        world.insert(
            root,
            Node {
                id,
                document: props.document,
                name: props.name.clone(),
                description: props.description.clone(),
                readonly: props.readonly,
                blocks: Vec::new(),
                seen: u64::MAX,
                slots: Vec::new(),
                top,
                bottom,
                dragging: false,
            },
        );
        world.insert(
            root,
            Semantics {
                name: Some(props.name),
                description: props.description,
                readonly: props.readonly,
            },
        );
        set_parent(world, root, parent);
        if let Some(session) = world
            .resource_mut::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get_mut(props.document))
        {
            let _ = session.move_caret(0, 0);
        }
        Ok(MarkdownSurfaceHandle { root })
    }
}

fn spacer(world: &mut World, parent: Entity) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(0.0),
        ..Layout::column()
    },));
    set_parent(world, entity, parent);
    entity
}

impl MarkdownSurfaceHandle {
    /// The mount is still the same entities.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        let node = world.get::<Node>(self.root).ok_or(UiError::Stale)?;
        if !world.has::<Layout>(self.root) || !world.has::<Scroll>(self.root) {
            return Err(UiError::Stale);
        }
        if world.get::<Parent>(node.top).map(|parent| parent.entity) != Some(self.root) {
            return Err(UiError::Stale);
        }
        Ok(())
    }

    /// Run a toolbar command against the session caret.
    pub fn command(
        self,
        world: &mut World,
        command: MarkdownCommand,
    ) -> Result<(), MarkdownSurfaceError> {
        self.validate(world)?;
        let (document, readonly) = {
            let node = world.get::<Node>(self.root).ok_or(UiError::Stale)?;
            (node.document, node.readonly)
        };
        if readonly {
            return Err(EditError::Refused.into());
        }
        let now = now_ms(world);
        let blocks = world
            .get::<Node>(self.root)
            .map(|node| node.blocks.clone())
            .unwrap_or_default();
        let result = {
            let Some(session) = world
                .resource_mut::<crate::MarkdownDocuments>()
                .and_then(|docs| docs.get_mut(document))
            else {
                return Err(MarkdownSurfaceError::Missing);
            };
            apply_command(session, &blocks, command, now)
        };
        result
    }

    /// Drop the view. The session stays open.
    pub fn destroy(self, world: &mut World) -> Result<(), MarkdownSurfaceError> {
        self.validate(world)?;
        forget_interaction_targets(world, &[self.root]);
        despawn(world, self.root);
        Ok(())
    }
}

fn apply_command(
    session: &mut Session,
    blocks: &[DisplayBlock],
    command: MarkdownCommand,
    now: u64,
) -> Result<(), MarkdownSurfaceError> {
    if session.is_composing() && !matches!(command, MarkdownCommand::Undo | MarkdownCommand::Redo) {
        return Err(EditError::Composing.into());
    }
    let head = session.caret();
    let anchor = session.anchor();
    let index = block_index(blocks, head).unwrap_or(0);
    let block = blocks.get(index);
    let revision = session.revision();
    let restore_caret = !matches!(command, MarkdownCommand::Undo | MarkdownCommand::Redo);
    let result = match command {
        MarkdownCommand::Undo => session.undo(now).map(|_| ()),
        MarkdownCommand::Redo => session.redo(now).map(|_| ()),
        MarkdownCommand::Strong => session.edit(
            Command::ToggleStrong {
                start: anchor.min(head),
                end: anchor.max(head),
            },
            now,
        ),
        MarkdownCommand::Emphasis => session.edit(
            Command::ToggleEmphasis {
                start: anchor.min(head),
                end: anchor.max(head),
            },
            now,
        ),
        MarkdownCommand::Code => session.edit(
            Command::ToggleCode {
                start: anchor.min(head),
                end: anchor.max(head),
            },
            now,
        ),
        MarkdownCommand::Strike => session.edit(
            Command::ToggleStrike {
                start: anchor.min(head),
                end: anchor.max(head),
            },
            now,
        ),
        MarkdownCommand::Link(destination) => session.edit(
            Command::ToggleLink {
                start: anchor.min(head),
                end: anchor.max(head),
                destination,
            },
            now,
        ),
        MarkdownCommand::SetField { start, end, value } => {
            session.edit(Command::SetField { start, end, value }, now)
        }
        MarkdownCommand::Heading(level) => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::set_heading(block, level), now)
        }
        MarkdownCommand::Bullet => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::toggle_bullet(block), now)
        }
        MarkdownCommand::Ordered => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::toggle_ordered(block), now)
        }
        MarkdownCommand::Quote => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::toggle_quote(session.bytes(), block), now)
        }
        MarkdownCommand::CodeFence => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::set_code_fence(session.bytes(), block), now)
        }
        MarkdownCommand::Liquid { kind } => {
            let Some(block) = block else {
                return Ok(());
            };
            session.edit(intent::set_liquid(block, &kind, &[("body", "")]), now)
        }
    };
    if result.is_ok() && session.revision() != revision && restore_caret {
        if let Some(span) = session.last_edit() {
            let caret = span.new_end.min(session.bytes().len());
            session.note_caret(caret, caret);
        }
    }
    result.map_err(MarkdownSurfaceError::from)
}

pub(crate) fn root_document(world: &World, entity: Entity) -> Option<MarkdownId> {
    world.get::<Node>(entity).map(|node| node.document)
}

pub(crate) fn drive(world: &mut World) {
    if world.resource::<PointerDragOwner>().is_none() {
        world.insert_resource(PointerDragOwner(owns_press));
    }
    place_pointer(world);
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let roots: Vec<Entity> = world.entities_with::<Node>();
    let blurred = events::<WindowFocus>(world).contains(&WindowFocus::Blurred);
    for root in roots {
        let focused = focus == Some(root);
        if !focused || blurred {
            cancel_if_composing(world, root);
            continue;
        }
        edit_keys(world, root);
    }
}

pub(crate) fn owns_press(world: &World, hit: Entity) -> bool {
    let mut entity = hit;
    for _ in 0..48 {
        if world.has::<Node>(entity) {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.entity;
    }
    false
}

fn cancel_if_composing(world: &mut World, root: Entity) {
    let Some(document) = root_document(world, root) else {
        return;
    };
    if let Some(session) = world
        .resource_mut::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get_mut(document))
    {
        if session.is_composing() {
            session.composition_cancel();
        }
    }
}

fn place_pointer(world: &mut World) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    let pointer = input.pointer;
    let roots: Vec<Entity> = world.entities_with::<Node>();
    for root in roots {
        let dragging = world.get::<Node>(root).is_some_and(|node| node.dragging);
        if pointer.pressed && pointer.button == PointerButton::Left {
            if let Some(hit) = hit_test(world, pointer.position[0], pointer.position[1]) {
                if !owns_press(world, hit) {
                    continue;
                }
                if let Some(focus) = world.resource_mut::<Focus>() {
                    focus.entity = Some(root);
                }
                place_hit(world, root, hit, pointer.position, false);
                if let Some(node) = world.get_mut::<Node>(root) {
                    node.dragging = true;
                }
            }
        } else if pointer.down && dragging {
            if let Some(hit) = hit_test(world, pointer.position[0], pointer.position[1]) {
                if owns_press(world, hit) {
                    place_hit(world, root, hit, pointer.position, true);
                }
            }
        } else if !pointer.down {
            if let Some(node) = world.get_mut::<Node>(root) {
                node.dragging = false;
            }
        }
    }
}

fn place_hit(world: &mut World, root: Entity, hit: Entity, at: [f32; 2], extend: bool) {
    let Some(slot) = world.get::<Slot>(hit).copied() else {
        return;
    };
    let Some(rect) = world.get::<Rect>(hit).copied() else {
        return;
    };
    let (document, blocks) = {
        let Some(node) = world.get::<Node>(root) else {
            return;
        };
        (node.document, node.blocks.clone())
    };
    let Some(block) = blocks
        .iter()
        .find(|block| block.span.start == slot.start && block.span.end == slot.end)
    else {
        return;
    };
    let width = rect.w.max(1.0);
    let style = style_for_kind(&block.kind);
    let engine = text_engine(world);
    let label = block.label_text();
    let spans = label_spans(block);
    let layout = engine.layout(&label, &style, &spans, (width - 8.0).max(1.0));
    let index = layout.index_at(at[0] - rect.x, at[1] - rect.y);
    let source = block.display_to_source(index);
    if let Some(session) = world
        .resource_mut::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get_mut(document))
    {
        if extend {
            let anchor = session.anchor();
            let _ = session.move_caret(anchor, source);
        } else {
            let _ = session.move_caret(source, source);
        }
    }
}

fn edit_keys(world: &mut World, root: Entity) {
    let Some(input) = world.resource::<Input>() else {
        return;
    };
    let frame = text_frame(input);
    let platform = platform_of(world);
    let shift = input.shift;
    let readonly = world.get::<Node>(root).is_some_and(|node| node.readonly);
    let document = root_document(world, root);
    let Some(document) = document else {
        return;
    };
    let now = now_ms(world);
    handle_ime(world, root, document, &frame, now, readonly);
    if !frame.preedit.is_empty() {
        clear_keys(world);
        return;
    }
    for event in &frame.keys {
        if event.released || event.synthetic {
            continue;
        }
        if let Some(action) = standard_action(platform, event) {
            apply_action(world, root, document, action, now, readonly);
            continue;
        }
        if event.modifiers.primary(platform) && !event.modifiers.alt {
            if let Some(command) = mark_chord(event) {
                if !readonly {
                    let _ = MarkdownSurfaceHandle { root }.command(world, command);
                }
                continue;
            }
        }
        match event.logical {
            LogicalKey::Named(NamedKey::Enter) if !event.modifiers.command() => {
                if !readonly {
                    structural_key(world, root, document, now, KeyKind::Enter);
                }
            }
            LogicalKey::Named(NamedKey::Backspace) if !event.modifiers.command() => {
                if !readonly {
                    structural_key(world, root, document, now, KeyKind::Backspace);
                }
            }
            LogicalKey::Named(NamedKey::Delete) if !event.modifiers.command() => {
                if !readonly {
                    structural_key(world, root, document, now, KeyKind::Delete);
                }
            }
            LogicalKey::Named(named)
                if matches!(
                    named,
                    NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                        | NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::Home
                        | NamedKey::End
                ) =>
            {
                move_key(world, root, document, named, event.modifiers.shift || shift);
            }
            _ => {
                if readonly {
                    continue;
                }
                if let Some(text) = event.text.as_deref() {
                    if !text.is_empty() && !event.modifiers.command() {
                        type_str(world, document, text, now);
                    }
                }
            }
        }
    }
    clear_keys(world);
}

enum KeyKind {
    Enter,
    Backspace,
    Delete,
}

fn handle_ime(
    world: &mut World,
    root: Entity,
    document: MarkdownId,
    frame: &TextFrame,
    now: u64,
    readonly: bool,
) {
    let action = {
        let Some(session) = session_mut(world, document) else {
            return;
        };
        if readonly {
            session.composition_cancel();
            return;
        }
        if !frame.preedit.is_empty() {
            if !session.is_composing() {
                let start = session.anchor().min(session.caret());
                let end = session.anchor().max(session.caret());
                if session.composition_start(start, end).is_err() {
                    return;
                }
            }
            let _ = session.composition_update(&frame.preedit);
            ImeStep::Paint
        } else if session.is_composing() {
            if !frame.commit.is_empty() {
                let _ = session.composition_update(&frame.commit);
                if session.composition_commit(now).is_ok() {
                    if let Some(span) = session.last_edit() {
                        let caret = span.new_end.min(session.bytes().len());
                        session.note_caret(caret, caret);
                    }
                }
            } else {
                session.composition_cancel();
            }
            ImeStep::Clear
        } else if !frame.commit.is_empty() {
            type_in(session, &frame.commit, now);
            ImeStep::None
        } else {
            ImeStep::None
        }
    };
    match action {
        ImeStep::Paint => paint_preedit(world, root, &frame.preedit, frame_cursor(&frame.preedit)),
        ImeStep::Clear => clear_preedit(world, root),
        ImeStep::None => {}
    }
}

enum ImeStep {
    Paint,
    Clear,
    None,
}

fn frame_cursor(preedit: &str) -> Option<(usize, usize)> {
    let end = preedit.len();
    Some((end, end))
}

fn paint_preedit(world: &mut World, root: Entity, text: &str, cursor: Option<(usize, usize)>) {
    let slots = world
        .get::<Node>(root)
        .map(|node| node.slots.clone())
        .unwrap_or_default();
    for slot in slots {
        if world.has::<Caret>(slot) {
            world.insert(
                slot,
                ImePreview {
                    text: text.to_string(),
                    cursor,
                },
            );
        }
    }
}

fn clear_preedit(world: &mut World, root: Entity) {
    let slots = world
        .get::<Node>(root)
        .map(|node| node.slots.clone())
        .unwrap_or_default();
    for slot in slots {
        world.remove::<ImePreview>(slot);
    }
}

fn mark_chord(event: &KeyEvent) -> Option<MarkdownCommand> {
    let letter = match &event.logical {
        LogicalKey::Character(ch) => Some(ch.to_ascii_lowercase()),
        _ => None,
    };
    let letter = letter.or_else(|| {
        event.code.and_then(|code| match code {
            KeyCode::Letter(ch) => Some(ch.to_ascii_lowercase()),
            _ => None,
        })
    })?;
    if event.modifiers.shift {
        return None;
    }
    match letter {
        'b' => Some(MarkdownCommand::Strong),
        'i' => Some(MarkdownCommand::Emphasis),
        'e' => Some(MarkdownCommand::Code),
        _ => None,
    }
}

fn apply_action(
    world: &mut World,
    root: Entity,
    document: MarkdownId,
    action: EditAction,
    now: u64,
    readonly: bool,
) {
    match action {
        EditAction::Undo if !readonly => {
            if let Some(session) = session_mut(world, document) {
                let _ = session.undo(now);
            }
        }
        EditAction::Redo if !readonly => {
            if let Some(session) = session_mut(world, document) {
                let _ = session.redo(now);
            }
        }
        EditAction::SelectAll => {
            if let Some(session) = session_mut(world, document) {
                let end = session.bytes().len();
                let _ = session.move_caret(0, end);
            }
        }
        EditAction::Copy => copy_selection(world, document, false, now, readonly),
        EditAction::Cut if !readonly => copy_selection(world, document, true, now, false),
        EditAction::Paste if !readonly => paste(world, document, now),
        EditAction::WordLeft | EditAction::WordRight => {
            let forward = action == EditAction::WordRight;
            word_move(world, root, document, forward);
        }
        _ => {}
    }
}

fn copy_selection(world: &mut World, document: MarkdownId, cut: bool, now: u64, readonly: bool) {
    let text = {
        let Some(session) = session_mut(world, document) else {
            return;
        };
        let start = session.anchor().min(session.caret());
        let end = session.anchor().max(session.caret());
        if start == end {
            return;
        }
        let text = session.bytes().get(start..end).unwrap_or("").to_string();
        if cut && !readonly {
            let _ = session.edit(Command::Delete { start, end }, now);
            let caret = start.min(session.bytes().len());
            session.note_caret(caret, caret);
        }
        text
    };
    if let Some(clip) = world.resource_mut::<Clipboard>() {
        clip.text = text;
        clip.html.clear();
        clip.html_text.clear();
        clip.dirty = true;
    }
}

fn paste(world: &mut World, document: MarkdownId, now: u64) {
    let text = world
        .resource::<Clipboard>()
        .map(|clip| clip.text.clone())
        .unwrap_or_default();
    if text.is_empty() {
        return;
    }
    let Some(session) = session_mut(world, document) else {
        return;
    };
    let start = session.anchor().min(session.caret());
    let end = session.anchor().max(session.caret());
    let revision = session.revision();
    if session
        .edit(
            Command::Replace {
                start,
                end,
                text: text.clone(),
            },
            now,
        )
        .is_ok()
        && session.revision() != revision
    {
        let caret = start + text.len();
        let caret = caret.min(session.bytes().len());
        session.note_caret(caret, caret);
    }
}

fn type_str(world: &mut World, document: MarkdownId, text: &str, now: u64) {
    if let Some(session) = session_mut(world, document) {
        type_in(session, text, now);
    }
}

fn type_in(session: &mut Session, text: &str, now: u64) {
    let start = session.anchor().min(session.caret());
    let end = session.anchor().max(session.caret());
    if start != end {
        let revision = session.revision();
        if session.edit(Command::Delete { start, end }, now).is_err() {
            return;
        }
        if session.revision() != revision {
            session.note_caret(
                start.min(session.bytes().len()),
                start.min(session.bytes().len()),
            );
        }
    }
    let at = session.caret();
    let revision = session.revision();
    if session
        .edit(
            Command::Insert {
                at,
                text: text.to_string(),
            },
            now,
        )
        .is_ok()
        && session.revision() != revision
    {
        if let Some(span) = session.last_edit() {
            let caret = span.new_end.min(session.bytes().len());
            session.note_caret(caret, caret);
        }
    }
}

fn structural_key(world: &mut World, root: Entity, document: MarkdownId, now: u64, kind: KeyKind) {
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let Some(session) = session_mut(world, document) else {
        return;
    };
    let head = session.caret();
    let anchor = session.anchor();
    let Some(index) = block_index(&blocks, head) else {
        return;
    };
    let block = &blocks[index];
    let command = match kind {
        KeyKind::Enter => intent::split(session.bytes(), block, head),
        KeyKind::Backspace => intent::backspace(session.bytes(), block, anchor, head),
        KeyKind::Delete => intent::delete_forward(block, anchor, head),
    };
    let revision = session.revision();
    if session.edit(command, now).is_ok() && session.revision() != revision {
        if let Some(span) = session.last_edit() {
            let caret = match kind {
                KeyKind::Backspace | KeyKind::Delete => span.old_start.min(session.bytes().len()),
                KeyKind::Enter => span.new_end.min(session.bytes().len()),
            };
            session.note_caret(caret, caret);
        }
    }
}

fn move_key(world: &mut World, root: Entity, document: MarkdownId, key: NamedKey, shift: bool) {
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let Some(session) = session_mut(world, document) else {
        return;
    };
    let head = session.caret();
    let anchor = if shift { session.anchor() } else { head };
    let Some(index) = block_index(&blocks, head) else {
        let _ = session.move_caret(anchor, head);
        return;
    };
    let block = &blocks[index];
    let source = match key {
        NamedKey::ArrowLeft => intent::move_by(block, head, false),
        NamedKey::ArrowRight => intent::move_by(block, head, true),
        NamedKey::ArrowUp => neighbor_block(&blocks, index, head, false),
        NamedKey::ArrowDown => neighbor_block(&blocks, index, head, true),
        NamedKey::Home => block.display_to_source(block.chrome.len()),
        NamedKey::End => block.display_to_source(block.label_text().len()),
        _ => head,
    };
    let anchor = if shift { anchor } else { source };
    let _ = session.move_caret(anchor, source);
}

fn neighbor_block(blocks: &[DisplayBlock], index: usize, head: usize, forward: bool) -> usize {
    let step = if forward { 1isize } else { -1 };
    let mut cursor = index as isize + step;
    while cursor >= 0 && (cursor as usize) < blocks.len() {
        let block = &blocks[cursor as usize];
        if block.kind != DisplayKind::Gap {
            return if forward {
                block.display_to_source(block.chrome.len())
            } else {
                block.display_to_source(block.label_text().len())
            };
        }
        cursor += step;
    }
    head
}

fn word_move(world: &mut World, _root: Entity, document: MarkdownId, forward: bool) {
    let Some(session) = session_mut(world, document) else {
        return;
    };
    let bytes = session.bytes().to_string();
    let mut at = session.caret();
    let anchor = session.anchor();
    if forward {
        while at < bytes.len()
            && bytes[at..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
        {
            at += bytes[at..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }
        while at < bytes.len()
            && bytes[at..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
        {
            at += bytes[at..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }
    } else {
        while at > 0 {
            let prev = prev_char(&bytes, at);
            if bytes[prev..at]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
            {
                at = prev;
            } else {
                break;
            }
        }
        while at > 0 {
            let prev = prev_char(&bytes, at);
            if bytes[prev..at]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
            {
                at = prev;
            } else {
                break;
            }
        }
    }
    let _ = session.move_caret(anchor, at);
}

fn prev_char(text: &str, index: usize) -> usize {
    let mut at = index.saturating_sub(1);
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn session_mut(world: &mut World, document: MarkdownId) -> Option<&mut Session> {
    world
        .resource_mut::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get_mut(document))
}

fn clear_keys(world: &mut World) {
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.ime_commit.clear();
        input.key = None;
    }
}

fn now_ms(world: &World) -> u64 {
    world
        .resource::<Time>()
        .map(|time| u64::try_from(time.now.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

pub(crate) fn poll(world: &mut World) {
    let now = now_ms(world);
    let Some(docs) = world.resource_mut::<crate::MarkdownDocuments>() else {
        return;
    };
    for (_, session) in docs.iter_mut() {
        let _ = session.poll(now);
    }
}

fn realize(world: &mut World, entity: Entity) -> bool {
    let Some(document) = root_document(world, entity) else {
        return false;
    };
    let (bytes, tree_edit) = {
        let Some(session) = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
        else {
            return false;
        };
        (session.bytes().to_string(), session.revision())
    };
    // The green tree is borrowed for the projection, then dropped before mutation.
    let projected = {
        let Some(session) = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
        else {
            return false;
        };
        let palette = Palette::default();
        let seen = world
            .get::<Node>(entity)
            .map(|node| node.seen)
            .unwrap_or(u64::MAX);
        let mut blocks = world
            .get::<Node>(entity)
            .map(|node| node.blocks.clone())
            .unwrap_or_default();
        if seen != session.revision() {
            if seen != u64::MAX && session.revision() == seen.saturating_add(1) {
                if let Some(span) = session.last_edit() {
                    project::update_projection(
                        &mut blocks,
                        session.bytes(),
                        session.tree(),
                        span,
                        &palette,
                    );
                } else {
                    blocks = project::project(session.bytes(), session.tree(), &palette);
                }
            } else {
                blocks = project::project(session.bytes(), session.tree(), &palette);
            }
        }
        let _ = bytes;
        let _ = tree_edit;
        blocks
    };
    let seen = world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
        .map(|session| session.revision());
    if let Some(node) = world.get_mut::<Node>(entity) {
        node.blocks = projected;
        if let Some(seen) = seen {
            node.seen = seen;
        }
    }
    let changed = rebuild_slots(world, entity);
    publish_access(world, entity);
    changed
}

fn rebuild_slots(world: &mut World, entity: Entity) -> bool {
    let rect = world.get::<Rect>(entity).copied().unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 640.0,
        h: 480.0,
    });
    let scroll = world
        .get::<Scroll>(entity)
        .map(|scroll| scroll.offset[1])
        .unwrap_or(0.0);
    let width = if rect.w > 1.0 { rect.w } else { 640.0 };
    let view = if rect.h > 1.0 { rect.h } else { 480.0 };
    let top_y = -scroll;
    let bottom_y = top_y + view + OVERSCAN;
    let start_y = (top_y - OVERSCAN).max(0.0);
    let document = root_document(world, entity);
    let (caret, anchor, preedit) = document
        .and_then(|id| world.resource::<crate::MarkdownDocuments>()?.get(id))
        .map(|session| {
            (
                session.caret(),
                session.anchor(),
                session.preedit().unwrap_or("").to_string(),
            )
        })
        .unwrap_or((0, 0, String::new()));
    let engine = text_engine(world);
    let mut y = 0.0;
    let mut visible: Vec<usize> = Vec::new();
    let block_count = world
        .get::<Node>(entity)
        .map(|node| node.blocks.len())
        .unwrap_or(0);
    for index in 0..block_count {
        let kind = world
            .get::<Node>(entity)
            .and_then(|node| node.blocks.get(index).map(|b| b.kind.clone()));
        let Some(kind) = kind else {
            continue;
        };
        let mut height = world
            .get::<Node>(entity)
            .and_then(|node| node.blocks.get(index).map(|block| block.height))
            .unwrap_or(0.0);
        if height <= 0.0 {
            if let Some(block) = world
                .get::<Node>(entity)
                .and_then(|node| node.blocks.get(index).cloned())
            {
                let style = style_for_kind(&kind);
                height = project::measure_block(
                    engine.as_ref(),
                    &block,
                    (width - 24.0).max(1.0),
                    &style,
                );
                if let Some(node) = world.get_mut::<Node>(entity) {
                    if let Some(block) = node.blocks.get_mut(index) {
                        block.height = height;
                    }
                }
            }
        }
        let bottom = y + height + GAP;
        if bottom >= start_y && y <= bottom_y && kind != DisplayKind::Gap {
            visible.push(index);
        }
        if kind == DisplayKind::Gap {
            // Gaps still occupy space. They are not slots.
        }
        y = bottom;
    }
    let content_h = y;
    let first_y = y_of(world, entity, visible.first().copied());
    let rest = (content_h - y_after(world, entity, visible.last().copied())).max(0.0);
    if let Some(node) = world.get::<Node>(entity) {
        let top = node.top;
        let bottom = node.bottom;
        if let Some(layout) = world.get_mut::<Layout>(top) {
            layout.height = Size::Px(first_y);
        }
        if let Some(layout) = world.get_mut::<Layout>(bottom) {
            layout.height = Size::Px(rest);
        }
    }
    // Rebuild slot entities to match `visible`.
    let old = world
        .get::<Node>(entity)
        .map(|node| node.slots.clone())
        .unwrap_or_default();
    for slot in &old {
        despawn(world, *slot);
    }
    let mut slots = Vec::new();
    for index in visible {
        let Some(block) = world
            .get::<Node>(entity)
            .and_then(|node| node.blocks.get(index).cloned())
        else {
            continue;
        };
        let slot = spawn_block(world, entity, &block, caret, anchor, &preedit, width);
        slots.push(slot);
    }
    // Slots sit between the spacers.
    if let Some(node) = world
        .get::<Node>(entity)
        .map(|node| (node.top, node.bottom))
    {
        if let Some(children) = world.get_mut::<Children>(entity) {
            children
                .list
                .retain(|child| *child == node.0 || *child == node.1);
            let bottom_at = children
                .list
                .iter()
                .position(|child| *child == node.1)
                .unwrap_or(children.list.len());
            for (offset, slot) in slots.iter().enumerate() {
                children.list.insert(bottom_at + offset, *slot);
            }
        }
    }
    if let Some(node) = world.get_mut::<Node>(entity) {
        node.slots = slots;
    }
    true
}

fn y_of(world: &World, entity: Entity, index: Option<usize>) -> f32 {
    let Some(index) = index else {
        return 0.0;
    };
    let Some(node) = world.get::<Node>(entity) else {
        return 0.0;
    };
    let mut y = 0.0;
    for block in node.blocks.iter().take(index) {
        y += block.height.max(if block.kind == DisplayKind::Gap {
            8.0
        } else {
            20.0
        }) + GAP;
    }
    y
}

fn y_after(world: &World, entity: Entity, index: Option<usize>) -> f32 {
    let Some(index) = index else {
        return 0.0;
    };
    y_of(world, entity, Some(index + 1))
}

fn spawn_block(
    world: &mut World,
    parent: Entity,
    block: &DisplayBlock,
    caret: usize,
    anchor: usize,
    preedit: &str,
    width: f32,
) -> Entity {
    let text = block.label_text();
    let mut label = Label::new(&text);
    label.overflow = TextOverflow::Wrap;
    let pad =
        12.0 + f32::from(block.quote) * 14.0 + f32::from(block.depth.saturating_sub(1)) * 18.0;
    let height = block.height.max(20.0);
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(height + GAP),
            padding: pad,
            ..Layout::column()
        },
        label,
        Spans {
            runs: label_spans(block),
        },
        style_for_kind(&block.kind),
        Slot {
            start: block.span.start,
            end: block.span.end,
        },
    ));
    let contains =
        block.span.start <= caret && caret <= block.span.end && block.kind != DisplayKind::Gap;
    if contains {
        let index = block.source_to_label(caret);
        world.insert(
            entity,
            Caret {
                index,
                anchor: block.source_to_label(anchor),
            },
        );
        if !preedit.is_empty() {
            world.insert(
                entity,
                ImePreview {
                    text: preedit.to_string(),
                    cursor: Some((preedit.len(), preedit.len())),
                },
            );
        }
    }
    if anchor != caret && block.kind != DisplayKind::Gap {
        let start = anchor.min(caret);
        let end = anchor.max(caret);
        if block.span.end > start && block.span.start < end {
            world.insert(
                entity,
                TextSelection {
                    anchor: block.source_to_label(start.max(block.span.start)),
                    head: block.source_to_label(end.min(block.span.end)),
                },
            );
        }
    }
    let _ = width;
    set_parent(world, entity, parent);
    entity
}

fn label_spans(block: &DisplayBlock) -> Vec<TextSpan> {
    let chrome = block.chrome.len();
    block
        .spans
        .iter()
        .cloned()
        .map(|mut span| {
            span.start += chrome;
            span.end += chrome;
            span
        })
        .collect()
}

fn style_for_kind(kind: &DisplayKind) -> TextStyle {
    let mut style = TextStyle::default();
    match kind {
        DisplayKind::Heading(level) => {
            style.size = match level {
                1 => 28.0,
                2 => 24.0,
                3 => 20.0,
                _ => 18.0,
            };
            style.weight = FONT_WEIGHT_BOLD;
        }
        DisplayKind::Code { .. }
        | DisplayKind::Opaque
        | DisplayKind::Liquid { valid: false, .. } => {
            style.family = "monospace".into();
            style.size = 14.0;
        }
        _ => style.size = 16.0,
    }
    style
}

fn publish_access(world: &mut World, entity: Entity) {
    let bounds: Vec<_> = world
        .get::<Node>(entity)
        .map(|node| node.slots.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|slot| {
            let range = world.get::<Slot>(slot).map(|slot| slot.start..slot.end)?;
            let rect = world.get::<Rect>(slot).copied().unwrap_or_default();
            Some((range, rect))
        })
        .collect();
    let clip = world.get::<Rect>(entity).copied().unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    });
    let document = root_document(world, entity);
    let Some(document) = document else {
        return;
    };
    let nodes = world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
        .map(|session| build_overlay(session.bytes(), session.tree(), &bounds, clip))
        .unwrap_or_default();
    world.insert(entity, AccessOverlay { nodes });
}

/// Document id of a mounted surface.
pub fn document_of(world: &World, root: Entity) -> Option<MarkdownId> {
    root_document(world, root)
}
