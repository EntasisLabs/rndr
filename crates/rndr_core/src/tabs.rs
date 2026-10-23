use crate::hierarchy::children_of;
use crate::input::Input;
use crate::layout::{ancestor_with, hit_test, Clip, Rect};
use crate::world::{Component, World};

impl Component for Tabs {}
impl Component for Tab {}
impl Component for TabPanel {}
impl Component for Hidden {}

/// Parent of [`Tab`] headers. `selected` is the visible [`Tab::id`].
///
/// [`crate::Chrome`] writes `selected` from a press on a header and hides
/// the other panels. A press whose hit sits inside a [`TabPanel`] leaves
/// `selected` alone. Each [`Tab`] is a child of this entity, and each
/// [`TabPanel`] is a child of its [`Tab`].
#[derive(Clone, Copy, Debug)]
pub struct Tabs {
    /// [`Tab::id`] of the panel that stays visible.
    pub selected: u64,
}

/// Clickable header. Its [`TabPanel`] children hide when this id is not selected.
///
/// [`crate::Chrome`] inserts [`Hidden`] on each direct [`TabPanel`] child
/// while this id differs from [`Tabs::selected`], and removes [`Hidden`]
/// when they match.
#[derive(Clone, Copy, Debug)]
pub struct Tab {
    /// Compared with [`Tabs::selected`].
    pub id: u64,
}

/// Body shown only while the parent [`Tab`] is selected.
///
/// Place it as a direct child of that [`Tab`]. [`Hidden`] covers the panel
/// and its descendants while another tab is selected.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabPanel;

/// Skipped by layout, hit-testing, and paint.
///
/// Descendants are skipped with the node. [`crate::Chrome`] clears the rect
/// and clip of a hidden [`TabPanel`]. Remove [`Hidden`] and the next layout
/// pass places the subtree again.
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
