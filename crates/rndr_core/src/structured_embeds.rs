//! Retained atomic extension ownership. Mounts carry no execution permission.
//!
//! A host may display a child editor in its own retained panel. It commits a
//! complete inert payload to the parent history, or takes a foreign-document
//! intent for its own admission. No request here opens URLs or performs IO.
use crate::editing::{EditOrigin, Revision};
use crate::structured::{AtomicPayload, BlockId, Operation, StructuredDocumentId, Transaction};
use crate::structured_native::{
    attached_structured, command_ready, is_composing, project, StructuredDocuments,
    StructuredHandle, StructuredViewError,
};
use crate::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Host-declared ownership of a mounted child; payload text never grants authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicOwnership {
    /// Explicit nested commits use the parent document's transaction/history.
    SameDocument,
    /// Edits remain host intents and cannot mutate parent history through a token.
    ForeignDocument,
}
/// Exact retained child ownership. Recreating an atom by undo never recreates this handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtomicMount {
    root: Entity,
    view: Entity,
    child: Entity,
    block: BlockId,
    document: StructuredHandle,
    identity: StructuredDocumentId,
    generation: u64,
    lifetime: u64,
    attachment: u64,
    ownership: AtomicOwnership,
}
impl Component for AtomicMount {}
/// Async response token. Private fences cannot be forged by a host callback.
#[derive(Debug)]
pub struct AtomicRequest {
    mount: AtomicMount,
    revision: Revision,
    policy_epoch: u64,
    policy: Vec<(Entity, u64)>,
    focus_epoch: u64,
    focused: Option<Entity>,
}
/// Permission-free foreign-document request; the library never executes it.
#[derive(Debug)]
pub struct AtomicHostIntent {
    /// Stable parent block, not a child ECS identity.
    pub block: BlockId,
    /// Inert host metadata to interpret under separate application admission.
    pub payload: AtomicPayload,
    /// Exact request fence for any later parent response.
    pub request: AtomicRequest,
}
fn policy_stamp(world: &World, entity: Entity) -> Vec<(Entity, u64)> {
    let mut result = vec![];
    let mut next = Some(entity);
    while let Some(e) = next {
        if result.iter().any(|(old, _)| *old == e) {
            break;
        }
        result.push((e, world.source_entity_epoch(e)));
        next = world
            .get::<FocusOwner>(e)
            .map(|p| p.0)
            .or_else(|| world.get::<Parent>(e).map(|p| p.entity));
    }
    result
}
impl AtomicMount {
    /// Bind live host-owned root/child entities to an existing atomic node.
    /// Root must own the child through hierarchy; it must not contain the parent view.
    pub fn mount(
        world: &mut World,
        view: Entity,
        block: BlockId,
        root: Entity,
        child: Entity,
    ) -> Result<Self, StructuredViewError> {
        Self::mount_with_ownership(
            world,
            view,
            block,
            root,
            child,
            AtomicOwnership::SameDocument,
        )
    }
    /// Mount a foreign-document child. Its response tokens cannot edit parent history.
    pub fn mount_foreign(
        world: &mut World,
        view: Entity,
        block: BlockId,
        root: Entity,
        child: Entity,
    ) -> Result<Self, StructuredViewError> {
        Self::mount_with_ownership(
            world,
            view,
            block,
            root,
            child,
            AtomicOwnership::ForeignDocument,
        )
    }
    fn mount_with_ownership(
        world: &mut World,
        view: Entity,
        block: BlockId,
        root: Entity,
        child: Entity,
        ownership: AtomicOwnership,
    ) -> Result<Self, StructuredViewError> {
        command_ready(world, view)?;
        if !world.is_alive(root)
            || !world.is_alive(child)
            || root == view
            || ancestor_with::<crate::structured_native::StructuredBinding>(world, child)
                == Some(view)
            || !is_descendant(world, child, root)
            || is_descendant(world, view, root)
        {
            return Err(StructuredViewError::Unavailable);
        }
        let document = attached_structured(world, view).ok_or(StructuredViewError::Unavailable)?;
        let doc = world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(document))
            .ok_or(StructuredViewError::Unavailable)?;
        doc.blocks()
            .iter()
            .find(|b| b.id == block)
            .and_then(|b| b.atomic_payload())
            .ok_or(StructuredViewError::Unavailable)?;
        let identity = doc.id();
        let lifetime = doc
            .atomic_lifetime(block)
            .ok_or(StructuredViewError::Unavailable)?;
        let attachment = crate::structured_native::structured_attachment_epoch(world, view)
            .ok_or(StructuredViewError::Unavailable)?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let generation = NEXT.fetch_add(1, Ordering::Relaxed);
        if let Some(old) = world.get::<AtomicMount>(root).copied() {
            old.detach(world);
        }
        let mount = Self {
            root,
            view,
            child,
            block,
            document,
            identity,
            generation,
            lifetime,
            attachment,
            ownership,
        };
        world.insert(root, mount);
        world.insert(root, FocusOwner(view));
        Ok(mount)
    }
    /// Ownership was selected by the host at mount, never by untrusted payloads.
    pub fn ownership(self) -> AtomicOwnership {
        self.ownership
    }
    /// Child root owned by the application. Detach does not destroy borrowed UI.
    pub fn root(self) -> Entity {
        self.root
    }
    /// Canonical child focus/input target; parent never replays its text events.
    pub fn child(self) -> Entity {
        self.child
    }
    /// Model-stable parent node.
    pub fn block(self) -> BlockId {
        self.block
    }
    /// Replace a clean standalone child draft and reset its local staging history.
    /// Bound source/structured child editors retain their own authority instead.
    pub fn reset_draft(self, world: &mut World, text: &str) -> Result<(), StructuredViewError> {
        self.validate(world)?;
        if world.has::<crate::editing::SourceBinding>(self.child)
            || world.has::<crate::structured_native::StructuredBinding>(self.child)
            || world.has::<ImePreview>(self.child)
        {
            return Err(StructuredViewError::Unavailable);
        }
        let buffer = world
            .get_mut::<Buffer>(self.child)
            .ok_or(StructuredViewError::Unavailable)?;
        crate::editor::replace_buffer(buffer, text, 0, 0);
        world.remove::<crate::typing::EditHistory>(self.child);
        Ok(())
    }
    /// Read current payload after exact attachment and lifetime validation.
    pub fn payload(self, world: &World) -> Result<AtomicPayload, StructuredViewError> {
        self.validate(world)?;
        world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(self.document))
            .and_then(|d| d.blocks().iter().find(|b| b.id == self.block))
            .and_then(|b| b.atomic_payload())
            .ok_or(StructuredViewError::Unavailable)
    }
    /// Validate retained generation, attachment, hierarchy and canonical node identity.
    pub fn validate(self, world: &World) -> Result<(), StructuredViewError> {
        if world.get::<AtomicMount>(self.root) != Some(&self)
            || !world.is_alive(self.child)
            || !is_descendant(world, self.child, self.root)
            || attached_structured(world, self.view) != Some(self.document)
        {
            return Err(StructuredViewError::Unavailable);
        }
        let doc = world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(self.document))
            .ok_or(StructuredViewError::Unavailable)?;
        if doc.id() != self.identity
            || doc.atomic_lifetime(self.block) != Some(self.lifetime)
            || crate::structured_native::structured_attachment_epoch(world, self.view)
                != Some(self.attachment)
            || !doc
                .blocks()
                .iter()
                .any(|b| b.id == self.block && b.atomic_payload().is_some())
        {
            return Err(StructuredViewError::Unavailable);
        }
        Ok(())
    }
    /// Capture a single-use async fence against revision, policy and focus ownership.
    /// Child IME must finish before a complete payload can be published.
    pub fn request(self, world: &World) -> Result<AtomicRequest, StructuredViewError> {
        self.validate(world)?;
        command_ready(world, self.view)?;
        let doc = world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(self.document))
            .ok_or(StructuredViewError::Unavailable)?;
        if doc.is_readonly()
            || access_details(world, self.view).readonly
            || access_details(world, self.child).readonly
        {
            return Err(crate::structured::StructuredError::Readonly.into());
        }
        if !access_details(world, self.view).focus || !access_details(world, self.child).focus {
            return Err(StructuredViewError::Unavailable);
        }
        if world.has::<ImePreview>(self.child)
            || world
                .resource::<Input>()
                .is_some_and(|i| !i.ime_preedit.is_empty())
        {
            return Err(StructuredViewError::Composing);
        }
        let mut policy = policy_stamp(world, self.view);
        policy.extend(policy_stamp(world, self.child));
        Ok(AtomicRequest {
            mount: self,
            revision: doc.revision(),
            policy_epoch: doc.policy_epoch(),
            policy,
            focus_epoch: world.source_focus_epoch,
            focused: world.resource::<Focus>().and_then(|f| f.entity),
        })
    }
    /// Create a foreign-document intent only. Parent/history are untouched.
    pub fn host_intent(
        self,
        world: &World,
        payload: AtomicPayload,
    ) -> Result<AtomicHostIntent, StructuredViewError> {
        Ok(AtomicHostIntent {
            block: self.block,
            payload,
            request: self.request(world)?,
        })
    }
    /// Revoke this generation and return focus only when its child still owns it.
    /// The host tears down its child UI/overlays after this call.
    pub fn detach(self, world: &mut World) {
        if world.get::<AtomicMount>(self.root) != Some(&self) {
            return;
        }
        let focused = world.resource::<Focus>().and_then(|f| f.entity);
        let mut owned = vec![self.root, self.child];
        let mut cursor = 0;
        while cursor < owned.len() {
            let e = owned[cursor];
            cursor += 1;
            for child in children_of(world, e) {
                if !owned.contains(&child) {
                    owned.push(child);
                }
            }
        }
        // Detached popups are logically owned by their anchor, not by Layout.
        // Close through their canonical callback before removing the scope.
        let mut popups = vec![];
        loop {
            let old_len = owned.len();
            for popup in world.entities_with::<AnchoredPopup>() {
                if world
                    .get::<AnchoredPopup>(popup)
                    .is_some_and(|p| owned.contains(&p.anchor))
                    && !owned.contains(&popup)
                {
                    owned.push(popup);
                    popups.push(popup);
                    let mut i = owned.len() - 1;
                    while i < owned.len() {
                        for child in children_of(world, owned[i]) {
                            if !owned.contains(&child) {
                                owned.push(child);
                            }
                        }
                        i += 1;
                    }
                }
            }
            if old_len == owned.len() {
                break;
            }
        }
        let return_focus = focused.is_some_and(|e| owned.contains(&e));
        for popup in popups {
            close_popup(world, popup, false);
            despawn(world, popup);
        }
        world.remove::<AtomicMount>(self.root);
        world.remove::<FocusOwner>(self.root);
        forget_interaction_targets(world, &owned);
        world.remove::<ImePreview>(self.child);
        if return_focus {
            request_focus(world, self.view);
        }
    }
}
impl AtomicRequest {
    /// Publish exactly one same-document nested commit through parent history.
    /// Undo/deletion, remount, blur, policy changes and active preedit refuse.
    pub fn commit(
        self,
        world: &mut World,
        payload: AtomicPayload,
    ) -> Result<bool, StructuredViewError> {
        let mount = self.mount;
        if mount.ownership == AtomicOwnership::ForeignDocument {
            return Err(StructuredViewError::Unavailable);
        }
        mount.validate(world)?;
        command_ready(world, mount.view)?;
        let now = mount.request(world)?;
        if now.revision != self.revision
            || now.policy_epoch != self.policy_epoch
            || now.policy != self.policy
            || now.focus_epoch != self.focus_epoch
            || now.focused != self.focused
        {
            return Err(crate::structured::StructuredError::StaleRevision.into());
        }
        let doc = world
            .resource_mut::<StructuredDocuments>()
            .and_then(|d| d.get_mut(mount.document))
            .ok_or(StructuredViewError::Unavailable)?;
        let changed = doc.apply(Transaction {
            document: doc.id(),
            base: self.revision,
            origin: EditOrigin::Host(0xE603),
            group: None,
            operations: vec![Operation::UpdateAtomic {
                block: mount.block,
                payload,
            }],
        })?;
        project(world, mount.view);
        Ok(changed)
    }
}
fn is_descendant(world: &World, mut child: Entity, root: Entity) -> bool {
    for _ in 0..256 {
        if child == root {
            return true;
        }
        let Some(p) = world.get::<Parent>(child) else {
            return false;
        };
        child = p.entity;
    }
    false
}
/// Revoke deleted/detached atoms and cancel inherited readonly/composition child ownership.
/// UI adapters must also destroy their owned panels after the mount disappears.
pub fn reconcile_atomic_mounts(world: &mut World) {
    for root in world.entities_with::<AtomicMount>() {
        let mount = *world.get::<AtomicMount>(root).unwrap();
        if mount.validate(world).is_err()
            || !interaction_visible(world, mount.view)
            || !access_details(world, mount.view).focus
        {
            mount.detach(world);
            continue;
        }
        let readonly = world
            .resource::<StructuredDocuments>()
            .and_then(|d| d.get(mount.document))
            .is_some_and(|d| d.is_readonly())
            || access_details(world, mount.view).readonly;
        if readonly || is_composing(world, mount.view) {
            mount.detach(world);
        }
    }
}
