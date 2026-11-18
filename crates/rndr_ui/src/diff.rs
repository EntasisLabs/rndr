//! Host-supplied diff presentation.
//!
//! The host owns hunk identity, row text, and a gutter marker. This view draws
//! that marker and moves between hunks with the variable-list anchor. It does
//! not parse a diff, name a repository, or use the editor gutter.

use crate::collections::{
    Item, ItemId, ListView, RowSlot, SelectionMode, VariableAnchor, ViewProps, VirtualViewHandle,
};
use crate::{Corner, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Stable hunk identity supplied by the host. Rows may share one id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HunkId(pub u64);

/// Generic gutter mark. The host chooses it; this view does not infer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMarker {
    /// Context row.
    Unchanged,
    /// Inserted row.
    Added,
    /// Deleted row.
    Removed,
    /// Hunk or file heading row. The text is still the host's string.
    Header,
}

/// One presented row. `id` is the virtual-list identity; `hunk` is the motion key.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffRow {
    /// Stable row identity. Unique within the view.
    pub id: ItemId,
    /// Hunk this row belongs to.
    pub hunk: HunkId,
    /// Row text, including empty source lines. Shown as one line.
    pub text: String,
    /// Gutter mark.
    pub marker: DiffMarker,
    /// Finite, strictly positive height in points.
    pub height: f32,
}

/// Optional source coordinates and syntax/inline-change runs for a row.
/// The host owns parsing, language highlighting and old/new line identity.
/// Omitted rows have empty number cells when any annotations are supplied.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffAnnotation {
    /// Identity of a row in this mount.
    pub id: ItemId,
    /// Original source line, absent on inserted lines or headings.
    pub old_line: Option<u32>,
    /// Revised source line, absent on removed lines or headings.
    pub new_line: Option<u32>,
    /// Styled UTF-8 byte ranges in `DiffRow::text`. RNDR does not tokenize code.
    pub spans: Vec<TextSpan>,
}

/// Name and empty-state copy for the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffProps {
    /// Non-blank accessible name.
    pub name: String,
    /// Non-blank copy shown when the host supplies no rows.
    pub empty: String,
}

impl DiffProps {
    /// Empty state reads "No changes to display".
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            empty: "No changes to display".into(),
        }
    }
}

/// Mounts a variable-height diff on an existing parent.
pub struct DiffView;

/// Handle for hunk motion. `root` is the variable-list viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffHandle {
    /// Two-axis scroll viewport. Fills its parent by default.
    /// Hosts may set its size; leave gap and padding at zero.
    pub root: Entity,
}

#[derive(Clone)]
struct DiffLine {
    text: String,
    marker: DiffMarker,
    annotation: Option<DiffAnnotation>,
}

#[derive(Clone)]
struct DiffBody {
    lines: Arc<[DiffLine]>,
    index: HashMap<ItemId, usize>,
    hunk_order: Arc<[HunkId]>,
    first_row: HashMap<HunkId, ItemId>,
    focus: Option<HunkId>,
    number_width: f32,
    content_width: f32,
    style: TextStyle,
    engine: std::sync::Weak<dyn TextEngine>,
}

impl Component for DiffBody {}

struct DiffGutter;
impl Component for DiffGutter {}
struct DiffNumbers;
impl Component for DiffNumbers {}
struct DiffRealized(ItemId);
impl Component for DiffRealized {}
struct DiffLineText;
impl Component for DiffLineText {}

/// Rows published for the mount that is currently realizing. Keyed by view id
/// so one mount cannot paint another view's lines.
struct DiffFeeds(HashMap<u64, DiffBody>);

impl DiffView {
    /// Show `rows` in host order. Call again to replace them.
    ///
    /// Heights follow the variable-list rule: finite, strictly positive, and at
    /// least one milli-point. Invalid rows refuse before the list changes.
    /// A focused hunk that disappears moves to the next surviving hunk, then
    /// the previous, then no hunk. That repaired hunk is placed at the anchor
    /// with remainder 0. A surviving anchor row keeps its id and remainder
    /// when the focused hunk does not change.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: DiffProps,
        rows: &[DiffRow],
    ) -> Result<DiffHandle, UiError> {
        Self::mount_annotated(world, parent, key, props, rows, &[])
    }

    /// Mount source-numbered, optionally highlighted code. Existing `mount`
    /// callers keep their unnumbered rows. Both use compact code typography,
    /// full-row change fills and a measured two-axis scroll extent.
    ///
    /// Unknown/duplicate annotation IDs and invalid UTF-8 span boundaries refuse
    /// before mutation. Source coordinates are never inferred from row text.
    pub fn mount_annotated(
        world: &mut World,
        parent: Entity,
        key: UiId,
        props: DiffProps,
        rows: &[DiffRow],
        annotations: &[DiffAnnotation],
    ) -> Result<DiffHandle, UiError> {
        if !world.is_alive(parent) || !world.has::<Layout>(parent) {
            return Err(UiError::Stale);
        }
        if props.name.trim().is_empty() || props.empty.trim().is_empty() {
            return Err(UiError::InvalidProps);
        }
        let body = body_from(world, rows, annotations).ok_or(UiError::InvalidProps)?;
        let existing = child_with_key(world, parent, key);
        if let Some(entity) = existing {
            if world.get::<DiffBody>(entity).is_none() {
                return Err(UiError::IdentityConflict);
            }
        }
        let old_focus =
            existing.and_then(|entity| world.get::<DiffBody>(entity).and_then(|body| body.focus));
        let old_order: Vec<HunkId> = existing
            .and_then(|entity| {
                world
                    .get::<DiffBody>(entity)
                    .map(|body| body.hunk_order.to_vec())
            })
            .unwrap_or_default();
        let focus = repair_focus(&old_order, old_focus, &body.hunk_order);
        let mut body = body;
        body.focus = focus;
        publish_feed(world, key, body.clone());
        let items = items_of(rows);
        let heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
        let mounted = ListView::mount_variable(
            world,
            parent,
            key,
            ViewProps {
                name: props.name,
                empty: props.empty,
                enabled: true,
                selection: SelectionMode::None,
                reorder: false,
            },
            &items,
            &heights,
            compose,
        );
        let handle = match mounted {
            Ok(handle) => handle,
            Err(error) => {
                clear_feed(world, key);
                return Err(error);
            }
        };
        if existing.is_none() {
            world.get_mut::<Layout>(handle.root).unwrap().height = Size::Fill;
        }
        world.insert(handle.root, body);
        clear_feed(world, key);
        if focus != old_focus {
            if let Some(hunk) = focus {
                place_hunk(world, handle.root, hunk)?;
            }
        }
        Ok(DiffHandle { root: handle.root })
    }
}

impl DiffHandle {
    /// Hunk last placed by motion or by removal repair. `None` when nothing is focused.
    pub fn focused_hunk(self, world: &World) -> Option<HunkId> {
        world.get::<DiffBody>(self.root).and_then(|body| body.focus)
    }

    /// Variable-list scroll identity for the presented rows.
    pub fn anchor(self, world: &World) -> Option<VariableAnchor> {
        VirtualViewHandle { root: self.root }.anchor(world)
    }

    /// Number of host rows, including those not currently realized.
    pub fn len(self, world: &World) -> usize {
        world
            .get::<DiffBody>(self.root)
            .map(|body| body.lines.len())
            .unwrap_or(0)
    }

    /// Place the next hunk's first row at the anchor. Stays put on the last hunk.
    /// With no focus, places the first hunk.
    pub fn next_hunk(self, world: &mut World) -> Result<(), UiError> {
        self.move_by(world, 1)
    }

    /// Place the previous hunk's first row at the anchor. Stays put on the first hunk.
    /// With no focus, places the last hunk.
    pub fn previous_hunk(self, world: &mut World) -> Result<(), UiError> {
        self.move_by(world, -1)
    }

    /// Place `hunk`'s first row at the anchor with remainder 0.
    pub fn jump_hunk(self, world: &mut World, hunk: HunkId) -> Result<(), UiError> {
        self.place(world, hunk)
    }

    fn move_by(self, world: &mut World, step: isize) -> Result<(), UiError> {
        let target = {
            let body = world.get::<DiffBody>(self.root).ok_or(UiError::Stale)?;
            if body.hunk_order.is_empty() {
                return Ok(());
            }
            match body
                .focus
                .and_then(|id| body.hunk_order.iter().position(|hunk| *hunk == id))
            {
                Some(index) => {
                    let next = index as isize + step;
                    if next < 0 || next >= body.hunk_order.len() as isize {
                        return Ok(());
                    }
                    body.hunk_order[next as usize]
                }
                None if step < 0 => *body.hunk_order.last().unwrap(),
                None => body.hunk_order[0],
            }
        };
        self.place(world, target)
    }

    fn place(self, world: &mut World, hunk: HunkId) -> Result<(), UiError> {
        let row = world
            .get::<DiffBody>(self.root)
            .ok_or(UiError::Stale)?
            .first_row
            .get(&hunk)
            .copied()
            .ok_or(UiError::InvalidProps)?;
        let list = VirtualViewHandle { root: self.root };
        list.jump(world, row, 0)?;
        world
            .get_mut::<DiffBody>(self.root)
            .ok_or(UiError::Stale)?
            .focus = Some(hunk);
        Ok(())
    }
}

fn place_hunk(world: &mut World, root: Entity, hunk: HunkId) -> Result<(), UiError> {
    DiffHandle { root }.place(world, hunk)
}

/// Next surviving hunk, then the previous, then none. A hunk still present keeps focus.
fn repair_focus(old: &[HunkId], focus: Option<HunkId>, new_hunks: &[HunkId]) -> Option<HunkId> {
    let focus = focus?;
    if new_hunks.contains(&focus) {
        return Some(focus);
    }
    let index = old.iter().position(|hunk| *hunk == focus)?;
    if let Some(next) = old[index + 1..]
        .iter()
        .find(|hunk| new_hunks.contains(hunk))
    {
        return Some(*next);
    }
    if let Some(previous) = old[..index]
        .iter()
        .rev()
        .find(|hunk| new_hunks.contains(hunk))
    {
        return Some(*previous);
    }
    None
}

fn body_from(world: &World, rows: &[DiffRow], annotations: &[DiffAnnotation]) -> Option<DiffBody> {
    let mut index = HashMap::with_capacity(rows.len());
    let mut first_row = HashMap::new();
    let mut hunk_order = Vec::new();
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        if !height_ok(row.height) || index.insert(row.id, lines.len()).is_some() {
            return None;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = first_row.entry(row.hunk) {
            entry.insert(row.id);
            hunk_order.push(row.hunk);
        }
        lines.push(DiffLine {
            text: row.text.clone(),
            marker: row.marker,
            annotation: None,
        });
    }
    let mut seen = std::collections::HashSet::new();
    for annotation in annotations {
        let line = &mut lines[*index.get(&annotation.id)?];
        if !seen.insert(annotation.id)
            || annotation.spans.iter().any(|span| {
                span.start > span.end
                    || span.end > line.text.len()
                    || !line.text.is_char_boundary(span.start)
                    || !line.text.is_char_boundary(span.end)
            })
        {
            return None;
        }
        line.annotation = Some(annotation.clone());
    }
    let style = code_style(world);
    let engine = text_engine(world);
    let (number_width, content_width) = measure_lines(&lines, &style, engine.as_ref());
    Some(DiffBody {
        lines: Arc::from(lines),
        index,
        hunk_order: Arc::from(hunk_order),
        first_row,
        focus: None,
        number_width,
        content_width,
        style,
        engine: Arc::downgrade(&engine),
    })
}

fn height_ok(height: f32) -> bool {
    if !height.is_finite() || height <= 0.0 {
        return false;
    }
    let milli = f64::from(height) * 1000.0;
    milli.is_finite() && (1.0..=i64::MAX as f64).contains(&milli.round())
}

fn items_of(rows: &[DiffRow]) -> Vec<Item> {
    rows.iter()
        .map(|row| Item {
            id: row.id,
            name: if row.text.trim().is_empty() {
                "Blank source line".into()
            } else {
                row.text.clone()
            },
            enabled: true,
        })
        .collect()
}

fn publish_feed(world: &mut World, key: UiId, body: DiffBody) {
    if world.resource::<DiffFeeds>().is_none() {
        world.insert_resource(DiffFeeds(HashMap::new()));
    }
    world
        .resource_mut::<DiffFeeds>()
        .unwrap()
        .0
        .insert(key.0, body);
}

fn clear_feed(world: &mut World, key: UiId) {
    if let Some(feeds) = world.resource_mut::<DiffFeeds>() {
        feeds.0.remove(&key.0);
    }
}

fn child_with_key(world: &World, parent: Entity, key: UiId) -> Option<Entity> {
    children_of(world, parent).into_iter().find(|entity| {
        crate::collections::key_at(world, *entity)
            .or_else(|| crate::controls::key_at(world, *entity))
            .or_else(|| crate::surfaces::key_at(world, *entity))
            == Some(key)
    })
}

fn code_style(world: &World) -> TextStyle {
    world
        .resource::<UiTheme>()
        .cloned()
        .unwrap_or_default()
        .typography(Typography::Code)
}

fn measure_lines(lines: &[DiffLine], style: &TextStyle, engine: &dyn TextEngine) -> (f32, f32) {
    let numbered = lines.iter().any(|line| line.annotation.is_some());
    let digits = lines
        .iter()
        .filter_map(|line| line.annotation.as_ref())
        .flat_map(|a| [a.old_line, a.new_line])
        .flatten()
        .map(|n| n.to_string().len())
        .max()
        .unwrap_or(1)
        .max(2);
    let number_width = if numbered {
        engine
            .measure(&"0".repeat(digits), style, &[], f32::INFINITY)
            .width
            .ceil()
            + 12.
    } else {
        0.
    };
    let width = lines
        .iter()
        .map(|line| {
            let runs = line
                .annotation
                .as_ref()
                .map(|a| a.spans.as_slice())
                .unwrap_or(&[]);
            engine.measure(&line.text, style, runs, f32::INFINITY).width
        })
        .fold(0., f32::max);
    (number_width, (number_width * 2. + 20. + width + 12.).ceil())
}

fn compose(world: &mut World, slot: RowSlot, item: &Item) {
    let Some(list) = world.get::<Parent>(slot.root).map(|parent| parent.entity) else {
        return;
    };
    let Some((line, number_width, content_width, style)) = line_of(world, list, item.id) else {
        return;
    };
    world.insert(slot.root, DiffRealized(item.id));
    world.get_mut::<Layout>(slot.root).unwrap().min_width = content_width;
    world.get_mut::<Layout>(slot.content).unwrap().axis = Axis::Row;
    let (background, foreground) = marker_tokens(line.marker);
    world.insert(
        slot.root,
        UiRecipe(UiStyle {
            background: Some(background.into()),
            corner: Some(Corner::Square),
            ..UiStyle::default()
        }),
    );
    // Source coordinates are separate cells, never part of the source string.
    // They therefore cannot corrupt span byte offsets or source indentation.
    let numbers = children_of(world, slot.content)
        .into_iter()
        .filter(|entity| world.has::<DiffNumbers>(*entity))
        .collect::<Vec<_>>();
    for (column, old) in [true, false].into_iter().enumerate() {
        let entity = numbers.get(column).copied().unwrap_or_else(|| {
            let entity = world.spawn((Layout::column(), DiffNumbers, Label::new("")));
            set_parent(world, entity, slot.content);
            entity
        });
        let value = line
            .annotation
            .as_ref()
            .and_then(|a| if old { a.old_line } else { a.new_line });
        world.get_mut::<Label>(entity).unwrap().text =
            value.map(|n| n.to_string()).unwrap_or_default();
        world.insert(
            entity,
            Layout {
                width: Size::Px(number_width),
                height: Size::Fill,
                ..Layout::column()
            },
        );
        world.insert(
            entity,
            LabelBox {
                inset_x: 6.,
                inset_y: 0.,
                min_height: 0.,
            },
        );
        let mut numbers_style = style.clone();
        numbers_style.align = TextAlign::End;
        world.insert(entity, numbers_style);
        world.insert(
            entity,
            UiRecipe(UiStyle {
                foreground: Some(crate::Token::Secondary.into()),
                ..UiStyle::default()
            }),
        );
        if number_width == 0. {
            world.insert(entity, Hidden);
        } else {
            world.remove::<Hidden>(entity);
        }
    }
    let gutter = child_with::<DiffGutter>(world, slot.content).unwrap_or_else(|| {
        let entity = world.spawn((
            Layout {
                width: Size::Px(20.),
                height: Size::Fill,
                ..Layout::column()
            },
            DiffGutter,
            Label::new(""),
        ));
        set_parent(world, entity, slot.content);
        entity
    });
    world.get_mut::<Label>(gutter).unwrap().text = marker_glyph(line.marker).into();
    world.insert(
        gutter,
        LabelBox {
            inset_x: 4.,
            inset_y: 0.,
            min_height: 0.,
        },
    );
    world.insert(gutter, style.clone());
    world.insert(
        gutter,
        UiRecipe(UiStyle {
            foreground: Some(foreground.into()),
            ..UiStyle::default()
        }),
    );
    let text = child_with::<DiffLineText>(world, slot.content).unwrap_or_else(|| {
        let entity = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            DiffLineText,
            Label::new(""),
        ));
        set_parent(world, entity, slot.content);
        entity
    });
    world.insert(
        text,
        LabelBox {
            inset_x: 4.,
            inset_y: 0.,
            min_height: 0.,
        },
    );
    world.insert(text, style);
    world.insert(
        text,
        UiRecipe(UiStyle {
            foreground: Some(
                if line.marker == DiffMarker::Header {
                    crate::Token::Secondary
                } else {
                    crate::Token::Text
                }
                .into(),
            ),
            ..UiStyle::default()
        }),
    );
    world.insert(
        text,
        Spans {
            runs: line.annotation.map(|a| a.spans).unwrap_or_default(),
        },
    );
    let label = world.get_mut::<Label>(text).unwrap();
    label.text = line.text;
    label.overflow = TextOverflow::Visible;
}

/// Refresh measured extents when the application changes code typography or
/// replaces its shaping engine. Unchanged metrics do not remeasure rows.
pub(crate) fn styles(world: &mut World) {
    let style = code_style(world);
    let engine = text_engine(world);
    for root in world.entities_with::<DiffBody>() {
        let changed = world.get::<DiffBody>(root).is_some_and(|body| {
            body.style != style || !body.engine.ptr_eq(&Arc::downgrade(&engine))
        });
        if !changed {
            continue;
        }
        {
            let body = world.get_mut::<DiffBody>(root).unwrap();
            (body.number_width, body.content_width) =
                measure_lines(&body.lines, &style, engine.as_ref());
            body.style = style.clone();
            body.engine = Arc::downgrade(&engine);
        }
        // Recompose only realized rows after a metrics change. Normal recycling
        // already invokes compose; idle frames do not copy source strings.
        for row in children_of(world, root) {
            let Some(id) = world.get::<DiffRealized>(row).map(|row| row.0) else {
                continue;
            };
            let Some(slot) = (VirtualViewHandle { root }).row(world, id) else {
                continue;
            };
            compose(
                world,
                slot,
                &Item {
                    id,
                    name: String::new(),
                    enabled: true,
                },
            );
        }
    }
}

fn line_of(world: &World, list: Entity, id: ItemId) -> Option<(DiffLine, f32, f32, TextStyle)> {
    let key = crate::collections::key_at(world, list)?;
    if let Some(body) = world
        .resource::<DiffFeeds>()
        .and_then(|feeds| feeds.0.get(&key.0))
    {
        return body.line(id);
    }
    world.get::<DiffBody>(list).and_then(|body| body.line(id))
}

impl DiffBody {
    fn line(&self, id: ItemId) -> Option<(DiffLine, f32, f32, TextStyle)> {
        self.index.get(&id).map(|index| {
            (
                self.lines[*index].clone(),
                self.number_width,
                self.content_width,
                self.style.clone(),
            )
        })
    }
}

fn child_with<T: Component>(world: &World, parent: Entity) -> Option<Entity> {
    children_of(world, parent)
        .into_iter()
        .find(|entity| world.has::<T>(*entity))
}

fn marker_glyph(marker: DiffMarker) -> &'static str {
    match marker {
        DiffMarker::Unchanged => " ",
        DiffMarker::Added => "+",
        DiffMarker::Removed => "-",
        DiffMarker::Header => "…",
    }
}

fn marker_tokens(marker: DiffMarker) -> (crate::Token, crate::Token) {
    match marker {
        DiffMarker::Unchanged => (crate::Token::Content, crate::Token::Secondary),
        DiffMarker::Added => (crate::Token::SuccessFill, crate::Token::SuccessText),
        DiffMarker::Removed => (crate::Token::DangerFill, crate::Token::DangerText),
        DiffMarker::Header => (crate::Token::Raised, crate::Token::Secondary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_repairs_to_the_next_hunk_then_the_previous_then_none() {
        let old = [HunkId(1), HunkId(2), HunkId(3)];
        assert_eq!(
            repair_focus(&old, Some(HunkId(2)), &[HunkId(1), HunkId(3)]),
            Some(HunkId(3))
        );
        assert_eq!(
            repair_focus(&old, Some(HunkId(3)), &[HunkId(1), HunkId(2)]),
            Some(HunkId(2))
        );
        assert_eq!(repair_focus(&old, Some(HunkId(2)), &[]), None);
        assert_eq!(
            repair_focus(&old, Some(HunkId(1)), &[HunkId(1), HunkId(9)]),
            Some(HunkId(1))
        );
        assert_eq!(repair_focus(&old, Some(HunkId(2)), &[HunkId(9)]), None);
    }
}
