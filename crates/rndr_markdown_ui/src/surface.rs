//! Retained markdown surface.
//!
//! The session in [`crate::MarkdownDocuments`] owns the bytes, the tree, the
//! undo stack, and the journal. This surface is a lens: it paints blocks from
//! the tree, and every keystroke becomes a [`rndr_markdown::Command`]. It does
//! not call `attach_source`. [`crate::editing`](rndr_ui::editing) remains the
//! source-session mount.

use rndr_core::*;
use rndr_liquid::parse_fence;
use rndr_liquid_ui::{release_editor_block, render_editor_block};
use rndr_markdown::{Command, EditError, Session};
use rndr_ui::{Token, UiError, UiId, UiTheme};

use crate::access::build_overlay;
use crate::document::MarkdownId;
use crate::intent;
use crate::project::{self, block_index, DisplayBlock, DisplayKind, Palette};

const OVERSCAN: f32 = 240.0;
const GAP: f32 = 8.0;
/// Air above the first block and below the last, so a heading or a table row
/// is not flush with the scroll clip.
const LEAD: f32 = 10.0;
const TASK_BOX: f32 = 14.0;
const TASK_GAP: f32 = 8.0;

struct Node {
    id: UiId,
    document: MarkdownId,
    name: String,
    description: String,
    readonly: bool,
    blocks: Vec<DisplayBlock>,
    seen: u64,
    /// Revision the slots were built for. A matching revision reuses a widget
    /// so a nested markdown body can sync on the following frame.
    painted: u64,
    slots: Vec<Entity>,
    top: Entity,
    bottom: Entity,
    dragging: bool,
    /// Theme colors baked into the projected spans.
    theme: u64,
    /// Caret the scrollport last brought into view. A later caret move reveals
    /// again; a wheel scroll with the same caret does not jump back.
    revealed: Option<usize>,
}

impl Component for Node {}

#[derive(Clone, Copy)]
struct Slot {
    start: usize,
    end: usize,
}

impl Component for Slot {}

/// Drawn task checkbox. The font's ballot box is a hairline; this is the box.
struct TaskBox;

impl Component for TaskBox {}

/// The task item's text, beside the checkbox. The chrome glyph is not in it.
struct TaskText;

impl Component for TaskText {}

/// One editable Liquid field value. `text_start` indexes [`DisplayBlock::text`].
#[derive(Clone, Copy)]
struct FieldValue {
    text_start: usize,
    text_len: usize,
}

impl Component for FieldValue {}

/// Key label for a field. A press puts the caret at the value.
#[derive(Clone, Copy)]
struct FieldKey {
    text_start: usize,
}

impl Component for FieldKey {}

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
                painted: u64::MAX,
                slots: Vec::new(),
                top,
                bottom,
                dragging: false,
                theme: 0,
                revealed: None,
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
    if let Some(document) = root_document(world, root) {
        if crate::grid::is_handle(world, hit) {
            if !extend {
                let _ = crate::grid::open_for(world, root, document, hit, at);
            }
            return;
        }
        if let Some(source) = crate::grid::source_at(world, hit, at) {
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
            return;
        }
    }
    let Some(hit) = slot_of(world, hit) else {
        return;
    };
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
    let Some(stored) = blocks
        .iter()
        .find(|block| block.span.start == slot.start && block.span.end == slot.end)
    else {
        return;
    };
    if stored.grid.is_some() {
        return;
    }
    let readonly = world.get::<Node>(root).is_some_and(|node| node.readonly);
    let block = presented(world, document, stored);
    if !extend && !readonly && task_box_hit(world, hit) {
        if let Some(command) = intent::toggle_task(document_bytes(world, document).as_str(), &block)
        {
            let now = now_ms(world);
            if let Some(session) = session_mut(world, document) {
                let _ = session.edit(command, now);
            }
            return;
        }
    }
    if let Some(source) = task_text_source(world, hit, &block, at) {
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
        return;
    }
    if let Some(source) = field_source(world, hit, &block, at) {
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
        return;
    }
    let width = rect.w.max(1.0);
    let style = style_for_kind(&block.kind, world);
    let engine = text_engine(world);
    let label = block.label_text();
    let spans = label_spans(&block);
    let layout = engine.layout(&label, &style, &spans, (width - 8.0).max(1.0));
    let index = layout.index_at(at[0] - rect.x, at[1] - rect.y);
    if !extend
        && !readonly
        && matches!(block.kind, DisplayKind::Task(_))
        && index < block.chrome.len()
    {
        if let Some(command) = intent::toggle_task(document_bytes(world, document).as_str(), &block)
        {
            let now = now_ms(world);
            if let Some(session) = session_mut(world, document) {
                let _ = session.edit(command, now);
            }
            return;
        }
    }
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

fn document_bytes_of_parent(world: &World, parent: Entity) -> String {
    root_document(world, parent)
        .map(|document| document_bytes(world, document))
        .unwrap_or_default()
}

fn document_bytes(world: &World, document: MarkdownId) -> String {
    world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
        .map(|session| session.bytes().to_string())
        .unwrap_or_default()
}

fn presented(world: &World, document: MarkdownId, block: &DisplayBlock) -> DisplayBlock {
    let Some(session) = world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
    else {
        return block.clone();
    };
    if matches!(block.kind, DisplayKind::Table) {
        let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
        let chrome = crate::grid::Chrome::from_theme(&theme);
        return project::show(
            block,
            session.bytes(),
            session.tree(),
            session.caret(),
            &crate::grid::palette_of(&chrome),
        );
    }
    project::present(
        block,
        session.bytes(),
        session.tree(),
        session.caret(),
        &theme_palette(world),
    )
}

fn theme_palette(world: &World) -> Palette {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    Palette::from_theme(&theme)
}

fn theme_stamp(world: &World) -> u64 {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let text = theme.color(Token::Text);
    let muted = theme.color(Token::Secondary);
    let link = theme.color(Token::Accent);
    let pack = |color: Color, shift: u32| -> u64 {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u64;
        (channel(color.r) | (channel(color.g) << 8) | (channel(color.b) << 16)) << shift
    };
    pack(text, 0)
        ^ (theme.reading_scale.to_bits() as u64).rotate_left(11)
        ^ (theme.text_scale.to_bits() as u64).rotate_left(31)
        ^ (u64::from(theme.density == rndr_ui::Density::Compact) << 61)
        ^ pack(muted, 24)
        ^ pack(link, 40)
        ^ match theme.appearance {
            rndr_ui::Appearance::Light => 0,
            rndr_ui::Appearance::Dark => 1 << 62,
        }
        ^ match theme.contrast {
            rndr_ui::Contrast::Standard => 0,
            rndr_ui::Contrast::High => 1 << 63,
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
            LogicalKey::Named(NamedKey::Tab)
                if !event.modifiers.alt && !event.modifiers.command() =>
            {
                if !readonly {
                    table_tab(world, root, document, event.modifiers.shift || shift, now);
                }
            }
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
                        type_str(world, root, document, text, now);
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
    let in_table = caret_in_table(world, root, document);
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
            if !frame.commit.is_empty() && in_table {
                session.composition_cancel();
                ImeStep::Type(frame.commit.clone())
            } else if !frame.commit.is_empty() {
                let _ = session.composition_update(&frame.commit);
                if session.composition_commit(now).is_ok() {
                    if let Some(span) = session.last_edit() {
                        let caret = span.new_end.min(session.bytes().len());
                        session.note_caret(caret, caret);
                    }
                }
                ImeStep::Clear
            } else {
                session.composition_cancel();
                ImeStep::Clear
            }
        } else if !frame.commit.is_empty() {
            if in_table {
                ImeStep::Type(frame.commit.clone())
            } else {
                type_in(session, &frame.commit, now);
                ImeStep::None
            }
        } else {
            ImeStep::None
        }
    };
    match action {
        ImeStep::Paint => paint_preedit(world, root, &frame.preedit, frame_cursor(&frame.preedit)),
        ImeStep::Clear => clear_preedit(world, root),
        ImeStep::Type(text) => {
            clear_preedit(world, root);
            if !table_type(world, root, document, &text, now) {
                if let Some(session) = session_mut(world, document) {
                    type_in(session, &text, now);
                }
            }
        }
        ImeStep::None => {}
    }
}

fn caret_in_table(world: &World, root: Entity, document: MarkdownId) -> bool {
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let Some(session) = world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
    else {
        return false;
    };
    let head = session.caret();
    let Some(index) = block_index(&blocks, head) else {
        return false;
    };
    presented(world, document, &blocks[index]).grid.is_some()
}

enum ImeStep {
    Paint,
    Clear,
    Type(String),
    None,
}

fn frame_cursor(preedit: &str) -> Option<(usize, usize)> {
    let end = preedit.len();
    Some((end, end))
}

fn paint_preedit(world: &mut World, root: Entity, text: &str, cursor: Option<(usize, usize)>) {
    for slot in caret_entities(world, root) {
        world.insert(
            slot,
            ImePreview {
                text: text.to_string(),
                cursor,
            },
        );
    }
}

fn clear_preedit(world: &mut World, root: Entity) {
    for slot in caret_entities(world, root) {
        world.remove::<ImePreview>(slot);
    }
}

fn caret_entities(world: &World, root: Entity) -> Vec<Entity> {
    let slots = world
        .get::<Node>(root)
        .map(|node| node.slots.clone())
        .unwrap_or_default();
    let mut out = Vec::new();
    for slot in slots {
        collect_carets(world, slot, &mut out);
    }
    out
}

fn collect_carets(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if world.has::<Caret>(entity) {
        out.push(entity);
    }
    for child in children_of(world, entity) {
        collect_carets(world, child, out);
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
        EditAction::Copy => copy_selection(world, root, document, false, now, readonly),
        EditAction::Cut if !readonly => copy_selection(world, root, document, true, now, false),
        EditAction::Paste if !readonly => paste(world, root, document, now),
        EditAction::WordLeft | EditAction::WordRight => {
            let forward = action == EditAction::WordRight;
            word_move(world, root, document, forward);
        }
        _ => {}
    }
}

fn copy_selection(
    world: &mut World,
    root: Entity,
    document: MarkdownId,
    cut: bool,
    now: u64,
    readonly: bool,
) {
    let text = {
        let Some(session) = session_mut(world, document) else {
            return;
        };
        let start = session.anchor().min(session.caret());
        let end = session.anchor().max(session.caret());
        if start == end {
            return;
        }
        session.bytes().get(start..end).unwrap_or("").to_string()
    };
    if cut && !readonly && !table_type(world, root, document, "", now) {
        if let Some(session) = session_mut(world, document) {
            let start = session.anchor().min(session.caret());
            let end = session.anchor().max(session.caret());
            let _ = session.edit(Command::Delete { start, end }, now);
            let caret = start.min(session.bytes().len());
            session.note_caret(caret, caret);
        }
    }
    if let Some(clip) = world.resource_mut::<Clipboard>() {
        clip.text = text;
        clip.html.clear();
        clip.html_text.clear();
        clip.dirty = true;
    }
}

fn paste(world: &mut World, root: Entity, document: MarkdownId, now: u64) {
    let text = world
        .resource::<Clipboard>()
        .map(|clip| clip.text.clone())
        .unwrap_or_default();
    if text.is_empty() {
        return;
    }
    if table_type(world, root, document, &text, now) {
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

fn type_str(world: &mut World, root: Entity, document: MarkdownId, text: &str, now: u64) {
    if table_type(world, root, document, text, now) {
        return;
    }
    if let Some(session) = session_mut(world, document) {
        type_in(session, text, now);
    }
}

fn table_type(world: &mut World, root: Entity, document: MarkdownId, text: &str, now: u64) -> bool {
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let (head, anchor) = {
        let Some(session) = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
        else {
            return false;
        };
        (session.caret(), session.anchor())
    };
    let Some(index) = block_index(&blocks, head).or_else(|| block_index(&blocks, anchor)) else {
        return false;
    };
    let block = presented(world, document, &blocks[index]);
    if block.grid.is_none() {
        return false;
    }
    let bytes = document_bytes(world, document);
    let Some(plans) = crate::grid::plan_type(&block, &bytes, anchor, head, text) else {
        return false;
    };
    if let Some(session) = session_mut(world, document) {
        let _ = crate::grid::apply_plans(session, &plans, now);
    }
    true
}

fn table_tab(world: &mut World, root: Entity, document: MarkdownId, shift: bool, now: u64) {
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let head = world
        .resource::<crate::MarkdownDocuments>()
        .and_then(|docs| docs.get(document))
        .map(|session| session.caret())
        .unwrap_or(0);
    let Some(index) = block_index(&blocks, head) else {
        return;
    };
    let block = presented(world, document, &blocks[index]);
    let Some(tab) = crate::grid::tab(&block, head, shift) else {
        return;
    };
    let Some(session) = session_mut(world, document) else {
        return;
    };
    match tab {
        crate::grid::Tab::Move(at) => {
            let _ = session.move_caret(at, at);
        }
        crate::grid::Tab::Append { at, index } => {
            let revision = session.revision();
            if session
                .edit(rndr_markdown::Command::InsertTableRow { at, index }, now)
                .is_ok()
                && session.revision() != revision
            {
                if let Some(caret) = crate::grid::caret_after_append(session, at, index) {
                    session.note_caret(caret, caret);
                }
            }
        }
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
    let (head, anchor) = {
        let Some(session) = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
        else {
            return;
        };
        (session.caret(), session.anchor())
    };
    let Some(index) = block_index(&blocks, head) else {
        return;
    };
    let block = presented(world, document, &blocks[index]);
    let bytes = document_bytes(world, document);
    if block.grid.is_some() {
        match kind {
            KeyKind::Enter => {
                if let Some(next) = crate::grid::enter(&block, head) {
                    if let Some(session) = session_mut(world, document) {
                        let _ = session.move_caret(next, next);
                    }
                }
                return;
            }
            KeyKind::Backspace | KeyKind::Delete => {
                let back = matches!(kind, KeyKind::Backspace);
                if let Some(plans) = crate::grid::plan_delete(&block, &bytes, anchor, head, back) {
                    if let Some(session) = session_mut(world, document) {
                        let _ = crate::grid::apply_plans(session, &plans, now);
                    }
                }
                return;
            }
        }
    }
    let command = match kind {
        KeyKind::Enter => intent::split(&bytes, &block, head),
        KeyKind::Backspace => intent::backspace(&bytes, &block, anchor, head),
        KeyKind::Delete => intent::delete_forward(&block, anchor, head),
    };
    let Some(session) = session_mut(world, document) else {
        return;
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
    let (head, stored_anchor) = {
        let Some(session) = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
        else {
            return;
        };
        (session.caret(), session.anchor())
    };
    let anchor = if shift { stored_anchor } else { head };
    let Some(index) = block_index(&blocks, head) else {
        if let Some(session) = session_mut(world, document) {
            let _ = session.move_caret(anchor, head);
        }
        return;
    };
    let block = presented(world, document, &blocks[index]);
    if let Some(motion) = crate::grid::move_key(&block, head, anchor, key, shift) {
        if let Some(session) = session_mut(world, document) {
            match motion {
                crate::grid::Motion::Inside(anchor, source) => {
                    let _ = session.move_caret(anchor, source);
                }
                crate::grid::Motion::Leave { forward } => {
                    let source = neighbor_block(&blocks, index, head, forward);
                    let anchor = if shift { anchor } else { source };
                    let _ = session.move_caret(anchor, source);
                }
            }
        }
        return;
    }
    let source = match key {
        NamedKey::ArrowLeft => intent::move_by(&block, head, false),
        NamedKey::ArrowRight => intent::move_by(&block, head, true),
        NamedKey::ArrowUp => neighbor_block(&blocks, index, head, false),
        NamedKey::ArrowDown => neighbor_block(&blocks, index, head, true),
        NamedKey::Home => block.display_to_source(block.chrome.len()),
        NamedKey::End => block.display_to_source(block.label_text().len()),
        _ => head,
    };
    let anchor = if shift { anchor } else { source };
    if let Some(session) = session_mut(world, document) {
        let _ = session.move_caret(anchor, source);
    }
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

fn word_move(world: &mut World, root: Entity, document: MarkdownId, forward: bool) {
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
    let head = session.caret();
    let blocks = world
        .get::<Node>(root)
        .map(|node| node.blocks.clone())
        .unwrap_or_default();
    let at = block_index(&blocks, head)
        .map(|index| presented(world, document, &blocks[index]))
        .map(|block| crate::grid::clamp_word(&block, head, at))
        .unwrap_or(at);
    if let Some(session) = session_mut(world, document) {
        let _ = session.move_caret(anchor, at);
    }
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
    let roots: Vec<Entity> = world.entities_with::<Node>();
    for root in roots {
        if let Some(document) = root_document(world, root) {
            let caret = world
                .resource::<crate::MarkdownDocuments>()
                .and_then(|docs| docs.get(document))
                .map(|session| session.caret())
                .unwrap_or(0);
            crate::grid::sync_handles(world, root, caret);
            crate::grid::on_update(world, root, document, now);
        }
    }
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
        let palette = theme_palette(world);
        let stamp = theme_stamp(world);
        let theme_changed = world
            .get::<Node>(entity)
            .is_some_and(|node| node.theme != stamp);
        let seen = if theme_changed {
            u64::MAX
        } else {
            world
                .get::<Node>(entity)
                .map(|node| node.seen)
                .unwrap_or(u64::MAX)
        };
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
    let stamp = theme_stamp(world);
    let theme_changed = world
        .get::<Node>(entity)
        .is_some_and(|node| node.theme != stamp);
    if let Some(node) = world.get_mut::<Node>(entity) {
        node.blocks = projected;
        node.theme = stamp;
        if theme_changed {
            node.painted = u64::MAX;
        }
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
    let mut scroll = world
        .get::<Scroll>(entity)
        .map(|scroll| scroll.offset[1])
        .unwrap_or(0.0);
    let width = if rect.w > 1.0 { rect.w } else { 640.0 };
    let view = if rect.h > 1.0 { rect.h } else { 480.0 };
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
    let mut y = LEAD;
    let mut placed: Vec<(usize, f32, f32)> = Vec::new();
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
        if height <= 0.0 || matches!(kind, DisplayKind::Table) {
            if let Some(block) = world
                .get::<Node>(entity)
                .and_then(|node| node.blocks.get(index).cloned())
            {
                let style = style_for_kind(&kind, world);
                let document = root_document(world, entity);
                let block = document
                    .map(|document| presented(world, document, &block))
                    .unwrap_or(block);
                height = if let Some(grid) = block.grid.as_ref() {
                    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
                    let chrome = crate::grid::Chrome::from_theme(&theme);
                    crate::grid::height_for(
                        engine.as_ref(),
                        grid,
                        width,
                        &style,
                        &chrome,
                        crate::grid::outer_pad(&block),
                    )
                } else {
                    let measured = project::measure_block(
                        engine.as_ref(),
                        &block,
                        (width - 24.0).max(1.0),
                        &style,
                    );
                    // The field form's padding is outside the row stack. Count it
                    // in the stored height so the slot is not a one-line kind bar.
                    if matches!(kind, DisplayKind::Liquid { valid: true, .. }) {
                        measured + crate::grid::outer_pad(&block) * 2.0
                    } else {
                        measured
                    }
                };
                if let Some(node) = world.get_mut::<Node>(entity) {
                    if let Some(block) = node.blocks.get_mut(index) {
                        block.height = height;
                    }
                }
            }
        }
        let bottom = y + advance(&kind, height);
        placed.push((index, y, bottom));
        y = bottom;
    }
    let content_h = y + LEAD;
    // A caret move reveals in this same layout pass, before place and paint.
    // The host's next frame then draws the block inside the viewport.
    let already = world.get::<Node>(entity).and_then(|node| node.revealed);
    if already != Some(caret) {
        if let Some((_, top, bottom)) = placed.iter().copied().find(|(index, _, _)| {
            world.get::<Node>(entity).is_some_and(|node| {
                node.blocks.get(*index).is_some_and(|block| {
                    block.kind != DisplayKind::Gap
                        && block.span.start <= caret
                        && caret <= block.span.end
                })
            })
        }) {
            let view_top = -scroll;
            let view_bottom = view_top + view;
            let block_h = bottom - top;
            if block_h > view {
                if top >= view_bottom || bottom <= view_top {
                    scroll = -top;
                }
            } else if top < view_top {
                scroll = -top;
            } else if bottom > view_bottom {
                scroll = view - bottom;
            }
            let min_y = (view - content_h).min(0.0);
            scroll = scroll.clamp(min_y, 0.0);
        }
        if let Some(node) = world.get_mut::<Node>(entity) {
            node.revealed = Some(caret);
        }
        if let Some(port) = world.get_mut::<Scroll>(entity) {
            port.offset[1] = scroll;
        }
    }
    let top_y = -scroll;
    let bottom_y = top_y + view + OVERSCAN;
    // Rows above the viewport would be the first label a focus reveal sees,
    // and that reveal would walk the offset back toward the top every frame.
    let start_y = top_y.max(0.0);
    let visible: Vec<usize> = placed
        .into_iter()
        .filter(|(index, y, bottom)| {
            *bottom >= start_y
                && *y <= bottom_y
                && world.get::<Node>(entity).is_some_and(|node| {
                    node.blocks
                        .get(*index)
                        .is_some_and(|block| block.kind != DisplayKind::Gap)
                })
        })
        .map(|(index, _, _)| index)
        .collect();
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
    // Rebuild slot entities to match `visible`. A widget is kept while the
    // bytes and the caret mode are unchanged, so its nested markdown can sync.
    let revision = root_document(world, entity)
        .and_then(|id| {
            world
                .resource::<crate::MarkdownDocuments>()?
                .get(id)
                .map(|session| session.revision())
        })
        .unwrap_or(0);
    let reuse_ok = world
        .get::<Node>(entity)
        .is_some_and(|node| node.painted == revision);
    let old = world
        .get::<Node>(entity)
        .map(|node| node.slots.clone())
        .unwrap_or_default();
    let mut used = vec![false; old.len()];
    let mut slots = Vec::new();
    for index in visible {
        let Some(block) = world
            .get::<Node>(entity)
            .and_then(|node| node.blocks.get(index).cloned())
        else {
            continue;
        };
        let document = root_document(world, entity);
        let block = document
            .map(|document| presented(world, document, &block))
            .unwrap_or(block);
        let widget = shows_widget(&block, caret);
        let reused = reuse_ok
            .then(|| {
                old.iter().enumerate().find_map(|(pos, slot)| {
                    (!used[pos] && slot_reusable(world, *slot, &block, widget))
                        .then_some((pos, *slot))
                })
            })
            .flatten();
        if let Some((pos, slot)) = reused {
            used[pos] = true;
            if !widget {
                place_caret(world, slot, &block, caret, anchor, &preedit);
            }
            slots.push(slot);
        } else {
            slots.push(spawn_block(
                world, entity, &block, caret, anchor, &preedit, width,
            ));
        }
    }
    for (pos, slot) in old.iter().enumerate() {
        if !used[pos] {
            release_editor_block(world, *slot);
            despawn(world, *slot);
        }
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
        node.painted = revision;
    }
    // Handles are spawned in this pass. Color them before paint; an earlier
    // update would be wiped by the rebuild.
    if let Some(document) = root_document(world, entity) {
        let caret = world
            .resource::<crate::MarkdownDocuments>()
            .and_then(|docs| docs.get(document))
            .map(|session| session.caret())
            .unwrap_or(0);
        crate::grid::sync_handles(world, entity, caret);
    }
    // The scrollport is the focus target. Revealing its first row fights a
    // caret that was just brought into view.
    if focus_is_inside(world, entity) {
        pause_focus_reveal(world);
    }
    true
}

fn focus_is_inside(world: &World, root: Entity) -> bool {
    let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return false;
    };
    let mut at = Some(focus);
    while let Some(entity) = at {
        if entity == root {
            return true;
        }
        at = world.get::<Parent>(entity).map(|parent| parent.entity);
    }
    false
}

fn shows_widget(block: &DisplayBlock, caret: usize) -> bool {
    !liquid_editing(block, caret) && matches!(block.kind, DisplayKind::Liquid { valid: true, .. })
}

fn slot_reusable(world: &World, slot: Entity, block: &DisplayBlock, widget: bool) -> bool {
    let Some(existing) = world.get::<Slot>(slot) else {
        return false;
    };
    if existing.start != block.span.start || existing.end != block.span.end {
        return false;
    }
    // A widget needs the next frame to sync. Keep it while the caret stays
    // outside. A field form and a callout rebuild when that mode changes.
    if matches!(block.kind, DisplayKind::Callout { .. }) {
        let rendered = !block.text.contains("[!");
        return rendered == world.has::<Color>(slot);
    }
    if matches!(block.kind, DisplayKind::Liquid { valid: true, .. }) {
        let form = !field_value_entities(world, slot).is_empty();
        return form != widget;
    }
    let labeled = world.has::<Label>(slot) || task_text_entity(world, slot).is_some();
    labeled != widget
}

fn advance(kind: &DisplayKind, height: f32) -> f32 {
    let body = if height > 0.0 {
        height
    } else if matches!(kind, DisplayKind::Gap) {
        8.0
    } else {
        20.0
    };
    if matches!(kind, DisplayKind::Bullet | DisplayKind::Ordered(_)) {
        return body + TEXT_ORIGIN_Y;
    }
    let slack = match kind {
        DisplayKind::Gap | DisplayKind::Table => 0.0,
        // The stored height already includes the form padding.
        DisplayKind::Liquid { valid: true, .. } => 0.0,
        _ => TEXT_ORIGIN_Y * 2.0,
    };
    body + slack + GAP
}

fn y_of(world: &World, entity: Entity, index: Option<usize>) -> f32 {
    let Some(index) = index else {
        return LEAD;
    };
    let Some(node) = world.get::<Node>(entity) else {
        return LEAD;
    };
    let mut y = LEAD;
    for block in node.blocks.iter().take(index) {
        y += advance(&block.kind, block.height);
    }
    y
}

fn y_after(world: &World, entity: Entity, index: Option<usize>) -> f32 {
    let Some(index) = index else {
        return 0.0;
    };
    y_of(world, entity, Some(index + 1))
}

fn slot_of(world: &World, mut hit: Entity) -> Option<Entity> {
    for _ in 0..64 {
        if world.has::<Slot>(hit) {
            return Some(hit);
        }
        hit = world.get::<Parent>(hit)?.entity;
    }
    None
}

/// The caret is editing this fence when it sits in the body. The fence lines
/// themselves stay on the widget, so a caret parked on the opening fence still
/// shows the rendered block.
fn liquid_editing(block: &DisplayBlock, caret: usize) -> bool {
    match &block.kind {
        DisplayKind::Liquid {
            valid: true, body, ..
        } => body.start <= caret && caret < body.end,
        _ => false,
    }
}

fn liquid_widget(
    world: &World,
    parent: Entity,
    block: &DisplayBlock,
) -> Option<rndr_liquid::LiquidBlock> {
    let DisplayKind::Liquid {
        canonical,
        valid: true,
        body,
    } = &block.kind
    else {
        return None;
    };
    let document = world.get::<Node>(parent)?.document;
    let bytes = world
        .resource::<crate::MarkdownDocuments>()?
        .get(document)?
        .bytes();
    let raw = bytes.get(body.clone())?.trim_end_matches(['\n', '\r']);
    parse_fence(canonical, raw)
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
    let pad =
        12.0 + f32::from(block.quote) * 14.0 + f32::from(block.depth.saturating_sub(1)) * 18.0;
    if block.grid.is_some() {
        let bytes = document_bytes_of_parent(world, parent);
        let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
        let chrome = crate::grid::Chrome::from_theme(&theme);
        let entity = crate::grid::spawn(
            world,
            parent,
            crate::grid::Paint {
                block,
                bytes: &bytes,
                caret,
                anchor,
                preedit,
                width,
                chrome: &chrome,
            },
        );
        world.insert(
            entity,
            Slot {
                start: block.span.start,
                end: block.span.end,
            },
        );
        return entity;
    }
    if !liquid_editing(block, caret) {
        if let Some(parsed) = liquid_widget(world, parent, block) {
            let entity = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    padding: pad,
                    gap: 8.0,
                    ..Layout::column()
                },
                Slot {
                    start: block.span.start,
                    end: block.span.end,
                },
            ));
            render_editor_block(world, entity, &parsed);
            set_parent(world, entity, parent);
            let _ = width;
            return entity;
        }
    }
    if liquid_editing(block, caret) {
        return spawn_liquid_fields(world, parent, block, caret, anchor, preedit, pad);
    }
    let task = matches!(block.kind, DisplayKind::Task(_));
    let text = if task {
        block.text.clone()
    } else {
        block.label_text()
    };
    let mut label = Label::new(&text);
    label.color = Some(theme_palette(world).text);
    label.overflow = TextOverflow::Wrap;
    let height = block.height.max(20.0);
    let runs = if task {
        block.spans.clone()
    } else {
        label_spans(block)
    };
    let mut layout = Layout {
        width: Size::Fill,
        height: Size::Px(advance(&block.kind, height)),
        padding: pad,
        ..Layout::column()
    };
    if task {
        layout.axis = Axis::Row;
        layout.gap = TASK_GAP;
    }
    let entity = world.spawn((
        layout,
        style_for_kind(&block.kind, world),
        Slot {
            start: block.span.start,
            end: block.span.end,
        },
    ));
    if task {
        world.insert(
            entity,
            Align {
                items: Items::Center,
                ..Align::default()
            },
        );
        paint_task_box(world, entity, block);
        let text_entity = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                ..Layout::column()
            },
            label,
            Spans { runs },
            style_for_kind(&block.kind, world),
            LabelBox::CONTENT,
            TaskText,
        ));
        // A caret would otherwise switch this label to the legacy 12-point
        // origin, so the first letters draw under the box.
        world.insert(text_entity, RetainLabelBox);
        set_parent(world, text_entity, entity);
    } else {
        world.insert(entity, label);
        world.insert(entity, Spans { runs });
    }
    paint_callout(world, entity, block);
    place_caret(world, entity, block, caret, anchor, preedit);
    let _ = width;
    set_parent(world, entity, parent);
    entity
}

fn task_text_entity(world: &World, slot: Entity) -> Option<Entity> {
    children_of(world, slot)
        .into_iter()
        .find(|child| world.has::<TaskText>(*child))
}

fn task_box_hit(world: &World, mut hit: Entity) -> bool {
    for _ in 0..16 {
        if world.has::<TaskBox>(hit) {
            return true;
        }
        let Some(parent) = world.get::<Parent>(hit) else {
            return false;
        };
        if world.has::<Slot>(hit) {
            return false;
        }
        hit = parent.entity;
    }
    false
}

fn task_text_source(
    world: &World,
    mut hit: Entity,
    block: &DisplayBlock,
    at: [f32; 2],
) -> Option<usize> {
    if !matches!(block.kind, DisplayKind::Task(_)) {
        return None;
    }
    for _ in 0..8 {
        if world.has::<TaskText>(hit) {
            let rect = world.get::<Rect>(hit).copied()?;
            let text = world.get::<Label>(hit).map(|label| label.text.clone())?;
            let style = world
                .get::<TextStyle>(hit)
                .cloned()
                .unwrap_or_else(|| style_for_kind(&block.kind, world));
            let layout = text_engine(world).layout(&text, &style, &[], rect.w.max(1.0));
            let index = layout.index_at(at[0] - rect.x, at[1] - rect.y);
            return Some(block.display_to_source(block.chrome.len() + index));
        }
        if world.has::<Slot>(hit) {
            return None;
        }
        hit = world.get::<Parent>(hit)?.entity;
    }
    None
}

fn field_source(
    world: &World,
    mut hit: Entity,
    block: &DisplayBlock,
    at: [f32; 2],
) -> Option<usize> {
    for _ in 0..16 {
        if let Some(key) = world.get::<FieldKey>(hit).copied() {
            return Some(block.display_to_source(block.chrome.len() + key.text_start));
        }
        if let Some(value) = world.get::<FieldValue>(hit).copied() {
            let rect = world.get::<Rect>(hit).copied()?;
            let text = world.get::<Label>(hit).map(|label| label.text.clone())?;
            let style = world
                .get::<TextStyle>(hit)
                .cloned()
                .unwrap_or_else(TextStyle::default);
            let layout = text_engine(world).layout(&text, &style, &[], rect.w.max(1.0));
            let index = layout
                .index_at(at[0] - rect.x, at[1] - rect.y)
                .min(value.text_len);
            return Some(block.display_to_source(block.chrome.len() + value.text_start + index));
        }
        if world.has::<Slot>(hit) {
            return None;
        }
        hit = world.get::<Parent>(hit)?.entity;
    }
    None
}

fn place_caret(
    world: &mut World,
    entity: Entity,
    block: &DisplayBlock,
    caret: usize,
    anchor: usize,
    preedit: &str,
) {
    clear_carets(world, entity);
    if place_field_carets(world, entity, block, caret, anchor, preedit) {
        return;
    }
    if let Some(text_entity) = task_text_entity(world, entity) {
        place_task_caret(world, text_entity, block, caret, anchor, preedit);
        return;
    }
    world.remove::<Caret>(entity);
    world.remove::<TextSelection>(entity);
    world.remove::<ImePreview>(entity);
    let contains =
        block.span.start <= caret && caret <= block.span.end && block.kind != DisplayKind::Gap;
    if contains {
        world.insert(
            entity,
            Caret {
                index: block.source_to_label(caret),
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
}

fn paint_task_box(world: &mut World, parent: Entity, block: &DisplayBlock) {
    let DisplayKind::Task(checked) = block.kind else {
        return;
    };
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let mut fill = theme.color(Token::Content);
    if checked {
        fill = theme.color(Token::Accent);
    }
    let checkbox = world.spawn((
        Layout {
            width: Size::Px(TASK_BOX),
            height: Size::Px(TASK_BOX),
            ..Layout::column()
        },
        fill,
        Border {
            color: theme.color(Token::StrongStroke),
            width: 1.5,
        },
        Radius(3.0),
        TaskBox,
    ));
    set_parent(world, checkbox, parent);
    if checked {
        let mark = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            theme.color(Token::OnAccent),
        ));
        set_parent(world, mark, checkbox);
        if let Some(layout) = world.get_mut::<Layout>(checkbox) {
            layout.padding = 4.0;
        }
    }
}

fn paint_callout(world: &mut World, entity: Entity, block: &DisplayBlock) {
    let DisplayKind::Callout { tone } = &block.kind else {
        return;
    };

    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let (fill, accent, ink) = callout_tone(&theme, tone);
    world.insert(entity, fill);
    world.insert(entity, Radius(theme.radius(rndr_ui::RadiusRole::Card)));
    world.insert(
        entity,
        Border {
            color: accent,
            width: 1.,
        },
    );
    let bar = world.spawn((
        Layout {
            width: Size::Px(2.0),
            height: Size::Fill,
            ..Layout::column()
        },
        Absolute {
            left: Some(0.0),
            top: Some(0.0),
            bottom: Some(0.0),
            ..Absolute::default()
        },
        accent,
    ));
    set_parent(world, bar, entity);
    if block.chrome.is_empty() {
        return;
    }
    if let Some(spans) = world.get_mut::<Spans>(entity) {
        let mut header = TextSpan::new(0, block.chrome.len(), ink);
        header.weight = Some(FONT_WEIGHT_BOLD);
        spans.runs.insert(0, header);
    }
}

fn callout_tone(theme: &UiTheme, tone: &str) -> (Color, Color, Color) {
    theme.markdown_theme().callouts[rndr_core::CalloutTone::from_marker(tone) as usize]
}

struct FieldLine {
    key: String,
    value: String,
    value_at: usize,
    value_len: usize,
    keyed: bool,
}

fn field_lines(block: &DisplayBlock) -> Vec<FieldLine> {
    let mut lines = Vec::new();
    let mut at = 0usize;
    for line in block.text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let key_len = block
            .spans
            .iter()
            .find(|span| span.start == start && span.weight == Some(FONT_WEIGHT_BOLD))
            .map(|span| span.end.saturating_sub(span.start))
            .unwrap_or(0);
        if key_len == 0 || key_len > trimmed.len() {
            lines.push(FieldLine {
                key: String::new(),
                value: trimmed.to_string(),
                value_at: start,
                value_len: trimmed.len(),
                keyed: false,
            });
            continue;
        }
        let key = trimmed[..key_len].to_string();
        let mut value_at = start + key_len;
        if block.text[value_at..].starts_with("  ") {
            value_at += 2;
        }
        let value_end = start + trimmed.len();
        let value_len = value_end.saturating_sub(value_at);
        let value = block
            .text
            .get(value_at..value_at + value_len)
            .unwrap_or("")
            .to_string();
        lines.push(FieldLine {
            key,
            value,
            value_at,
            value_len,
            keyed: true,
        });
    }
    lines
}

fn spawn_liquid_fields(
    world: &mut World,
    parent: Entity,
    block: &DisplayBlock,
    caret: usize,
    anchor: usize,
    preedit: &str,
    pad: f32,
) -> Entity {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let palette = theme_palette(world);
    let content = if block.height > pad * 2.0 + 24.0 {
        block.height
    } else {
        let style = style_for_kind(&block.kind, world);
        project::measure_block(
            text_engine(world).as_ref(),
            block,
            (world
                .get::<Rect>(parent)
                .map(|rect| rect.w)
                .unwrap_or(640.0)
                - pad * 2.0)
                .max(1.0),
            &style,
        ) + pad * 2.0
    };
    let height = advance(&block.kind, content);
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(height),
            padding: pad,
            gap: 4.0,
            ..Layout::column()
        },
        theme.color(Token::Raised),
        Radius(6.0),
        Slot {
            start: block.span.start,
            end: block.span.end,
        },
    ));
    let kind = block.chrome.trim();
    if !kind.is_empty() {
        let style = TextStyle {
            size: 13.0,
            weight: FONT_WEIGHT_BOLD,
            ..TextStyle::default()
        };
        let mut label = Label::new(kind).with_color(palette.text);
        label.overflow = TextOverflow::Ellipsis;
        let header = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                ..Layout::column()
            },
            label,
            style,
        ));
        set_parent(world, header, entity);
    }
    let style = style_for_kind(&block.kind, world);
    for line in field_lines(block) {
        let row = world.spawn((Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.0,
            ..Layout::row()
        },));
        set_parent(world, row, entity);
        if line.keyed {
            let mut key = Label::new(&line.key).with_color(palette.muted);
            key.overflow = TextOverflow::Ellipsis;
            let key_entity = world.spawn((
                Layout {
                    width: Size::Px(96.0),
                    height: Size::Hug,
                    ..Layout::column()
                },
                key,
                style.clone(),
                FieldKey {
                    text_start: line.value_at,
                },
            ));
            set_parent(world, key_entity, row);
        }
        let mut value = Label::new(&line.value).with_color(palette.text);
        value.overflow = TextOverflow::Wrap;
        let value_entity = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                ..Layout::column()
            },
            value,
            style.clone(),
            FieldValue {
                text_start: line.value_at,
                text_len: line.value_len,
            },
        ));
        set_parent(world, value_entity, row);
    }
    place_caret(world, entity, block, caret, anchor, preedit);
    set_parent(world, entity, parent);
    entity
}

fn place_task_caret(
    world: &mut World,
    entity: Entity,
    block: &DisplayBlock,
    caret: usize,
    anchor: usize,
    preedit: &str,
) {
    let contains =
        block.span.start <= caret && caret <= block.span.end && block.kind != DisplayKind::Gap;
    if contains {
        let index = block
            .source_to_label(caret)
            .saturating_sub(block.chrome.len());
        let anchor_index = block
            .source_to_label(anchor)
            .saturating_sub(block.chrome.len());
        world.insert(
            entity,
            Caret {
                index,
                anchor: anchor_index,
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
                    anchor: block
                        .source_to_label(start.max(block.span.start))
                        .saturating_sub(block.chrome.len()),
                    head: block
                        .source_to_label(end.min(block.span.end))
                        .saturating_sub(block.chrome.len()),
                },
            );
        }
    }
}

fn clear_carets(world: &mut World, entity: Entity) {
    let mut stack = vec![entity];
    while let Some(current) = stack.pop() {
        world.remove::<Caret>(current);
        world.remove::<TextSelection>(current);
        world.remove::<ImePreview>(current);
        stack.extend(children_of(world, current));
    }
}

fn place_field_carets(
    world: &mut World,
    entity: Entity,
    block: &DisplayBlock,
    caret: usize,
    anchor: usize,
    preedit: &str,
) -> bool {
    let values = field_value_entities(world, entity);
    if values.is_empty() {
        return false;
    }
    let text_at = |source: usize| {
        block
            .source_to_label(source)
            .saturating_sub(block.chrome.len())
    };
    let head = text_at(caret);
    let anchor_at = text_at(anchor);
    let (sel_from, sel_to) = (head.min(anchor_at), head.max(anchor_at));
    for value_entity in values {
        let Some(value) = world.get::<FieldValue>(value_entity).copied() else {
            continue;
        };
        let end = value.text_start + value.text_len;
        if head >= value.text_start && head <= end {
            world.insert(
                value_entity,
                Caret {
                    index: head - value.text_start,
                    anchor: anchor_at.clamp(value.text_start, end) - value.text_start,
                },
            );
            if !preedit.is_empty() {
                world.insert(
                    value_entity,
                    ImePreview {
                        text: preedit.to_string(),
                        cursor: Some((preedit.len(), preedit.len())),
                    },
                );
            }
        }
        if anchor != caret && sel_to > value.text_start && sel_from < end {
            world.insert(
                value_entity,
                TextSelection {
                    anchor: sel_from.clamp(value.text_start, end) - value.text_start,
                    head: sel_to.clamp(value.text_start, end) - value.text_start,
                },
            );
        }
    }
    true
}

fn field_value_entities(world: &World, entity: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![entity];
    while let Some(current) = stack.pop() {
        if world.has::<FieldValue>(current) {
            out.push(current);
        }
        stack.extend(children_of(world, current));
    }
    out
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

fn style_for_kind(kind: &DisplayKind, world: &World) -> TextStyle {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let mut style = theme.typography(rndr_ui::Typography::Reading);
    match kind {
        DisplayKind::Heading(level) => {
            style = rndr_core::markdown_heading_style(&style, *level);
        }
        DisplayKind::Code { .. }
        | DisplayKind::Opaque
        | DisplayKind::Liquid { valid: false, .. } => {
            style = theme.typography(rndr_ui::Typography::Code);
        }
        _ => {
            // Preserve the token's 14/22 reading metrics.
        }
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
