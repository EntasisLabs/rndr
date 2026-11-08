//! The authority for one document: it admits transactions, keeps undo and
//! redo history and remembers recent request ids so retries are harmless.
//! Stores persist records; they never edit documents another way.

use crate::error::{invalid, CanvasError, Result};
use crate::ops::{apply_operations, HistoryRequest, Transaction};
use crate::schema::{Asset, Document, Node, Scene};
use crate::validate::{validate_document, validate_id, MAX_ASSET_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Undo history is capped by entry count and by serialized size.
pub const MAX_UNDO: usize = 100;
pub const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;
/// Request ids remembered for idempotent retries.
pub const MAX_RECEIPTS: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    /// Chosen by the client so a retried create is harmless.
    pub id: String,
    pub title: String,
    pub scene: Scene,
}

/// The document after a request, and what history now allows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mutation {
    pub document: Document,
    /// The revision the request produced. For a retried request this is the
    /// revision it produced the first time.
    pub applied_revision: u64,
    /// The request had already been applied; nothing changed this time.
    pub duplicate: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redo_label: Option<String>,
}

/// The document state a history step restores.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Patch {
    nodes: Vec<(String, Option<Node>)>,
    order: Vec<String>,
    title: String,
    pinned: bool,
    background: String,
}

impl Patch {
    fn between(from: &Document, to: &Document) -> Self {
        let previous: BTreeMap<_, _> = from.scene.nodes.iter().map(|n| (&n.id, n)).collect();
        let current: BTreeMap<_, _> = to.scene.nodes.iter().map(|n| (&n.id, n)).collect();
        let ids: BTreeSet<_> = previous.keys().chain(current.keys()).copied().collect();
        Self {
            nodes: ids
                .into_iter()
                .filter(|id| previous.get(id) != current.get(id))
                .map(|id| (id.clone(), current.get(id).map(|n| (*n).clone())))
                .collect(),
            order: to.scene.nodes.iter().map(|n| n.id.clone()).collect(),
            title: to.title.clone(),
            pinned: to.pinned,
            background: to.scene.background.clone(),
        }
    }

    fn apply(&self, doc: &Document) -> Result<Document> {
        let mut next = doc.clone();
        let mut nodes: BTreeMap<_, _> = next
            .scene
            .nodes
            .drain(..)
            .map(|n| (n.id.clone(), n))
            .collect();
        for (id, node) in &self.nodes {
            match node {
                Some(node) => nodes.insert(id.clone(), node.clone()),
                None => nodes.remove(id),
            };
        }
        next.scene.nodes = self
            .order
            .iter()
            .map(|id| {
                nodes
                    .remove(id)
                    .ok_or_else(|| invalid("corrupt history order"))
            })
            .collect::<Result<_>>()?;
        if !nodes.is_empty() {
            return Err(invalid("corrupt history nodes"));
        }
        next.title = self.title.clone();
        next.pinned = self.pinned;
        next.scene.background = self.background.clone();
        validate_document(&next)?;
        Ok(next)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Step {
    forward: Patch,
    backward: Patch,
    label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Receipt {
    id: String,
    hash: String,
    revision: u64,
}

/// A document with its history. Serializable so stores can persist it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasRecord {
    document: Document,
    create_hash: String,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    undo: Vec<Step>,
    #[serde(default)]
    redo: Vec<Step>,
    #[serde(default)]
    receipts: Vec<Receipt>,
}

pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| CanvasError::Storage(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn json_len<T: Serialize>(value: &T) -> Result<usize> {
    serde_json::to_vec(value)
        .map(|b| b.len())
        .map_err(|e| CanvasError::Storage(e.to_string()))
}

impl CanvasRecord {
    /// Starts a document at revision 0. `now` is an RFC 3339 timestamp.
    pub fn create(request: CreateRequest, now: &str) -> Result<Self> {
        validate_id(&request.id)?;
        let create_hash = digest(&request)?;
        let document = Document {
            id: request.id,
            revision: 0,
            title: request.title,
            pinned: false,
            created_at: now.into(),
            updated_at: now.into(),
            scene: request.scene,
            assets: BTreeMap::new(),
        };
        validate_document(&document)?;
        Ok(Self {
            document,
            create_hash,
            deleted: false,
            undo: Vec::new(),
            redo: Vec::new(),
            receipts: Vec::new(),
        })
    }

    /// Whether `request` is the one that created this record, so a retried
    /// create can return it instead of failing.
    pub fn created_by(&self, request: &CreateRequest) -> Result<bool> {
        Ok(self.create_hash == digest(request)?)
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    /// Checks a record loaded from storage.
    pub fn validate(&self) -> Result<()> {
        validate_document(&self.document)
    }

    pub fn mutation(&self) -> Mutation {
        self.response(self.document.revision, false)
    }

    fn response(&self, revision: u64, duplicate: bool) -> Mutation {
        Mutation {
            document: self.document.clone(),
            applied_revision: revision,
            duplicate,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            undo_label: self.undo.last().map(|s| s.label.clone()),
            redo_label: self.redo.last().map(|s| s.label.clone()),
        }
    }

    fn receipt(&self, id: &str, hash: &str) -> Result<Option<Mutation>> {
        match self.receipts.iter().find(|r| r.id == id) {
            Some(r) if r.hash != hash => Err(invalid("request id reused with different input")),
            Some(r) => Ok(Some(self.response(r.revision, true))),
            None => Ok(None),
        }
    }

    fn check_revision(&self, expected: u64) -> Result<()> {
        if self.document.revision == expected {
            Ok(())
        } else {
            Err(CanvasError::Conflict(self.document.revision))
        }
    }

    fn bump(&mut self, id: &str, hash: String, now: &str) -> Result<()> {
        self.document.revision = self
            .document
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("revision exhausted"))?;
        self.document.updated_at = now.into();
        self.receipts.push(Receipt {
            id: id.into(),
            hash,
            revision: self.document.revision,
        });
        if self.receipts.len() > MAX_RECEIPTS {
            self.receipts.remove(0);
        }
        Ok(())
    }

    /// Applies a transaction made against the current revision. A retry with
    /// the same id and input returns the original result marked `duplicate`.
    /// On error the record is unchanged.
    pub fn transact(&mut self, request: &Transaction, now: &str) -> Result<Mutation> {
        validate_id(&request.id)?;
        let hash = digest(request)?;
        if let Some(done) = self.receipt(&request.id, &hash)? {
            return Ok(done);
        }
        if request.label.len() > 256 || request.label.contains('\0') {
            return Err(invalid("invalid transaction label"));
        }
        self.check_revision(request.expected_revision)?;
        let next = apply_operations(&self.document, &request.operations)?;
        let step = Step {
            forward: Patch::between(&self.document, &next),
            backward: Patch::between(&next, &self.document),
            label: request.label.clone(),
        };
        if json_len(&step)? > MAX_HISTORY_BYTES {
            return Err(invalid(
                "single edit exceeds undo capacity; split it into smaller gestures",
            ));
        }
        self.document = next;
        self.redo.clear();
        self.undo.push(step);
        while self.undo.len() > MAX_UNDO || json_len(&self.undo)? > MAX_HISTORY_BYTES {
            self.undo.remove(0);
        }
        self.bump(&request.id, hash, now)?;
        Ok(self.mutation())
    }

    /// Undoes or redoes one step. Each step is a new revision.
    pub fn history(&mut self, request: &HistoryRequest, now: &str) -> Result<Mutation> {
        validate_id(&request.id)?;
        let hash = digest(&("history", request))?;
        if let Some(done) = self.receipt(&request.id, &hash)? {
            return Ok(done);
        }
        self.check_revision(request.expected_revision)?;
        let (from, to) = if request.redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let step = from
            .last()
            .ok_or_else(|| invalid("no history in that direction"))?;
        let patch = if request.redo {
            &step.forward
        } else {
            &step.backward
        };
        self.document = patch.apply(&self.document)?;
        to.push(from.pop().expect("step checked above"));
        self.bump(&request.id, hash, now)?;
        Ok(self.mutation())
    }

    /// Soft-deletes the document; stores stop returning it.
    pub fn delete(&mut self, expected_revision: u64) -> Result<()> {
        self.check_revision(expected_revision)?;
        self.deleted = true;
        Ok(())
    }

    /// Registers a bitmap and returns its asset, which is unchanged if the
    /// same bytes were added before. New assets advance the revision but are
    /// not undoable. The caller stores the bytes.
    pub fn add_asset(&mut self, bytes: &[u8], now: &str) -> Result<(Asset, bool)> {
        let asset = bitmap_asset(bytes)?;
        if let Some(existing) = self.document.assets.get(&asset.id) {
            return Ok((existing.clone(), false));
        }
        let mut next = self.document.clone();
        next.assets.insert(asset.id.clone(), asset.clone());
        validate_document(&next)?;
        self.document = next;
        self.bump(
            &format!("asset_{}", &asset.sha256[..32]),
            digest(&asset)?,
            now,
        )?;
        Ok((asset, true))
    }
}

/// Identifies a PNG, JPEG, GIF or WebP bitmap and fingerprints it.
pub fn bitmap_asset(bytes: &[u8]) -> Result<Asset> {
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err(invalid("asset must contain 1-25 MiB"));
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        return Err(invalid("asset must be a PNG, JPEG, GIF or WebP bitmap"));
    };
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    Ok(Asset {
        id: format!("a_{sha256}"),
        mime: mime.into(),
        byte_size: bytes.len() as u64,
        sha256,
    })
}
