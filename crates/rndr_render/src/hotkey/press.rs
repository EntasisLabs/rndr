use std::sync::{Arc, Mutex};

use rndr_core::hotkey::{HotkeyEdge, HotkeyId};

pub(super) struct PressQueue {
    edges: Mutex<Vec<HotkeyEdge>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl PressQueue {
    pub(super) fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            edges: Mutex::new(Vec::new()),
            wake,
        }
    }

    pub(super) fn push(&self, id: HotkeyId) {
        self.push_edge(HotkeyEdge::Press(id));
    }

    /// Windows `RegisterHotKey` has no key-up, so this stays unused there.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub(super) fn push_release(&self, id: HotkeyId) {
        self.push_edge(HotkeyEdge::Release(id));
    }

    fn push_edge(&self, edge: HotkeyEdge) {
        self.edges
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(edge);
        (self.wake)();
    }

    pub(super) fn drain(&self) -> Vec<HotkeyEdge> {
        std::mem::take(&mut *self.edges.lock().unwrap_or_else(|err| err.into_inner()))
    }
}
