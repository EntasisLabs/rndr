//! Compile and runtime fixtures for legacy facade names and additive editing namespaces.
use rndr::{edit_buffer, App, Buffer, BufferEditError, Chrome, Editor, Entity, Label, World};
use std::any::TypeId;

#[test]
fn legacy_root_types_and_function_signature_are_unchanged() {
    assert_eq!(TypeId::of::<Buffer>(), TypeId::of::<rndr_core::Buffer>());
    assert_eq!(TypeId::of::<Editor>(), TypeId::of::<rndr_core::Editor>());
    assert_eq!(
        TypeId::of::<rndr::TextSelection>(),
        TypeId::of::<rndr_core::TextSelection>()
    );
    let legacy_edit: fn(
        &mut World,
        Entity,
        std::ops::Range<usize>,
        &str,
    ) -> Result<bool, BufferEditError> = edit_buffer;
    let mut app = App::new();
    app.add_plugin(Chrome);
    let note = app.world_mut().spawn((
        Buffer::from_str("\u{feff}é\r\n"),
        Editor::default(),
        Label::new(""),
    ));
    assert_eq!(legacy_edit(app.world_mut(), note, 3..5, "calm"), Ok(true));
    assert_eq!(
        app.world().get::<Buffer>(note).unwrap().text(),
        "\u{feff}calm\r\n"
    );
}

#[test]
fn additive_document_namespaces_resolve_without_ui_feature() {
    use rndr::editing::{EditOrigin, SourceDocuments};
    use rndr::source_projection::{Projection, ProjectionSelection};
    assert_eq!(
        TypeId::of::<SourceDocuments>(),
        TypeId::of::<rndr_core::editing::SourceDocuments>()
    );
    assert_eq!(
        TypeId::of::<rndr::structured::StructuredDocument>(),
        TypeId::of::<rndr_core::structured::StructuredDocument>()
    );
    let mut docs = SourceDocuments::default();
    let id = docs.open("A *quiet* line\r\n");
    let source = docs.get_mut(id).unwrap();
    let projection = Projection::build(&source.snapshot());
    let target = projection
        .capture(source, ProjectionSelection::inward(2, 7))
        .unwrap();
    target.replace(source, "calm", EditOrigin::Host(1)).unwrap();
    source.undo().unwrap();
    assert_eq!(source.text(), "A *quiet* line\r\n");
}

#[cfg(feature = "ui")]
#[test]
fn opt_in_ui_does_not_shadow_legacy_root_names() {
    assert_eq!(
        TypeId::of::<rndr::Button>(),
        TypeId::of::<rndr_core::Button>()
    );
    assert_ne!(
        TypeId::of::<rndr::Button>(),
        TypeId::of::<rndr::ui::controls::Button>()
    );
    assert_eq!(
        TypeId::of::<rndr::ui::editing::EditorSurface>(),
        TypeId::of::<rndr_ui::editing::EditorSurface>()
    );
    assert_eq!(
        TypeId::of::<rndr::ui::structured::StructuredSurface>(),
        TypeId::of::<rndr_ui::structured::StructuredSurface>()
    );
}
