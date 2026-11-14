use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rndr_markdown::{hash_bytes, recover, Command, JournalError, Session};

fn scratch(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rndr-md-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("note.md");
    let journal = dir.join("note.md.journal");
    (doc, journal)
}

#[test]
fn opening_a_file_writes_nothing() {
    let (doc, journal) = scratch("open");
    fs::write(&doc, "alpha\n").unwrap();
    let before = fs::read(&doc).unwrap();
    let session = Session::open_file(&doc, &journal).unwrap();
    assert_eq!(session.journal_frames(), 0);
    assert_eq!(session.bytes(), "alpha\n");
    assert!(!journal.exists());
    assert_eq!(fs::read(&doc).unwrap(), before);
    let _ = fs::remove_dir_all(doc.parent().unwrap());
}

#[test]
fn replay_after_a_group_commit_and_refuse_a_foreign_file() {
    let (doc, journal) = scratch("replay");
    fs::write(&doc, "alpha\n").unwrap();
    let mut session = Session::open_file(&doc, &journal).unwrap();
    session
        .edit(
            Command::Insert {
                at: 0,
                text: "Z".into(),
            },
            0,
        )
        .unwrap();
    session.poll(100).unwrap();
    assert!(journal.exists());
    assert_eq!(fs::read(&doc).unwrap(), b"alpha\n");
    let recovered = recover(&doc, &journal).unwrap();
    assert_eq!(recovered, b"Zalpha\n");
    // Replaying again is idempotent: the file is still the old checkpoint,
    // so the same frames apply once.
    let again = recover(&doc, &journal).unwrap();
    assert_eq!(again, recovered);

    fs::write(&doc, "someone else\n").unwrap();
    let err = recover(&doc, &journal).unwrap_err();
    assert!(matches!(err, JournalError::Conflict(_)));
    assert!(
        journal.with_extension("journal.quarantine").exists() || {
            let mut q = journal.into_os_string();
            q.push(".quarantine");
            std::path::PathBuf::from(q).exists()
        }
    );
    let _ = fs::remove_dir_all(doc.parent().unwrap());
}

#[test]
fn checkpoint_matches_the_hash_and_a_clean_flush_does_not_rewrite() {
    let (doc, journal) = scratch("check");
    fs::write(&doc, "alpha\n").unwrap();
    let mut session = Session::open_file(&doc, &journal).unwrap();
    let hash = session.flush().unwrap();
    assert_eq!(hash, hash_bytes(b"alpha\n"));
    let modified = fs::metadata(&doc).unwrap().modified().unwrap();
    let hash_again = session.flush().unwrap();
    assert_eq!(hash_again, hash);
    assert_eq!(fs::metadata(&doc).unwrap().modified().unwrap(), modified);

    session
        .edit(
            Command::Insert {
                at: 5,
                text: "!".into(),
            },
            0,
        )
        .unwrap();
    let hashed = session.flush().unwrap();
    assert_eq!(fs::read(&doc).unwrap(), b"alpha!\n");
    assert_eq!(hashed, hash_bytes(b"alpha!\n"));
    let _ = fs::remove_dir_all(doc.parent().unwrap());
}

#[test]
fn a_torn_tail_is_ignored() {
    let (doc, journal) = scratch("torn");
    fs::write(&doc, "alpha\n").unwrap();
    let mut session = Session::open_file(&doc, &journal).unwrap();
    session
        .edit(
            Command::Insert {
                at: 0,
                text: "Q".into(),
            },
            0,
        )
        .unwrap();
    session.poll(100).unwrap();
    let mut bytes = fs::read(&journal).unwrap();
    bytes.extend_from_slice(&[1, 2, 3, 4, 5]);
    fs::write(&journal, &bytes).unwrap();
    let recovered = recover(&doc, &journal).unwrap();
    assert_eq!(recovered, b"Qalpha\n");
    let _ = fs::remove_dir_all(doc.parent().unwrap());
}
