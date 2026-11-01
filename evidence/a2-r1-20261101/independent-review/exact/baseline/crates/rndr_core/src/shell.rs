use crate::app::{App, Plugin, Schedule};
use crate::caret::{apply_caret, Editing};
use crate::focus::apply_focus;
use crate::fs::{apply_dirty_marks, apply_fs_edit, apply_fs_load, apply_fs_save};
use crate::input::Focus;
use crate::layout::{apply_scroll, ScrollDrag};
use crate::pane::{apply_pane, Selection};
use crate::split::{apply_split, Drag};

/// Desktop interaction systems: scroll, split, focus, pane, caret, and files.
///
/// Inserts [`crate::Focus`], [`crate::Selection`], [`crate::Editing`],
/// [`crate::Drag`], and [`crate::ScrollDrag`], replacing any previous
/// values. On [`crate::Schedule::Update`] the order is scroll, split, focus,
/// file load, pane, caret, file save, file edit, then dirty marks. Add
/// [`crate::Chrome`] for tabs, buttons, and text fields. File bytes come
/// from a [`crate::Files`] resource, which this plugin does not insert.
pub struct Shell;

impl Plugin for Shell {
    fn build(self, app: &mut App) {
        app.world_mut().insert_resource(Focus::default());
        app.world_mut().insert_resource(Selection::default());
        app.world_mut().insert_resource(Editing::default());
        app.world_mut().insert_resource(Drag::default());
        app.world_mut().insert_resource(ScrollDrag::default());
        app.add_system(Schedule::Update, apply_scroll);
        app.add_system(Schedule::Update, apply_split);
        app.add_system(Schedule::Update, apply_focus);
        app.add_system(Schedule::Update, apply_fs_load);
        app.add_system(Schedule::Update, apply_pane);
        app.add_system(Schedule::Update, apply_caret);
        app.add_system(Schedule::Update, apply_fs_save);
        app.add_system(Schedule::Update, apply_fs_edit);
        app.add_system(Schedule::Update, apply_dirty_marks);
    }
}
