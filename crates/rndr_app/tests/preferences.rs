use rndr_app::Preferences;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prefs {
    version: u8,
    name: String,
}
fn store() -> Preferences {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/preferences-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    Preferences::at(dir.join("prefs.json"))
}
#[test]
fn missing_roundtrip_replace_and_malformed_are_distinct() {
    let store = store();
    assert_eq!(store.load::<Prefs>().unwrap(), None);
    let initial = Prefs {
        version: 1,
        name: "café 日本語 👋".into(),
    };
    store.save(&initial).unwrap();
    assert_eq!(store.load::<Prefs>().unwrap(), Some(initial));
    let next = Prefs {
        version: 1,
        name: "changed".into(),
    };
    store.save(&next).unwrap();
    assert_eq!(store.load::<Prefs>().unwrap(), Some(next));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(store.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    std::fs::write(
        store.path(),
        br#"{"version":1,"name":"ok","api_key":"not-allowed"}"#,
    )
    .unwrap();
    assert!(store.load::<Prefs>().is_err());
    std::fs::write(store.path(), "{broken").unwrap();
    assert!(store.load::<Prefs>().is_err());
}
#[test]
fn bounded_reads_and_failed_saves_preserve_destination() {
    let store = store();
    store
        .save(&Prefs {
            version: 1,
            name: "keep".into(),
        })
        .unwrap();
    let before = std::fs::read(store.path()).unwrap();
    assert!(store.save(&"x".repeat(17000)).is_err());
    assert_eq!(std::fs::read(store.path()).unwrap(), before);
    std::fs::write(store.path(), vec![b' '; 16385]).unwrap();
    assert!(store.load::<Prefs>().is_err());
    // An existing directory cannot be replaced by a file. Its contents survive,
    // and the uniquely owned sibling temp is removed after rename fails.
    let directory = store.path().parent().unwrap().join("directory");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("keep"), "keep").unwrap();
    assert!(Preferences::at(&directory).save(&1).is_err());
    assert_eq!(
        std::fs::read_to_string(directory.join("keep")).unwrap(),
        "keep"
    );
    assert!(!std::fs::read_dir(store.path().parent().unwrap())
        .unwrap()
        .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}
#[test]
fn app_identifier_cannot_escape_config_directory() {
    for id in [
        "",
        "..",
        ".",
        "../other",
        "/absolute",
        "with space",
        "back\\slash",
    ] {
        assert!(Preferences::for_app(id).is_err(), "accepted {id}");
    }
    assert!(Preferences::for_app("org.rndr.test")
        .unwrap()
        .path()
        .is_absolute());
}
