//! The browser stays off the core, UI, and paint graphs, and off by default.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn browser_is_absent_from_core_ui_paint_and_render() {
    for manifest in [
        "crates/rndr_core/Cargo.toml",
        "crates/rndr_ui/Cargo.toml",
        "crates/rndr_paint/Cargo.toml",
        "crates/rndr_render/Cargo.toml",
    ] {
        let text = read(manifest);
        assert!(
            !text.contains("rndr_browser"),
            "{manifest} must not depend on rndr_browser"
        );
        assert!(
            !text.contains("objc2-web-kit"),
            "{manifest} must not link WebKit"
        );
    }
}

#[test]
fn facade_browser_feature_is_off_and_does_not_enable_dialog() {
    let facade = read("crates/rndr/Cargo.toml");
    let browser = facade
        .lines()
        .find(|line| line.trim_start().starts_with("browser = "))
        .expect("browser feature");
    assert_eq!(browser.trim(), "browser = [\"dep:rndr_browser\"]");
    assert!(facade.contains("default = [\"hotkey\", \"tray\", \"parley\", \"accesskit\"]"));
    let browser_crate = read("crates/rndr_browser/Cargo.toml");
    assert!(
        !browser_crate.contains("publish = false"),
        "an unpublished browser crate breaks default facade packaging"
    );
    let publish_order = read("scripts/publish-crates.sh");
    assert!(
        publish_order.contains("rndr_render rndr_browser rndr"),
        "rndr_browser must be packaged immediately before the facade"
    );
    let ci = read(".github/workflows/ci.yml");
    assert!(
        !ci.contains("--exclude rndr_browser"),
        "CI must package rndr_browser with the facade"
    );
    assert!(browser_crate.contains("objc2-web-kit"));
    assert!(!browser_crate.contains("rndr_core"));
    assert!(!browser_crate.contains("rndr_ui"));
    assert!(!browser_crate.contains("rndr_paint"));
}

#[test]
fn sources_do_not_grow_a_guest_command_bridge() {
    let root = workspace_root().join("crates/rndr_browser/src");
    let mut files = Vec::new();
    collect_rust(&root, &mut files);
    let forbidden = [
        "evaluateJavaScript",
        "evaluate_script",
        "WKScriptMessageHandler",
        "WKURLSchemeHandler",
        "findString",
        "READ_GUEST",
        "post_message",
        "add_message_handler",
    ];
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in forbidden {
            assert!(
                !text.contains(needle),
                "{} contains {needle}",
                path.display()
            );
        }
    }
}

fn collect_rust(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_rust(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
