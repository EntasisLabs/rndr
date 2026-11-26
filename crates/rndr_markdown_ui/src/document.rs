//! Host-owned markdown sessions. One id is one writer.

use rndr_markdown::{JournalError, Session};

/// Identity of one open markdown document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownId(pub u64);

/// Sessions mounted by a host. The surface borrows one by id.
#[derive(Default)]
pub struct MarkdownDocuments {
    next: u64,
    sessions: Vec<(MarkdownId, Session)>,
}

impl MarkdownDocuments {
    /// Open `bytes` with no journal file.
    pub fn open(&mut self, bytes: &str) -> MarkdownId {
        self.insert(Session::open(bytes))
    }

    /// Open a file and replay a matching journal. A missing journal is not created.
    pub fn open_file(
        &mut self,
        doc: &std::path::Path,
        journal: &std::path::Path,
    ) -> Result<MarkdownId, JournalError> {
        Ok(self.insert(Session::open_file(doc, journal)?))
    }

    fn insert(&mut self, session: Session) -> MarkdownId {
        self.next += 1;
        let id = MarkdownId(self.next);
        self.sessions.push((id, session));
        id
    }

    /// Borrow the session.
    pub fn get(&self, id: MarkdownId) -> Option<&Session> {
        self.sessions
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, session)| session)
    }

    /// Borrow the session for an edit.
    pub fn get_mut(&mut self, id: MarkdownId) -> Option<&mut Session> {
        self.sessions
            .iter_mut()
            .find(|(key, _)| *key == id)
            .map(|(_, session)| session)
    }

    /// Release a session and everything it holds. Unmount surfaces that
    /// borrow it first. Ids are never reused. `None` when it was not open.
    pub fn close(&mut self, id: MarkdownId) -> Option<Session> {
        let at = self.sessions.iter().position(|(key, _)| *key == id)?;
        let (_, session) = self.sessions.swap_remove(at);
        if self.sessions.is_empty() {
            self.sessions.shrink_to_fit();
        }
        Some(session)
    }

    /// Every open session, for a host that polls journals.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (MarkdownId, &mut Session)> {
        self.sessions.iter_mut().map(|(id, session)| (*id, session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_releases_one_session_and_keeps_the_rest() {
        let mut docs = MarkdownDocuments::default();
        let a = docs.open("# a");
        let b = docs.open("# b");
        assert!(docs.close(a).is_some());
        assert!(docs.get(a).is_none());
        assert!(docs.close(a).is_none());
        assert!(docs.get(b).is_some());
        let c = docs.open("# c");
        assert!(c != a && c != b);
    }
}
