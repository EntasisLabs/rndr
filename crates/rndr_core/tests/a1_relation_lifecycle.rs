use rndr_core::*;

#[test]
fn invalidated_tab_structure_rejects_actions_and_hides_panel() {
    for remove_tab in [false, true] {
        let mut app = App::new();
        app.add_plugin(GenericInteraction);
        let group = app.world_mut().spawn((Layout::row(), Tabs { selected: 1 }));
        let tab = app.world_mut().spawn((Layout::column(), Tab { id: 1 }));
        set_parent(app.world_mut(), tab, group);
        let panel = app.world_mut().spawn((Layout::column(), TabPanel));
        link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
        if remove_tab {
            app.world_mut().remove::<Tab>(tab);
        } else {
            let other = app.world_mut().spawn((Layout::row(), Tabs { selected: 2 }));
            set_parent(app.world_mut(), tab, other);
        }
        assert!(!access_details(app.world(), tab).activate);
        assert_eq!(access_details(app.world(), tab).controls, None);
        assert_eq!(access_details(app.world(), panel).labelled_by, None);
        assert_eq!(
            try_activate(app.world_mut(), tab),
            ActivationResult::Rejected
        );
        app.update();
        assert!(!interaction_visible(app.world(), panel));
    }
}
