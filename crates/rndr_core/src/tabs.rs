use crate::hierarchy::children_of;
use crate::input::Input;
use crate::layout::{ancestor_with, hit_test, Clip, Rect};
use crate::world::{Component, World};

impl Component for Tabs {}
impl Component for Tab {}
impl Component for TabPanel {}
impl Component for Hidden {}

/// Parent of [`Tab`] headers. `selected` is the visible [`Tab::id`].
#[derive(Clone, Copy, Debug)]
pub struct Tabs {
    pub selected: u64,
}

/// Clickable header. Its [`TabPanel`] children hide when this id is not selected.
#[derive(Clone, Copy, Debug)]
pub struct Tab {
    pub id: u64,
}

/// Body shown only while the parent [`Tab`] is selected.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabPanel;

/// Skipped by layout, hit-testing, and paint.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hidden;

pub fn apply_tabs(world: &mut World) {
    if world
        .resource::<Input>()
        .is_some_and(|input| input.pointer.pressed)
    {
        let position = world.resource::<Input>().unwrap().pointer.position;
        if let Some(hit) = hit_test(world, position[0], position[1]) {
            if let Some(tab) = ancestor_with::<Tab>(world, hit) {
                if ancestor_with::<TabPanel>(world, hit).is_none() {
                    let id = world.get::<Tab>(tab).map(|tab| tab.id);
                    let tabs = ancestor_with::<Tabs>(world, tab);
                    if let (Some(id), Some(tabs)) = (id, tabs) {
                        if let Some(tabs) = world.get_mut::<Tabs>(tabs) {
                            tabs.selected = id;
                        }
                    }
                }
            }
        }
    }
    sync_panels(world);
}

fn sync_panels(world: &mut World) {
    let tabs = world.entities_with::<Tabs>();
    for tabs_entity in tabs {
        let selected = world.get::<Tabs>(tabs_entity).map(|tabs| tabs.selected);
        let Some(selected) = selected else {
            continue;
        };
        for tab_entity in children_of(world, tabs_entity) {
            let Some(id) = world.get::<Tab>(tab_entity).map(|tab| tab.id) else {
                continue;
            };
            let hide = id != selected;
            for child in children_of(world, tab_entity) {
                if !world.has::<TabPanel>(child) {
                    continue;
                }
                if hide {
                    world.insert(child, Hidden);
                    world.insert(child, Rect::default());
                    world.insert(child, Clip::default());
                } else {
                    world.remove::<Hidden>(child);
                }
            }
        }
    }
}
