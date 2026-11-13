use rndr_core::{access_id, access_tree, Icon, Label, World};

#[test]
fn inline_icons_leave_the_accessible_name_and_icon_only_labels_are_omitted() {
    let mut world = World::new();
    let mixed = format!("Save {} file", Icon::Check.scalar());
    world.spawn((Label::new(mixed),));
    let decorative = world.spawn((Label::new(Icon::X.scalar().to_string()),));
    let tree = access_tree(&world);
    let names: Vec<&str> = tree.nodes.iter().map(|node| node.name.as_str()).collect();
    assert!(names.contains(&"Save file"), "{names:?}");
    assert!(names
        .iter()
        .all(|name| !name.chars().any(rndr_core::is_icon_scalar)));
    let hidden = access_id(decorative);
    assert!(tree.nodes.iter().all(|node| node.id != hidden));
}

#[test]
fn stripping_icons_keeps_unrelated_spacing() {
    assert_eq!(rndr_core::strip_icon_scalars("  Save  "), "  Save  ");
    assert_eq!(
        rndr_core::strip_icon_scalars(&format!("{}Reject", Icon::X.scalar())),
        "Reject"
    );
}
