//! Literal source search and atomic replacement, independent of UI or language transport.
use crate::editing::{
    DocumentId, EditError, EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments,
    SourceSelection,
};
use crate::Entity;
use std::ops::Range;

/// Revision-bound literal search. Empty queries deliberately match nothing (bounded zero-length policy).
#[derive(Clone, Debug)]
pub struct SourceSearch {
    document: DocumentId,
    revision: Revision,
    query: String,
    matches: Vec<Range<usize>>,
    current: Option<usize>,
}
impl SourceSearch {
    /// Capture nonoverlapping, case-sensitive UTF-8 literal matches in the current revision.
    /// No regex grammar, normalization or implicit newline conversion is performed.
    pub fn new(
        docs: &SourceDocuments,
        document: DocumentId,
        query: impl Into<String>,
    ) -> Result<Self, EditError> {
        let session = docs.get(document).ok_or(EditError::MissingDocument)?;
        let query = query.into();
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            session
                .text()
                .match_indices(&query)
                .map(|(start, s)| start..start + s.len())
                .collect()
        };
        Ok(Self {
            document,
            revision: session.revision(),
            query,
            matches,
            current: None,
        })
    }
    /// Captured literal query.
    pub fn query(&self) -> &str {
        &self.query
    }
    /// Captured revision. Refresh explicitly after a document edit.
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// Nonoverlapping match ranges, in captured source coordinates.
    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }
    /// Index of the last navigated match.
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    fn validate(&self, docs: &SourceDocuments) -> Result<(), EditError> {
        let session = docs.get(self.document).ok_or(EditError::MissingDocument)?;
        if session.revision() != self.revision {
            return Err(EditError::StaleRevision);
        }
        Ok(())
    }
    /// Navigate with wrapping and select in one attached view; composition refuses.
    /// The controller advances only after selection admission succeeds.
    pub fn navigate(
        &mut self,
        docs: &mut SourceDocuments,
        view: Entity,
        backwards: bool,
    ) -> Result<Option<Range<usize>>, EditError> {
        self.validate(docs)?;
        if self.matches.is_empty() {
            return Ok(None);
        }
        let count = self.matches.len();
        let index = match (self.current, backwards) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(i), false) => (i + 1) % count,
            (Some(i), true) => (i + count - 1) % count,
        };
        let range = self.matches[index].clone();
        docs.get_mut(self.document)
            .ok_or(EditError::MissingDocument)?
            .set_view_selection(
                view,
                self.revision,
                SourceSelection {
                    anchor: range.start,
                    head: range.end,
                },
            )?;
        self.current = Some(index);
        Ok(Some(range))
    }
    /// Replace all captured matches as one revision and one undo step.
    /// Stale, readonly and composing sessions refuse atomically via the session boundary.
    pub fn replace_all(
        &self,
        docs: &mut SourceDocuments,
        replacement: &str,
    ) -> Result<usize, EditError> {
        self.validate(docs)?;
        let session = docs
            .get_mut(self.document)
            .ok_or(EditError::MissingDocument)?;
        session.commit(SourceBatch {
            document: self.document,
            base: self.revision,
            origin: EditOrigin::Host(0x534541524348),
            changes: self
                .matches
                .iter()
                .cloned()
                .map(|range| SourceChange {
                    range,
                    replacement: replacement.into(),
                })
                .collect(),
        })?;
        Ok(self.matches.len())
    }
    /// Replace the navigated match (or first match) as one undoable operation.
    pub fn replace_current(
        &self,
        docs: &mut SourceDocuments,
        replacement: &str,
    ) -> Result<bool, EditError> {
        self.validate(docs)?;
        let Some(range) = self.matches.get(self.current.unwrap_or(0)) else {
            return Ok(false);
        };
        docs.get_mut(self.document)
            .ok_or(EditError::MissingDocument)?
            .commit(SourceBatch {
                document: self.document,
                base: self.revision,
                origin: EditOrigin::Host(0x534541524348),
                changes: vec![SourceChange {
                    range: range.clone(),
                    replacement: replacement.into(),
                }],
            })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replace_all_is_one_undo_and_stale_refuses() {
        let mut docs = SourceDocuments::default();
        let id = docs.open("é\r\né 👩‍💻 é");
        let search = SourceSearch::new(&docs, id, "é").unwrap();
        assert_eq!(search.matches().len(), 3);
        assert_eq!(search.replace_all(&mut docs, "😀").unwrap(), 3);
        assert_eq!(docs.get(id).unwrap().text(), "😀\r\n😀 👩‍💻 😀");
        assert_eq!(
            search.replace_all(&mut docs, "x"),
            Err(EditError::StaleRevision)
        );
        docs.get_mut(id).unwrap().undo().unwrap();
        assert_eq!(docs.get(id).unwrap().text(), "é\r\né 👩‍💻 é");
    }
    #[test]
    fn empty_overlap_and_readonly_are_explicit() {
        let mut docs = SourceDocuments::default();
        let id = docs.open("aaaa");
        assert!(SourceSearch::new(&docs, id, "")
            .unwrap()
            .matches()
            .is_empty());
        let search = SourceSearch::new(&docs, id, "aa").unwrap();
        assert_eq!(search.matches(), &[0..2, 2..4]);
        docs.get_mut(id).unwrap().set_readonly(true).unwrap();
        assert_eq!(search.replace_all(&mut docs, "b"), Err(EditError::Readonly));
        assert_eq!(docs.get(id).unwrap().text(), "aaaa");
    }
}
