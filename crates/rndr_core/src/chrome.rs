use crate::action::{apply_actions, Actions, Fired};
use crate::app::{App, Plugin, Schedule};
use crate::button::apply_buttons;
use crate::clipboard::{apply_clipboard, copy_binding, cut_binding, paste_binding, Clipboard};
use crate::editor::apply_editor;
use crate::field::apply_fields;
use crate::list::apply_lists;
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
        if !app.world().has_resource::<Clipboard>() {
            app.world_mut().insert_resource(Clipboard::default());
        }
        {
            let actions = app.world_mut().resource_mut::<Actions>().unwrap();
            actions.bindings.push(copy_binding());
            actions.bindings.push(cut_binding());
            actions.bindings.push(paste_binding());
        }
        app.add_system_front(Schedule::Update, apply_chrome);
        app.add_system(Schedule::PostUpdate, apply_fields);
        app.add_system(Schedule::PostUpdate, apply_lists);
        app.add_system(Schedule::PostUpdate, apply_editor);
        app.add_system(Schedule::PostUpdate, apply_clipboard);
    }
}

fn apply_chrome(world: &mut crate::world::World) {
    apply_actions(world);
    apply_overlay(world);
    apply_buttons(world);
    apply_tabs(world);
}
