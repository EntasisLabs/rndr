use std::sync::{Arc, Mutex};

use rndr_core::hotkey::HotkeyId;

pub(super) struct PressQueue {
    ids: Mutex<Vec<HotkeyId>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl PressQueue {
    pub(super) fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            ids: Mutex::new(Vec::new()),
            wake,
        }
    }

    pub(super) fn push(&self, id: HotkeyId) {
        self.ids
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(id);
        (self.wake)();
    }

    pub(super) fn drain(&self) -> Vec<HotkeyId> {
        std::mem::take(&mut *self.ids.lock().unwrap_or_else(|err| err.into_inner()))
    }
}
