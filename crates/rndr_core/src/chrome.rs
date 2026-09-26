use crate::action::{apply_actions, Actions, Fired};
use crate::app::{App, Plugin, Schedule};
use crate::button::apply_buttons;
use crate::field::apply_fields;
use crate::overlay::apply_overlay;
use crate::tabs::apply_tabs;

/// Opt-in widget systems. The file-tree [`crate::Shell`] does not register these.
pub struct Chrome;

impl Plugin for Chrome {
    fn build(self, app: &mut App) {
        if !app.world().has_resource::<Actions>() {
            app.world_mut().insert_resource(Actions::default());
        }
        if !app.world().has_resource::<Fired>() {
            app.world_mut().insert_resource(Fired::default());
        }
        app.add_system_front(Schedule::Update, apply_chrome);
        app.add_system(Schedule::PostUpdate, apply_fields);
    }
}

fn apply_chrome(world: &mut crate::world::World) {
    apply_actions(world);
    apply_overlay(world);
    apply_buttons(world);
    apply_tabs(world);
}
