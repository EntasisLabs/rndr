//! Exact request identity and synchronous mutation-boundary admission.
use crate::*;
use std::any::Any;

/// Application-chosen typed numeric key, never user prose or a target lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LaneKey(pub u16);

/// Native identity is supplied by the mutation owner. Include its domain,
/// session and turn incarnations in D; flow never interprets or substitutes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity<D> {
    pub target: D,
    pub operation: u64,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryMode {
    Once,
    Ordered { recoverable: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestToken {
    pub(crate) activity: ActivityId,
    pub(crate) lane: LaneKey,
    pub(crate) generation: u64,
    pub(crate) world: WorldId,
}
#[derive(Clone, Debug)]
pub struct Request<D> {
    pub(crate) token: RequestToken,
    pub(crate) identity: Identity<D>,
}
impl<D: Clone> Request<D> {
    pub fn token(&self) -> RequestToken {
        self.token
    }
    pub fn identity(&self) -> &Identity<D> {
        &self.identity
    }
    pub fn completion<T>(&self, sequence: Option<u64>, value: T) -> Completion<D, T> {
        Completion {
            request: self.clone(),
            sequence,
            value,
            receipt: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Completion<D, T> {
    request: Request<D>,
    sequence: Option<u64>,
    value: T,
    receipt: Option<Arc<work::DeliveryReceipt>>,
}
impl<D, T> Completion<D, T> {
    pub(crate) fn with_value<U>(self, value: U) -> Completion<D, U> {
        let Completion {
            request,
            sequence,
            receipt,
            ..
        } = self;
        Completion {
            request,
            sequence,
            value,
            receipt,
        }
    }
    pub(crate) fn with_receipt(mut self, status: WorkStatus) -> Self {
        self.receipt = Some(Arc::new(work::DeliveryReceipt::new(status)));
        self
    }
}
impl<D, T, E> Completion<D, Result<T, E>> {
    /// Extract a successful task payload without rebuilding/blessing its
    /// captured identity, sequence or delivery receipt. Handle Err as failure,
    /// never as a native write/save success. Transpose before the commit gate.
    pub fn transpose(self) -> Result<Completion<D, T>, E> {
        let Completion {
            request,
            sequence,
            value,
            receipt,
        } = self;
        Ok(Completion {
            request,
            sequence,
            value: value?,
            receipt,
        })
    }
}
impl<D: Send + Sync + 'static, T: Send + Sync + 'static> rndr_core::Event for Completion<D, T> {}

pub(crate) struct Lane {
    pub generation: u64,
    pub identity: Box<dyn Any>,
    pub mode: DeliveryMode,
    pub consumed: bool,
    pub active: bool,
    pub sequence: Option<u64>,
    // Native source progress advances only from this operation's commit receipt.
    pub native_revision: u64,
}

/// Implement only at the authoritative domain writer, not a selected-tab or
/// Entity projection helper. This synchronous method MUST atomically validate
/// exact native identity/revision and admit/commit, with no user callback or
/// reentrant/async gap. Err must leave domain/history/revisions untouched.
/// The writer receives no coordinator access. Prefer a concrete native adapter.
pub trait CompletionTarget<D, T> {
    type Output;
    type Error;
    fn commit(&mut self, identity: &Identity<D>, value: T) -> Result<Self::Output, Self::Error>;
}
/// Consumer-owned World mutation seam. Implement beside the UI/domain owner;
/// flow never depends on UI, follows selection, or resolves a target for it.
pub trait WorldCompletionTarget<D, T> {
    type Output;
    type Error;
    fn commit(
        &mut self,
        world: &mut World,
        identity: &Identity<D>,
        value: T,
    ) -> Result<Self::Output, Self::Error>;
}
struct WorldWriter<'a, W> {
    world: &'a mut World,
    writer: &'a mut W,
}
impl<D, T, W: WorldCompletionTarget<D, T>> CompletionTarget<D, T> for WorldWriter<'_, W> {
    type Output = W::Output;
    type Error = W::Error;
    fn commit(&mut self, identity: &Identity<D>, value: T) -> Result<Self::Output, Self::Error> {
        self.writer.commit(self.world, identity, value)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum CompletionError<E> {
    Flow(Error),
    Native(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestSuspendError {
    Flow(Error),
    UnrecoverableStream,
}
impl Coordinator {
    /// Detailed F4 admission; legacy F3 suspend also refuses unrecoverable
    /// streams, using its existing Error::Invalid compatibility shape.
    pub fn suspend_requests(&mut self, scope: ScopeId) -> Result<(), RequestSuspendError> {
        let owner = self.live_scope(scope).map_err(RequestSuspendError::Flow)?;
        if owner
            .lanes
            .values()
            .any(|l| l.active && matches!(l.mode, DeliveryMode::Ordered { recoverable: false }))
        {
            return Err(RequestSuspendError::UnrecoverableStream);
        }
        self.suspend(scope, Recovery::ReconcileFromDurableState)
            .map_err(RequestSuspendError::Flow)
    }
    fn request_preflight(
        &self,
        world: &World,
        scope: ScopeId,
        key: LaneKey,
    ) -> Result<(ActivityId, WorldId, u64), Error> {
        let activity = self.activity(scope)?;
        let world_id = world.local::<WorldOwner>().ok_or(Error::Stale)?.id();
        self.live_scope(world_id.0)?;
        // World binding is explicit; a scope in another World may not publish
        // here. Application scopes may target a registered World intentionally.
        let mut cursor = scope;
        loop {
            let s = self.live_scope(cursor)?;
            if s.role == Role::World && cursor != world_id.0 {
                return Err(Error::Stale);
            }
            if let Some(p) = s.parent {
                cursor = p;
            } else {
                break;
            }
        }
        let lanes = &self.scopes[&scope].lanes;
        if !lanes.contains_key(&key) && lanes.len() >= self.work.limits.lanes_per_scope {
            return Err(Error::Capacity);
        }
        let generation = lanes
            .get(&key)
            .map_or(0, |l| l.generation)
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        Ok((activity, world_id, generation))
    }
    pub(crate) fn prepare_request(
        &self,
        world: &World,
        scope: ScopeId,
        key: LaneKey,
    ) -> Result<(ActivityId, WorldId, u64), Error> {
        self.request_preflight(world, scope, key)
    }
    pub(crate) fn install_request<D: Clone + Eq + 'static>(
        &mut self,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        mode: DeliveryMode,
        prepared: (ActivityId, WorldId, u64),
    ) -> Request<D> {
        let (activity, world, generation) = prepared;
        self.scopes.get_mut(&scope).unwrap().lanes.insert(
            key,
            Lane {
                generation,
                identity: Box::new(identity.clone()),
                mode,
                consumed: false,
                active: true,
                sequence: None,
                native_revision: identity.revision,
            },
        );
        self.rescan = true;
        Request {
            token: RequestToken {
                activity,
                lane: key,
                generation,
                world,
            },
            identity,
        }
    }
    /// Metadata-only request. Owned work should use replace_local/background/
    /// timer/feed, which preflight BOTH lane and work capacity before replacement.
    /// Old work is invalidated immediately and cancellation is drained by pump_work.
    pub fn replace_request<D: Clone + Eq + 'static>(
        &mut self,
        world: &World,
        scope: ScopeId,
        key: LaneKey,
        identity: Identity<D>,
        mode: DeliveryMode,
    ) -> Result<Request<D>, Error> {
        let prepared = self.request_preflight(world, scope, key)?;
        Ok(self.install_request(scope, key, identity, mode, prepared))
    }
    /// Cancel an exact lane without allocating a generation/queue entry. Works
    /// even at generation max; a later replacement must still pass exhaustion.
    pub fn cancel_request(&mut self, token: RequestToken) -> Result<(), Error> {
        if !self.activity_is_current(token.activity) {
            return Err(Error::Stale);
        }
        self.live_scope(token.activity.scope)?;
        let lane = self
            .scopes
            .get_mut(&token.activity.scope)
            .unwrap()
            .lanes
            .get_mut(&token.lane)
            .ok_or(Error::Stale)?;
        if lane.generation != token.generation {
            return Err(Error::Stale);
        }
        lane.active = false;
        self.rescan = true;
        Ok(())
    }
    pub fn request_is_current(&self, token: RequestToken) -> bool {
        self.activity_is_current(token.activity)
            && self.live_scope(token.world.0).is_ok()
            && self.scopes[&token.activity.scope]
                .lanes
                .get(&token.lane)
                .is_some_and(|l| l.active && l.generation == token.generation)
    }
    pub fn commit_completion<D: Clone + Eq + 'static, T, W: CompletionTarget<D, T>>(
        &mut self,
        world: &World,
        completion: Completion<D, T>,
        writer: &mut W,
    ) -> Result<W::Output, CompletionError<W::Error>> {
        self.commit_at(
            world.local::<WorldOwner>().map(WorldOwner::id),
            completion,
            writer,
        )
    }
    pub fn commit_world_completion<D: Clone + Eq + 'static, T, W: WorldCompletionTarget<D, T>>(
        &mut self,
        world: &mut World,
        completion: Completion<D, T>,
        writer: &mut W,
    ) -> Result<W::Output, CompletionError<W::Error>> {
        let binding = world.local::<WorldOwner>().map(WorldOwner::id);
        self.commit_at(binding, completion, &mut WorldWriter { world, writer })
    }
    fn commit_at<D: Clone + Eq + 'static, T, W: CompletionTarget<D, T>>(
        &mut self,
        world: Option<WorldId>,
        completion: Completion<D, T>,
        writer: &mut W,
    ) -> Result<W::Output, CompletionError<W::Error>> {
        let Completion {
            request,
            sequence,
            value,
            receipt: _receipt,
        } = completion;
        let token = request.token;
        if world != Some(token.world) || !self.request_is_current(token) {
            return Err(CompletionError::Flow(Error::Stale));
        }
        let lane = &self.scopes[&token.activity.scope].lanes[&token.lane];
        if lane.identity.downcast_ref::<Identity<D>>() != Some(&request.identity) || lane.consumed {
            return Err(CompletionError::Flow(Error::Stale));
        }
        match (lane.mode, sequence) {
            (DeliveryMode::Once, None) => {}
            (DeliveryMode::Ordered { .. }, Some(seq))
                if lane.sequence.is_none_or(|old| seq > old) => {}
            _ => return Err(CompletionError::Flow(Error::Stale)),
        }
        // No callback before native admission/commit and no await. The concrete
        // source adapter below calls the existing native atomic owner directly.
        let output = writer
            .commit(&request.identity, value)
            .map_err(CompletionError::Native)?;
        let lane = self
            .scopes
            .get_mut(&token.activity.scope)
            .unwrap()
            .lanes
            .get_mut(&token.lane)
            .unwrap();
        match lane.mode {
            DeliveryMode::Once => lane.consumed = true,
            DeliveryMode::Ordered { .. } => lane.sequence = sequence,
        }
        Ok(output)
    }
}

/// Source authority, not an Entity/Markdown projection. Session/turn identity is
/// included in D by hosts of other domains; a source document has its native ID.
struct SourceWriter<'a>(&'a mut World, u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCompletionError {
    Operation,
    Native(rndr_core::editing::EditError),
}
impl Coordinator {
    /// Exact SourceDocuments writer, including pending native Input composition
    /// before Chrome has observed it. Focus is consulted ONLY for IME policy,
    /// never to choose the mutation target. Ordered batches supply exact native
    /// base revisions; progress advances only from this lane's successful native
    /// receipts. External edits cannot be silently rebased/blessed. No projection
    /// writes or per-chunk lane replacement.
    pub fn commit_source_completion(
        &mut self,
        world: &mut World,
        completion: Completion<rndr_core::editing::DocumentId, rndr_core::editing::SourceBatch>,
    ) -> Result<Option<rndr_core::editing::ChangeEvent>, CompletionError<SourceCompletionError>>
    {
        let binding = world.local::<WorldOwner>().map(WorldOwner::id);
        let token = completion.request.token;
        let expected = self
            .scopes
            .get(&token.activity.scope)
            .and_then(|s| s.lanes.get(&token.lane))
            .map_or(0, |l| l.native_revision);
        let result = self.commit_at(binding, completion, &mut SourceWriter(world, expected));
        if let Ok(Some(change)) = &result {
            self.scopes
                .get_mut(&token.activity.scope)
                .unwrap()
                .lanes
                .get_mut(&token.lane)
                .unwrap()
                .native_revision = change.after.0;
        }
        result
    }
}
impl CompletionTarget<rndr_core::editing::DocumentId, rndr_core::editing::SourceBatch>
    for SourceWriter<'_>
{
    type Output = Option<rndr_core::editing::ChangeEvent>;
    type Error = SourceCompletionError;
    fn commit(
        &mut self,
        identity: &Identity<rndr_core::editing::DocumentId>,
        batch: rndr_core::editing::SourceBatch,
    ) -> Result<Self::Output, Self::Error> {
        use rndr_core::editing::EditError;
        let native = SourceCompletionError::Native;
        if batch.document != identity.target {
            return Err(native(EditError::MissingDocument));
        }
        if batch.base.0 != self.1 {
            return Err(native(EditError::StaleRevision));
        }
        if batch.origin != rndr_core::editing::EditOrigin::Host(identity.operation) {
            return Err(SourceCompletionError::Operation);
        }
        let composing = self
            .0
            .resource::<rndr_core::Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty());
        let focused_source = self
            .0
            .resource::<rndr_core::Focus>()
            .and_then(|f| f.entity)
            .and_then(|e| rndr_core::editing::attached_source(self.0, e));
        if composing && focused_source == Some(identity.target) {
            return Err(native(EditError::Composing));
        }
        self.0
            .resource_mut::<rndr_core::editing::SourceDocuments>()
            .and_then(|docs| docs.get_mut(identity.target))
            .ok_or(native(EditError::MissingDocument))?
            .commit(batch)
            .map_err(native)
    }
}
