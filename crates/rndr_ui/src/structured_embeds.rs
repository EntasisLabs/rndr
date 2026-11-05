//! Host-placed retained atomic editor panels. The parent document renders an
//! inert inline card; this opt-in panel owns a staged child editor separately.
use crate::{Corner, Token, Typography, UiId, UiRecipe, UiStyle};
use rndr_core::structured::{AtomicPayload, BlockId};
use rndr_core::structured_embeds::{AtomicHostIntent, AtomicMount, AtomicOwnership};
use rndr_core::structured_native::StructuredViewError;
use rndr_core::*;

#[derive(Clone)]
struct Panel {
    key: UiId,
    mount: AtomicMount,
    payload: AtomicPayload,
}
impl Component for Panel {}
/// Retained panel and child input identity, distinct from the parent document.
#[derive(Clone, Copy, Debug)]
pub struct AtomicPanelHandle {
    /// Root placed by the host as a normal retained child (not an OS window).
    pub root: Entity,
    /// Staging text editor. Its input/IME/history never edits the parent directly.
    pub editor: Entity,
}
impl AtomicPanelHandle {
    /// Mount or update by a stable host key. The host chooses the containing panel;
    /// child text remains a draft until an explicit same-document commit.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        key: UiId,
        view: Entity,
        block: BlockId,
    ) -> Result<Self, StructuredViewError> {
        Self::mount_with_ownership(
            world,
            parent,
            key,
            view,
            block,
            AtomicOwnership::SameDocument,
        )
    }
    /// Mount a foreign-document child. It emits intents only; committing to the
    /// parent is refused even if a host accidentally calls `commit`.
    pub fn mount_foreign(
        world: &mut World,
        parent: Entity,
        key: UiId,
        view: Entity,
        block: BlockId,
    ) -> Result<Self, StructuredViewError> {
        Self::mount_with_ownership(
            world,
            parent,
            key,
            view,
            block,
            AtomicOwnership::ForeignDocument,
        )
    }
    fn mount_with_ownership(
        world: &mut World,
        parent: Entity,
        key: UiId,
        view: Entity,
        block: BlockId,
        ownership: AtomicOwnership,
    ) -> Result<Self, StructuredViewError> {
        if !world.is_alive(parent) {
            return Err(StructuredViewError::Unavailable);
        }
        if let Some(root) = children_of(world, parent)
            .into_iter()
            .find(|e| world.get::<Panel>(*e).is_some_and(|p| p.key == key))
        {
            let panel = world.get::<Panel>(root).unwrap();
            if panel.mount.block() == block
                && panel.mount.ownership() == ownership
                && panel.mount.validate(world).is_ok()
            {
                let handle = Self {
                    root,
                    editor: panel.mount.child(),
                };
                handle.refresh(world)?;
                return Ok(handle);
            }
            Self {
                root,
                editor: panel.mount.child(),
            }
            .unmount(world);
        }
        let root = world.spawn((Layout::column(),));
        set_parent(world, root, parent);
        world.insert(
            root,
            Layout {
                width: Size::Fill,
                height: Size::Px(150.0),
                padding: 12.0,
                ..Layout::column()
            },
        );
        world.insert(
            root,
            UiRecipe(UiStyle {
                background: Some(Token::Raised.into()),
                corner: Some(Corner::Group),
                ..UiStyle::default()
            }),
        );
        world.insert(
            root,
            Semantics {
                name: Some("Embedded editor — staged until committed".into()),
                ..Default::default()
            },
        );
        let editor = world.spawn((Editor::default(),));
        set_parent(world, editor, root);
        world.insert(
            editor,
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
        );
        world.insert(editor, Buffer::from_str(""));
        world.insert(editor, Editor::default());
        world.insert(editor, Label::new(""));
        world.insert(
            editor,
            UiRecipe(UiStyle {
                typography: Some(Typography::Reading),
                ..UiStyle::editor()
            }),
        );
        world.insert(editor, Focusable);
        world.insert(
            editor,
            Semantics {
                name: Some("Embedded text; Escape returns to parent".into()),
                ..Default::default()
            },
        );
        let result = if ownership == AtomicOwnership::ForeignDocument {
            AtomicMount::mount_foreign(world, view, block, root, editor)
        } else {
            AtomicMount::mount(world, view, block, root, editor)
        };
        let mount = match result {
            Ok(mount) => mount,
            Err(e) => {
                despawn(world, root);
                return Err(e);
            }
        };
        let payload = mount.payload(world)?;
        mount.reset_draft(world, &payload.fallback)?;
        world.insert(
            root,
            Panel {
                key,
                mount,
                payload,
            },
        );
        Ok(Self { root, editor })
    }
    /// Verify exact retained ownership. A stale UI handle never targets a remount.
    pub fn validate(self, world: &World) -> Result<(), StructuredViewError> {
        let panel = world
            .get::<Panel>(self.root)
            .ok_or(StructuredViewError::Unavailable)?;
        if panel.mount.child() != self.editor {
            return Err(StructuredViewError::Unavailable);
        }
        panel.mount.validate(world)
    }
    /// Update a clean draft after host payload changes; preserve a dirty draft and
    /// reject conflicting parent changes rather than silently overwriting either.
    pub fn refresh(self, world: &mut World) -> Result<(), StructuredViewError> {
        self.validate(world)?;
        let panel = world.get::<Panel>(self.root).unwrap().clone();
        let payload = panel.mount.payload(world)?;
        if payload == panel.payload {
            return Ok(());
        }
        let draft = world
            .get::<Buffer>(self.editor)
            .ok_or(StructuredViewError::Unavailable)?
            .text()
            .to_owned();
        if draft != panel.payload.fallback {
            return Err(rndr_core::structured::StructuredError::StaleRevision.into());
        }
        panel.mount.reset_draft(world, &payload.fallback)?;
        world.get_mut::<Panel>(self.root).unwrap().payload = payload;
        Ok(())
    }
    /// Transfer focus to the child only through normal visible/enabled admission.
    pub fn focus(self, world: &mut World) -> Result<(), StructuredViewError> {
        self.validate(world)?;
        if !request_focus(world, self.editor) {
            return Err(StructuredViewError::Unavailable);
        }
        Ok(())
    }
    /// One explicit same-document nested commit is one parent undo entry.
    pub fn commit(self, world: &mut World) -> Result<bool, StructuredViewError> {
        self.validate(world)?;
        let panel = world.get::<Panel>(self.root).unwrap().clone();
        if panel.mount.payload(world)? != panel.payload {
            return Err(rndr_core::structured::StructuredError::StaleRevision.into());
        }
        let text = world
            .get::<Buffer>(self.editor)
            .ok_or(StructuredViewError::Unavailable)?
            .text()
            .to_owned();
        let mut payload = panel.payload;
        payload.fallback = text;
        let request = panel.mount.request(world)?;
        let changed = request.commit(world, payload.clone())?;
        world.get_mut::<Panel>(self.root).unwrap().payload = payload;
        Ok(changed)
    }
    /// Return a foreign-document intent to the host, with no parent/history edit
    /// and no implicit disk/network dispatch.
    pub fn foreign_intent(self, world: &World) -> Result<AtomicHostIntent, StructuredViewError> {
        self.validate(world)?;
        let panel = world.get::<Panel>(self.root).unwrap();
        let mut payload = panel.payload.clone();
        payload.fallback = world
            .get::<Buffer>(self.editor)
            .ok_or(StructuredViewError::Unavailable)?
            .text()
            .to_owned();
        panel.mount.host_intent(world, payload)
    }
    /// Revoke callbacks, return owned focus and destroy the complete child UI.
    pub fn unmount(self, world: &mut World) {
        let Some(panel) = world.get::<Panel>(self.root) else {
            return;
        };
        if panel.mount.child() != self.editor {
            return;
        }
        let mount = panel.mount;
        mount.detach(world);
        despawn(world, self.root);
    }
}
/// Teardown invalid mounts before input/layout. Never retain hidden child focus,
/// captured IME, toolbar actions, or a popup from a deleted parent atom.
pub(crate) fn reconcile(world: &mut World) {
    rndr_core::structured_embeds::reconcile_atomic_mounts(world);
    for root in world.entities_with::<Panel>() {
        let panel = world.get::<Panel>(root).unwrap();
        let handle = AtomicPanelHandle {
            root,
            editor: panel.mount.child(),
        };
        if handle.validate(world).is_err() {
            handle.unmount(world);
        } else {
            // Clean drafts follow parent undo/host updates; dirty conflicts and
            // active preedit stay untouched and explicit commit will refuse.
            let _ = handle.refresh(world);
        }
    }
}
struct ExitRequested;
impl Component for ExitRequested {}
/// Capture only an owned child Escape before generic routing. Other chronological
/// text stays with the child for this frame; teardown happens after editor input.
pub(crate) fn capture_exit(world: &mut World) {
    reconcile(world);
    let Some(focused) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return;
    };
    let Some(root) = world.entities_with::<Panel>().into_iter().find(|root| {
        world
            .get::<Panel>(*root)
            .is_some_and(|p| p.mount.child() == focused)
    }) else {
        return;
    };
    let escape = world.resource::<Input>().is_some_and(|i| {
        i.keys.iter().any(|k| {
            !k.released && !k.synthetic && k.logical == LogicalKey::Named(NamedKey::Escape)
        }) || (i.keys.is_empty() && i.key == Some(Key::Escape))
    });
    if escape {
        world.insert(root, ExitRequested);
        if let Some(input) = world.resource_mut::<Input>() {
            input
                .keys
                .retain(|k| k.logical != LogicalKey::Named(NamedKey::Escape));
            if input.key == Some(Key::Escape) {
                input.key = None;
            }
        }
    }
}
/// After core input has consumed the ordered child event queue, Escape returns
/// focus. The parent input pass is already complete, so queued text cannot leak.
pub(crate) fn prepare_input(world: &mut World) {
    reconcile(world);
    let Some(root) = world
        .entities_with::<ExitRequested>()
        .into_iter()
        .find(|r| world.has::<Panel>(*r))
    else {
        return;
    };
    let mount = world.get::<Panel>(root).unwrap().mount;
    // Detach clears preedit/capture and returns focus before the host destroys UI.
    AtomicPanelHandle {
        root,
        editor: mount.child(),
    }
    .unmount(world);
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.key = None;
        input.chars.clear();
        input.ime_commit.clear();
        input.ime_preedit.clear();
    }
}
