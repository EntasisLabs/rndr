//! Saving through a host that answers later.
//!
//! A [`CanvasEditor`](crate::editor::CanvasEditor) opened with
//! [`CanvasEditor::remote`](crate::editor::CanvasEditor::remote) never calls a
//! store. Each finished edit shows at once, as the confirmed document with
//! every unconfirmed edit applied on top, and joins an outbox. The host takes
//! requests with
//! [`take_outgoing`](crate::editor::CanvasEditor::take_outgoing), sends them
//! however it likes (an async client, a worker thread, a socket), and reports
//! back with [`confirm`](crate::editor::CanvasEditor::confirm) or
//! [`fail`](crate::editor::CanvasEditor::fail).
//!
//! One request is in flight at a time. An edit takes its expected revision
//! when it leaves the queue, from the last confirmed document, so edits made
//! while another is saving chain correctly. A failed request keeps its id, so
//! [`retry`](crate::editor::CanvasEditor::retry) is idempotent on a store that
//! keeps receipts. A stale revision drops the unconfirmed edits and asks for
//! the current document. Undo and redo are not applied locally: the store owns
//! history, so they wait until nothing else is unconfirmed.

use rndr_canvas::{
    apply_operations, CanvasError, HistoryRequest, Mutation, Operation, Transaction,
};
use std::collections::VecDeque;

/// A request for the host to send for the document being edited.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    /// Read the document and pass it to
    /// [`replace`](crate::editor::CanvasEditor::replace).
    Get,
    Transact(Transaction),
    History(HistoryRequest),
}

impl Outgoing {
    /// The idempotency id the reply is matched by. `Get` has none.
    pub fn id(&self) -> Option<&str> {
        match self {
            Outgoing::Get => None,
            Outgoing::Transact(t) => Some(&t.id),
            Outgoing::History(h) => Some(&h.id),
        }
    }
}

#[derive(Debug, Clone)]
struct Edit {
    id: String,
    label: String,
    operations: Vec<Operation>,
}

#[derive(Debug, Clone)]
pub(crate) struct Remote {
    /// The last document the host confirmed.
    pub(crate) confirmed: Mutation,
    queue: VecDeque<Edit>,
    in_flight: Option<Outgoing>,
    /// The in-flight request has been handed to the host and not answered.
    sent: bool,
    /// The host should read the document.
    wants_get: bool,
    get_sent: bool,
    /// The last send failed without an answer from the store.
    stalled: bool,
}

impl Remote {
    pub(crate) fn new(confirmed: Mutation) -> Self {
        Self {
            confirmed,
            queue: VecDeque::new(),
            in_flight: None,
            sent: false,
            wants_get: false,
            get_sent: false,
            stalled: false,
        }
    }

    pub(crate) fn stalled(&self) -> bool {
        self.stalled
    }

    /// Unconfirmed edits, in flight first.
    fn edits(&self) -> impl Iterator<Item = &[Operation]> {
        let flying = match &self.in_flight {
            Some(Outgoing::Transact(t)) => Some(t.operations.as_slice()),
            _ => None,
        };
        flying
            .into_iter()
            .chain(self.queue.iter().map(|e| e.operations.as_slice()))
    }

    pub(crate) fn pending(&self) -> bool {
        self.in_flight.is_some() || !self.queue.is_empty()
    }

    pub(crate) fn busy(&self) -> bool {
        self.pending() || self.wants_get
    }

    /// The confirmed document with every unconfirmed edit applied. An edit
    /// that no longer applies is skipped here; the store refuses it too.
    pub(crate) fn shown(&self) -> Mutation {
        let mut shown = self.confirmed.clone();
        let mut edited = false;
        for ops in self.edits() {
            if let Ok(next) = apply_operations(&shown.document, ops) {
                shown.document = next;
                edited = true;
            }
        }
        if edited {
            shown.can_undo = true;
            shown.can_redo = false;
            shown.undo_label =
                self.queue
                    .back()
                    .map(|e| e.label.clone())
                    .or_else(|| match &self.in_flight {
                        Some(Outgoing::Transact(t)) => Some(t.label.clone()),
                        _ => None,
                    });
            shown.redo_label = None;
        }
        // History waits on confirmation, so nothing is undoable mid-request.
        if matches!(self.in_flight, Some(Outgoing::History(_))) {
            shown.can_undo = false;
            shown.can_redo = false;
        }
        shown
    }

    pub(crate) fn push(&mut self, id: String, label: &str, operations: Vec<Operation>) {
        self.queue.push_back(Edit {
            id,
            label: label.into(),
            operations,
        });
    }

    /// Queues an undo or redo. Refused while an edit is unconfirmed.
    pub(crate) fn history(&mut self, id: String, redo: bool) -> bool {
        if self.busy() {
            return false;
        }
        self.in_flight = Some(Outgoing::History(HistoryRequest {
            id,
            expected_revision: self.confirmed.document.revision,
            redo,
        }));
        self.sent = false;
        true
    }

    pub(crate) fn request_get(&mut self) {
        self.wants_get = true;
        self.get_sent = false;
    }

    pub(crate) fn take(&mut self) -> Vec<Outgoing> {
        if self.wants_get {
            if self.get_sent {
                return Vec::new();
            }
            self.get_sent = true;
            return vec![Outgoing::Get];
        }
        if self.in_flight.is_none() {
            if let Some(edit) = self.queue.pop_front() {
                self.in_flight = Some(Outgoing::Transact(Transaction {
                    id: edit.id,
                    expected_revision: self.confirmed.document.revision,
                    label: edit.label,
                    operations: edit.operations,
                }));
                self.sent = false;
            }
        }
        match &self.in_flight {
            Some(request) if !self.sent => {
                self.sent = true;
                vec![request.clone()]
            }
            _ => Vec::new(),
        }
    }

    /// Accepts the store's answer to request `id`. A stale answer is
    /// ignored and returns `false`.
    pub(crate) fn confirm(&mut self, id: &str, mutation: Mutation) -> bool {
        let matches = self
            .in_flight
            .as_ref()
            .and_then(Outgoing::id)
            .is_some_and(|flying| flying == id);
        if !matches || mutation.document.id != self.confirmed.document.id {
            return false;
        }
        self.in_flight = None;
        self.sent = false;
        self.stalled = false;
        self.confirmed = mutation;
        true
    }

    /// Records that request `id` failed. A conflict drops every
    /// unconfirmed edit and asks for the document; a refusal drops only that
    /// request; anything else (the store was unreachable) keeps it for
    /// [`Remote::retry`].
    pub(crate) fn fail(&mut self, id: &str, error: &CanvasError) -> bool {
        let matches = self
            .in_flight
            .as_ref()
            .and_then(Outgoing::id)
            .is_some_and(|flying| flying == id);
        if !matches {
            return false;
        }
        match error {
            CanvasError::Conflict(_) | CanvasError::NotFound => {
                self.in_flight = None;
                self.sent = false;
                self.queue.clear();
                self.request_get();
            }
            CanvasError::Invalid(_) | CanvasError::Unsupported => {
                self.in_flight = None;
                self.sent = false;
            }
            // Stays sent until the host retries, so a dead link is not
            // hammered every frame.
            CanvasError::Storage(_) => self.stalled = true,
        }
        true
    }

    /// Hands a failed request out again with the same id. A `Get` that
    /// failed is asked for again.
    pub(crate) fn retry(&mut self) {
        self.sent = false;
        self.get_sent = false;
        self.stalled = false;
    }

    /// Records that a `Get` failed; [`Remote::retry`] asks again.
    pub(crate) fn fail_get(&mut self) {
        if self.wants_get {
            self.stalled = true;
        }
    }

    /// Adopts a document read from the store. Unconfirmed edits stay queued
    /// on top; a stale one comes back as a conflict.
    pub(crate) fn replace(&mut self, mutation: Mutation) -> bool {
        if mutation.document.id != self.confirmed.document.id {
            return false;
        }
        self.wants_get = false;
        self.get_sent = false;
        self.stalled = false;
        // A history answer is lost with the document it applied to; the
        // document read here already includes it.
        if matches!(self.in_flight, Some(Outgoing::History(_))) {
            self.in_flight = None;
            self.sent = false;
        }
        self.confirmed = mutation;
        true
    }
}
