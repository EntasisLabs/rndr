//! Markdown text that can grow while tokens arrive.
//!
//! [`parse_markdown`] turns a string into blocks with [`pulldown_cmark`].
//! [`markdown_view`] is a scrollport that turns those blocks into entities:
//! a wrapping label for a paragraph, heading, or list item, a monospace label
//! on a distinct fill for a fenced block, a bar beside a quote, and a thin
//! rect for a rule. Inline bold, italic, code, and links are [`TextSpan`]s.
//! A link click sends [`LinkActivated`] during Input. The view does not open
//! the destination.
//!
//! # Streaming
//!
//! [`MarkdownView::push_str`] appends to the source. On the next
//! [`MarkdownPlugin`] sync the view parses again from a stable boundary and
//! writes only the blocks whose text or style changed. A boundary is the byte
//! after two blank lines outside a fence, or the start of an unclosed fence.
//! An unclosed fence stays inside that tail, so earlier blocks keep their
//! entities and their shaped lines. A trailing inline that is still open
//! (bold, italic, a code span, or a link whose destination has not closed)
//! is closed for the parse. The frame shows the styled text. Raw markers
//! are not drawn, and the source still holds only the characters that arrived.
//!
//! A link reference definition (`[id]: url`) disables the boundary: the whole
//! string is parsed, and unchanged blocks still keep their entities.
//!
//! # Scroll
//!
//! The view starts pinned to the bottom. [`MarkdownPlugin`] registers
//! [`crate::apply_scroll`]. While the view is pinned, sync sets the vertical
//! offset to the bottom after layout clamps it. A wheel or a drag that leaves
//! the bottom clears the pin. Returning to the bottom sets it again.
//! [`MarkdownView::set_follow`] turns the pin off. A drag that starts inside
//! the view, including the padding gutter, selects text and puts the offset
//! back, so the same drag does not scroll.
//!
//! # Selection
//!
//! The range lives on the view. Sync paints it with a [`TextSelection`] on
//! each selected block label: one highlight, and no caret. Primary+A selects
//! the document and publishes the X11/Wayland primary selection.
//! Primary+C, or [`Key::Copy`](crate::Key::Copy), writes
//! [`selected_text`](MarkdownView::selected_text) to
//! [`Clipboard`](crate::Clipboard). Releasing a drag publishes the same
//! string as the primary selection. Copy writes markdown that round-trips
//! when the selection covers whole blocks: heading marks, list markers,
//! quote prefixes, rules, and fences stay, and nested list indents parse
//! back as the same nesting. Bold, italic, and inline code keep their
//! markers. A link fully inside the selection is copied with its
//! destination. A block that is only partly selected is plain text, except
//! that an inline span lying entirely inside the selection still keeps its
//! markers. Strikethrough is copied as plain text. Blocks that are not
//! tight list items are separated by a blank line. Tight items share one
//! newline.
//!
//! Italic is stored on the span. The desktop shaper slants that run. The
//! builtin bitmap face has no italic outlines, so those glyphs stay upright.
//!
//! The optional `highlight` feature colors fenced code by token class. Without
//! it, a code block is one color.

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::app::{App, Plugin, Schedule};
use crate::caret::TextSelection;
use crate::clipboard::{note_clipboard, note_primary, primary_selection};
use crate::entity::Entity;
use crate::event::{self, Event as EventTrait};
use crate::font::{field_text_width, Spans, TextSpan};
use crate::hierarchy::{children_of, despawn, set_parent};
use crate::input::{CursorStyle, Input, MotionWatch, PointerButton};
use crate::keys::{platform_of, standard_action, EditAction, Key};
use crate::layout::{hit_test, Axis, Layout, Rect, Scroll, Size, SCROLL_DRAG_SLOP};
use crate::style::{Color, Label, TextOverflow, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::text::{
    style_of, text_engine, FontStyle, TextStyle, FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR,
};
use crate::theme::{theme_of, Radius, Theme};
use crate::world::{Component, World};

impl Component for MarkdownView {}

const PAD: f32 = 16.0;
const BLOCK_GAP: f32 = 12.0;
const TIGHT_GAP: f32 = 4.0;
const ROW_GAP: f32 = 8.0;
const BAR_W: f32 = 3.0;
const QUOTE_STEP: f32 = 12.0;
const BULLET: f32 = 6.0;
const CHECK: f32 = 14.0;
const CHECK_PAD: f32 = 2.0;
const CODE_FAMILY: &str = "monospace";

fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// One styled range of a [`MarkdownBlock::text`].
///
/// `start` and `end` are byte offsets into that block's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLink {
    /// First byte of the link label.
    pub start: usize,
    /// Byte after the link label.
    pub end: usize,
    /// Destination. The view does not open it.
    pub href: String,
}

/// Kind of one block in reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownKind {
    /// A wrapping paragraph.
    Paragraph,
    /// An ATX or setext heading. The level is `1..=6`.
    Heading(u8),
    /// A fenced or indented code block. The text is the body.
    Code,
    /// A bullet item. [`MarkdownBlock::text`] is the body, without the marker.
    Bullet,
    /// A numbered item. The text is the body. The number is this value.
    Numbered(u64),
    /// A task item. `true` means checked. The text is the body, without the box.
    Task(bool),
    /// A thematic break. The text is empty. Copy writes `---`.
    Rule,
}

/// One block of a parsed document.
///
/// [`MarkdownView::blocks`] is this list after the plugin has synced.
/// [`parse_markdown`] returns it directly and uses [`MarkdownTheme::default`].
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownBlock {
    /// Paragraph, heading, code, list item, or rule.
    pub kind: MarkdownKind,
    /// List nesting. `0` is not a list. A top-level item is `1`.
    pub depth: u8,
    /// Quote nesting. `0` is not a quote.
    pub quote: u8,
    /// `true` when this list item contains no paragraph.
    ///
    /// Two adjacent tight items share a small gap and a single newline when
    /// copied. A loose item, and every block that is not a list item, is
    /// `false` and keeps the paragraph gap.
    pub tight: bool,
    /// Visible text. List markers are not included. A rule is empty.
    pub text: String,
    /// Fence info string. Empty for every block that is not fenced code.
    pub language: String,
    /// Inline ranges. Empty means the whole block uses the label color.
    pub spans: Vec<TextSpan>,
    /// Clickable ranges inside [`Self::text`].
    pub links: Vec<MarkdownLink>,
}

/// Colors for a markdown view.
///
/// Absent from the world, the plugin builds one with [`Self::from_theme`].
/// [`parse_markdown`] uses [`Self::default`] so a test can compare colors
/// without a world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkdownTheme {
    /// Scrollport fill, and the fill behind ordinary text.
    pub surface: Color,
    /// Body and heading glyphs.
    pub text: Color,
    /// List markers and strikethrough.
    pub muted: Color,
    /// Link glyphs.
    pub link: Color,
    /// Code glyphs.
    pub code: Color,
    /// Fenced block fill.
    pub code_bg: Color,
    /// Inline code highlight.
    pub inline_bg: Color,
    /// Quote bar.
    pub quote: Color,
    /// Thematic break.
    pub rule: Color,
    /// Keyword tokens. Used when the `highlight` feature is on.
    pub keyword: Color,
    /// String tokens. Used when the `highlight` feature is on.
    pub string: Color,
    /// Number tokens. Used when the `highlight` feature is on.
    pub number: Color,
    /// Comment tokens. Used when the `highlight` feature is on.
    pub comment: Color,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            surface: Color::rgb(0.10, 0.11, 0.13),
            text: Color::rgb(0.93, 0.94, 0.96),
            muted: Color::rgb(0.62, 0.66, 0.72),
            link: Color::rgb(0.45, 0.68, 0.95),
            code: Color::rgb(0.86, 0.89, 0.93),
            code_bg: Color::rgb(0.16, 0.17, 0.21),
            inline_bg: Color::rgb(0.20, 0.24, 0.30),
            quote: Color::rgb(0.45, 0.55, 0.68),
            rule: Color::rgb(0.30, 0.33, 0.38),
            keyword: Color::rgb(0.55, 0.62, 0.95),
            string: Color::rgb(0.62, 0.80, 0.55),
            number: Color::rgb(0.86, 0.70, 0.42),
            comment: Color::rgb(0.55, 0.58, 0.62),
        }
    }
}

impl MarkdownTheme {
    /// Pane, text, body, caret, and seam from `theme`, with fixed accents.
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            surface: theme.pane,
            text: theme.text,
            code: theme.text,
            code_bg: theme.body,
            inline_bg: theme.body,
            quote: theme.caret,
            rule: theme.seam,
            ..Self::default()
        }
    }
}

/// A click on a link label. Sent during Input and readable in Update.
///
/// The view does not open [`Self::href`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkActivated {
    /// View that contains the link.
    pub view: Entity,
    /// Destination from the parsed label.
    pub href: String,
}

impl EventTrait for LinkActivated {}

#[derive(Clone, Copy, Debug)]
struct CaretPos {
    block: usize,
    byte: usize,
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    index: usize,
    byte: usize,
    x: f32,
    y: f32,
    moved: bool,
    scroll: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    entity: Entity,
    shape: u16,
    fingerprint: u64,
    width_bits: u32,
    index: u32,
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    view: Entity,
    index: u32,
}

impl Component for Piece {}

struct Marker;

impl Component for Marker {}

/// Left pad that right-aligns a marker inside its column.
struct MarkerGap;

impl Component for MarkerGap {}

/// Scrollport of markdown blocks.
///
/// Build one with [`markdown_view`] and add [`MarkdownPlugin`]. The source
/// stays as given. [`Self::push_str`] appends. The block list and the child
/// entities update on the plugin's PostUpdate pass.
pub struct MarkdownView {
    source: String,
    blocks: Vec<MarkdownBlock>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    commit: usize,
    parsed_from: usize,
    slots: Vec<Slot>,
    theme: MarkdownTheme,
    width: f32,
    follow: bool,
    pinned: bool,
    dirty: bool,
    anchor: Option<CaretPos>,
    head: Option<CaretPos>,
    gesture: Option<Gesture>,
    engaged: bool,
}

impl MarkdownView {
    fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            blocks: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            commit: 0,
            parsed_from: 0,
            slots: Vec::new(),
            theme: MarkdownTheme::default(),
            width: 0.0,
            follow: true,
            pinned: true,
            dirty: true,
            anchor: None,
            head: None,
            gesture: None,
            engaged: false,
        }
    }

    /// Source, including text that has not been synced yet.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Blocks from the last sync. Empty until [`MarkdownPlugin`] runs.
    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    /// Append `chunk` and mark the view dirty.
    ///
    /// An empty chunk does nothing. The parse runs on the next sync.
    /// The selection is clamped to the new text.
    pub fn push_str(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.source.push_str(chunk);
        self.dirty = true;
    }

    /// Replace the source, clear the selection, and parse from the start.
    pub fn set_markdown(&mut self, source: impl Into<String>) {
        self.source = source.into();
        self.commit = 0;
        self.parsed_from = 0;
        self.starts.clear();
        self.ends.clear();
        self.anchor = None;
        self.head = None;
        self.gesture = None;
        self.dirty = true;
    }

    /// Whether sync keeps the vertical offset at the bottom.
    ///
    /// A new view follows. [`Self::set_follow`] changes it.
    pub fn follow(&self) -> bool {
        self.follow
    }

    /// Turn bottom pinning on or off.
    ///
    /// `false` leaves the offset where it is. `true` pins on the next sync
    /// if the offset is already at the bottom.
    pub fn set_follow(&mut self, follow: bool) {
        self.follow = follow;
        if !follow {
            self.pinned = false;
        }
    }

    /// Whether the last sync kept the view at the bottom.
    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// Byte offset of the stable prefix chosen by the last sync.
    ///
    /// Zero means the whole source was parsed. A later append can reuse
    /// blocks that end at or before this offset.
    pub fn stable_prefix(&self) -> usize {
        self.commit
    }

    /// Where the last sync started parsing the tail.
    ///
    /// Zero means a full parse. A positive value means the blocks before
    /// that byte were kept.
    pub fn parsed_from(&self) -> usize {
        self.parsed_from
    }

    /// Select every block.
    pub fn select_all(&mut self) {
        if self.blocks.is_empty() {
            self.anchor = None;
            self.head = None;
            return;
        }
        let last = self.blocks.len() - 1;
        self.anchor = Some(CaretPos { block: 0, byte: 0 });
        self.head = Some(CaretPos {
            block: last,
            byte: self.blocks[last].text.len(),
        });
    }

    /// Text inside the selection.
    ///
    /// A block the selection covers from its first byte through its last is
    /// markdown: a heading keeps its `#` marks, a list item keeps its marker
    /// and an indent that parses back at the same depth, a quote keeps a
    /// `> ` prefix on each line, a rule is `---`, and a code block keeps its
    /// fence. Bold, italic, and inline code inside that block keep `**`,
    /// `*`, and backtick markers. A link the selection covers completely is
    /// `[label](url)`.
    ///
    /// A block the selection only partly covers contributes its plain slice.
    /// An inline span that lies entirely inside the slice still keeps its
    /// markers, so a whole link or a whole bold word survives a partial
    /// block. Strikethrough is copied as plain text. Blocks that are not
    /// tight list items are separated by a blank line. Tight items share
    /// one newline. Empty when the range is empty or the view has no
    /// selection.
    pub fn selected_text(&self) -> String {
        selection_text(&self.blocks, self.anchor, self.head)
    }
}

/// Spawn a fill scrollport whose source is `markdown`.
///
/// The entity is a root until something parents it. [`MarkdownPlugin`] builds
/// the blocks, scrolls, and handles links and selection. Without the plugin
/// the source is stored and nothing is parsed.
///
/// # Example
///
/// ```
/// use rndr_core::{markdown_view, App, MarkdownPlugin, MarkdownView};
///
/// let mut app = App::new();
/// app.add_plugin(MarkdownPlugin);
/// let view = markdown_view(app.world_mut(), "# Title\n\nHello.");
/// app.update();
/// let blocks = app.world().get::<MarkdownView>(view).unwrap().blocks();
/// assert_eq!(blocks[0].text, "Title");
/// assert_eq!(blocks[1].text, "Hello.");
/// ```
pub fn markdown_view(world: &mut World, markdown: impl Into<String>) -> Entity {
    let theme = theme_for(world);
    world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: PAD,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Scroll::default(),
        theme.surface,
        MarkdownView::new(markdown),
    ))
}

/// Parse `source` with [`MarkdownTheme::default`].
///
/// The result matches what a view stores after a sync that uses the same theme.
///
/// # Example
///
/// ```
/// use rndr_core::{parse_markdown, MarkdownKind};
///
/// let blocks = parse_markdown("# Title\n\nHello.");
/// assert_eq!(blocks[0].kind, MarkdownKind::Heading(1));
/// assert_eq!(blocks[0].text, "Title");
/// assert_eq!(blocks[1].text, "Hello.");
/// ```
pub fn parse_markdown(source: &str) -> Vec<MarkdownBlock> {
    parse_with(source, &MarkdownTheme::default())
}

/// Systems for [`markdown_view`]: input, scroll, and the block sync.
///
/// Input handles presses, drags, link clicks, select-all, and copy.
/// Update runs [`crate::apply_scroll`]. PostUpdate rebuilds dirty tails,
/// paints the selection, and pins the scroll offset. Installing the plugin
/// twice registers the scroll system twice. The second call sees a zero
/// wheel delta and a drag cursor that has already moved, so the offset
/// changes once.
pub struct MarkdownPlugin;

impl Plugin for MarkdownPlugin {
    fn build(self, app: &mut App) {
        app.add_system(Schedule::Input, markdown_input);
        app.add_system(Schedule::Update, crate::layout::apply_scroll);
        app.add_system(Schedule::PostUpdate, markdown_sync);
    }
}

fn theme_for(world: &World) -> MarkdownTheme {
    world
        .resource::<MarkdownTheme>()
        .copied()
        .unwrap_or_else(|| MarkdownTheme::from_theme(&theme_of(world)))
}

fn markdown_sync(world: &mut World) {
    let ids = world.entities_with::<MarkdownView>();
    for entity in ids {
        sync_one(world, entity);
    }
}

/// Remeasure block heights when layout has assigned a new width.
///
/// PostUpdate runs before [`layout_propagate`](crate::layout_propagate), so
/// the heights it wrote used the previous rect. Returns whether a second
/// layout pass is required. A pinned view is sent back to the bottom before
/// that pass.
pub(crate) fn sync_widths(world: &mut World) -> bool {
    let ids = world.entities_with::<MarkdownView>();
    let mut changed = false;
    for entity in ids {
        let width = inner_width(world, entity);
        let stored = world
            .get::<MarkdownView>(entity)
            .map(|view| view.width)
            .unwrap_or(0.0);
        if (stored - width).abs() <= 0.5 {
            continue;
        }
        let base = style_of(world);
        let Some(mut view) = world.remove::<MarkdownView>(entity) else {
            continue;
        };
        let selecting = view.gesture.is_some_and(|gesture| gesture.moved);
        let columns = list_columns(world, &base, &view.blocks);
        reconcile(world, entity, &mut view, width, &base, &columns);
        view.width = width;
        let pin = view.pinned && view.follow && !selecting;
        apply_selection(world, &view);
        world.insert(entity, view);
        if pin {
            if let Some(scroll) = world.get_mut::<Scroll>(entity) {
                scroll.offset[1] = f32::MIN;
            }
        }
        changed = true;
    }
    changed
}

fn sync_one(world: &mut World, entity: Entity) {
    let theme = theme_for(world);
    let base = style_of(world);
    let width = inner_width(world, entity);
    let Some(mut view) = world.remove::<MarkdownView>(entity) else {
        return;
    };
    if view.theme != theme {
        view.theme = theme;
        view.commit = 0;
        view.dirty = true;
    }
    if view.dirty {
        let assembled = assemble(
            &view.source,
            &view.theme,
            view.commit,
            &view.blocks,
            &view.starts,
            &view.ends,
        );
        view.blocks = assembled.blocks;
        view.starts = assembled.starts;
        view.ends = assembled.ends;
        view.commit = assembled.commit;
        view.parsed_from = assembled.parsed_from;
        view.dirty = false;
        clamp_selection(&mut view);
    }
    let columns = list_columns(world, &base, &view.blocks);
    let width_changed = (view.width - width).abs() > 0.5;
    if width_changed || view.slots.len() != view.blocks.len() || slots_stale(&view, &columns) {
        reconcile(world, entity, &mut view, width, &base, &columns);
        view.width = width;
    }
    apply_selection(world, &view);
    restore_selection_scroll(world, entity, &view);
    settle_pin(world, entity, &mut view);
    world.insert(entity, view.theme.surface);
    world.insert(entity, view);
}

fn slots_stale(view: &MarkdownView, columns: &ListColumns) -> bool {
    view.slots.iter().enumerate().any(|(i, slot)| {
        view.blocks.get(i).is_none_or(|block| {
            slot.shape != shape_of(block, columns)
                || slot.fingerprint != fingerprint(block, columns)
        })
    })
}

fn clamp_selection(view: &mut MarkdownView) {
    let Some(anchor) = view.anchor else {
        view.head = None;
        return;
    };
    if view.blocks.is_empty() {
        view.anchor = None;
        view.head = None;
        return;
    }
    view.anchor = Some(clamp_pos(&view.blocks, anchor));
    if let Some(head) = view.head {
        view.head = Some(clamp_pos(&view.blocks, head));
    }
}

fn clamp_pos(blocks: &[MarkdownBlock], pos: CaretPos) -> CaretPos {
    let block = pos.block.min(blocks.len() - 1);
    let byte = floor_boundary(&blocks[block].text, pos.byte);
    CaretPos { block, byte }
}

fn inner_width(world: &World, entity: Entity) -> f32 {
    let outer = world
        .get::<Rect>(entity)
        .map(|rect| rect.w)
        .filter(|w| *w > 1.0)
        .unwrap_or_else(|| {
            world
                .resource::<crate::input::Viewport>()
                .map(|viewport| viewport.content_width())
                .unwrap_or(800.0)
        });
    (outer - PAD * 2.0).max(32.0)
}

fn reconcile(
    world: &mut World,
    view: Entity,
    doc: &mut MarkdownView,
    width: f32,
    base: &TextStyle,
    columns: &ListColumns,
) {
    let theme = doc.theme;
    let mut index = 0;
    while index < doc.blocks.len() {
        let shape = shape_of(&doc.blocks[index], columns);
        let fingerprint = fingerprint(&doc.blocks[index], columns);
        let alive = doc
            .slots
            .get(index)
            .is_some_and(|slot| slot.shape == shape && world.is_alive(slot.entity));
        if !alive {
            while doc.slots.len() > index {
                despawn(world, doc.slots.pop().expect("slot").entity);
            }
        }
        let content = block_height(world, base, &doc.blocks[index], width, columns);
        let outer = content + trail_after(&doc.blocks, index);
        if index == doc.slots.len() {
            let entity = spawn_block(
                world,
                view,
                index,
                &doc.blocks[index],
                content,
                outer,
                &theme,
                base,
                columns,
            );
            doc.slots.push(Slot {
                entity,
                shape,
                fingerprint,
                width_bits: width.to_bits(),
                index: index as u32,
            });
        } else if doc.slots[index].fingerprint != fingerprint
            || doc.slots[index].width_bits != width.to_bits()
        {
            let entity = doc.slots[index].entity;
            if !update_block(
                world,
                view,
                entity,
                index,
                &doc.blocks[index],
                content,
                outer,
                &theme,
                base,
                columns,
            ) {
                despawn(world, entity);
                let entity = spawn_block(
                    world,
                    view,
                    index,
                    &doc.blocks[index],
                    content,
                    outer,
                    &theme,
                    base,
                    columns,
                );
                doc.slots[index] = Slot {
                    entity,
                    shape,
                    fingerprint,
                    width_bits: width.to_bits(),
                    index: index as u32,
                };
            } else {
                doc.slots[index].fingerprint = fingerprint;
                doc.slots[index].width_bits = width.to_bits();
                doc.slots[index].index = index as u32;
            }
        } else {
            apply_height(world, doc.slots[index].entity, content, outer);
            doc.slots[index].index = index as u32;
        }
        index += 1;
    }
    while doc.slots.len() > doc.blocks.len() {
        despawn(world, doc.slots.pop().expect("slot").entity);
    }
}

fn shape_of(block: &MarkdownBlock, columns: &ListColumns) -> u16 {
    if matches!(block.kind, MarkdownKind::Rule) {
        return 0;
    }
    let mut shape = 1u16;
    if block.quote > 0 {
        shape |= 2;
    }
    if spacer_width(columns, block) > 0.0 {
        shape |= 4;
    }
    if matches!(block.kind, MarkdownKind::Code) {
        shape |= 8;
    }
    match block.kind {
        MarkdownKind::Bullet => shape |= 16,
        MarkdownKind::Numbered(_) => shape |= 32,
        MarkdownKind::Task(_) => shape |= 64,
        _ => {}
    }
    shape
}

fn fingerprint(block: &MarkdownBlock, columns: &ListColumns) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind_key(block.kind).hash(&mut hasher);
    column_of(columns, block).to_bits().hash(&mut hasher);
    spacer_width(columns, block).to_bits().hash(&mut hasher);
    block.depth.hash(&mut hasher);
    block.quote.hash(&mut hasher);
    block.tight.hash(&mut hasher);
    block.text.hash(&mut hasher);
    block.language.hash(&mut hasher);
    for span in &block.spans {
        span.start.hash(&mut hasher);
        span.end.hash(&mut hasher);
        span.weight.hash(&mut hasher);
        span.italic.hash(&mut hasher);
        span.underline.hash(&mut hasher);
        span.family.hash(&mut hasher);
        span.color.r.to_bits().hash(&mut hasher);
        span.color.g.to_bits().hash(&mut hasher);
        span.color.b.to_bits().hash(&mut hasher);
        span.color.a.to_bits().hash(&mut hasher);
        match span.highlight {
            Some(color) => {
                1u8.hash(&mut hasher);
                color.r.to_bits().hash(&mut hasher);
                color.g.to_bits().hash(&mut hasher);
                color.b.to_bits().hash(&mut hasher);
                color.a.to_bits().hash(&mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }
    }
    for link in &block.links {
        link.start.hash(&mut hasher);
        link.end.hash(&mut hasher);
        link.href.hash(&mut hasher);
    }
    hasher.finish()
}

fn kind_key(kind: MarkdownKind) -> u64 {
    match kind {
        MarkdownKind::Paragraph => 1,
        MarkdownKind::Heading(level) => 10 + u64::from(level),
        MarkdownKind::Code => 20,
        MarkdownKind::Bullet => 21,
        MarkdownKind::Numbered(n) => 100 + n,
        MarkdownKind::Task(checked) => 30 + u64::from(checked),
        MarkdownKind::Rule => 22,
    }
}

fn trail_after(blocks: &[MarkdownBlock], index: usize) -> f32 {
    let Some(next) = blocks.get(index + 1) else {
        return 0.0;
    };
    if blocks[index].tight && next.tight {
        TIGHT_GAP
    } else {
        BLOCK_GAP
    }
}

fn block_height(
    world: &World,
    base: &TextStyle,
    block: &MarkdownBlock,
    inner: f32,
    columns: &ListColumns,
) -> f32 {
    if matches!(block.kind, MarkdownKind::Rule) {
        return 2.0;
    }
    let style = block_style(base, block);
    let width = label_width(inner, block, columns);
    let engine = text_engine(world);
    let wrap = field_text_width(width, engine.unit_advance(&style));
    let measured = engine.measure(&block.text, &style, &block.spans, wrap);
    let lines = measured.line_count.max(1) as f32;
    (TEXT_ORIGIN_Y + lines * engine.line_height(&style) + TEXT_ORIGIN_Y).max(36.0)
}

fn label_width(inner: f32, block: &MarkdownBlock, columns: &ListColumns) -> f32 {
    let mut width = inner;
    let mut gaps = 0.0;
    if block.quote > 0 {
        width -= BAR_W;
        gaps += ROW_GAP;
    }
    let spacer = spacer_width(columns, block);
    if spacer > 0.0 {
        width -= spacer;
        gaps += ROW_GAP;
    }
    let marker = column_of(columns, block);
    if marker > 0.0 {
        width -= marker;
        gaps += ROW_GAP;
    }
    (width - gaps).max(16.0)
}

/// Marker column shared by every list depth, so a two-digit number and a
/// bullet put their text on the same x, and each nested level steps by the
/// same amount.
struct ListColumns {
    width: f32,
}

fn list_columns(world: &World, base: &TextStyle, blocks: &[MarkdownBlock]) -> ListColumns {
    let mut width = 0.0f32;
    for block in blocks {
        width = width.max(marker_outer(world, base, block));
    }
    ListColumns { width }
}

fn marker_outer(world: &World, base: &TextStyle, block: &MarkdownBlock) -> f32 {
    match block.kind {
        MarkdownKind::Numbered(n) => number_label_width(world, base, n),
        MarkdownKind::Bullet => BULLET + TEXT_ORIGIN_X,
        MarkdownKind::Task(_) => CHECK + TEXT_ORIGIN_X,
        _ => 0.0,
    }
}

fn number_label_width(world: &World, base: &TextStyle, n: u64) -> f32 {
    let engine = text_engine(world);
    let measured = engine.measure(&format!("{n}."), base, &[], f32::INFINITY);
    measured.width + 2.0 * TEXT_ORIGIN_X
}

fn column_of(columns: &ListColumns, block: &MarkdownBlock) -> f32 {
    if marker_kind(block.kind) {
        columns.width
    } else {
        0.0
    }
}

fn spacer_width(columns: &ListColumns, block: &MarkdownBlock) -> f32 {
    let quote = f32::from(block.quote.saturating_sub(1)) * QUOTE_STEP;
    let depth = block.depth;
    let nest = if depth <= 1 || columns.width <= 0.0 {
        0.0
    } else {
        // One gap sits between the spacer and the marker. Extra levels fold
        // that gap into the spacer so each level's text starts one column
        // further right than the level above it.
        f32::from(depth - 1) * columns.width + f32::from(depth - 2) * ROW_GAP
    };
    quote + nest
}

fn marker_lead(block: &MarkdownBlock) -> f32 {
    match block.kind {
        MarkdownKind::Bullet | MarkdownKind::Task(_) => TEXT_ORIGIN_X,
        _ => 0.0,
    }
}

fn marker_box(world: &World, base: &TextStyle, block: &MarkdownBlock) -> f32 {
    match block.kind {
        MarkdownKind::Numbered(n) => number_label_width(world, base, n),
        MarkdownKind::Bullet => BULLET,
        MarkdownKind::Task(_) => CHECK,
        _ => 0.0,
    }
}

fn marker_top(line_h: f32, size: f32) -> f32 {
    (TEXT_ORIGIN_Y + (line_h - size) * 0.5).max(0.0)
}

fn is_row(block: &MarkdownBlock) -> bool {
    block.quote > 0 || marker_kind(block.kind)
}

fn marker_kind(kind: MarkdownKind) -> bool {
    matches!(
        kind,
        MarkdownKind::Bullet | MarkdownKind::Numbered(_) | MarkdownKind::Task(_)
    )
}

fn block_style(base: &TextStyle, block: &MarkdownBlock) -> TextStyle {
    match block.kind {
        MarkdownKind::Heading(level) => {
            let mut style = base.clone();
            let scale = match level {
                1 => 1.75,
                2 => 1.5,
                3 => 1.25,
                _ => 1.05,
            };
            style.size = base.size * scale;
            style.weight = FONT_WEIGHT_BOLD;
            style
        }
        MarkdownKind::Code => TextStyle {
            family: CODE_FAMILY.into(),
            size: base.size,
            weight: FONT_WEIGHT_REGULAR,
            style: FontStyle::Normal,
        },
        _ => base.clone(),
    }
}

fn fill_color(theme: &MarkdownTheme, block: &MarkdownBlock) -> Color {
    match block.kind {
        MarkdownKind::Code => theme.code_bg,
        MarkdownKind::Rule => theme.rule,
        _ => theme.surface,
    }
}

fn glyph_color(theme: &MarkdownTheme, block: &MarkdownBlock) -> Color {
    match block.kind {
        MarkdownKind::Code => theme.code,
        _ => theme.text,
    }
}

fn box_layout(axis: Axis, width: Size, height: Size) -> Layout {
    Layout {
        axis,
        gap: 0.0,
        padding: 0.0,
        width,
        height,
        min_width: 0.0,
        min_height: 0.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_block(
    world: &mut World,
    view: Entity,
    index: usize,
    block: &MarkdownBlock,
    content: f32,
    outer: f32,
    theme: &MarkdownTheme,
    base: &TextStyle,
    columns: &ListColumns,
) -> Entity {
    if matches!(block.kind, MarkdownKind::Rule) {
        let column = world.spawn((
            box_layout(Axis::Column, Size::Fill, Size::Px(outer)),
            Piece {
                view,
                index: index as u32,
            },
        ));
        set_parent(world, column, view);
        let bar = world.spawn((
            box_layout(Axis::Column, Size::Fill, Size::Px(content)),
            theme.rule,
        ));
        set_parent(world, bar, column);
        return column;
    }
    if matches!(block.kind, MarkdownKind::Code) && !is_row(block) {
        let column = world.spawn((
            box_layout(Axis::Column, Size::Fill, Size::Px(outer)),
            Piece {
                view,
                index: index as u32,
            },
        ));
        set_parent(world, column, view);
        let label = spawn_label(world, view, index, block, content, Size::Fill, theme, base);
        set_parent(world, label, column);
        return column;
    }
    if !is_row(block) {
        let entity = spawn_label(world, view, index, block, outer, Size::Fill, theme, base);
        set_parent(world, entity, view);
        return entity;
    }
    let row = world.spawn((
        Layout {
            axis: Axis::Row,
            gap: ROW_GAP,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Px(outer),
            min_width: 0.0,
            min_height: 0.0,
        },
        Piece {
            view,
            index: index as u32,
        },
    ));
    set_parent(world, row, view);
    if block.quote > 0 {
        let bar = world.spawn((
            box_layout(Axis::Column, Size::Px(BAR_W), Size::Fill),
            theme.quote,
        ));
        set_parent(world, bar, row);
    }
    let spacer = spacer_width(columns, block);
    if spacer > 0.0 {
        let gap = world.spawn(box_layout(Axis::Column, Size::Px(spacer), Size::Px(1.0)));
        set_parent(world, gap, row);
    }
    if marker_kind(block.kind) {
        let marker = spawn_marker(world, block, theme, base, columns);
        set_parent(world, marker, row);
    }
    let label = spawn_label(world, view, index, block, content, Size::Fill, theme, base);
    set_parent(world, label, row);
    row
}

fn spawn_marker(
    world: &mut World,
    block: &MarkdownBlock,
    theme: &MarkdownTheme,
    base: &TextStyle,
    columns: &ListColumns,
) -> Entity {
    let column = column_of(columns, block).max(marker_outer(world, base, block));
    let row = world.spawn((
        Marker,
        Layout {
            axis: Axis::Row,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(column),
            height: Size::Hug,
            min_width: 0.0,
            min_height: 0.0,
        },
    ));
    let box_w = marker_box(world, base, block);
    let gap_w = (column - box_w - marker_lead(block)).max(0.0);
    let gap = world.spawn((
        MarkerGap,
        box_layout(Axis::Column, Size::Px(gap_w), Size::Px(1.0)),
    ));
    set_parent(world, gap, row);
    let inner = spawn_marker_inner(world, block, theme, base);
    set_parent(world, inner, row);
    row
}

fn spawn_marker_inner(
    world: &mut World,
    block: &MarkdownBlock,
    theme: &MarkdownTheme,
    base: &TextStyle,
) -> Entity {
    let line_h = text_engine(world).line_height(base);
    match block.kind {
        MarkdownKind::Numbered(n) => world.spawn((
            box_layout(Axis::Column, Size::Hug, Size::Hug),
            Label::new(format!("{n}.")).with_color(theme.muted),
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            base.clone(),
        )),
        MarkdownKind::Task(checked) => spawn_check(world, checked, theme, line_h),
        _ => spawn_bullet(world, theme, line_h),
    }
}

fn spawn_bullet(world: &mut World, theme: &MarkdownTheme, line_h: f32) -> Entity {
    let column = world.spawn(box_layout(Axis::Column, Size::Px(BULLET), Size::Hug));
    let top = world.spawn(box_layout(
        Axis::Column,
        Size::Px(BULLET),
        Size::Px(marker_top(line_h, BULLET)),
    ));
    set_parent(world, top, column);
    let dot = world.spawn((
        box_layout(Axis::Column, Size::Px(BULLET), Size::Px(BULLET)),
        Radius(BULLET * 0.5),
        theme.muted,
    ));
    set_parent(world, dot, column);
    column
}

fn spawn_check(world: &mut World, checked: bool, theme: &MarkdownTheme, line_h: f32) -> Entity {
    let column = world.spawn(box_layout(Axis::Column, Size::Px(CHECK), Size::Hug));
    let top = world.spawn(box_layout(
        Axis::Column,
        Size::Px(CHECK),
        Size::Px(marker_top(line_h, CHECK)),
    ));
    set_parent(world, top, column);
    let border = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: CHECK_PAD,
            width: Size::Px(CHECK),
            height: Size::Px(CHECK),
            min_width: 0.0,
            min_height: 0.0,
        },
        theme.muted,
    ));
    set_parent(world, border, column);
    // A hollow center matches the page. A checked item fills that center.
    let hole = if checked { theme.text } else { theme.surface };
    let mark = world.spawn((box_layout(Axis::Column, Size::Fill, Size::Fill), hole));
    set_parent(world, mark, border);
    column
}

#[allow(clippy::too_many_arguments)]
fn spawn_label(
    world: &mut World,
    view: Entity,
    index: usize,
    block: &MarkdownBlock,
    height: f32,
    width: Size,
    theme: &MarkdownTheme,
    base: &TextStyle,
) -> Entity {
    world.spawn((
        box_layout(Axis::Column, width, Size::Px(height)),
        fill_color(theme, block),
        Label::new(&block.text)
            .with_color(glyph_color(theme, block))
            .overflow(TextOverflow::Wrap),
        Spans {
            runs: block.spans.clone(),
        },
        block_style(base, block),
        Piece {
            view,
            index: index as u32,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn update_block(
    world: &mut World,
    view: Entity,
    entity: Entity,
    index: usize,
    block: &MarkdownBlock,
    content: f32,
    outer: f32,
    theme: &MarkdownTheme,
    base: &TextStyle,
    columns: &ListColumns,
) -> bool {
    world.insert(
        entity,
        Piece {
            view,
            index: index as u32,
        },
    );
    if matches!(block.kind, MarkdownKind::Rule) {
        world.insert(
            entity,
            box_layout(Axis::Column, Size::Fill, Size::Px(outer)),
        );
        world.remove::<Color>(entity);
        let bar = children_of(world, entity)
            .into_iter()
            .find(|child| world.has::<Color>(*child) && !world.has::<Label>(*child));
        let Some(bar) = bar else {
            return false;
        };
        world.insert(bar, box_layout(Axis::Column, Size::Fill, Size::Px(content)));
        world.insert(bar, theme.rule);
        return true;
    }
    if world.has::<Label>(entity) {
        write_label(world, entity, block, outer, Size::Fill, theme, base);
        return true;
    }
    if matches!(block.kind, MarkdownKind::Code) && !is_row(block) {
        world.insert(
            entity,
            box_layout(Axis::Column, Size::Fill, Size::Px(outer)),
        );
        let label = children_of(world, entity)
            .into_iter()
            .find(|child| world.has::<Label>(*child));
        let Some(label) = label else {
            return false;
        };
        write_label(world, label, block, content, Size::Fill, theme, base);
        world.insert(
            label,
            Piece {
                view,
                index: index as u32,
            },
        );
        return true;
    }
    world.insert(
        entity,
        Layout {
            axis: Axis::Row,
            gap: ROW_GAP,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Px(outer),
            min_width: 0.0,
            min_height: 0.0,
        },
    );
    let kids = children_of(world, entity);
    let label = kids
        .iter()
        .copied()
        .find(|child| world.has::<Label>(*child) && world.has::<Piece>(*child));
    let marker = kids
        .iter()
        .copied()
        .find(|child| world.has::<Marker>(*child));
    let bar = kids.iter().copied().find(|child| {
        world.has::<Color>(*child) && !world.has::<Label>(*child) && !world.has::<Marker>(*child)
    });
    let spacer = kids.iter().copied().find(|child| {
        !world.has::<Label>(*child) && !world.has::<Color>(*child) && !world.has::<Marker>(*child)
    });
    let Some(label) = label else {
        return false;
    };
    if (block.quote > 0) != bar.is_some() {
        return false;
    }
    if (spacer_width(columns, block) > 0.0) != spacer.is_some() {
        return false;
    }
    if marker_kind(block.kind) != marker.is_some() {
        return false;
    }
    if let Some(bar) = bar {
        world.insert(bar, box_layout(Axis::Column, Size::Px(BAR_W), Size::Fill));
        world.insert(bar, theme.quote);
    }
    if let Some(spacer) = spacer {
        world.insert(
            spacer,
            box_layout(
                Axis::Column,
                Size::Px(spacer_width(columns, block)),
                Size::Px(1.0),
            ),
        );
    }
    if let Some(marker) = marker {
        if !sync_marker(world, marker, block, theme, base, columns) {
            return false;
        }
    }
    write_label(world, label, block, content, Size::Fill, theme, base);
    world.insert(
        label,
        Piece {
            view,
            index: index as u32,
        },
    );
    true
}

fn sync_marker(
    world: &mut World,
    marker: Entity,
    block: &MarkdownBlock,
    theme: &MarkdownTheme,
    base: &TextStyle,
    columns: &ListColumns,
) -> bool {
    let column = column_of(columns, block).max(marker_outer(world, base, block));
    let Some(layout) = world.get::<Layout>(marker).copied() else {
        return false;
    };
    if layout.axis != Axis::Row {
        return false;
    }
    world.insert(
        marker,
        Layout {
            width: Size::Px(column),
            ..layout
        },
    );
    let kids = children_of(world, marker);
    let Some(gap) = kids
        .iter()
        .copied()
        .find(|child| world.has::<MarkerGap>(*child))
    else {
        return false;
    };
    let box_w = marker_box(world, base, block);
    let gap_w = (column - box_w - marker_lead(block)).max(0.0);
    world.insert(
        gap,
        box_layout(Axis::Column, Size::Px(gap_w), Size::Px(1.0)),
    );
    let Some(inner) = kids
        .into_iter()
        .find(|child| !world.has::<MarkerGap>(*child))
    else {
        return false;
    };
    match block.kind {
        MarkdownKind::Numbered(n) => {
            if !world.has::<Label>(inner) {
                return false;
            }
            world.insert(inner, Label::new(format!("{n}.")).with_color(theme.muted));
            world.insert(inner, base.clone());
            true
        }
        MarkdownKind::Task(checked) => sync_check(world, inner, checked, theme),
        MarkdownKind::Bullet => {
            let dot = children_of(world, inner)
                .into_iter()
                .find(|child| world.has::<Color>(*child));
            if let Some(dot) = dot {
                world.insert(dot, theme.muted);
            }
            true
        }
        _ => false,
    }
}

fn sync_check(world: &mut World, inner: Entity, checked: bool, theme: &MarkdownTheme) -> bool {
    let Some(border) = children_of(world, inner)
        .into_iter()
        .find(|child| world.has::<Color>(*child))
    else {
        return false;
    };
    world.insert(border, theme.muted);
    let Some(hole) = children_of(world, border).into_iter().next() else {
        return false;
    };
    let color = if checked { theme.text } else { theme.surface };
    world.insert(hole, color);
    true
}

fn apply_height(world: &mut World, entity: Entity, content: f32, outer: f32) {
    if world.has::<Label>(entity) {
        set_px_height(world, entity, outer);
        return;
    }
    set_px_height(world, entity, outer);
    if let Some(label) = label_entity(world, entity) {
        set_px_height(world, label, content);
    }
}

fn set_px_height(world: &mut World, entity: Entity, height: f32) {
    if let Some(layout) = world.get_mut::<Layout>(entity) {
        layout.height = Size::Px(height);
    }
}

fn write_label(
    world: &mut World,
    entity: Entity,
    block: &MarkdownBlock,
    height: f32,
    width: Size,
    theme: &MarkdownTheme,
    base: &TextStyle,
) {
    world.insert(entity, box_layout(Axis::Column, width, Size::Px(height)));
    world.insert(entity, fill_color(theme, block));
    world.insert(
        entity,
        Label::new(&block.text)
            .with_color(glyph_color(theme, block))
            .overflow(TextOverflow::Wrap),
    );
    world.insert(
        entity,
        Spans {
            runs: block.spans.clone(),
        },
    );
    world.insert(entity, block_style(base, block));
}

fn apply_selection(world: &mut World, view: &MarkdownView) {
    for slot in &view.slots {
        if let Some(label) = label_entity(world, slot.entity) {
            world.remove::<crate::caret::Caret>(label);
            world.remove::<TextSelection>(label);
        }
    }
    let Some((start, end)) = ordered(view.anchor, view.head) else {
        return;
    };
    if start.block == end.block && start.byte == end.byte {
        return;
    }
    for slot in &view.slots {
        let index = slot.index as usize;
        if index < start.block || index > end.block {
            continue;
        }
        let Some(label) = label_entity(world, slot.entity) else {
            continue;
        };
        let len = world
            .get::<Label>(label)
            .map(|text| text.text.len())
            .unwrap_or(0);
        let from = if index == start.block {
            start.byte.min(len)
        } else {
            0
        };
        let to = if index == end.block {
            end.byte.min(len)
        } else {
            len
        };
        if from == to {
            continue;
        }
        world.insert(
            label,
            TextSelection {
                anchor: from,
                head: to,
            },
        );
    }
}

fn label_entity(world: &World, entity: Entity) -> Option<Entity> {
    if world.has::<Label>(entity) && world.has::<Piece>(entity) {
        return Some(entity);
    }
    children_of(world, entity)
        .into_iter()
        .find(|child| world.has::<Label>(*child) && world.has::<Piece>(*child))
}

fn restore_selection_scroll(world: &mut World, entity: Entity, view: &MarkdownView) {
    let Some(gesture) = view.gesture.filter(|gesture| gesture.moved) else {
        return;
    };
    if let Some(scroll) = world.get_mut::<Scroll>(entity) {
        scroll.offset = gesture.scroll;
    }
}

fn settle_pin(world: &mut World, entity: Entity, view: &mut MarkdownView) {
    if !view.follow {
        view.pinned = false;
        return;
    }
    let Some(scroll) = world.get::<Scroll>(entity).copied() else {
        return;
    };
    let rect = world.get::<Rect>(entity).copied().unwrap_or_default();
    let min_y = (rect.h - scroll.content[1]).min(0.0);
    // Layout clamps after this pass. An overshoot past the bottom is the bottom.
    let clamped = scroll.offset[1].clamp(min_y, 0.0);
    let at_bottom = (clamped - min_y).abs() <= 1.5;
    view.pinned = at_bottom;
    if view.pinned {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.offset[1] = f32::MIN;
        }
    }
}

fn markdown_input(world: &mut World) {
    let (pressed, released, down, button, x, y, shift) = world
        .resource::<Input>()
        .map(|input| {
            (
                input.pointer.pressed,
                input.pointer.released,
                input.pointer.down,
                input.pointer.button,
                input.pointer.position[0],
                input.pointer.position[1],
                input.shift,
            )
        })
        .unwrap_or((false, false, false, PointerButton::Left, 0.0, 0.0, false));
    let ids = world.entities_with::<MarkdownView>();
    if pressed && button == PointerButton::Left {
        let hit = hit_block(world, x, y);
        let pressed_at = hit
            .map(|hit| {
                (
                    hit.view,
                    CaretPos {
                        block: hit.index,
                        byte: hit.byte,
                    },
                )
            })
            .or_else(|| {
                let view = view_at(world, x, y)?;
                locate(world, view, x, y).map(|pos| (view, pos))
            });
        let view_hit = pressed_at.map(|(view, _)| view);
        for id in &ids {
            if let Some(view) = world.get_mut::<MarkdownView>(*id) {
                view.engaged = Some(*id) == view_hit;
                if Some(*id) != view_hit {
                    view.gesture = None;
                }
            }
        }
        if let Some((view_id, pos)) = pressed_at {
            let scroll = world
                .get::<Scroll>(view_id)
                .map(|scroll| scroll.offset)
                .unwrap_or([0.0, 0.0]);
            if let Some(view) = world.get_mut::<MarkdownView>(view_id) {
                if shift && view.anchor.is_some() {
                    view.head = Some(pos);
                } else {
                    view.anchor = Some(pos);
                    view.head = Some(pos);
                }
                view.gesture = Some(Gesture {
                    index: pos.block,
                    byte: pos.byte,
                    x,
                    y,
                    moved: false,
                    scroll,
                });
                view.engaged = true;
            }
        }
    } else if down {
        drag_selection(world, &ids, x, y);
    }
    if released || (!down && !pressed) {
        release_selection(world, &ids);
    }
    apply_keys(world, &ids);
    update_cursor(world);
}

fn drag_selection(world: &mut World, ids: &[Entity], x: f32, y: f32) {
    for id in ids {
        let Some(gesture) = world.get::<MarkdownView>(*id).and_then(|view| view.gesture) else {
            continue;
        };
        let moved = gesture.moved || (x - gesture.x).hypot(y - gesture.y) >= SCROLL_DRAG_SLOP;
        if !moved {
            continue;
        }
        let hit = locate(world, *id, x, y).unwrap_or(CaretPos {
            block: gesture.index,
            byte: gesture.byte,
        });
        if let Some(view) = world.get_mut::<MarkdownView>(*id) {
            if let Some(gesture) = view.gesture.as_mut() {
                gesture.moved = true;
            }
            view.head = Some(hit);
        }
    }
}

fn release_selection(world: &mut World, ids: &[Entity]) {
    for id in ids {
        let Some(gesture) = world.get::<MarkdownView>(*id).and_then(|view| view.gesture) else {
            continue;
        };
        let text = world
            .get::<MarkdownView>(*id)
            .map(|view| view.selected_text())
            .unwrap_or_default();
        if !gesture.moved {
            if let Some(href) = link_href(world, *id, gesture.index, gesture.byte) {
                event::send(world, LinkActivated { view: *id, href });
            }
        }
        if !text.is_empty() && primary_selection(platform_of(world)) {
            note_primary(world, &text);
        }
        if let Some(view) = world.get_mut::<MarkdownView>(*id) {
            view.gesture = None;
        }
    }
}

fn apply_keys(world: &mut World, ids: &[Entity]) {
    let (select_all, copy) = edit_keys(world);
    if !select_all && !copy {
        return;
    }
    let mut acted = false;
    for id in ids {
        let (primary, copied) = {
            let Some(view) = world.get_mut::<MarkdownView>(*id) else {
                continue;
            };
            if !view.engaged {
                continue;
            }
            let mut primary = String::new();
            if select_all {
                view.select_all();
                primary = view.selected_text();
                acted = true;
            }
            let copied = if copy {
                view.selected_text()
            } else {
                String::new()
            };
            (primary, copied)
        };
        if !primary.is_empty() && primary_selection(platform_of(world)) {
            note_primary(world, &primary);
        }
        if !copied.is_empty() {
            note_clipboard(world, Some(&copied));
            acted = true;
        }
    }
    if acted {
        let platform = platform_of(world);
        if let Some(input) = world.resource_mut::<Input>() {
            if input.key == Some(Key::Copy) {
                input.key = None;
            }
            input.keys.retain(|event| {
                !matches!(
                    standard_action(platform, event),
                    Some(EditAction::Copy | EditAction::SelectAll)
                )
            });
        }
    }
}

fn edit_keys(world: &World) -> (bool, bool) {
    let Some(input) = world.resource::<Input>() else {
        return (false, false);
    };
    let platform = platform_of(world);
    let mut select_all = false;
    let mut copy = input.key == Some(Key::Copy);
    for event in &input.keys {
        match standard_action(platform, event) {
            Some(EditAction::SelectAll) => select_all = true,
            Some(EditAction::Copy) => copy = true,
            _ => {}
        }
    }
    (select_all, copy)
}

#[derive(Clone, Copy)]
struct BlockHit {
    view: Entity,
    index: usize,
    byte: usize,
}

fn hit_block(world: &World, x: f32, y: f32) -> Option<BlockHit> {
    let hit = hit_test(world, x, y)?;
    let mut current = Some(hit);
    while let Some(id) = current {
        if let Some(piece) = world.get::<Piece>(id).copied() {
            // A marker, quote bar, or the row beside the text is the same
            // line as the label under the pointer. Byte 0 would be the first
            // line of a wrapped item.
            let byte = label_entity(world, id)
                .and_then(|label| byte_at(world, label, x, y))
                .unwrap_or(0);
            return Some(BlockHit {
                view: piece.view,
                index: piece.index as usize,
                byte,
            });
        }
        current = world
            .get::<crate::hierarchy::Parent>(id)
            .map(|parent| parent.entity);
    }
    None
}

fn update_cursor(world: &mut World) {
    if world.entities_with::<MarkdownView>().is_empty() {
        return;
    }
    world.insert_resource(MotionWatch { active: true });
    let (x, y) = world
        .resource::<Input>()
        .map(|input| (input.pointer.position[0], input.pointer.position[1]))
        .unwrap_or((0.0, 0.0));
    let hand = hit_block(world, x, y)
        .is_some_and(|hit| link_href(world, hit.view, hit.index, hit.byte).is_some());
    world.insert_resource(if hand {
        CursorStyle::Hand
    } else {
        CursorStyle::Arrow
    });
}

fn view_at(world: &World, x: f32, y: f32) -> Option<Entity> {
    let mut current = hit_test(world, x, y);
    while let Some(id) = current {
        if world.has::<MarkdownView>(id) {
            return Some(id);
        }
        current = world
            .get::<crate::hierarchy::Parent>(id)
            .map(|parent| parent.entity);
    }
    None
}

fn locate(world: &World, view: Entity, x: f32, y: f32) -> Option<CaretPos> {
    if let Some(hit) = hit_block(world, x, y) {
        if hit.view == view {
            return Some(CaretPos {
                block: hit.index,
                byte: hit.byte,
            });
        }
    }
    let slots: Vec<(u32, Entity)> = world
        .get::<MarkdownView>(view)?
        .slots
        .iter()
        .map(|slot| (slot.index, slot.entity))
        .collect();
    let mut nearest = None;
    for (index, entity) in slots {
        let rect = world.get::<Rect>(entity).copied()?;
        nearest = Some((index, entity));
        if y < rect.y + rect.h {
            return Some(CaretPos {
                block: index as usize,
                byte: label_byte(world, entity, x, y),
            });
        }
    }
    let (index, entity) = nearest?;
    Some(CaretPos {
        block: index as usize,
        byte: label_byte(world, entity, x, y),
    })
}

fn label_byte(world: &World, entity: Entity, x: f32, y: f32) -> usize {
    label_entity(world, entity)
        .and_then(|label| byte_at(world, label, x, y))
        .unwrap_or(0)
}

fn byte_at(world: &World, label: Entity, x: f32, y: f32) -> Option<usize> {
    let rect = world.get::<Rect>(label).copied()?;
    let layout = world.get::<crate::font::TextLayout>(label)?;
    let text_len = world.get::<Label>(label)?.text.len();
    let local_x = x - rect.x - TEXT_ORIGIN_X;
    let local_y = y - rect.y - TEXT_ORIGIN_Y;
    Some(layout.index_at(local_x, local_y).min(text_len))
}

fn link_href(world: &World, view: Entity, index: usize, byte: usize) -> Option<String> {
    let doc = world.get::<MarkdownView>(view)?;
    let block = doc.blocks.get(index)?;
    block
        .links
        .iter()
        .find(|link| byte >= link.start && byte < link.end)
        .or_else(|| {
            block
                .links
                .iter()
                .find(|link| byte > link.start && byte <= link.end)
        })
        .map(|link| link.href.clone())
}

fn ordered(anchor: Option<CaretPos>, head: Option<CaretPos>) -> Option<(CaretPos, CaretPos)> {
    let anchor = anchor?;
    let head = head?;
    if (anchor.block, anchor.byte) <= (head.block, head.byte) {
        Some((anchor, head))
    } else {
        Some((head, anchor))
    }
}

fn selection_text(
    blocks: &[MarkdownBlock],
    anchor: Option<CaretPos>,
    head: Option<CaretPos>,
) -> String {
    let Some((start, end)) = ordered(anchor, head) else {
        return String::new();
    };
    if start.block == end.block && start.byte == end.byte {
        return String::new();
    }
    if start.block >= blocks.len() || end.block >= blocks.len() {
        return String::new();
    }
    let mut out = String::new();
    let mut previous = None;
    for index in start.block..=end.block {
        let block = &blocks[index];
        let from = if index == start.block { start.byte } else { 0 };
        let to = if index == end.block {
            end.byte
        } else {
            usize::MAX
        };
        if index == end.block && index != start.block && to == 0 && !block.text.is_empty() {
            break;
        }
        if let Some(prev) = previous {
            out.push_str(block_joiner(&blocks[prev], block));
        }
        out.push_str(&format_block(blocks, index, from, to));
        previous = Some(index);
    }
    out
}

fn block_joiner(left: &MarkdownBlock, right: &MarkdownBlock) -> &'static str {
    if left.tight && right.tight {
        "\n"
    } else {
        "\n\n"
    }
}

fn format_block(blocks: &[MarkdownBlock], index: usize, from: usize, to: usize) -> String {
    let block = &blocks[index];
    let len = block.text.len();
    let end = to.min(len);
    let whole = from == 0 && to >= len;
    if matches!(block.kind, MarkdownKind::Code) {
        let body = slice_text(&block.text, from, end).to_string();
        if !whole {
            return body;
        }
        return prefix_lines(&format!("```{}\n{body}\n```", block.language), block.quote);
    }
    let body = format_inline(block, from, end);
    if !whole {
        return body;
    }
    let formatted = match block.kind {
        MarkdownKind::Heading(level) => format!("{} {body}", "#".repeat(usize::from(level))),
        MarkdownKind::Paragraph => body,
        MarkdownKind::Bullet => with_marker(&copy_indent(blocks, index), "- ", &body),
        MarkdownKind::Numbered(n) => {
            with_marker(&copy_indent(blocks, index), &format!("{n}. "), &body)
        }
        MarkdownKind::Task(checked) => {
            let mark = if checked { "[x]" } else { "[ ]" };
            with_marker(&copy_indent(blocks, index), &format!("- {mark} "), &body)
        }
        MarkdownKind::Rule => "---".to_string(),
        MarkdownKind::Code => body,
    };
    prefix_lines(&formatted, block.quote)
}

fn copy_indent(blocks: &[MarkdownBlock], index: usize) -> String {
    let depth = blocks[index].depth;
    if depth <= 1 {
        return String::new();
    }
    let mut cols = 0usize;
    for level in 1..depth {
        let width = blocks[..=index]
            .iter()
            .rev()
            .find(|block| block.depth == level && marker_kind(block.kind))
            .map(marker_columns)
            .unwrap_or(2);
        cols += width;
    }
    " ".repeat(cols)
}

fn marker_columns(block: &MarkdownBlock) -> usize {
    match block.kind {
        MarkdownKind::Bullet => 2,
        MarkdownKind::Numbered(n) => format!("{n}. ").len(),
        MarkdownKind::Task(_) => 6,
        _ => 2,
    }
}

fn with_marker(indent: &str, marker: &str, body: &str) -> String {
    let mut lines = body.split('\n');
    let first = lines.next().unwrap_or("");
    let mut out = format!("{indent}{marker}{first}");
    let cont = " ".repeat(marker.len());
    for line in lines {
        out.push('\n');
        out.push_str(indent);
        if !line.is_empty() {
            out.push_str(&cont);
            out.push_str(line);
        }
    }
    out
}

struct InlineMark {
    at: usize,
    rank: u8,
    text: String,
}

fn format_inline(block: &MarkdownBlock, from: usize, to: usize) -> String {
    let text = &block.text;
    let from = floor_boundary(text, from.min(text.len()));
    let to = floor_boundary(text, to.min(text.len()));
    if from >= to {
        return String::new();
    }
    let mut marks = Vec::new();
    for span in &block.spans {
        if span.start < from || span.end > to || span.start >= span.end {
            continue;
        }
        if span_is_code(span) {
            let (open, close) = code_delimiters(&text[span.start..span.end]);
            marks.push(InlineMark {
                at: span.start,
                rank: 3,
                text: open,
            });
            marks.push(InlineMark {
                at: span.end,
                rank: 4,
                text: close,
            });
        }
        if span.italic {
            marks.push(InlineMark {
                at: span.start,
                rank: 2,
                text: "*".to_string(),
            });
            marks.push(InlineMark {
                at: span.end,
                rank: 5,
                text: "*".to_string(),
            });
        }
        if span.weight == Some(FONT_WEIGHT_BOLD) {
            marks.push(InlineMark {
                at: span.start,
                rank: 1,
                text: "**".to_string(),
            });
            marks.push(InlineMark {
                at: span.end,
                rank: 6,
                text: "**".to_string(),
            });
        }
    }
    for link in &block.links {
        if link.start < from || link.end > to || link.start >= link.end {
            continue;
        }
        marks.push(InlineMark {
            at: link.start,
            rank: 0,
            text: "[".to_string(),
        });
        marks.push(InlineMark {
            at: link.end,
            rank: 7,
            text: format!("]({})", link_dest(&link.href)),
        });
    }
    marks.sort_by(|a, b| a.at.cmp(&b.at).then(a.rank.cmp(&b.rank)));
    let mut out = String::new();
    let mut cursor = from;
    for mark in marks {
        if mark.at > cursor {
            out.push_str(&text[cursor..mark.at]);
            cursor = mark.at;
        }
        out.push_str(&mark.text);
    }
    if cursor < to {
        out.push_str(&text[cursor..to]);
    }
    out
}

fn span_is_code(span: &TextSpan) -> bool {
    span.family.as_deref() == Some(CODE_FAMILY) || span.highlight.is_some()
}

fn code_delimiters(text: &str) -> (String, String) {
    let mut longest = 0usize;
    let mut run = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let tick = "`".repeat(longest + 1);
    if text.starts_with('`') || text.ends_with('`') {
        (format!("{tick} "), format!(" {tick}"))
    } else {
        (tick.clone(), tick)
    }
}

fn link_dest(href: &str) -> String {
    let awkward = href
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '(' || ch == ')' || ch.is_control());
    if awkward {
        format!("<{href}>")
    } else {
        href.to_string()
    }
}

fn prefix_lines(text: &str, quote: u8) -> String {
    if quote == 0 {
        return text.to_string();
    }
    let prefix = "> ".repeat(usize::from(quote));
    if text.is_empty() {
        return prefix.trim_end().to_string();
    }
    text.split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn slice_text(text: &str, start: usize, end: usize) -> &str {
    let mut from = floor_boundary(text, start.min(text.len()));
    let mut to = floor_boundary(text, end.min(text.len()));
    if from > to {
        std::mem::swap(&mut from, &mut to);
    }
    &text[from..to]
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

struct Assembled {
    blocks: Vec<MarkdownBlock>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    commit: usize,
    parsed_from: usize,
}

fn assemble(
    source: &str,
    theme: &MarkdownTheme,
    old_commit: usize,
    old_blocks: &[MarkdownBlock],
    old_starts: &[usize],
    old_ends: &[usize],
) -> Assembled {
    let commit = commit_len(source);
    let reuse_at = if commit >= old_commit && old_commit > 0 && source.is_char_boundary(old_commit)
    {
        old_commit
    } else {
        0
    };
    let mut reuse = 0;
    while reuse < old_blocks.len()
        && reuse < old_ends.len()
        && old_ends[reuse] > 0
        && old_ends[reuse] <= reuse_at
    {
        reuse += 1;
    }
    let mut from = if reuse_at == 0 {
        0
    } else if reuse < old_starts.len() && old_starts[reuse] < reuse_at {
        old_starts[reuse]
    } else {
        reuse_at
    };
    if !source.is_char_boundary(from) {
        from = 0;
    }
    while reuse > 0 && (reuse > old_ends.len() || old_ends[reuse - 1] > from) {
        reuse -= 1;
    }
    if from == 0 {
        reuse = 0;
    }
    let mut blocks = old_blocks[..reuse].to_vec();
    let mut starts = old_starts[..reuse.min(old_starts.len())].to_vec();
    let mut ends = old_ends[..reuse.min(old_ends.len())].to_vec();
    let parsed = parse_range(source, from, theme);
    blocks.extend(parsed.blocks);
    starts.extend(parsed.starts);
    ends.extend(parsed.ends);
    Assembled {
        blocks,
        starts,
        ends,
        commit,
        parsed_from: from,
    }
}

fn parse_with(source: &str, theme: &MarkdownTheme) -> Vec<MarkdownBlock> {
    parse_range(source, 0, theme).blocks
}

struct Parsed {
    blocks: Vec<MarkdownBlock>,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

fn parse_range(source: &str, origin: usize, theme: &MarkdownTheme) -> Parsed {
    if origin > source.len() || !source.is_char_boundary(origin) {
        return Parsed {
            blocks: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
        };
    }
    let slice = &source[origin..];
    let prepared = prepare_partial(slice);
    let limit = slice.len();
    let mut builder = Builder::new(theme, origin);
    for (event, range) in Parser::new_ext(prepared.as_ref(), parser_options()).into_offset_iter() {
        let Some(event) = clip_event(event, range.start, range.end, limit) else {
            continue;
        };
        let start = origin + range.start.min(limit);
        let end = origin + range.end.min(limit);
        builder.push(event, start, end);
    }
    builder.finish(source.len());
    Parsed {
        blocks: builder.blocks,
        starts: builder.starts,
        ends: builder.ends,
    }
}

fn clip_event<'a>(event: Event<'a>, start: usize, end: usize, limit: usize) -> Option<Event<'a>> {
    let clip = |text: CowStr<'a>| -> Option<CowStr<'a>> {
        if start >= limit {
            return None;
        }
        let keep = end.min(limit).saturating_sub(start);
        if keep >= text.len() {
            return Some(text);
        }
        if text.is_char_boundary(keep) {
            Some(CowStr::from(text[..keep].to_string()))
        } else {
            Some(text)
        }
    };
    Some(match event {
        Event::Text(text) => Event::Text(clip(text)?),
        Event::Code(text) => Event::Code(clip(text)?),
        Event::Html(text) => Event::Html(clip(text)?),
        Event::InlineHtml(text) => Event::InlineHtml(clip(text)?),
        Event::FootnoteReference(text) => Event::FootnoteReference(clip(text)?),
        Event::InlineMath(text) => Event::InlineMath(clip(text)?),
        Event::DisplayMath(text) => Event::DisplayMath(clip(text)?),
        other => other,
    })
}

#[derive(Clone, Copy)]
enum OpenerKind {
    Emph(char),
    Strike,
    Label,
    Dest,
}

struct Opener {
    kind: OpenerKind,
    at: usize,
    len: usize,
}

/// Close a trailing inline so a partial link, emphasis, or code span does not
/// draw its raw markers. Markers with no body yet are cut off instead.
fn prepare_partial(source: &str) -> Cow<'_, str> {
    if source.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut i = 0usize;
    let mut fence: Option<(char, usize)> = None;
    let mut code: Option<(usize, usize)> = None;
    let mut stack: Vec<Opener> = Vec::new();
    let mut dangling: Option<usize> = None;
    while i < source.len() {
        if let Some((ch, len)) = fence {
            let (line, next) = split_line(&source[i..]);
            if next == 0 {
                break;
            }
            let body = line.trim_end_matches(['\n', '\r']);
            if closing_fence(body, ch, len) {
                fence = None;
            }
            i += next;
            continue;
        }
        if at_line_start(source, i) {
            let (line, next) = split_line(&source[i..]);
            let body = line.trim_end_matches(['\n', '\r']);
            if let Some((ch, len)) = opening_fence(body) {
                let after = i + next;
                if fence_runs_to_eof(source, after, ch, len) {
                    return close_before_fence(source, i);
                }
                fence = Some((ch, len));
                i = after;
                continue;
            }
        }
        let Some(ch) = source[i..].chars().next() else {
            break;
        };
        let ch_len = ch.len_utf8();
        if ch == '\\' {
            dangling = None;
            i += ch_len;
            if let Some(escaped) = source[i..].chars().next() {
                i += escaped.len_utf8();
            }
            continue;
        }
        if ch == '`' && code.is_none() && !in_dest(&stack) {
            let n = run_len(source, i, '`');
            code = Some((i, n));
            dangling = None;
            i += n;
            continue;
        }
        if ch == '`' {
            if let Some((_, n)) = code {
                let run = run_len(source, i, '`');
                if run == n {
                    code = None;
                }
                dangling = None;
                i += run;
                continue;
            }
        }
        if code.is_some() {
            i += ch_len;
            continue;
        }
        if in_dest(&stack) {
            if ch == ')' {
                stack.pop();
            }
            dangling = None;
            i += ch_len;
            continue;
        }
        if ch == '[' {
            if !is_task_bracket(source, i) {
                stack.push(Opener {
                    kind: OpenerKind::Label,
                    at: i,
                    len: 1,
                });
            }
            dangling = None;
            i += 1;
            continue;
        }
        if ch == ']' {
            if let Some(pos) = stack
                .iter()
                .rposition(|op| matches!(op.kind, OpenerKind::Label))
            {
                let next = source[i + 1..].chars().next();
                stack.truncate(pos);
                if next == Some('(') {
                    stack.push(Opener {
                        kind: OpenerKind::Dest,
                        at: i,
                        len: 2,
                    });
                    dangling = None;
                    i += 2;
                    continue;
                }
            }
            dangling = None;
            i += 1;
            continue;
        }
        if ch == '~' {
            let n = run_len(source, i, '~');
            if n == 2 {
                note_delim(
                    source,
                    &mut stack,
                    &mut dangling,
                    i,
                    2,
                    OpenerKind::Strike,
                    true,
                );
            } else {
                dangling = None;
            }
            i += n;
            continue;
        }
        if ch == '*' || ch == '_' {
            let n = run_len(source, i, ch);
            note_delim(
                source,
                &mut stack,
                &mut dangling,
                i,
                n,
                OpenerKind::Emph(ch),
                false,
            );
            i += n;
            continue;
        }
        dangling = None;
        i += ch_len;
    }
    finish_partial(source, stack, code, dangling)
}

fn note_delim(
    source: &str,
    stack: &mut Vec<Opener>,
    dangling: &mut Option<usize>,
    at: usize,
    len: usize,
    kind: OpenerKind,
    strike: bool,
) {
    let prev = prev_char(source, at);
    let next_at = at + len;
    let next = source[next_at..].chars().next();
    let left = left_flanking(prev, next);
    let right = right_flanking(prev, next);
    let both_alnum =
        prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric);
    let (can_open, can_close) = if strike {
        (left && !both_alnum, right)
    } else if matches!(kind, OpenerKind::Emph('_')) {
        let open = left && (!right || prev.is_some_and(is_punct));
        let close = right && (!left || next.is_some_and(is_punct));
        (open, close)
    } else {
        (left && !both_alnum, right)
    };
    let matched = can_close
        .then(|| stack.iter().rposition(|op| opener_matches(op, &kind, len)))
        .flatten();
    if let Some(pos) = matched {
        stack.truncate(pos);
        *dangling = None;
        return;
    }
    if can_open {
        stack.push(Opener { kind, at, len });
        *dangling = None;
        return;
    }
    // A closer that did not pair stays in the source. Only a trailing run
    // that can neither open nor close (`Hello **`) is cut off.
    if next.is_none() && !can_close {
        *dangling = Some(at);
    } else {
        *dangling = None;
    }
}

fn opener_matches(op: &Opener, kind: &OpenerKind, len: usize) -> bool {
    op.len == len
        && match (op.kind, kind) {
            (OpenerKind::Strike, OpenerKind::Strike) => true,
            (OpenerKind::Emph(a), OpenerKind::Emph(b)) => a == *b,
            _ => false,
        }
}

fn in_dest(stack: &[Opener]) -> bool {
    matches!(stack.last(), Some(op) if matches!(op.kind, OpenerKind::Dest))
}

fn finish_partial(
    source: &str,
    stack: Vec<Opener>,
    code: Option<(usize, usize)>,
    dangling: Option<usize>,
) -> Cow<'_, str> {
    // An opener that already has a later closer stays for the real parse.
    // Appending another closer would change a finished string.
    let stack: Vec<Opener> = stack
        .into_iter()
        .filter(|op| !emph_closed_later(source, op))
        .collect();
    let mut end = source.len();
    if let Some(at) = dangling {
        let code_ok = code.is_none_or(|(code_at, _)| code_at < at);
        if code_ok && stack.iter().all(|op| op.at < at) && source.is_char_boundary(at) {
            end = at;
        }
    }
    let mut code_closer = String::new();
    if let Some((at, n)) = code {
        if at < end {
            let body_at = at + n;
            let body = if body_at <= end {
                source[body_at..end].trim()
            } else {
                ""
            };
            if body.is_empty() {
                end = at;
            } else {
                code_closer = "`".repeat(n);
            }
        }
    }
    let suffix = inline_closers(&stack, end);
    if end == source.len() && code_closer.is_empty() && suffix.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut out = source[..end].to_string();
    out.push_str(&code_closer);
    out.push_str(&suffix);
    Cow::Owned(out)
}

fn inline_closers(stack: &[Opener], end: usize) -> String {
    let mut out = String::new();
    for op in stack.iter().rev() {
        if op.at >= end {
            continue;
        }
        match op.kind {
            OpenerKind::Emph(ch) => out.extend(std::iter::repeat_n(ch, op.len)),
            OpenerKind::Strike => out.push_str("~~"),
            OpenerKind::Label => out.push_str("]()"),
            OpenerKind::Dest => out.push(')'),
        }
    }
    out
}

fn close_before_fence<'a>(source: &'a str, fence_at: usize) -> Cow<'a, str> {
    if fence_at == 0 || !source.is_char_boundary(fence_at) {
        return Cow::Borrowed(source);
    }
    let prefix = prepare_partial(&source[..fence_at]);
    if matches!(prefix, Cow::Borrowed(_)) {
        Cow::Borrowed(source)
    } else {
        let mut out = prefix.into_owned();
        out.push_str(&source[fence_at..]);
        Cow::Owned(out)
    }
}

fn at_line_start(source: &str, index: usize) -> bool {
    index == 0 || source.as_bytes().get(index.wrapping_sub(1)) == Some(&b'\n')
}

fn prev_char(source: &str, index: usize) -> Option<char> {
    source[..index].chars().next_back()
}

fn run_len(source: &str, index: usize, ch: char) -> usize {
    source[index..].chars().take_while(|c| *c == ch).count()
}

fn left_flanking(prev: Option<char>, next: Option<char>) -> bool {
    if next.is_none_or(char::is_whitespace) {
        return false;
    }
    let next_punct = next.is_some_and(is_punct);
    let prev_boundary = prev.is_none_or(|ch| ch.is_whitespace() || is_punct(ch));
    !next_punct || prev_boundary
}

fn emph_closed_later(source: &str, op: &Opener) -> bool {
    let OpenerKind::Emph(ch) = op.kind else {
        return false;
    };
    later_closer(source, op.at.saturating_add(op.len), ch)
}

/// A right-flanking run of `ch` after `from`, ignoring backslash escapes and
/// backtick spans. That run is the document's own closer.
fn later_closer(source: &str, from: usize, ch: char) -> bool {
    let mut i = from;
    let mut code: Option<usize> = None;
    while i < source.len() {
        let Some(current) = source[i..].chars().next() else {
            break;
        };
        if current == '\\' {
            i += current.len_utf8();
            if let Some(escaped) = source[i..].chars().next() {
                i += escaped.len_utf8();
            }
            continue;
        }
        if current == '`' && code.is_none() {
            let n = run_len(source, i, '`');
            code = Some(n);
            i += n;
            continue;
        }
        if current == '`' {
            if let Some(n) = code {
                let run = run_len(source, i, '`');
                if run == n {
                    code = None;
                }
                i += run;
                continue;
            }
        }
        if code.is_some() {
            i += current.len_utf8();
            continue;
        }
        if current == ch {
            let n = run_len(source, i, ch);
            let prev = prev_char(source, i);
            let next = source[i + n..].chars().next();
            if right_flanking(prev, next) {
                return true;
            }
            i += n;
            continue;
        }
        i += current.len_utf8();
    }
    false
}

fn right_flanking(prev: Option<char>, next: Option<char>) -> bool {
    if prev.is_none_or(char::is_whitespace) {
        return false;
    }
    let prev_punct = prev.is_some_and(is_punct);
    let next_boundary = next.is_none_or(|ch| ch.is_whitespace() || is_punct(ch));
    !prev_punct || next_boundary
}

fn is_punct(ch: char) -> bool {
    !ch.is_whitespace() && !ch.is_alphanumeric()
}

fn is_task_bracket(source: &str, index: usize) -> bool {
    let line_start = source[..index].rfind('\n').map_or(0, |pos| pos + 1);
    let prefix = &source[line_start..index];
    let trimmed = prefix.trim_start();
    if prefix.len() - trimmed.len() > 3 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && matches!(bytes[0], b'-' | b'*' | b'+')
        && bytes[1..].iter().all(u8::is_ascii_whitespace)
        && bytes[1..].contains(&b' ')
    {
        return true;
    }
    let mut digits = 0usize;
    while digits < bytes.len() && bytes[digits].is_ascii_digit() {
        digits += 1;
    }
    if digits == 0 || digits > 9 || digits >= bytes.len() {
        return false;
    }
    if bytes[digits] != b'.' && bytes[digits] != b')' {
        return false;
    }
    let rest = &bytes[digits + 1..];
    !rest.is_empty() && rest.iter().all(u8::is_ascii_whitespace)
}

struct Builder<'a> {
    theme: &'a MarkdownTheme,
    blocks: Vec<MarkdownBlock>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    quote: u8,
    lists: Vec<ListFrame>,
    mode: Mode,
    buf: String,
    spans: Vec<TextSpan>,
    links: Vec<MarkdownLink>,
    link_stack: Vec<(String, usize)>,
    strong: u8,
    emphasis: u8,
    strike: u8,
    language: String,
    block_start: usize,
    item_kind: Option<ItemKind>,
    item_loose: bool,
    cell_sep: bool,
}

struct ListFrame {
    ordered: bool,
    next: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Paragraph,
    Heading(u8),
    Item,
    Code,
    Html,
}

#[derive(Clone, Copy)]
enum ItemKind {
    Bullet,
    Numbered(u64),
    Task(bool),
}

impl<'a> Builder<'a> {
    fn new(theme: &'a MarkdownTheme, origin: usize) -> Self {
        Self {
            theme,
            blocks: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            quote: 0,
            lists: Vec::new(),
            mode: Mode::Idle,
            buf: String::new(),
            spans: Vec::new(),
            links: Vec::new(),
            link_stack: Vec::new(),
            strong: 0,
            emphasis: 0,
            strike: 0,
            language: String::new(),
            block_start: origin,
            item_kind: None,
            item_loose: false,
            cell_sep: false,
        }
    }

    fn push(&mut self, event: Event<'_>, start: usize, end: usize) {
        match event {
            Event::Start(tag) => self.start_tag(tag, start),
            Event::End(tag) => self.end_tag(tag, end),
            Event::Text(text) => {
                self.ensure_text(start);
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    self.buf.push_str(&text);
                } else {
                    self.push_styled(&text);
                }
            }
            Event::Code(text) => {
                self.ensure_text(start);
                self.push_code(&text);
            }
            Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::Html(text)
            | Event::InlineHtml(text) => {
                self.ensure_text(start);
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    self.buf.push_str(&text);
                } else {
                    self.push_styled(&text);
                }
            }
            Event::FootnoteReference(text) => {
                self.ensure_text(start);
                self.push_styled(&text);
            }
            Event::SoftBreak => {
                self.ensure_text(start);
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    self.buf.push('\n');
                } else {
                    self.push_styled(" ");
                }
            }
            Event::HardBreak => {
                self.ensure_text(start);
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    self.buf.push('\n');
                } else {
                    self.push_styled("\n");
                }
            }
            Event::Rule => {
                self.emit(start);
                self.push_rule(start, end);
            }
            Event::TaskListMarker(checked) => {
                self.item_kind = Some(ItemKind::Task(checked));
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>, start: usize) {
        match tag {
            Tag::Paragraph => {
                if self.mode == Mode::Item {
                    self.item_loose = true;
                    if !self.buf.is_empty() && !self.buf.ends_with("\n\n") {
                        self.push_styled("\n\n");
                    }
                    return;
                }
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    return;
                }
                self.emit(start);
                self.mode = Mode::Paragraph;
                self.block_start = start;
            }
            Tag::Heading { level, .. } => {
                self.emit(start);
                self.mode = Mode::Heading(level as u8);
                self.block_start = start;
            }
            Tag::BlockQuote(_) => self.quote = self.quote.saturating_add(1),
            Tag::CodeBlock(kind) => {
                self.emit(start);
                self.mode = Mode::Code;
                self.block_start = start;
                self.language = match kind {
                    CodeBlockKind::Indented => String::new(),
                    CodeBlockKind::Fenced(lang) => lang.trim().to_string(),
                };
            }
            Tag::HtmlBlock => {
                self.emit(start);
                self.mode = Mode::Html;
                self.block_start = start;
            }
            Tag::List(first) => {
                self.emit(start);
                self.lists.push(ListFrame {
                    ordered: first.is_some(),
                    next: first.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.emit(start);
                self.mode = Mode::Item;
                self.block_start = start;
                self.item_loose = false;
                self.push_marker();
            }
            Tag::Emphasis => self.emphasis = self.emphasis.saturating_add(1),
            Tag::Strong => self.strong = self.strong.saturating_add(1),
            Tag::Strikethrough => self.strike = self.strike.saturating_add(1),
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                self.link_stack.push((dest_url.to_string(), self.buf.len()));
            }
            Tag::TableRow | Tag::TableHead => {
                self.emit(start);
                self.mode = Mode::Paragraph;
                self.block_start = start;
                self.cell_sep = false;
            }
            Tag::TableCell => {
                if self.cell_sep && !self.buf.is_empty() {
                    self.push_styled(" | ");
                }
                self.cell_sep = true;
            }
            Tag::FootnoteDefinition(_)
            | Tag::Table(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_)
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd, end: usize) {
        match tag {
            TagEnd::Paragraph => {
                if self.mode == Mode::Paragraph {
                    self.emit(end);
                }
            }
            TagEnd::Heading(_) => {
                if matches!(self.mode, Mode::Heading(_)) {
                    self.emit(end);
                }
            }
            TagEnd::BlockQuote(_) => self.quote = self.quote.saturating_sub(1),
            TagEnd::CodeBlock | TagEnd::HtmlBlock => {
                if matches!(self.mode, Mode::Code | Mode::Html) {
                    self.emit(end);
                }
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Item => {
                if self.mode == Mode::Item {
                    self.emit(end);
                }
            }
            TagEnd::Emphasis => self.emphasis = self.emphasis.saturating_sub(1),
            TagEnd::Strong => self.strong = self.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link | TagEnd::Image => {
                if let Some((href, from)) = self.link_stack.pop() {
                    let to = self.buf.len();
                    if to > from {
                        self.links.push(MarkdownLink {
                            start: from,
                            end: to,
                            href,
                        });
                    }
                }
            }
            TagEnd::TableRow | TagEnd::TableHead => {
                if self.mode == Mode::Paragraph {
                    self.emit(end);
                }
            }
            TagEnd::TableCell
            | TagEnd::Table
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_)
            | TagEnd::Superscript
            | TagEnd::Subscript => {}
        }
    }

    fn finish(&mut self, end: usize) {
        self.emit(end);
    }

    fn ensure_text(&mut self, at: usize) {
        if self.mode == Mode::Idle {
            self.mode = Mode::Paragraph;
            self.block_start = at;
        }
    }

    fn push_marker(&mut self) {
        let Some(list) = self.lists.last_mut() else {
            self.item_kind = Some(ItemKind::Bullet);
            return;
        };
        if list.ordered {
            let n = list.next;
            list.next = list.next.saturating_add(1);
            self.item_kind = Some(ItemKind::Numbered(n));
        } else {
            self.item_kind = Some(ItemKind::Bullet);
        }
    }

    fn push_styled(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut style = InlineStyle::of(self.inline_color());
        style.weight = (self.strong > 0).then_some(FONT_WEIGHT_BOLD);
        style.italic = self.emphasis > 0;
        style.underline = !self.link_stack.is_empty();
        self.push_style(text, style);
    }

    fn push_code(&mut self, text: &str) {
        let mut style = InlineStyle::of(self.theme.code);
        style.highlight = Some(self.theme.inline_bg);
        style.family = Some(CODE_FAMILY);
        self.push_style(text, style);
    }

    fn push_style(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.buf.len();
        self.buf.push_str(text);
        let end = self.buf.len();
        if style.is_plain(self.theme.text) {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.end == start && style.same(last) {
                last.end = end;
                return;
            }
        }
        self.spans.push(style.span(start, end));
    }

    fn inline_color(&self) -> Color {
        if !self.link_stack.is_empty() {
            self.theme.link
        } else if self.strike > 0 {
            self.theme.muted
        } else {
            self.theme.text
        }
    }

    fn emit(&mut self, end: usize) {
        let mode = std::mem::replace(&mut self.mode, Mode::Idle);
        if mode == Mode::Idle {
            return;
        }
        let kind = match mode {
            Mode::Idle => return,
            Mode::Paragraph | Mode::Html => MarkdownKind::Paragraph,
            Mode::Heading(level) => MarkdownKind::Heading(level),
            Mode::Item => match self.item_kind.take() {
                Some(ItemKind::Numbered(n)) => MarkdownKind::Numbered(n),
                Some(ItemKind::Task(checked)) => MarkdownKind::Task(checked),
                _ => MarkdownKind::Bullet,
            },
            Mode::Code => MarkdownKind::Code,
        };
        if matches!(kind, MarkdownKind::Code) && self.buf.ends_with('\n') {
            self.buf.pop();
        }
        let tight = matches!(mode, Mode::Item) && !self.item_loose;
        let keep = match kind {
            MarkdownKind::Code => true,
            MarkdownKind::Paragraph | MarkdownKind::Heading(_) if self.buf.trim().is_empty() => {
                false
            }
            _ => !self.buf.is_empty(),
        };
        if keep {
            let spans = if matches!(kind, MarkdownKind::Code) {
                code_spans(&self.buf, self.theme)
            } else {
                std::mem::take(&mut self.spans)
            };
            let depth = u8::try_from(self.lists.len()).unwrap_or(u8::MAX);
            self.blocks.push(MarkdownBlock {
                kind,
                depth: if matches!(kind, MarkdownKind::Rule) {
                    0
                } else {
                    depth
                },
                quote: self.quote,
                tight,
                text: std::mem::take(&mut self.buf),
                language: if matches!(kind, MarkdownKind::Code) {
                    std::mem::take(&mut self.language)
                } else {
                    String::new()
                },
                spans,
                links: std::mem::take(&mut self.links),
            });
            self.starts.push(self.block_start);
            self.ends.push(end.max(self.block_start));
        }
        self.buf.clear();
        self.spans.clear();
        self.links.clear();
        self.link_stack.clear();
        self.language.clear();
        self.strong = 0;
        self.emphasis = 0;
        self.strike = 0;
        self.item_kind = None;
        self.item_loose = false;
        self.cell_sep = false;
    }

    fn push_rule(&mut self, start: usize, end: usize) {
        self.blocks.push(MarkdownBlock {
            kind: MarkdownKind::Rule,
            depth: 0,
            quote: self.quote,
            tight: false,
            text: String::new(),
            language: String::new(),
            spans: Vec::new(),
            links: Vec::new(),
        });
        self.starts.push(start);
        self.ends.push(end.max(start));
    }
}

#[derive(Clone, Copy)]
struct InlineStyle {
    color: Color,
    weight: Option<u16>,
    italic: bool,
    highlight: Option<Color>,
    family: Option<&'static str>,
    underline: bool,
}

impl InlineStyle {
    fn of(color: Color) -> Self {
        Self {
            color,
            weight: None,
            italic: false,
            highlight: None,
            family: None,
            underline: false,
        }
    }

    fn is_plain(self, text: Color) -> bool {
        self.color == text
            && self.weight.is_none()
            && !self.italic
            && self.highlight.is_none()
            && self.family.is_none()
            && !self.underline
    }

    fn same(self, span: &TextSpan) -> bool {
        span.color == self.color
            && span.weight == self.weight
            && span.italic == self.italic
            && span.highlight == self.highlight
            && span.family.as_deref() == self.family
            && span.underline == self.underline
    }

    fn span(self, start: usize, end: usize) -> TextSpan {
        let mut span = TextSpan::new(start, end, self.color);
        if let Some(weight) = self.weight {
            span = span.weight(weight);
        }
        if self.italic {
            span = span.italic();
        }
        if let Some(color) = self.highlight {
            span = span.highlight(color);
        }
        if let Some(family) = self.family {
            span = span.family(family);
        }
        if self.underline {
            span = span.underline();
        }
        span
    }
}

fn code_spans(text: &str, theme: &MarkdownTheme) -> Vec<TextSpan> {
    #[cfg(feature = "highlight")]
    {
        highlight_code(text, theme)
    }
    #[cfg(not(feature = "highlight"))]
    {
        let _ = (text, theme);
        Vec::new()
    }
}

#[cfg(feature = "highlight")]
fn highlight_code(text: &str, theme: &MarkdownTheme) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            push_token(&mut spans, start, i, theme.comment);
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            push_token(&mut spans, start, i, theme.comment);
            continue;
        }
        if bytes[i] == b'#'
            && (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && bytes.get(i + 1) != Some(&b'[')
        {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            push_token(&mut spans, start, i, theme.comment);
            continue;
        }
        if bytes[i] == b'"'
            || (bytes[i] == b'\'' && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()))
        {
            let quote = bytes[i];
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'\n' {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            push_token(&mut spans, start, i, theme.string);
            continue;
        }
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphabetic()) {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            push_token(&mut spans, start, i, theme.number);
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            if is_keyword(word) {
                push_token(&mut spans, start, i, theme.keyword);
            }
            continue;
        }
        i += 1;
    }
    spans
}

#[cfg(feature = "highlight")]
fn push_token(spans: &mut Vec<TextSpan>, start: usize, end: usize, color: Color) {
    if end <= start {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.end == start
            && last.color == color
            && last.highlight.is_none()
            && last.weight.is_none()
            && !last.italic
        {
            last.end = end;
            return;
        }
    }
    spans.push(TextSpan::new(start, end, color));
}

#[cfg(feature = "highlight")]
fn is_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "class", "const", "continue", "def", "else", "enum",
        "false", "fn", "for", "from", "function", "if", "impl", "import", "in", "let", "loop",
        "match", "mod", "mut", "pub", "return", "self", "struct", "trait", "true", "type", "use",
        "var", "where", "while", "yield",
    ];
    KEYWORDS.binary_search(&word).is_ok()
}

fn commit_len(source: &str) -> usize {
    if has_link_reference(source) {
        return 0;
    }
    let mut commit = 0usize;
    let mut blanks = 0u32;
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0usize;
    while offset < source.len() {
        let (line, next) = split_line(&source[offset..]);
        if next == 0 {
            break;
        }
        let next_abs = offset + next;
        let body = line.trim_end_matches(['\n', '\r']);
        if let Some((ch, len)) = fence {
            if closing_fence(body, ch, len) {
                fence = None;
                blanks = 0;
            }
        } else if let Some((ch, len)) = opening_fence(body) {
            fence = Some((ch, len));
            blanks = 0;
            if next_abs >= source.len() || fence_runs_to_eof(source, next_abs, ch, len) {
                return offset;
            }
        } else if body.trim().is_empty() {
            blanks += 1;
            if blanks >= 2 {
                commit = next_abs;
            }
        } else {
            blanks = 0;
        }
        offset = next_abs;
    }
    if fence.is_some() {
        return unclosed_fence_start(source).unwrap_or(commit);
    }
    commit
}

fn fence_runs_to_eof(source: &str, from: usize, ch: char, len: usize) -> bool {
    let mut offset = from;
    while offset < source.len() {
        let (line, next) = split_line(&source[offset..]);
        if next == 0 {
            break;
        }
        let body = line.trim_end_matches(['\n', '\r']);
        if closing_fence(body, ch, len) {
            return false;
        }
        offset += next;
    }
    true
}

fn unclosed_fence_start(source: &str) -> Option<usize> {
    let mut open: Option<(usize, char, usize)> = None;
    let mut offset = 0usize;
    while offset < source.len() {
        let (line, next) = split_line(&source[offset..]);
        if next == 0 {
            break;
        }
        let body = line.trim_end_matches(['\n', '\r']);
        if let Some((_, ch, len)) = open {
            if closing_fence(body, ch, len) {
                open = None;
            }
        } else if let Some((ch, len)) = opening_fence(body) {
            open = Some((offset, ch, len));
        }
        offset += next;
    }
    open.map(|(at, _, _)| at)
}

fn has_link_reference(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('[') && trimmed.contains("]:")
    })
}

fn split_line(source: &str) -> (&str, usize) {
    match source.find('\n') {
        Some(index) => {
            let end = index + 1;
            (&source[..end], end)
        }
        None => (source, source.len()),
    }
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    let rest = fence_body(line)?;
    let ch = rest.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let count = rest.chars().take_while(|c| *c == ch).count();
    if count < 3 {
        return None;
    }
    let after = &rest[count..];
    if ch == '`' && after.contains('`') {
        return None;
    }
    Some((ch, count))
}

fn closing_fence(line: &str, ch: char, len: usize) -> bool {
    let Some(rest) = fence_body(line) else {
        return false;
    };
    let rest = rest.trim_end();
    !rest.is_empty() && rest.chars().all(|c| c == ch) && rest.chars().count() >= len
}

fn fence_body(line: &str) -> Option<&str> {
    let mut cols = 0usize;
    for (index, ch) in line.char_indices() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += 4,
            _ => {
                if cols > 3 {
                    return None;
                }
                return Some(&line[index..]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_text(source: &str) -> Vec<(MarkdownKind, String, u8, u8)> {
        parse_markdown(source)
            .into_iter()
            .map(|block| (block.kind, block.text, block.depth, block.quote))
            .collect()
    }

    #[test]
    fn parses_headings_emphasis_code_links_lists_quotes_and_rules() {
        let blocks = parse_markdown("# Title\n\nHello **world** and *note* and `x`.\n");
        assert_eq!(blocks[0].kind, MarkdownKind::Heading(1));
        assert_eq!(blocks[0].text, "Title");
        assert_eq!(blocks[1].text, "Hello world and note and x.");
        assert!(blocks[1].spans.iter().any(|span| {
            span.weight == Some(FONT_WEIGHT_BOLD)
                && &blocks[1].text[span.start..span.end] == "world"
        }));
        assert!(blocks[1]
            .spans
            .iter()
            .any(|span| { span.italic && &blocks[1].text[span.start..span.end] == "note" }));
        assert!(blocks[1].spans.iter().any(|span| {
            span.highlight.is_some()
                && span.family.as_deref() == Some("monospace")
                && &blocks[1].text[span.start..span.end] == "x"
        }));

        let linked = parse_markdown("See [the docs](https://example.com).\n");
        assert_eq!(linked[0].links.len(), 1);
        assert_eq!(linked[0].links[0].href, "https://example.com");
        assert_eq!(
            &linked[0].text[linked[0].links[0].start..linked[0].links[0].end],
            "the docs"
        );
        assert!(linked[0].spans.iter().any(|span| span.underline));

        let nested = parse_markdown("- a\n  - b\n");
        assert!(nested.iter().all(|block| block.tight));
        assert_eq!(
            kind_text("- a\n  - b\n"),
            vec![
                (MarkdownKind::Bullet, "a".into(), 1, 0),
                (MarkdownKind::Bullet, "b".into(), 2, 0),
            ]
        );
        let loose = parse_markdown("- a\n\n- b\n");
        assert!(loose.iter().all(|block| !block.tight));
        assert_eq!(
            kind_text("1. one\n2. two\n"),
            vec![
                (MarkdownKind::Numbered(1), "one".into(), 1, 0),
                (MarkdownKind::Numbered(2), "two".into(), 1, 0),
            ]
        );
        assert_eq!(
            kind_text("- [x] done\n- [ ] open\n"),
            vec![
                (MarkdownKind::Task(true), "done".into(), 1, 0),
                (MarkdownKind::Task(false), "open".into(), 1, 0),
            ]
        );
        assert_eq!(
            kind_text("> hello\n"),
            vec![(MarkdownKind::Paragraph, "hello".into(), 0, 1)]
        );
        assert_eq!(
            kind_text("---\n"),
            vec![(MarkdownKind::Rule, String::new(), 0, 0)]
        );

        let code = parse_markdown("```rust\nlet x = 1;\n```\n");
        assert_eq!(code[0].kind, MarkdownKind::Code);
        assert_eq!(code[0].language, "rust");
        assert_eq!(code[0].text, "let x = 1;");
        #[cfg(not(feature = "highlight"))]
        assert!(code[0].spans.is_empty());
    }

    #[test]
    fn copied_selection_keeps_markdown_structure() {
        let source = "\
# Title

Hello.

- a
  - b

Spacer.

- loose

  more

> quote

---

```rust
let x = 1;
```

- [x] done
- [ ] open
";
        let blocks = parse_markdown(source);
        let last = blocks.len() - 1;
        let text = selection_text(
            &blocks,
            Some(CaretPos { block: 0, byte: 0 }),
            Some(CaretPos {
                block: last,
                byte: blocks[last].text.len(),
            }),
        );
        assert_eq!(
            text,
            "\
# Title

Hello.

- a
  - b

Spacer.

- loose

  more

> quote

---

```rust
let x = 1;
```

- [x] done
- [ ] open"
        );
    }

    #[test]
    fn partial_fence_and_emphasis_leave_the_paragraph_above_them() {
        let fenced = parse_markdown("Para\n\n```\nlet\n");
        assert_eq!(fenced[0].text, "Para");
        assert_eq!(fenced[1].kind, MarkdownKind::Code);
        assert!(fenced[1].text.contains("let"));

        let open = parse_markdown("Done.\n\n**still\n");
        assert_eq!(open[0].text, "Done.");
        assert!(!open[0]
            .spans
            .iter()
            .any(|span| span.weight == Some(FONT_WEIGHT_BOLD)));
    }

    #[test]
    fn streamed_prefixes_match_a_full_parse() {
        let source = "\
# Answer

Hello **world**.

- one
  - nested

> quoted

---

```
let x = 1;
```

Paragraph one stays put.

Paragraph two stays put.

Tail";
        let theme = MarkdownTheme::default();
        let mut blocks = Vec::new();
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut commit = 0usize;
        let mut saw_tail = false;
        let mut built = String::new();
        for ch in source.chars() {
            built.push(ch);
            let next = assemble(&built, &theme, commit, &blocks, &starts, &ends);
            let full = parse_with(&built, &theme);
            assert_eq!(next.blocks, full, "at {built:?}");
            if next.parsed_from > 0 {
                saw_tail = true;
            }
            blocks = next.blocks;
            starts = next.starts;
            ends = next.ends;
            commit = next.commit;
        }
        assert!(saw_tail, "commit {commit}");
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn highlight_colors_keywords_comments_strings_and_numbers() {
        let blocks = parse_markdown(
            "```rust\nfn main() {\n    // note\n    let n = 1;\n    let s = \"hi\";\n}\n```\n",
        );
        let code = blocks
            .iter()
            .find(|block| block.kind == MarkdownKind::Code)
            .unwrap();
        let theme = MarkdownTheme::default();
        let word = |text: &str| {
            code.spans
                .iter()
                .find(|span| &code.text[span.start..span.end] == text)
        };
        assert_eq!(word("fn").unwrap().color, theme.keyword);
        assert_eq!(word("let").unwrap().color, theme.keyword);
        assert!(code.spans.iter().any(|span| {
            span.color == theme.comment && code.text[span.start..span.end].contains("note")
        }));
        assert!(code.spans.iter().any(
            |span| span.color == theme.string && code.text[span.start..span.end].contains("hi")
        ));
        assert!(code
            .spans
            .iter()
            .any(|span| &code.text[span.start..span.end] == "1" && span.color == theme.number));
    }

    fn whole_text(blocks: &[MarkdownBlock]) -> String {
        let last = blocks.len() - 1;
        selection_text(
            blocks,
            Some(CaretPos { block: 0, byte: 0 }),
            Some(CaretPos {
                block: last,
                byte: blocks[last].text.len(),
            }),
        )
    }

    fn kinds(source: &str) -> Vec<(MarkdownKind, String, u8)> {
        parse_markdown(source)
            .into_iter()
            .map(|block| (block.kind, block.text, block.depth))
            .collect()
    }

    #[test]
    fn copied_inline_markers_and_nested_numbers_round_trip() {
        let source = "\
See **bold** and *italic* and `code` and [notes](https://example.com/a).

1. parent
   1. child

10. tenth
    1. under

- [x] done
      - nested
";
        let blocks = parse_markdown(source);
        let copied = whole_text(&blocks);
        let again = parse_markdown(&format!("{copied}\n"));
        assert_eq!(again[0].text, blocks[0].text);
        assert!(again[0].spans.iter().any(|span| {
            span.weight == Some(FONT_WEIGHT_BOLD) && &again[0].text[span.start..span.end] == "bold"
        }));
        assert!(again[0]
            .spans
            .iter()
            .any(|span| { span.italic && &again[0].text[span.start..span.end] == "italic" }));
        assert!(again[0]
            .spans
            .iter()
            .any(|span| { span_is_code(span) && &again[0].text[span.start..span.end] == "code" }));
        assert_eq!(again[0].links.len(), 1);
        assert_eq!(again[0].links[0].href, "https://example.com/a");
        assert_eq!(kinds(&format!("{copied}\n")), kinds(source));

        let paragraph = &blocks[0];
        let bold = paragraph
            .spans
            .iter()
            .find(|span| span.weight == Some(FONT_WEIGHT_BOLD))
            .unwrap();
        let cut = selection_text(
            &blocks,
            Some(CaretPos {
                block: 0,
                byte: bold.start + 1,
            }),
            Some(CaretPos {
                block: 0,
                byte: paragraph.text.len(),
            }),
        );
        assert!(!cut.contains("**"), "{cut}");
        assert!(cut.starts_with(&paragraph.text[bold.start + 1..bold.end]));

        let kept = selection_text(
            &blocks,
            Some(CaretPos { block: 0, byte: 0 }),
            Some(CaretPos {
                block: 0,
                byte: bold.end,
            }),
        );
        assert_eq!(kept, "See **bold**");

        let link = &paragraph.links[0];
        let linked = selection_text(
            &blocks,
            Some(CaretPos {
                block: 0,
                byte: link.start,
            }),
            Some(CaretPos {
                block: 0,
                byte: link.end,
            }),
        );
        assert_eq!(linked, "[notes](https://example.com/a)");
    }

    #[test]
    fn partial_inline_constructs_do_not_show_raw_markers() {
        let link = parse_markdown("See [the notes](https://exa");
        assert_eq!(link[0].text, "See the notes");
        assert!(!link[0].text.contains('['));
        assert_eq!(link[0].links[0].href, "https://exa");

        let label = parse_markdown("See [the notes");
        assert_eq!(label[0].text, "See the notes");
        assert!(!label[0].text.contains('['));
        assert!(label[0].links[0].href.is_empty());

        let bold = parse_markdown("Hello **Weights");
        assert_eq!(bold[0].text, "Hello Weights");
        assert!(!bold[0].text.contains('*'));
        assert!(bold[0].spans.iter().any(|span| {
            span.weight == Some(FONT_WEIGHT_BOLD)
                && &bold[0].text[span.start..span.end] == "Weights"
        }));

        let italic = parse_markdown("*routing");
        assert_eq!(italic[0].text, "routing");
        assert!(italic[0].spans.iter().any(|span| span.italic));

        let code = parse_markdown("the `palette");
        assert_eq!(code[0].text, "the palette");
        assert!(!code[0].text.contains('`'));
        assert!(code[0].spans.iter().any(|span| {
            span_is_code(span) && &code[0].text[span.start..span.end] == "palette"
        }));

        let finished = parse_markdown("See [the notes](https://example.com).");
        assert_eq!(finished[0].text, "See the notes.");
        assert_eq!(finished[0].links[0].href, "https://example.com");

        let literal = parse_markdown("See [literal]");
        assert!(literal[0].links.is_empty());
        assert!(literal[0].text.contains('['), "{}", literal[0].text);

        let bullet = parse_markdown("- keep the pin\n");
        assert_eq!(bullet[0].kind, MarkdownKind::Bullet);
        assert_eq!(bullet[0].text, "keep the pin");

        let star = parse_markdown("* item\n");
        assert_eq!(star[0].kind, MarkdownKind::Bullet);
        assert_eq!(star[0].text, "item");

        let fence = parse_markdown("Para\n\n```\nlet\n");
        assert_eq!(fence[0].text, "Para");
        assert_eq!(fence[1].kind, MarkdownKind::Code);
        assert!(fence[1].text.contains("let"));

        let samples = [
            "See **bold** and *italic* and `code` and [notes](https://example.com/a).\n",
            "- a\n  - b\n",
            "1. parent\n   1. child\n",
            "10. tenth\n    1. under\n",
            "- [x] done\n- [ ] open\n",
            "> quote\n",
            "```rust\nlet x = 1;\n```\n",
            "***bold***\n",
            "**bold *italic***\n",
            "*foo**bar*\n",
            "See [literal]\n",
            "word**\n",
        ];
        for sample in samples {
            assert!(
                matches!(prepare_partial(sample), Cow::Borrowed(_)),
                "rewrote {sample:?} into {}",
                prepare_partial(sample)
            );
        }
    }
}
