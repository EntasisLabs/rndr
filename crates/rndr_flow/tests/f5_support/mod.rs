#![allow(dead_code)]
use rndr_core::editing::{self, EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments};
use rndr_core::{App, CloseGuard};
use rndr_flow::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Document(u32),
}
pub type History = Retained<Route, ProtectedTarget, FrozenDocuments>;
pub struct Fixture {
    pub flow: Coordinator,
    pub root: ScopeOwner,
    pub app: App,
    pub world: WorldId,
    pub tab: ScopeId,
    pub host: ProtectionHost,
    pub history: History,
    pub target: ProtectedTarget,
}
impl Fixture {
    pub fn new() -> Self {
        let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
        let owner = flow.register_world().unwrap();
        let world = owner.id();
        let window = owner.scope();
        let mut app = App::new();
        owner.bind(app.world_mut()).unwrap();
        app.world_mut().insert_resource(CloseGuard::default());
        let tab_owner = flow.create_scope(window).unwrap();
        let mut history = History::new(HistoryLimits::default()).unwrap();
        let tab = history.add_tab(&flow, world, tab_owner).unwrap();
        let mut docs = SourceDocuments::default();
        let native = docs.open("unsaved α\nexact bytes");
        app.world_mut().insert_resource(docs);
        let document = flow.create_document().unwrap();
        let target = ProtectedTarget {
            document,
            manager: world,
            native: NativeDocument::Source(native),
        };
        let mut host = ProtectionHost::new(64).unwrap();
        host.register(target, None).unwrap();
        Self {
            flow,
            root,
            app,
            world,
            tab,
            host,
            history,
            target,
        }
    }
    pub fn entry(&mut self, n: u32) -> RetainedEntry<Route, ProtectedTarget> {
        self.entry_for(self.tab, n, self.target)
    }
    pub fn entry_for(
        &mut self,
        tab: ScopeId,
        n: u32,
        target: ProtectedTarget,
    ) -> RetainedEntry<Route, ProtectedTarget> {
        let owner = self.flow.create_scope(tab).unwrap();
        let lease = self.flow.lease(owner.id(), target.document).unwrap();
        RetainedEntry {
            route: Route::Document(n),
            document: target,
            owner,
            lease,
        }
    }
    pub fn world_scope(&self) -> ScopeId {
        self.app.world().local::<WorldOwner>().unwrap().scope()
    }
    pub fn new_target(&mut self) -> ProtectedTarget {
        let native = self
            .app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .open("independent");
        let document = self.flow.create_document().unwrap();
        let target = ProtectedTarget {
            document,
            manager: self.world,
            native: NativeDocument::Source(native),
        };
        self.host.register(target, None).unwrap();
        target
    }
    pub fn tick(&mut self) {
        let mut worlds = [HostWorld::new(self.world, self.app.world_mut()).unwrap()];
        self.history
            .tick(&mut self.host.pass(&self.flow, &mut worlds));
    }
    pub fn choose(&mut self, id: OperationId, choice: Choice) -> Result<(), Error> {
        let decision = self.history.decision(id).ok_or(Error::Stale)?;
        let mut worlds = [HostWorld::new(self.world, self.app.world_mut()).unwrap()];
        self.history.choose(
            &mut self.host.pass(&self.flow, &mut worlds),
            decision,
            choice,
        )
    }
    pub fn push(&mut self, n: u32) -> ScopeId {
        let entry = self.entry(n);
        let id = entry.id();
        let after = self.history.selected(self.tab).unwrap();
        let op = self
            .history
            .sender()
            .submit(Navigation::Push {
                tab: self.tab,
                after,
                entry,
            })
            .unwrap();
        self.tick();
        assert_eq!(self.history.phase(op), Some(Phase::Committed));
        id
    }
    pub fn source(&self) -> &editing::SourceSession {
        let NativeDocument::Source(id) = self.target.native else {
            panic!()
        };
        self.app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
    }
    pub fn source_mut(&mut self) -> &mut editing::SourceSession {
        let NativeDocument::Source(id) = self.target.native else {
            panic!()
        };
        self.app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap()
    }
    pub fn batch(&self) -> SourceBatch {
        let NativeDocument::Source(id) = self.target.native else {
            panic!()
        };
        SourceBatch {
            document: id,
            base: self.source().revision(),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "EDIT".into(),
            }],
        }
    }
    pub fn close(&mut self, entries: Vec<ScopeId>) -> OperationId {
        let id = self
            .history
            .sender()
            .submit(Navigation::Close { entries })
            .unwrap();
        self.tick();
        id
    }
    pub fn save<F>(&mut self, id: OperationId, ticket: SaveTicket, f: F) -> Result<(), Error>
    where
        F: FnOnce(&rndr_core::World, NativeDocument) -> Result<(), SaveFailure>,
    {
        let mut worlds = [HostWorld::new(self.world, self.app.world_mut()).unwrap()];
        self.host.pass(&self.flow, &mut worlds).save(
            self.history.guard_mut(id).ok_or(Error::Stale)?,
            ticket,
            f,
        )
    }
    pub fn snapshot(
        &self,
    ) -> (
        String,
        Revision,
        usize,
        editing::SourceSelection,
        Vec<ScopeId>,
        Option<ScopeId>,
        Counts,
        WorkCounts,
    ) {
        let d = self.source();
        (
            d.text().into(),
            d.revision(),
            d.history_retained_bytes(),
            d.selection(),
            self.history
                .entries(self.tab)
                .unwrap()
                .iter()
                .map(|e| e.id())
                .collect(),
            self.history.selected(self.tab).unwrap(),
            self.flow.counts(),
            self.flow.work_counts(),
        )
    }
}
