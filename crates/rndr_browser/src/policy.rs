//! Decisions shared by the Windows backend. No WebView2 type lives here, so
//! the tests run on every host.

#![cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`. The WebView2 loader returns
/// this when the runtime is not installed.
pub(crate) fn is_missing_runtime(code: i32) -> bool {
    code as u32 == 0x8007_0002
}

/// Logical points times a window scale. A missing or non-finite scale is 1.
pub(crate) fn logical_to_pixels(logical: f32, scale: f64) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let px = f64::from(logical) * scale;
    if !px.is_finite() || px <= 0.0 {
        0
    } else if px >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        px.round() as u32
    }
}

/// Logical points to parent-client pixels. A zero DPI is treated as 96.
pub(crate) fn client_pixels(logical: f32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { dpi };
    let px = f64::from(logical) * f64::from(dpi) / 96.0;
    px.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// `Page.fileChooserOpened` mode. Anything other than `selectMultiple` is one file.
pub(crate) fn file_chooser_multiple(json: &str) -> bool {
    json.split_whitespace()
        .collect::<String>()
        .contains("\"mode\":\"selectMultiple\"")
}

/// `Page.handleFileChooser` parameters. `None` and an empty list cancel.
pub(crate) fn file_chooser_command(paths: Option<&[String]>) -> String {
    let Some(paths) = paths.filter(|paths| !paths.is_empty()) else {
        return "{\"action\":\"cancel\"}".to_string();
    };
    let files = paths
        .iter()
        .map(|path| format!("\"{}\"", json_escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"action\":\"accept\",\"files\":[{files}]}}")
}

pub(crate) fn suggested_download_name(disposition: &str, url: &str) -> String {
    if let Some(name) = filename_from_disposition(disposition) {
        return name;
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit(['/', '\\']).next().unwrap_or("").trim();
    if name.is_empty() {
        "download".to_string()
    } else {
        name.to_string()
    }
}

fn filename_from_disposition(header: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();
    let index = lower.find("filename=")?;
    let rest = header[index + "filename=".len()..].trim_start();
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        let name = &rest[..end];
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else {
        let name = rest.split([';', ' ', '\t', '\r', '\n']).next()?.trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other if other.is_control() => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_is_file_not_found() {
        assert!(is_missing_runtime(0x8007_0002_u32 as i32));
        assert!(!is_missing_runtime(0));
        assert!(!is_missing_runtime(-1));
    }

    #[test]
    fn logical_points_follow_the_window_scale() {
        assert_eq!(logical_to_pixels(10.0, 1.0), 10);
        assert_eq!(logical_to_pixels(10.0, 2.0), 20);
        assert_eq!(logical_to_pixels(0.0, 2.0), 0);
        assert_eq!(logical_to_pixels(10.0, 0.0), 10);
    }

    #[test]
    fn logical_points_scale_with_dpi() {
        assert_eq!(client_pixels(10.0, 96), 10);
        assert_eq!(client_pixels(10.0, 0), 10);
        assert_eq!(client_pixels(10.0, 192), 20);
        assert_eq!(client_pixels(0.0, 144), 0);
        assert_eq!(client_pixels(1.0, 144), 2);
    }

    #[test]
    fn file_chooser_mode_and_command() {
        assert!(!file_chooser_multiple(r#"{"mode":"selectSingle"}"#));
        assert!(file_chooser_multiple(r#"{"mode": "selectMultiple"}"#));
        assert_eq!(file_chooser_command(None), r#"{"action":"cancel"}"#);
        assert_eq!(file_chooser_command(Some(&[])), r#"{"action":"cancel"}"#);
        assert_eq!(
            file_chooser_command(Some(&[r"C:\a\b.txt".into(), "quote\"x".into()])),
            r#"{"action":"accept","files":["C:\\a\\b.txt","quote\"x"]}"#
        );
    }

    #[test]
    fn download_name_prefers_the_disposition() {
        assert_eq!(
            suggested_download_name(
                "attachment; filename=\"rndr.txt\"",
                "https://example.test/a"
            ),
            "rndr.txt"
        );
        assert_eq!(
            suggested_download_name("attachment; filename=plain.bin", "https://example.test/a"),
            "plain.bin"
        );
        assert_eq!(
            suggested_download_name("", "https://example.test/dir/file.bin?x=1"),
            "file.bin"
        );
        assert_eq!(
            suggested_download_name("", "https://example.test/"),
            "download"
        );
    }
}
