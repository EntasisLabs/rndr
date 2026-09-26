use crate::hierarchy::{children_of, despawn, set_parent};
use crate::input::Input;
use crate::layout::{ancestor_with, hit_test, Axis, Layout, Rect, Size};
use crate::style::{Color, Label};
use crate::world::{Component, World};

impl Component for List {}
impl Component for ListItem {}

/// Virtual row window. `offset` is the content scroll in points.
/// `clicked` is the item index hit this frame, not the pool slot.
#[derive(Clone, Copy, Debug)]
pub struct List {
    pub count: u32,
    pub row_height: f32,
    pub offset: f32,
    pub clicked: Option<u32>,
}

impl List {
    pub fn new(count: u32, row_height: f32) -> Self {
        Self {
            count,
            row_height,
            offset: 0.0,
            clicked: None,
        }
    }
}

/// Data index currently bound to a pooled row.
#[derive(Clone, Copy, Debug)]
pub struct ListItem {
    pub index: u32,
}

pub fn apply_lists(world: &mut World) {
    let lists = world.entities_with::<List>();
    for entity in lists {
        let Some(mut list) = world.get::<List>(entity).copied() else {
            continue;
        };
        list.clicked = None;
        let height = world.get::<Rect>(entity).map(|rect| rect.h).unwrap_or(0.0);
        let (first, len) = window(list.offset, height, list.row_height, list.count);
        let max_offset = (list.count as f32 * list.row_height - height).max(0.0);
        if list.row_height > 0.0 {
            list.offset = list.offset.clamp(0.0, max_offset);
        }
        let rows = sync_pool(world, entity, len, list.row_height);
        for (slot, row) in rows.iter().copied().enumerate() {
            let index = first.saturating_add(slot as u32);
            if let Some(item) = world.get_mut::<ListItem>(row) {
                item.index = index;
            }
            if let Some(label) = world.get_mut::<Label>(row) {
                label.text = index.to_string();
            }
        }
        if world
            .resource::<Input>()
            .is_some_and(|input| input.pointer.pressed)
        {
            let position = world.resource::<Input>().unwrap().pointer.position;
            if let Some(hit) = hit_test(world, position[0], position[1]) {
                if ancestor_with::<List>(world, hit) == Some(entity) {
                    if let Some(row) = ancestor_with::<ListItem>(world, hit) {
                        list.clicked = world.get::<ListItem>(row).map(|item| item.index);
                    }
                }
            }
        }
        world.insert(entity, list);
    }
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

fn sync_pool(
    world: &mut World,
    list: crate::entity::Entity,
    len: u32,
    row_height: f32,
) -> Vec<crate::entity::Entity> {
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
        let width = world.get::<Rect>(list).map(|rect| rect.w).unwrap_or(0.0);
        let row = world.spawn((
            Layout {
                axis: Axis::Column,
                gap: 0.0,
                padding: 0.0,
                width: Size::Px(width),
                height: Size::Px(row_height),
                min_width: 0.0,
                min_height: 0.0,
            },
            Color::rgb(0.16, 0.17, 0.19),
            Label::new(""),
            ListItem { index: 0 },
        ));
        set_parent(world, row, list);
        rows.push(row);
    }
    rows
}
