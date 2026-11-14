//! Per-document splice journal and checkpoints.
//!
//! Frames are appended and fsynced as a group. A checkpoint writes the bytes
//! to a temporary file in the same directory, fsyncs it, renames it over the
//! document, fsyncs the directory, and records `SHA-256(bytes)` plus the
//! sequence in the journal header. Recovery replays only from the sequence
//! whose hash matches the file. An unknown hash is a conflict: the journal is
//! quarantined and the file is reloaded.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// SHA-256 of document bytes.
pub type Hash = [u8; 32];

const MAGIC: &[u8; 16] = b"rndr-md-journal\0";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 16 + 4 + 8 + 32;

/// One committed splice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Sequence number. The opened file is sequence 0.
    pub seq: u64,
    /// Start of the replaced range.
    pub start: u64,
    /// End of the replaced range.
    pub end: u64,
    /// Bytes that occupied the range before the splice.
    pub deleted: Vec<u8>,
    /// Bytes written by the splice.
    pub replacement: Vec<u8>,
    /// Hash of the document after this splice.
    pub post_hash: Hash,
}

/// The file changed outside this journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// Hash of the bytes on disk.
    pub file_hash: Hash,
    /// Hash of the session, when the session still has them.
    pub session_hash: Hash,
    /// Last sequence the session committed.
    pub sequence: u64,
    /// The session still holds its own bytes.
    pub in_memory: bool,
}

/// A journal could not be read or written.
#[derive(Debug)]
pub enum JournalError {
    /// Filesystem failure.
    Io(io::Error),
    /// The on-disk hash matches neither the header nor a frame.
    Conflict(Conflict),
}

impl From<io::Error> for JournalError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl PartialEq for JournalError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Conflict(a), Self::Conflict(b)) => a == b,
            _ => false,
        }
    }
}

/// Hash `bytes`.
pub fn hash_bytes(bytes: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Buffered frames for one document. Nothing is written until [`Journal::commit_group`].
#[derive(Clone, Debug, Default)]
pub struct Journal {
    pending: Vec<Frame>,
    /// Sequence of the last frame included in a checkpoint, or 0.
    pub checkpoint_seq: u64,
    /// Hash recorded in the header. `None` until the first group commit.
    pub checkpoint_hash: Option<Hash>,
    next_seq: u64,
}

impl Journal {
    /// An empty journal. Opening a document does not create one.
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            checkpoint_seq: 0,
            checkpoint_hash: None,
            next_seq: 1,
        }
    }

    /// Frames waiting for a group commit.
    pub fn pending(&self) -> &[Frame] {
        &self.pending
    }

    /// Remember a splice. The frame is not on disk until [`Self::commit_group`].
    pub fn push(
        &mut self,
        start: usize,
        end: usize,
        deleted: &[u8],
        replacement: &[u8],
        post: &[u8],
    ) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push(Frame {
            seq,
            start: start as u64,
            end: end as u64,
            deleted: deleted.to_vec(),
            replacement: replacement.to_vec(),
            post_hash: hash_bytes(post),
        });
    }

    /// Append pending frames and fsync them. The document file is not rewritten.
    pub fn commit_group(&mut self, path: &Path, base_hash: Hash) -> io::Result<()> {
        if self.pending.is_empty() && path.exists() {
            return Ok(());
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        if !path.exists() {
            write_header(path, 0, base_hash)?;
            self.checkpoint_seq = 0;
            self.checkpoint_hash = Some(base_hash);
        }
        let mut file = OpenOptions::new().append(true).open(path)?;
        for frame in &self.pending {
            file.write_all(&encode_frame(frame))?;
        }
        file.sync_all()?;
        self.pending.clear();
        Ok(())
    }

    /// Write `bytes` over `doc` and record the hash and sequence.
    ///
    /// Refuses when the file's hash is neither the last checkpoint nor `bytes`.
    pub fn checkpoint(
        &mut self,
        doc: &Path,
        journal: &Path,
        bytes: &[u8],
    ) -> Result<Hash, JournalError> {
        let hash = hash_bytes(bytes);
        if doc.exists() {
            let on_disk = fs::read(doc)?;
            let disk_hash = hash_bytes(&on_disk);
            let known = self.checkpoint_hash == Some(disk_hash) || disk_hash == hash;
            if !known {
                return Err(JournalError::Conflict(Conflict {
                    file_hash: disk_hash,
                    session_hash: hash,
                    sequence: self.checkpoint_seq,
                    in_memory: true,
                }));
            }
        }
        if !self.pending.is_empty() {
            let base = self.checkpoint_hash.unwrap_or(hash);
            self.commit_group(journal, base)?;
        }
        let tmp = temp_path(doc);
        {
            let mut file = File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, doc)?;
        fsync_dir(doc)?;
        let seq = self.next_seq.saturating_sub(1);
        write_header_hash(journal, seq, hash)?;
        self.checkpoint_seq = seq;
        self.checkpoint_hash = Some(hash);
        Ok(hash)
    }
}

/// Replay `journal` onto `doc` when the file hash matches a recorded sequence.
///
/// A hash that matches nothing quarantines the journal and returns a conflict.
/// The file bytes are left as they are.
pub fn recover(doc: &Path, journal: &Path) -> Result<Vec<u8>, JournalError> {
    let bytes = fs::read(doc)?;
    if !journal.exists() {
        return Ok(bytes);
    }
    let file = fs::read(journal)?;
    let (header_seq, header_hash, frames) = match decode_journal(&file) {
        Some(decoded) => decoded,
        None => return Ok(bytes),
    };
    let hash = hash_bytes(&bytes);
    if hash == header_hash {
        return replay_from(&bytes, header_seq, &frames).map_err(|file_hash| {
            JournalError::Conflict(Conflict {
                file_hash,
                session_hash: header_hash,
                sequence: header_seq,
                in_memory: false,
            })
        });
    }
    if let Some(frame) = frames.iter().find(|frame| frame.post_hash == hash) {
        return replay_from(&bytes, frame.seq, &frames).map_err(|file_hash| {
            JournalError::Conflict(Conflict {
                file_hash,
                session_hash: hash,
                sequence: frame.seq,
                in_memory: false,
            })
        });
    }
    let _ = quarantine(journal);
    Err(JournalError::Conflict(Conflict {
        file_hash: hash,
        session_hash: header_hash,
        sequence: header_seq,
        in_memory: false,
    }))
}

fn replay_from(bytes: &[u8], seq: u64, frames: &[Frame]) -> Result<Vec<u8>, Hash> {
    let mut current = bytes.to_vec();
    for frame in frames.iter().filter(|frame| frame.seq > seq) {
        let hash = hash_bytes(&current);
        if hash == frame.post_hash {
            continue;
        }
        let start = frame.start as usize;
        let end = frame.end as usize;
        if end <= current.len() && current[start..end] == frame.deleted {
            current.splice(start..end, frame.replacement.iter().copied());
            if hash_bytes(&current) != frame.post_hash {
                return Err(hash_bytes(&current));
            }
            continue;
        }
        if end <= current.len()
            && end - start == frame.replacement.len()
            && current[start..start + frame.replacement.len()] == frame.replacement
            && hash == frame.post_hash
        {
            continue;
        }
        return Err(hash);
    }
    Ok(current)
}

fn quarantine(path: &Path) -> io::Result<()> {
    let mut dest = path.as_os_str().to_os_string();
    dest.push(".quarantine");
    fs::rename(path, PathBuf::from(dest))
}

fn temp_path(doc: &Path) -> PathBuf {
    let mut name = doc.file_name().unwrap_or_default().to_os_string();
    name.push(".rndr-tmp");
    doc.with_file_name(name)
}

fn fsync_dir(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let dir = File::open(parent)?;
    dir.sync_all()
}

fn write_header(path: &Path, seq: u64, hash: Hash) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(&header_bytes(seq, hash))?;
    file.sync_all()?;
    Ok(())
}

fn write_header_hash(path: &Path, seq: u64, hash: Hash) -> io::Result<()> {
    if !path.exists() {
        return write_header(path, seq, hash);
    }
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.write_all(&header_bytes(seq, hash))?;
    file.sync_all()?;
    Ok(())
}

fn header_bytes(seq: u64, hash: Hash) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[..16].copy_from_slice(MAGIC);
    out[16..20].copy_from_slice(&VERSION.to_le_bytes());
    out[20..28].copy_from_slice(&seq.to_le_bytes());
    out[28..60].copy_from_slice(&hash);
    out
}

fn encode_frame(frame: &Frame) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&frame.seq.to_le_bytes());
    body.extend_from_slice(&frame.start.to_le_bytes());
    body.extend_from_slice(&frame.end.to_le_bytes());
    body.extend_from_slice(&(frame.deleted.len() as u64).to_le_bytes());
    body.extend_from_slice(&frame.deleted);
    body.extend_from_slice(&(frame.replacement.len() as u64).to_le_bytes());
    body.extend_from_slice(&frame.replacement);
    body.extend_from_slice(&frame.post_hash);
    let sum = checksum(&body);
    body.extend_from_slice(&sum.to_le_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut sum = 0xcbf29ce484222325u64;
    for byte in bytes {
        sum ^= u64::from(*byte);
        sum = sum.wrapping_mul(0x100000001b3);
    }
    sum
}

fn decode_journal(bytes: &[u8]) -> Option<(u64, Hash, Vec<Frame>)> {
    if bytes.len() < HEADER_LEN || &bytes[..16] != MAGIC {
        return None;
    }
    let seq = u64::from_le_bytes(bytes[20..28].try_into().ok()?);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes[28..60]);
    let mut frames = Vec::new();
    let mut i = HEADER_LEN;
    while i + 8 <= bytes.len() {
        let len = u64::from_le_bytes(bytes[i..i + 8].try_into().ok()?) as usize;
        i += 8;
        if i + len > bytes.len() || len < 8 {
            break;
        }
        let body = &bytes[i..i + len - 8];
        let stored = u64::from_le_bytes(bytes[i + len - 8..i + len].try_into().ok()?);
        if checksum(body) != stored {
            break;
        }
        frames.push(decode_frame(body)?);
        i += len;
    }
    Some((seq, hash, frames))
}

fn decode_frame(body: &[u8]) -> Option<Frame> {
    if body.len() < 8 * 3 {
        return None;
    }
    let seq = u64::from_le_bytes(body[0..8].try_into().ok()?);
    let start = u64::from_le_bytes(body[8..16].try_into().ok()?);
    let end = u64::from_le_bytes(body[16..24].try_into().ok()?);
    let deleted_len = u64::from_le_bytes(body[24..32].try_into().ok()?) as usize;
    let mut at = 32;
    if at + deleted_len + 8 > body.len() {
        return None;
    }
    let deleted = body[at..at + deleted_len].to_vec();
    at += deleted_len;
    let repl_len = u64::from_le_bytes(body[at..at + 8].try_into().ok()?) as usize;
    at += 8;
    if at + repl_len + 32 > body.len() {
        return None;
    }
    let replacement = body[at..at + repl_len].to_vec();
    at += repl_len;
    let mut post_hash = [0u8; 32];
    post_hash.copy_from_slice(&body[at..at + 32]);
    Some(Frame {
        seq,
        start,
        end,
        deleted,
        replacement,
        post_hash,
    })
}
