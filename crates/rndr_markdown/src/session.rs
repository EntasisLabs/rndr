//! One markdown document. The session owns the bytes, the tree, the ids, the
//! undo stack, and the journal. Views do not.
//!
//! Opening a document and issuing no command appends no journal frame and
//! changes no byte. Ids are a preorder numbering of the red tree at load.
//! A typed/raw flip does not reparse and does not allocate an id.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use unicode_segmentation::GraphemeCursor;

use crate::edit::{self, apply_view, Applied, Command, EditError, Splice};
use crate::green::{Green, RedTree};
use crate::journal::{self, hash_bytes, Conflict, Hash, Journal, JournalError};
use crate::kind::SyntaxKind;
use crate::parse::parse;

/// Which projection is showing. Both read the same tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// The typed projection.
    Typed,
    /// The raw bytes.
    Raw,
}

/// In-memory document plus optional on-disk journal.
pub struct Session {
    bytes: String,
    tree: Green,
    red: RedTree,
    ids: Vec<u64>,
    next_id: u64,
    undo: Vec<Step>,
    redo: Vec<Step>,
    mode: ViewMode,
    composition: Option<Composition>,
    journal: Journal,
    /// Frames recorded since open. A clean open stays at zero.
    recorded: u64,
    revision: u64,
    /// Moving end of the selection. A byte offset into [`Self::bytes`].
    caret: usize,
    /// Anchor of the selection. Equals [`Self::caret`] when the range is empty.
    anchor: usize,
    /// The splice footprint of the last edit that changed bytes, in the
    /// coordinates described on [`EditSpan`].
    last_edit: Option<EditSpan>,
    last_edit_ms: Option<u64>,
    doc_path: Option<PathBuf>,
    journal_path: Option<PathBuf>,
    /// Hash of the bytes at open, used as sequence 0.
    base_hash: Hash,
}

struct Step {
    forward: Vec<Splice>,
    inverse: Vec<Splice>,
    ids_before: Vec<u64>,
    ids_after: Vec<u64>,
    next_id_before: u64,
    next_id_after: u64,
    caret_before: usize,
    anchor_before: usize,
    caret_after: usize,
    anchor_after: usize,
    /// Milliseconds of the last edit merged into this step.
    at_ms: u64,
    /// The step is a run of typed characters.
    typing: bool,
}

struct Composition {
    start: usize,
    end: usize,
    preedit: String,
}

impl Session {
    /// Open `bytes`. No journal file is created and no frame is recorded.
    pub fn open(bytes: &str) -> Self {
        let tree = parse(bytes);
        let red = RedTree::from_green(&tree);
        let (ids, next_id) = load_ids(&red);
        Self {
            base_hash: hash_bytes(bytes.as_bytes()),
            bytes: bytes.to_string(),
            tree,
            red,
            ids,
            next_id,
            undo: Vec::new(),
            redo: Vec::new(),
            mode: ViewMode::Typed,
            composition: None,
            journal: Journal::new(),
            recorded: 0,
            revision: 0,
            caret: bytes.len(),
            anchor: bytes.len(),
            last_edit: None,
            last_edit_ms: None,
            doc_path: None,
            journal_path: None,
        }
    }

    /// Open a file. A missing journal is not created. A matching journal is
    /// replayed into memory and the file is not rewritten.
    pub fn open_file(doc: &Path, journal_path: &Path) -> Result<Self, JournalError> {
        let bytes = fs::read(doc)?;
        let text = String::from_utf8(bytes.clone()).map_err(|err| {
            JournalError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;
        if journal_path.exists() {
            let recovered = journal::recover(doc, journal_path)?;
            let text = String::from_utf8(recovered).map_err(|err| {
                JournalError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
            })?;
            let mut session = Self::open(&text);
            session.doc_path = Some(doc.to_path_buf());
            session.journal_path = Some(journal_path.to_path_buf());
            session.base_hash = hash_bytes(text.as_bytes());
            return Ok(session);
        }
        let mut session = Self::open(&text);
        session.doc_path = Some(doc.to_path_buf());
        session.journal_path = Some(journal_path.to_path_buf());
        let _ = bytes;
        Ok(session)
    }

    /// Current bytes.
    pub fn bytes(&self) -> &str {
        &self.bytes
    }

    /// Concrete tree.
    pub fn tree(&self) -> &Green {
        &self.tree
    }

    /// Red tree.
    pub fn red(&self) -> &RedTree {
        &self.red
    }

    /// Session ids parallel to [`Self::red`] nodes. Load assigns `1..=n`.
    pub fn ids(&self) -> &[u64] {
        &self.ids
    }

    /// Journal frames recorded for this session. Zero after a clean open.
    pub fn journal_frames(&self) -> u64 {
        self.recorded
    }

    /// Current projection.
    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    /// Revision. A no-op command does not advance it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Moving end of the selection, snapped to a byte the session last stored.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Selection anchor. Equals [`Self::caret`] when nothing is selected.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// Move the selection and end the open typing run.
    ///
    /// A caret move is not an undo step. It closes the run so the next
    /// character starts a new one, including when the offset happens to sit
    /// where the run left off.
    pub fn move_caret(&mut self, anchor: usize, head: usize) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        let anchor = floor_grapheme(&self.bytes, anchor.min(self.bytes.len()));
        let head = floor_grapheme(&self.bytes, head.min(self.bytes.len()));
        if anchor != self.anchor || head != self.caret {
            if let Some(last) = self.undo.last_mut() {
                last.typing = false;
            }
        }
        self.anchor = anchor;
        self.caret = head;
        Ok(())
    }

    /// Footprint of the last edit that changed bytes.
    ///
    /// `None` after a clean open. A no-op leaves the previous footprint and
    /// does not advance [`Self::revision`]. The next accepted edit replaces
    /// the footprint. Prefix bytes before [`EditSpan::old_start`] are unchanged,
    /// and bytes that began at [`EditSpan::old_end`] begin at [`EditSpan::new_end`].
    pub fn last_edit(&self) -> Option<EditSpan> {
        self.last_edit
    }

    /// Record the caret after an edit without closing the typing run.
    ///
    /// Undo restores the caret from before the step. Redo restores the caret
    /// stored here.
    pub fn note_caret(&mut self, anchor: usize, head: usize) {
        let anchor = floor_grapheme(&self.bytes, anchor.min(self.bytes.len()));
        let head = floor_grapheme(&self.bytes, head.min(self.bytes.len()));
        self.anchor = anchor;
        self.caret = head;
        if let Some(last) = self.undo.last_mut() {
            last.anchor_after = anchor;
            last.caret_after = head;
        }
    }

    /// True while a preedit is open. The preedit is not part of the bytes.
    pub fn is_composing(&self) -> bool {
        self.composition.is_some()
    }

    /// Switch projection. Refused while composing. Ids and bytes stay put.
    pub fn set_mode(&mut self, mode: ViewMode) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        self.mode = mode;
        Ok(())
    }

    /// Begin a composition over `[start, end)`.
    pub fn composition_start(&mut self, start: usize, end: usize) -> Result<(), EditError> {
        if start > end
            || end > self.bytes.len()
            || !self.bytes.is_char_boundary(start)
            || !self.bytes.is_char_boundary(end)
        {
            return Err(EditError::InvalidRange);
        }
        self.composition = Some(Composition {
            start,
            end,
            preedit: String::new(),
        });
        Ok(())
    }

    /// Replace the preedit. The document bytes do not change.
    pub fn composition_update(&mut self, preedit: &str) -> Result<(), EditError> {
        let Some(composition) = self.composition.as_mut() else {
            return Err(EditError::Refused);
        };
        composition.preedit = preedit.to_string();
        Ok(())
    }

    /// The uncommitted preedit, if any.
    pub fn preedit(&self) -> Option<&str> {
        self.composition.as_ref().map(|c| c.preedit.as_str())
    }

    /// Commit the preedit as one splice, one undo step, and one journal frame.
    pub fn composition_commit(&mut self, now_ms: u64) -> Result<(), EditError> {
        let Some(composition) = self.composition.take() else {
            return Err(EditError::Refused);
        };
        self.apply(
            Command::SetField {
                start: composition.start,
                end: composition.end,
                value: composition.preedit,
            },
            now_ms,
            false,
        )
    }

    /// Drop the preedit. Nothing is written.
    pub fn composition_cancel(&mut self) {
        self.composition = None;
    }

    /// Apply `cmd` at `now_ms`. `now_ms` groups typing runs (600 ms).
    pub fn edit(&mut self, cmd: Command, now_ms: u64) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        self.apply(cmd, now_ms, true)
    }

    /// Undo one step. The inverse splice is a new journal frame.
    pub fn undo(&mut self, now_ms: u64) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        let Some(step) = self.undo.pop() else {
            return Ok(());
        };
        let applied = match apply_splices(&self.bytes, &self.tree, &step.inverse) {
            Ok(applied) => applied,
            Err(err) => {
                self.undo.push(step);
                return Err(err);
            }
        };
        let ids_before = step.ids_before.clone();
        let next_id_before = step.next_id_before;
        let caret_before = step.caret_before;
        let anchor_before = step.anchor_before;
        let span = span_of(&applied.splices, applied.widened_to_root);
        self.record_journal(&step.inverse, &applied);
        self.install(applied, Some(ids_before), next_id_before);
        self.caret = caret_before.min(self.bytes.len());
        self.anchor = anchor_before.min(self.bytes.len());
        self.redo.push(step);
        self.last_edit = Some(span);
        self.last_edit_ms = Some(now_ms);
        self.revision += 1;
        Ok(())
    }

    /// Redo one undone step. The forward splice is a new journal frame.
    pub fn redo(&mut self, now_ms: u64) -> Result<(), EditError> {
        if self.composition.is_some() {
            return Err(EditError::Composing);
        }
        let Some(step) = self.redo.pop() else {
            return Ok(());
        };
        let applied = match apply_splices(&self.bytes, &self.tree, &step.forward) {
            Ok(applied) => applied,
            Err(err) => {
                self.redo.push(step);
                return Err(err);
            }
        };
        let ids_after = step.ids_after.clone();
        let next_id_after = step.next_id_after;
        let caret_after = step.caret_after;
        let anchor_after = step.anchor_after;
        let span = span_of(&applied.splices, applied.widened_to_root);
        self.record_journal(&step.forward, &applied);
        self.install(applied, Some(ids_after), next_id_after);
        self.caret = caret_after.min(self.bytes.len());
        self.anchor = anchor_after.min(self.bytes.len());
        self.undo.push(step);
        self.last_edit = Some(span);
        self.last_edit_ms = Some(now_ms);
        self.revision += 1;
        Ok(())
    }

    /// Group-commit pending frames when the oldest is at least 80 ms old and
    /// not more often than every 50 ms. No paths means the frames stay counted
    /// and nothing is written.
    pub fn poll(&mut self, now_ms: u64) -> Result<(), JournalError> {
        let Some(last) = self.last_edit_ms else {
            return Ok(());
        };
        if self.journal.pending().is_empty() {
            return Ok(());
        }
        if now_ms.saturating_sub(last) < 50 {
            return Ok(());
        }
        if now_ms.saturating_sub(last) < 80 && self.journal.pending().len() == 1 {
            // The contract is a group inside 50–100 ms. A single fresh frame
            // waits until 80 ms; a host may also call [`Self::flush`].
            if now_ms.saturating_sub(last) < 80 {
                return Ok(());
            }
        }
        self.commit_group()
    }

    /// Group-commit and checkpoint. A clean session does not rewrite the file.
    pub fn flush(&mut self) -> Result<Hash, JournalError> {
        if let Some(path) = self.journal_path.clone() {
            self.journal.commit_group(&path, self.base_hash)?;
        }
        if self.journal.pending().is_empty() && self.doc_path.is_none() {
            return Ok(hash_bytes(self.bytes.as_bytes()));
        }
        let Some(doc) = self.doc_path.clone() else {
            return Ok(hash_bytes(self.bytes.as_bytes()));
        };
        let journal = self
            .journal_path
            .clone()
            .unwrap_or_else(|| doc.with_extension("rndr-journal"));
        if hash_bytes(self.bytes.as_bytes()) == self.base_hash && self.journal_frames() == 0 {
            if doc.exists() {
                let on_disk = fs::read(&doc)?;
                if hash_bytes(&on_disk) != self.base_hash {
                    return Err(JournalError::Conflict(Conflict {
                        file_hash: hash_bytes(&on_disk),
                        session_hash: self.base_hash,
                        sequence: 0,
                        in_memory: true,
                    }));
                }
            }
            return Ok(self.base_hash);
        }
        let hash = self
            .journal
            .checkpoint(&doc, &journal, self.bytes.as_bytes())?;
        self.base_hash = hash;
        Ok(hash)
    }

    fn commit_group(&mut self) -> Result<(), JournalError> {
        if let Some(path) = self.journal_path.clone() {
            self.journal.commit_group(&path, self.base_hash)?;
        } else {
            let carried = self.journal.checkpoint_hash;
            let seq = self.journal.checkpoint_seq;
            self.journal = Journal::new();
            self.journal.checkpoint_hash = carried;
            self.journal.checkpoint_seq = seq;
        }
        Ok(())
    }

    fn apply(&mut self, cmd: Command, now_ms: u64, group: bool) -> Result<(), EditError> {
        let before_ids = self.ids.clone();
        let before_next = self.next_id;
        let before_caret = self.caret;
        let before_anchor = self.anchor;
        let applied = apply_view(&self.bytes, &self.tree, &cmd)?;
        if applied.splices.is_empty() {
            return Ok(());
        }
        if group && self.try_group(&cmd, &applied, now_ms) {
            return Ok(());
        }
        let forward = applied.splices.clone();
        let inverse = invert(&applied.splices, &self.bytes);
        let span = span_of(&applied.splices, applied.widened_to_root);
        self.record_journal(&applied.splices, &applied);
        self.install(applied, None, before_next);
        self.last_edit = Some(span);
        let ids_after = self.ids.clone();
        let next_after = self.next_id;
        self.undo.push(Step {
            forward,
            inverse,
            ids_before: before_ids,
            ids_after,
            next_id_before: before_next,
            next_id_after: next_after,
            caret_before: before_caret,
            anchor_before: before_anchor,
            caret_after: before_caret,
            anchor_after: before_anchor,
            at_ms: now_ms,
            typing: matches!(cmd, Command::Insert { ref text, .. } if text.chars().count() == 1),
        });
        self.redo.clear();
        self.last_edit_ms = Some(now_ms);
        self.revision += 1;
        Ok(())
    }

    fn try_group(&mut self, cmd: &Command, applied: &Applied, now_ms: u64) -> bool {
        let Command::Insert { at, text } = cmd else {
            return false;
        };
        if text.chars().count() != 1 || text.chars().any(char::is_whitespace) {
            return false;
        }
        let can = self.undo.last().is_some_and(|last| {
            last.typing
                && now_ms.saturating_sub(last.at_ms)
                    <= u64::try_from(TYPING_GAP.as_millis()).unwrap_or(600)
                && last.inverse.len() == 1
                && last.inverse[0].replacement.is_empty()
                && last.inverse[0].end == *at
        });
        if !can {
            return false;
        }
        let extra = text.len();
        let span = span_of(&applied.splices, applied.widened_to_root);
        self.record_journal(&applied.splices, applied);
        self.install(applied.clone(), None, self.next_id);
        self.last_edit = Some(span);
        if let Some(last) = self.undo.last_mut() {
            last.inverse[0].end += extra;
            if let Some(forward) = last.forward.first_mut() {
                forward.replacement.push_str(text);
            }
            last.ids_after.clone_from(&self.ids);
            last.next_id_after = self.next_id;
            last.at_ms = now_ms;
        }
        self.redo.clear();
        self.last_edit_ms = Some(now_ms);
        self.revision += 1;
        true
    }

    fn record_journal(&mut self, splices: &[Splice], applied: &Applied) {
        if splices.len() == 1 {
            let splice = &splices[0];
            if splice.end > self.bytes.len() || splice.start > splice.end {
                return;
            }
            let deleted = self.bytes.as_bytes()[splice.start..splice.end].to_vec();
            self.recorded += 1;
            self.journal.push(
                splice.start,
                splice.end,
                &deleted,
                splice.replacement.as_bytes(),
                applied.bytes.as_bytes(),
            );
            return;
        }
        self.recorded += 1;
        self.journal.push(
            0,
            self.bytes.len(),
            self.bytes.as_bytes(),
            applied.bytes.as_bytes(),
            applied.bytes.as_bytes(),
        );
    }

    fn install(&mut self, applied: Applied, restore_ids: Option<Vec<u64>>, next_floor: u64) {
        let edit_start = applied.splices.iter().map(|s| s.start).min().unwrap_or(0);
        let old_tail = applied
            .splices
            .iter()
            .map(|s| s.end)
            .max()
            .unwrap_or(edit_start);
        let delta = applied.bytes.len() as isize - self.bytes.len() as isize;
        let new_red = RedTree::from_green(&applied.tree);
        let (ids, next_id) = if let Some(saved) = restore_ids {
            if saved.len() == new_red.nodes().len() {
                (saved, next_floor)
            } else {
                let mut next = self.next_id.max(next_floor);
                let ids = reassign(
                    &self.red, &self.ids, &new_red, edit_start, old_tail, delta, &mut next,
                );
                (ids, next)
            }
        } else {
            let mut next = self.next_id.max(next_floor);
            let ids = reassign(
                &self.red, &self.ids, &new_red, edit_start, old_tail, delta, &mut next,
            );
            (ids, next)
        };
        self.bytes = applied.bytes;
        self.tree = applied.tree;
        self.red = new_red;
        self.ids = ids;
        self.next_id = next_id;
    }
}

fn load_ids(red: &RedTree) -> (Vec<u64>, u64) {
    let ids: Vec<u64> = (1..=red.nodes().len() as u64).collect();
    let next = ids.last().copied().unwrap_or(0) + 1;
    (ids, next)
}

fn reassign(
    old: &RedTree,
    old_ids: &[u64],
    new: &RedTree,
    edit_start: usize,
    old_tail: usize,
    delta: isize,
    next_id: &mut u64,
) -> Vec<u64> {
    let mut map = std::collections::HashMap::<(SyntaxKind, usize), u64>::new();
    for (index, node) in old.nodes().iter().enumerate() {
        map.entry((node.kind, node.offset))
            .or_insert(old_ids[index]);
    }
    let new_tail = (old_tail as isize + delta).max(0) as usize;
    new.nodes()
        .iter()
        .map(|node| {
            let old_offset = if node.offset < edit_start || node.offset + node.width <= edit_start {
                Some(node.offset)
            } else if node.offset >= new_tail {
                Some((node.offset as isize - delta).max(0) as usize)
            } else {
                None
            };
            if let Some(offset) = old_offset {
                if let Some(id) = map.get(&(node.kind, offset)).copied() {
                    return id;
                }
            }
            let id = *next_id;
            *next_id += 1;
            id
        })
        .collect()
}

fn apply_splices(bytes: &str, tree: &Green, splices: &[Splice]) -> Result<Applied, EditError> {
    if splices.is_empty() {
        return Ok(Applied {
            splices: Vec::new(),
            bytes: bytes.to_string(),
            tree: tree.clone(),
            widened_to_root: false,
        });
    }
    // Replay each inverse splice as its own insert/delete command so the
    // incremental reparse runs.
    let mut text = bytes.to_string();
    let mut current = tree.clone();
    let mut widened = false;
    for splice in splices.iter().rev() {
        let via = edit::apply_view(
            &text,
            &current,
            &Command::SetField {
                start: splice.start,
                end: splice.end,
                value: splice.replacement.clone(),
            },
        )?;
        widened |= via.widened_to_root;
        text = via.bytes;
        current = via.tree;
    }
    Ok(Applied {
        splices: splices.to_vec(),
        bytes: text,
        tree: current,
        widened_to_root: widened,
    })
}

fn invert(splices: &[Splice], original: &str) -> Vec<Splice> {
    let mut out = Vec::new();
    let mut shift = 0isize;
    for splice in splices {
        let new_start = (splice.start as isize + shift) as usize;
        let deleted = original[splice.start..splice.end].to_string();
        out.push(Splice {
            start: new_start,
            end: new_start + splice.replacement.len(),
            replacement: deleted,
        });
        shift += splice.replacement.len() as isize - (splice.end - splice.start) as isize;
    }
    out
}

/// Snap `index` down to a grapheme boundary. Offsets inside a cluster move
/// to the cluster's start.
pub fn floor_grapheme(text: &str, index: usize) -> usize {
    let len = text.len();
    let index = index.min(len);
    if !text.is_char_boundary(index) {
        let mut at = index;
        while at > 0 && !text.is_char_boundary(at) {
            at -= 1;
        }
        return floor_grapheme(text, at);
    }
    let mut cursor = GraphemeCursor::new(index, len, true);
    if cursor.is_boundary(text, 0).unwrap_or(false) {
        index
    } else {
        cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
    }
}

/// How long a typing run stays open.
pub const TYPING_GAP: Duration = Duration::from_millis(600);

/// Footprint of one accepted edit.
///
/// [`Self::old_start`] and [`Self::old_end`] are offsets in the bytes from
/// before the edit. [`Self::new_end`] is where that range ends afterwards.
/// The start of the new range is still [`Self::old_start`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditSpan {
    /// First touched byte before the edit.
    pub old_start: usize,
    /// End of the touched range before the edit.
    pub old_end: usize,
    /// End of the touched range after the edit.
    pub new_end: usize,
    /// The reparse grew until it covered the whole document.
    pub widened_to_root: bool,
}

fn span_of(splices: &[Splice], widened_to_root: bool) -> EditSpan {
    let old_start = splices.iter().map(|splice| splice.start).min().unwrap_or(0);
    let old_end = splices
        .iter()
        .map(|splice| splice.end)
        .max()
        .unwrap_or(old_start);
    let deleted: usize = splices.iter().map(|splice| splice.end - splice.start).sum();
    let inserted: usize = splices.iter().map(|splice| splice.replacement.len()).sum();
    EditSpan {
        old_start,
        old_end,
        new_end: old_end + inserted - deleted,
        widened_to_root,
    }
}
