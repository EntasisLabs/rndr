//! Experimental retained source editing on core's document-owned sessions.
//!
//! This is one view per document in E1, not structured editing or a persistence
//! service. Install `UiPlugin`, and explicitly use `apply_theme_bridge` for core
//! caret/selection colors. Native IME and accessibility need platform review.
use crate::{Token, Typography, UiError, UiId, UiRecipe, UiStyle};
use rndr_core::editing::{attach_source, detach_source, DocumentId, EditError, SourceDocuments};
use rndr_core::*;

/// Properties for one retained source view. The document never changes in place.
#[derive(Clone, Debug)]
pub struct EditorSurfaceProps {
    /// Session opened by the host in `SourceDocuments`.
    pub document: DocumentId,
    /// Required persistent accessible name, distinct from source text.
    pub name: String,
    /// Supplementary accessible help.
    pub description: String,
    /// Authoritative session readonly setting; navigation remains enabled.
    pub readonly: bool,
    /// Show a sibling line-number gutter.
    pub gutter: bool,
}
impl EditorSurfaceProps {
    /// Editable view with line numbers and a persistent name.
    pub fn new(document: DocumentId, name: impl Into<String>) -> Self {
        Self {
            document,
            name: name.into(),
            description: String::new(),
            readonly: false,
            gutter: true,
        }
    }
}
/// Failure from retained lifecycle validation or core's session boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorSurfaceError {
    /// Invalid parent, identity, props or stale/malformed handle.
    Ui(UiError),
    /// Document missing, already attached, composing or otherwise unavailable.
    Edit(EditError),
}
impl From<UiError> for EditorSurfaceError {
    fn from(error: UiError) -> Self {
        Self::Ui(error)
    }
}
impl From<EditError> for EditorSurfaceError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}
#[derive(Clone, Copy)]
struct Node {
    id: UiId,
    document: DocumentId,
    editor: Entity,
    gutter: Option<Entity>,
}
impl Component for Node {}
/// Read identity through the library's unified adapter.
pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<Node>(entity).map(|node| node.id)
}
/// Retained root plus canonical core editing entity; no parallel input route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSurfaceHandle {
    /// Layout root owned by this surface.
    pub root: Entity,
    /// Canonical focus/input target. Do not remove/reparent its components.
    pub editor: Entity,
}
/// Mount/update entry point for source sessions.
pub struct EditorSurface;
impl EditorSurface {
    /// Mount or update under a parent-scoped key. Updates preserve entity,
    /// selection, viewport and document history. Changing documents requires
    /// explicit destroy/remount; an incompatible key is rejected, not rebound.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: EditorSurfaceProps,
    ) -> Result<EditorSurfaceHandle, EditorSurfaceError> {
        if !world.is_alive(parent) || !world.has::<Layout>(parent) {
            return Err(UiError::Stale.into());
        }
        if props.name.trim().is_empty() {
            return Err(UiError::InvalidProps.into());
        }
        let existing = children_of(world, parent)
            .into_iter()
            .find(|e| crate::identity(world, *e) == Some(id));
        let old = if let Some(root) = existing {
            let node = *world.get::<Node>(root).ok_or(UiError::IdentityConflict)?;
            if node.document != props.document {
                return Err(UiError::IdentityConflict.into());
            }
            let handle = EditorSurfaceHandle {
                root,
                editor: node.editor,
            };
            handle.validate(world)?;
            Some(handle)
        } else {
            None
        };
        // Validate before allocating or changing any visual state. Readonly is
        // owned by the session, not a second view-local mutation permission.
        let session = world
            .resource_mut::<SourceDocuments>()
            .and_then(|docs| docs.get_mut(props.document))
            .ok_or(UiError::InvalidProps)?;
        let previous_readonly = session.is_readonly();
        session.set_readonly(props.readonly)?;
        let handle = if let Some(handle) = old {
            handle
        } else {
            let root = world.spawn((Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::row()
            },));
            let editor = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Fill,
                    ..Layout::column()
                },
                Label::new(""),
                Buffer::from_str(""),
                Editor::default(),
                CapturesKeys,
                Focusable,
            ));
            world.insert(editor, UiRecipe(UiStyle::editor()));
            set_parent(world, editor, root);
            if let Err(error) = attach_source(world, editor, props.document) {
                despawn(world, root);
                // Attach rejection (e.g. a second view) must not change the
                // permissions of the view already attached to this session.
                if let Some(session) = world
                    .resource_mut::<SourceDocuments>()
                    .and_then(|docs| docs.get_mut(props.document))
                {
                    let _ = session.set_readonly(previous_readonly);
                }
                return Err(error.into());
            }
            set_parent(world, root, parent);
            world.insert(
                root,
                Node {
                    id,
                    document: props.document,
                    editor,
                    gutter: None,
                },
            );
            EditorSurfaceHandle { root, editor }
        };
        world.insert(
            handle.editor,
            Semantics {
                name: Some(props.name),
                description: props.description,
                readonly: props.readonly,
            },
        );
        let gutter = world.get::<Node>(handle.root).unwrap().gutter;
        match (props.gutter, gutter) {
            (true, None) => {
                let gutter = world.spawn((
                    Layout {
                        width: Size::Px(56.0),
                        height: Size::Fill,
                        ..Layout::column()
                    },
                    Label::new(""),
                    Gutter,
                    UiRecipe(UiStyle {
                        background: Some(Token::Content.into()),
                        foreground: Some(Token::Secondary.into()),
                        typography: Some(Typography::Code),
                        ..UiStyle::default()
                    }),
                ));
                // Core line-number sync finds the sibling; order gutter first.
                set_parent(world, gutter, handle.root);
                if let Some(children) = world.get_mut::<Children>(handle.root) {
                    children.list.retain(|e| *e != gutter);
                    children.list.insert(0, gutter);
                }
                world.get_mut::<Node>(handle.root).unwrap().gutter = Some(gutter);
            }
            (false, Some(gutter)) => {
                despawn(world, gutter);
                world.get_mut::<Node>(handle.root).unwrap().gutter = None;
            }
            _ => {}
        }
        Ok(handle)
    }
}
impl EditorSurfaceHandle {
    /// Check entity generations, component shape and parent relationships.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        let node = world.get::<Node>(self.root).ok_or(UiError::Stale)?;
        if node.editor != self.editor
            || !world.has::<Layout>(self.root)
            || !world.has::<Editor>(self.editor)
            || !world.has::<Buffer>(self.editor)
            || rndr_core::editing::attached_source(world, self.editor) != Some(node.document)
            || !world.has::<Label>(self.editor)
            || world.get::<Parent>(self.editor).map(|p| p.entity) != Some(self.root)
            || node.gutter.is_some_and(|g| {
                !world.has::<Gutter>(g)
                    || !world.has::<Label>(g)
                    || world.get::<Parent>(g).map(|p| p.entity) != Some(self.root)
            })
        {
            return Err(UiError::Stale);
        }
        Ok(())
    }
    /// Detach through core before freeing the view. History stays in the
    /// document; failed detach (such as composition) leaves the surface alive.
    /// Caller-attached root children are borrowed and detached, not destroyed.
    pub fn destroy(self, world: &mut World) -> Result<(), EditorSurfaceError> {
        self.validate(world)?;
        detach_source(world, self.editor)?;
        let node = *world.get::<Node>(self.root).unwrap();
        for child in children_of(world, self.root) {
            if child != self.editor && Some(child) != node.gutter {
                world.remove::<Parent>(child);
                if let Some(children) = world.get_mut::<Children>(self.root) {
                    children.list.retain(|e| *e != child);
                }
            }
        }
        forget_interaction_targets(world, &[self.root, self.editor]);
        despawn(world, self.root);
        Ok(())
    }
}
