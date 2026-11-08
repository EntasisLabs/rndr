//! Persistence for canvas documents. Every store edits through
//! [`CanvasRecord`], so revision checks, history and idempotency behave the
//! same whatever holds the bytes.

use crate::error::{invalid, CanvasError, Result};
use crate::ops::{HistoryRequest, Transaction};
use crate::record::{CanvasRecord, CreateRequest, Mutation};
use crate::schema::{Asset, Document};
use crate::validate::{validate_id, MAX_ASSET_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A page request for [`CanvasStore::list`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListQuery {
    /// The last id of the previous page.
    pub cursor: Option<String>,
    /// 1 to 100; defaults to 50.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Summary {
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub pinned: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPage {
    /// Sorted by id.
    pub documents: Vec<Summary>,
    pub next_cursor: Option<String>,
}

/// Where canvas documents live. Methods take `&self` so a store can be
/// shared between threads; each call is atomic for its document.
pub trait CanvasStore {
    /// Creates a document at revision 0. Repeating the same request returns
    /// the existing document; reusing the id for anything else is refused.
    fn create(&self, request: CreateRequest) -> Result<Document>;
    fn get(&self, id: &str) -> Result<Mutation>;
    fn list(&self, query: ListQuery) -> Result<ListPage>;
    fn transact(&self, id: &str, request: Transaction) -> Result<Mutation>;
    fn history(&self, id: &str, request: HistoryRequest) -> Result<Mutation>;
    /// Soft-deletes; the document is no longer returned.
    fn delete(&self, id: &str, expected_revision: u64) -> Result<()>;
    /// Adds a PNG, JPEG, GIF or WebP bitmap to the document.
    fn import_asset(&self, id: &str, bytes: &[u8]) -> Result<Asset>;
    fn asset(&self, id: &str, asset_id: &str) -> Result<(Asset, Vec<u8>)>;
}

/// The current UTC time as RFC 3339 with millisecond precision.
pub fn now_rfc3339() -> String {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days, Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        since.subsec_millis()
    )
}

fn page<'a>(
    query: &ListQuery,
    records: impl Iterator<Item = Result<CanvasRecord>> + 'a,
) -> Result<ListPage> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(invalid("list limit must be 1-100"));
    }
    if let Some(cursor) = &query.cursor {
        validate_id(cursor)?;
    }
    let mut result = ListPage::default();
    for record in records {
        let record = record?;
        if record.is_deleted() {
            continue;
        }
        let doc = record.document();
        if query.cursor.as_ref().is_some_and(|c| &doc.id <= c) {
            continue;
        }
        if result.documents.len() == limit {
            result.next_cursor = result.documents.last().map(|s| s.id.clone());
            break;
        }
        result.documents.push(Summary {
            id: doc.id.clone(),
            revision: doc.revision,
            title: doc.title.clone(),
            pinned: doc.pinned,
            updated_at: doc.updated_at.clone(),
        });
    }
    Ok(result)
}

fn live(record: CanvasRecord) -> Result<CanvasRecord> {
    if record.is_deleted() {
        Err(CanvasError::NotFound)
    } else {
        Ok(record)
    }
}

fn create_or_replay(existing: Option<&CanvasRecord>, request: &CreateRequest) -> Result<()> {
    match existing {
        Some(record) if !record.is_deleted() && record.created_by(request)? => Ok(()),
        Some(_) => Err(invalid("document id is already in use")),
        None => Ok(()),
    }
}

/// Keeps documents and assets in memory. Useful for tests, previews and
/// applications that persist elsewhere.
#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: Mutex<MemoryInner>,
}

#[derive(Debug, Default)]
struct MemoryInner {
    records: BTreeMap<String, CanvasRecord>,
    blobs: BTreeMap<(String, String), Vec<u8>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut MemoryInner) -> Result<T>) -> Result<T> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| CanvasError::Storage("store lock poisoned".into()))?;
        f(&mut inner)
    }

    fn edit<T>(&self, id: &str, f: impl FnOnce(&mut CanvasRecord) -> Result<T>) -> Result<T> {
        self.with(|inner| {
            let stored = inner.records.get(id).ok_or(CanvasError::NotFound)?;
            let mut record = live(stored.clone())?;
            let value = f(&mut record)?;
            inner.records.insert(id.into(), record);
            Ok(value)
        })
    }
}

impl CanvasStore for MemoryStore {
    fn create(&self, request: CreateRequest) -> Result<Document> {
        self.with(|inner| {
            create_or_replay(inner.records.get(&request.id), &request)?;
            if let Some(record) = inner.records.get(&request.id) {
                return Ok(record.document().clone());
            }
            let record = CanvasRecord::create(request, &now_rfc3339())?;
            let document = record.document().clone();
            inner.records.insert(document.id.clone(), record);
            Ok(document)
        })
    }

    fn get(&self, id: &str) -> Result<Mutation> {
        self.with(|inner| {
            let record = inner.records.get(id).ok_or(CanvasError::NotFound)?;
            Ok(live(record.clone())?.mutation())
        })
    }

    fn list(&self, query: ListQuery) -> Result<ListPage> {
        self.with(|inner| page(&query, inner.records.values().cloned().map(Ok)))
    }

    fn transact(&self, id: &str, request: Transaction) -> Result<Mutation> {
        self.edit(id, |record| record.transact(&request, &now_rfc3339()))
    }

    fn history(&self, id: &str, request: HistoryRequest) -> Result<Mutation> {
        self.edit(id, |record| record.history(&request, &now_rfc3339()))
    }

    fn delete(&self, id: &str, expected_revision: u64) -> Result<()> {
        self.edit(id, |record| record.delete(expected_revision))
    }

    fn import_asset(&self, id: &str, bytes: &[u8]) -> Result<Asset> {
        self.with(|inner| {
            let stored = inner.records.get(id).ok_or(CanvasError::NotFound)?;
            let mut record = live(stored.clone())?;
            let (asset, _) = record.add_asset(bytes, &now_rfc3339())?;
            inner
                .blobs
                .insert((id.into(), asset.id.clone()), bytes.to_vec());
            inner.records.insert(id.into(), record);
            Ok(asset)
        })
    }

    fn asset(&self, id: &str, asset_id: &str) -> Result<(Asset, Vec<u8>)> {
        self.with(|inner| {
            let record = live(inner.records.get(id).ok_or(CanvasError::NotFound)?.clone())?;
            let asset = record
                .document()
                .assets
                .get(asset_id)
                .ok_or(CanvasError::NotFound)?
                .clone();
            let bytes = inner
                .blobs
                .get(&(id.to_string(), asset_id.to_string()))
                .ok_or(CanvasError::NotFound)?
                .clone();
            Ok((asset, bytes))
        })
    }
}

/// Keeps each document as `documents/{id}.json` under a root directory, with
/// assets in `assets/{id}/{asset_id}`.
///
/// Writes go to a temporary file that is synced and renamed into place, so
/// a crash leaves the previous or the next version. Each document has a
/// lock file held with an exclusive OS lock during a request, so processes
/// sharing the directory do not interleave edits.
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
}

/// Records larger than this are refused when read or written.
pub const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;

impl FsStore {
    /// Opens or creates a store rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("documents"))?;
        fs::create_dir_all(root.join("assets"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.root.join("documents").join(format!("{id}.json")))
    }

    fn lock(&self, id: &str) -> Result<File> {
        validate_id(id)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.root.join("documents").join(format!("{id}.lock")))?;
        file.lock()?;
        Ok(file)
    }

    fn read(&self, id: &str) -> Result<Option<CanvasRecord>> {
        let bytes = match read_limited(&self.file(id)?, MAX_RECORD_BYTES) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| CanvasError::Storage(e.to_string()))?;
        let schema = raw
            .pointer("/document/scene/schema")
            .and_then(|v| v.as_str());
        let version = raw
            .pointer("/document/scene/version")
            .and_then(|v| v.as_u64());
        if !crate::schema::supported(schema.unwrap_or_default(), version.unwrap_or_default()) {
            return Err(CanvasError::Unsupported);
        }
        let record: CanvasRecord =
            serde_json::from_value(raw).map_err(|e| CanvasError::Storage(e.to_string()))?;
        if record.document().id != id {
            return Err(invalid("stored document identity mismatch"));
        }
        record.validate()?;
        Ok(Some(record))
    }

    fn write(&self, record: &CanvasRecord) -> Result<()> {
        let bytes = serde_json::to_vec(record).map_err(|e| CanvasError::Storage(e.to_string()))?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(invalid("document record capacity exceeded"));
        }
        atomic_write(&self.file(&record.document().id)?, &bytes)
    }

    fn edit<T>(&self, id: &str, f: impl FnOnce(&mut CanvasRecord) -> Result<T>) -> Result<T> {
        let _lock = self.lock(id)?;
        let mut record = live(self.read(id)?.ok_or(CanvasError::NotFound)?)?;
        let before = record.clone();
        let value = f(&mut record)?;
        if record != before {
            self.write(&record)?;
        }
        Ok(value)
    }

    fn asset_path(&self, id: &str, asset_id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        validate_id(asset_id)?;
        Ok(self.root.join("assets").join(id).join(asset_id))
    }
}

impl CanvasStore for FsStore {
    fn create(&self, request: CreateRequest) -> Result<Document> {
        let _lock = self.lock(&request.id)?;
        let existing = self.read(&request.id)?;
        create_or_replay(existing.as_ref(), &request)?;
        if let Some(record) = existing {
            return Ok(record.document().clone());
        }
        let record = CanvasRecord::create(request, &now_rfc3339())?;
        self.write(&record)?;
        Ok(record.document().clone())
    }

    fn get(&self, id: &str) -> Result<Mutation> {
        Ok(live(self.read(id)?.ok_or(CanvasError::NotFound)?)?.mutation())
    }

    fn list(&self, query: ListQuery) -> Result<ListPage> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.root.join("documents"))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            if let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) {
                if validate_id(id).is_ok() {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        page(
            &query,
            ids.into_iter().filter_map(|id| self.read(&id).transpose()),
        )
    }

    fn transact(&self, id: &str, request: Transaction) -> Result<Mutation> {
        self.edit(id, |record| record.transact(&request, &now_rfc3339()))
    }

    fn history(&self, id: &str, request: HistoryRequest) -> Result<Mutation> {
        self.edit(id, |record| record.history(&request, &now_rfc3339()))
    }

    fn delete(&self, id: &str, expected_revision: u64) -> Result<()> {
        self.edit(id, |record| record.delete(expected_revision))
    }

    fn import_asset(&self, id: &str, bytes: &[u8]) -> Result<Asset> {
        self.edit(id, |record| {
            let (asset, _) = record.add_asset(bytes, &now_rfc3339())?;
            // The blob is published before the record that references it.
            // A retry after a crash in between finds identical bytes.
            let path = self.asset_path(id, &asset.id)?;
            match read_limited(&path, MAX_ASSET_BYTES as u64) {
                Ok(held) if Sha256::digest(&held) == Sha256::digest(bytes) => {}
                Ok(_) => return Err(CanvasError::Storage("asset content mismatch".into())),
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    fs::create_dir_all(path.parent().expect("asset path has a parent"))?;
                    atomic_write(&path, bytes)?;
                }
                Err(e) => return Err(e.into()),
            }
            Ok(asset)
        })
    }

    fn asset(&self, id: &str, asset_id: &str) -> Result<(Asset, Vec<u8>)> {
        let record = live(self.read(id)?.ok_or(CanvasError::NotFound)?)?;
        let asset = record
            .document()
            .assets
            .get(asset_id)
            .ok_or(CanvasError::NotFound)?
            .clone();
        let bytes = read_limited(&self.asset_path(id, asset_id)?, MAX_ASSET_BYTES as u64)?;
        if format!("{:x}", Sha256::digest(&bytes)) != asset.sha256 {
            return Err(CanvasError::Storage("asset digest mismatch".into()));
        }
        Ok((asset, bytes))
    }
}

fn read_limited(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "file exceeds size limit",
        ));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("invalid store path"))?;
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    Ok(result?)
}
