//! Virtual list. Only the visible rows exist, and they stretch to the list width.
//!
//! [`List::count`] and [`List::row_height`] are uniform, so the window is
//! `offset / row_height`. A [`ListRows`] callback fills each pooled row. It runs
//! every frame with the data index; update children in place, because the same
//! row entity is reused as the window scrolls. The list's own children are that
//! pool. Put title and shortcut widgets on the row, not on the list.
//!
//! [`ListKeys`] installs the standard keymap actions ([`LIST_UP`], [`LIST_DOWN`],
//! [`LIST_PAGE_UP`], [`LIST_PAGE_DOWN`], [`LIST_HOME`], [`LIST_END`],
//! [`LIST_ACTIVATE`]) for the list's context stack. Arrow keys move the selection
//! while focus is anywhere in that context, which is how a search field and the
//! list share one palette. Enter and a double-click send [`ListActivated`] during
//! Input, so Update on the same frame can run the command.
//!
//! Changing [`List::count`] clamps [`List::selected`] into range and resets
//! [`List::offset`]. The selection is then scrolled back into view, aligned to
//! a row so the selected row is fully inside the viewport. Set `count` from
//! Update; the pool rebuilds in PostUpdate, before layout, and again after
//! layout on the same frame when the height was still unknown.
//!
//! The list layout should use a gap of 0. Row height is the stride, and
//! [`Size::Fill`] on each row is what makes a resize follow the list width.

use crate::entity::Entity;
use crate::event::{send, Event};
use crate::hierarchy::{children_of, despawn, set_parent, Parent};
use crate::input::{Focus, Input};
use crate::keymap::{self, context_stack, KeyBinding, Keymap};
use crate::keys::{Keystroke, NamedKey};
use crate::layout::{ancestor_with, hit_test, Axis, Layout, Rect, Size};
use crate::style::Label;
use crate::world::{Component, World};

impl Component for List {}
impl Component for ListItem {}
impl Component for ListKeys {}
impl Component for ListRows {}

/// Move the selection up one row.
pub const LIST_UP: &str = "list.up";
/// Move the selection down one row.
pub const LIST_DOWN: &str = "list.down";
/// Move the selection up by one view.
pub const LIST_PAGE_UP: &str = "list.page_up";
/// Move the selection down by one view.
pub const LIST_PAGE_DOWN: &str = "list.page_down";
/// Select the first row.
pub const LIST_HOME: &str = "list.home";
/// Select the last row.
pub const LIST_END: &str = "list.end";
/// Activate the selected row. Bound to Enter.
pub const LIST_ACTIVATE: &str = "list.activate";

const NAV_ACTIONS: [&str; 7] = [
    LIST_UP,
    LIST_DOWN,
    LIST_PAGE_UP,
    LIST_PAGE_DOWN,
    LIST_HOME,
    LIST_END,
    LIST_ACTIVATE,
];

const DOUBLE_CLICK_MS: u64 = 400;

/// Virtual row window. `offset` is the content scroll in points.
/// `clicked` is the item index pressed this frame, not the pool slot.
#[derive(Clone, Copy, Debug)]
pub struct List {
    pub count: u32,
    pub row_height: f32,
    pub offset: f32,
    pub clicked: Option<u32>,
    /// Selected data index. `None` when `count` is 0.
    pub selected: Option<u32>,
    seen: u32,
    /// Selection we last scrolled into view. A bare `offset` write leaves this
    /// alone, so the window does not jump back on the next frame.
    anchored: Option<u32>,
    press_index: Option<u32>,
    press_ms: u64,
    press_frame: u64,
    pointer: [f32; 2],
    pointer_ready: bool,
    /// Viewport height the pool was last built for. `-1` until the first build.
    built_for: f32,
}

impl List {
    pub fn new(count: u32, row_height: f32) -> Self {
        Self {
            count,
            row_height,
            offset: 0.0,
            clicked: None,
            selected: (count > 0).then_some(0),
            seen: count,
            anchored: None,
            press_index: None,
            press_ms: 0,
            press_frame: 0,
            pointer: [0.0, 0.0],
            pointer_ready: false,
            built_for: -1.0,
        }
    }
}

/// Data index currently bound to a pooled row.
#[derive(Clone, Copy, Debug)]
pub struct ListItem {
    pub index: u32,
    pub selected: bool,
}

/// Opt into standard list navigation for this list's context stack.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListKeys;

/// Builds the widgets for data index `index` on `row`.
///
/// Called for every visible row, every frame. The row entity is stable for a
/// pool slot; spawn children once and then update them.
#[derive(Clone, Copy)]
pub struct ListRows(pub fn(&mut World, Entity, u32));

/// Enter or a double-click on a row. Sent during Input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListActivated {
    pub list: Entity,
    pub index: u32,
}

impl Event for ListActivated {}

#[derive(Clone, Default)]
struct InstalledNav {
    bindings: Vec<KeyBinding>,
}

/// Install or drop the standard navigation bindings for every [`ListKeys`] list.
pub(crate) fn prepare_list_keys(world: &mut World) {
    let desired = desired_bindings(world);
    let previous = world
        .resource::<InstalledNav>()
        .map(|installed| installed.bindings.clone())
        .unwrap_or_default();
    let Some(keymap) = world.resource_mut::<Keymap>() else {
        return;
    };
    keymap
        .bindings
        .retain(|binding| !previous.iter().any(|old| old == binding));
    let mut installed = Vec::new();
    for binding in desired {
        let taken = keymap.bindings.iter().any(|existing| {
            existing.strokes == binding.strokes && existing.context == binding.context
        });
        if taken {
            continue;
        }
        keymap.bindings.push(binding.clone());
        installed.push(binding);
    }
    world.insert_resource(InstalledNav {
        bindings: installed,
    });
}

/// Keymap actions, hover, and click. Runs in Input, before text fields.
pub(crate) fn apply_list_input(world: &mut World) {
    let actions = world
        .resource::<keymap::KeymapFired>()
        .map(|fired| fired.actions.clone())
        .unwrap_or_default();
    let focus_stack = focused_stack(world);
    let (pointer, pressed) = match world.resource::<Input>() {
        Some(input) => (input.pointer.position, input.pointer.pressed),
        None => ([0.0, 0.0], false),
    };
    let (frame, now_ms) = world
        .resource::<crate::clock::Time>()
        .map(|time| (time.frame, millis(time.now)))
        .unwrap_or((0, 0));
    let lists = world.entities_with::<List>();
    let mut activations = Vec::new();
    for entity in lists {
        let Some(mut list) = world.get::<List>(entity).copied() else {
            continue;
        };
        list.clicked = None;
        let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
        let page = page_len(height, list.row_height);
        if world.has::<ListKeys>(entity) && list_targeted(world, entity, &focus_stack) {
            for action in &actions {
                apply_nav(&mut list, action, page);
                if action == LIST_ACTIVATE {
                    if let Some(index) = list.selected {
                        activations.push((entity, index));
                    }
                }
            }
        }
        let moved = list.pointer_ready && pointer_moved(list.pointer, pointer);
        list.pointer = pointer;
        list.pointer_ready = true;
        if pressed || moved {
            if let Some(index) = row_at(world, entity, pointer) {
                if pressed {
                    list.selected = Some(index);
                    list.clicked = Some(index);
                    if is_double(&list, index, frame, now_ms) {
                        activations.push((entity, index));
                    }
                    list.press_index = Some(index);
                    list.press_ms = now_ms;
                    list.press_frame = frame;
                } else if moved {
                    list.selected = Some(index);
                }
            }
        }
        reveal_if_moved(&mut list, height);
        world.insert(entity, list);
    }
    for (list, index) in activations {
        send(world, ListActivated { list, index });
    }
}

/// Clamp, scroll the selection into view, and rebuild the visible rows.
pub(crate) fn apply_lists(world: &mut World) {
    let lists = world.entities_with::<List>();
    for entity in lists {
        let Some(mut list) = world.get::<List>(entity).copied() else {
            continue;
        };
        let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
        if list.count != list.seen {
            list.offset = 0.0;
            list.selected = if list.count == 0 {
                None
            } else {
                Some(list.selected.unwrap_or(0).min(list.count - 1))
            };
            list.seen = list.count;
            list.anchored = None;
        } else if list.count == 0 {
            list.selected = None;
        } else if let Some(selected) = list.selected {
            if selected >= list.count {
                list.selected = Some(list.count - 1);
            }
        }
        reveal_if_moved(&mut list, height);
        let max_offset = max_scroll(list.count, list.row_height, height);
        if list.row_height > 0.0 {
            list.offset = list.offset.clamp(0.0, max_offset);
        }
        list.built_for = height;
        let (first, len) = window(list.offset, height, list.row_height, list.count);
        let selected = list.selected;
        world.insert(entity, list);
        let rows = sync_pool(world, entity, len, list.row_height);
        let build = world.get::<ListRows>(entity).map(|rows| rows.0);
        for (slot, row) in rows.iter().copied().enumerate() {
            let index = first.saturating_add(slot as u32);
            if let Some(layout) = world.get_mut::<Layout>(row) {
                layout.width = Size::Fill;
                layout.height = Size::Px(list.row_height);
            }
            world.insert(
                row,
                ListItem {
                    index,
                    selected: selected == Some(index),
                },
            );
            if let Some(build) = build {
                build(world, row, index);
            }
        }
    }
}

fn desired_bindings(world: &World) -> Vec<KeyBinding> {
    let mut out = Vec::new();
    for entity in world.entities_with::<ListKeys>() {
        if !world.has::<List>(entity) {
            continue;
        }
        let stack = context_stack(world, entity);
        if stack.is_empty() {
            continue;
        }
        for action in NAV_ACTIONS {
            out.push(KeyBinding {
                action: action.to_string(),
                strokes: vec![nav_stroke(action)],
                context: stack.clone(),
            });
        }
    }
    out
}

fn nav_stroke(action: &str) -> Keystroke {
    let named = match action {
        LIST_UP => NamedKey::ArrowUp,
        LIST_DOWN => NamedKey::ArrowDown,
        LIST_PAGE_UP => NamedKey::PageUp,
        LIST_PAGE_DOWN => NamedKey::PageDown,
        LIST_HOME => NamedKey::Home,
        LIST_END => NamedKey::End,
        _ => NamedKey::Enter,
    };
    Keystroke::named(named)
}

fn apply_nav(list: &mut List, action: &str, page: u32) {
    let count = list.count;
    let current = list.selected.unwrap_or(0).min(count.saturating_sub(1));
    let Some(next) = (match action {
        LIST_UP => Some(current.saturating_sub(1)),
        LIST_DOWN => Some(current.saturating_add(1).min(count.saturating_sub(1))),
        LIST_PAGE_UP => Some(current.saturating_sub(page.max(1))),
        LIST_PAGE_DOWN => Some(
            current
                .saturating_add(page.max(1))
                .min(count.saturating_sub(1)),
        ),
        LIST_HOME => Some(0),
        LIST_END => Some(count.saturating_sub(1)),
        _ => None,
    }) else {
        return;
    };
    list.selected = if count == 0 { None } else { Some(next) };
}

fn pointer_moved(previous: [f32; 2], next: [f32; 2]) -> bool {
    previous[0].to_bits() != next[0].to_bits() || previous[1].to_bits() != next[1].to_bits()
}

fn list_targeted(world: &World, list: Entity, focus_stack: &[String]) -> bool {
    let stack = context_stack(world, list);
    if stack.is_empty() {
        focus_inside(world, list)
    } else {
        keymap::context_matches(&stack, focus_stack)
    }
}

fn focused_stack(world: &World) -> Vec<String> {
    let Some(entity) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return Vec::new();
    };
    if !world.is_alive(entity) {
        return Vec::new();
    }
    context_stack(world, entity)
}

fn focus_inside(world: &World, list: Entity) -> bool {
    let Some(mut current) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return false;
    };
    for _ in 0..64 {
        if current == list {
            return true;
        }
        match world.get::<Parent>(current).map(|parent| parent.entity) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

fn row_at(world: &World, list: Entity, pointer: [f32; 2]) -> Option<u32> {
    let hit = hit_test(world, pointer[0], pointer[1])?;
    if ancestor_with::<List>(world, hit) != Some(list) {
        return None;
    }
    let row = ancestor_with::<ListItem>(world, hit)?;
    world.get::<ListItem>(row).map(|item| item.index)
}

fn is_double(list: &List, index: u32, frame: u64, now_ms: u64) -> bool {
    if list.press_index != Some(index) {
        return false;
    }
    let frames = frame.saturating_sub(list.press_frame);
    if frames == 0 {
        return false;
    }
    let dt = now_ms.saturating_sub(list.press_ms);
    (dt == 0 && frames == 1) || (dt > 0 && dt <= DOUBLE_CLICK_MS)
}

fn reveal_if_moved(list: &mut List, height: f32) {
    // A list has no rect until the first layout. Scrolling against a zero
    // height would park the selection one row down.
    if height <= 0.0 {
        return;
    }
    if list.selected == list.anchored {
        return;
    }
    reveal(list, height);
    list.anchored = list.selected;
}

/// Rows that fit entirely. Half a point of slack keeps an exact multiple
/// (a 200px view of 20px rows) from dropping the last row to float noise.
fn full_rows(height: f32, row_height: f32) -> u32 {
    if row_height <= 0.0 || height <= 0.0 {
        return 1;
    }
    let rows = ((height + 0.5) / row_height).floor() as u32;
    rows.max(1)
}

fn max_scroll(count: u32, row_height: f32, height: f32) -> f32 {
    if height <= 0.0 || row_height <= 0.0 || count == 0 {
        return 0.0;
    }
    let visible = full_rows(height, row_height);
    count.saturating_sub(visible) as f32 * row_height
}

/// Scroll so the selection lies on a fully visible row.
///
/// Rows are placed from `floor(offset / row_height)`, so a fractional offset
/// never brings the next row fully in. The offset is therefore row-aligned.
fn reveal(list: &mut List, height: f32) {
    let Some(selected) = list.selected else {
        return;
    };
    if list.row_height <= 0.0 || height <= 0.0 {
        return;
    }
    let visible = full_rows(height, list.row_height);
    let first = (list.offset.max(0.0) / list.row_height).floor() as u32;
    let last = first.saturating_add(visible.saturating_sub(1));
    if selected < first {
        list.offset = selected as f32 * list.row_height;
    } else if selected > last {
        let start = selected.saturating_add(1).saturating_sub(visible);
        list.offset = start as f32 * list.row_height;
    }
}

/// Rebuild any list whose viewport height changed after layout.
pub(crate) fn sync_viewport(world: &mut World) -> bool {
    let dirty = world.entities_with::<List>().into_iter().any(|entity| {
        let Some(list) = world.get::<List>(entity) else {
            return false;
        };
        let Some(rect) = world.get::<Rect>(entity) else {
            return false;
        };
        rect.h > 0.0 && (rect.h - list.built_for).abs() > 0.5
    });
    if dirty {
        apply_lists(world);
    }
    dirty
}

fn page_len(height: f32, row_height: f32) -> u32 {
    if row_height <= 0.0 || height <= 0.0 {
        1
    } else {
        ((height / row_height).floor() as u32).max(1)
    }
}

fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn window(offset: f32, height: f32, row_height: f32, count: u32) -> (u32, u32) {
    if count == 0 || row_height <= 0.0 || height <= 0.0 {
        return (0, 0);
    }
    let first = (offset.max(0.0) / row_height).floor() as u32;
    let first = first.min(count - 1);
    let last = ((offset.max(0.0) + height) / row_height).ceil() as u32;
    let last = last.min(count).max(first);
    (first, last - first)
}

fn sync_pool(world: &mut World, list: Entity, len: u32, row_height: f32) -> Vec<Entity> {
    let mut rows: Vec<_> = children_of(world, list)
        .into_iter()
        .filter(|child| world.has::<ListItem>(*child))
        .collect();
    while rows.len() > len as usize {
        if let Some(extra) = rows.pop() {
            despawn(world, extra);
        }
    }
    while rows.len() < len as usize {
        let row = world.spawn((
            Layout {
                axis: Axis::Row,
                gap: 0.0,
                padding: 0.0,
                width: Size::Fill,
                height: Size::Px(row_height),
                min_width: 0.0,
                min_height: 0.0,
            },
            Label::new(""),
            ListItem {
                index: 0,
                selected: false,
            },
        ));
        set_parent(world, row, list);
        rows.push(row);
    }
    rows
}
