//! YAML-ish `key: value` helpers shared by every fence grammar.

use std::collections::HashMap;
pub(crate) type Fields = HashMap<String, String>;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new($pat).unwrap());
    };
}
pub(crate) use re;

re!(LIST_MARKER, r"^[-*+]\s+");
re!(ORDERED_MARKER, r"^\d+\.\s+");
re!(PIPE_KV, r"\|\s*[a-zA-Z][a-zA-Z0-9_-]*\s*:");
re!(YAML_BLOCK_SCALAR, r"^[|>][+-]?$");
re!(NEW_FIELD, r"^([a-zA-Z][a-zA-Z0-9_-]*)\s*:(.*)$");
re!(HR, r"(?m)^---[ \t]*$");
re!(HR_PREFIX, r"^---[ \t]*\n?");
re!(GFM_SEP, r"^:?-+:?$");
re!(SLUG_STRIP, r"[^a-z0-9]+");

/// Normalize CRLF line endings.
pub(crate) fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Models often emit `- title: …` / `1. Label: …` inside fences.
pub(crate) fn strip_line_chrome(line: &str) -> String {
    let t = line.trim();
    let t = LIST_MARKER.replace(t, "");
    ORDERED_MARKER.replace(&t, "").into_owned()
}

pub(crate) fn has_pipe_kv(line: &str) -> bool {
    line.contains('|') && PIPE_KV.is_match(line)
}

pub(crate) fn is_block_scalar(value: &str) -> bool {
    YAML_BLOCK_SCALAR.is_match(value)
}

/// `title: Sol | body: Flagship` on one line.
pub(crate) fn parse_kv_line(line: &str) -> Fields {
    let mut out = Fields::new();
    for part in strip_line_chrome(line).split('|').map(str::trim) {
        if part.is_empty() {
            continue;
        }
        let Some(colon) = part.find(':').filter(|&c| c > 0) else {
            continue;
        };
        let key = part[..colon].trim().to_lowercase();
        let value = part[colon + 1..].trim();
        if !key.is_empty() && !value.is_empty() {
            out.insert(key, value.to_string());
        }
    }
    out
}

pub(crate) fn pipe_labels(raw: Option<&String>) -> Vec<String> {
    raw.map(|r| {
        r.split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    })
    .unwrap_or_default()
}

/// True when a line starts a new `key: value` field rather than a URL in prose.
pub(crate) fn is_new_field_line(line: &str) -> bool {
    let Some(caps) = NEW_FIELD.captures(line) else {
        return false;
    };
    let key = caps[1].to_lowercase();
    key != "http" && key != "https"
}

/// Splits `key: value` at the first colon (colon must not be first).
pub(crate) fn split_kv(line: &str) -> Option<(String, String)> {
    let colon = line.find(':').filter(|&c| c > 0)?;
    Some((
        line[..colon].trim().to_lowercase(),
        line[colon + 1..].trim().to_string(),
    ))
}

/// Collects a YAML block scalar body that follows `key: |-`.
pub(crate) fn take_block_scalar(lines: &[&str], i: &mut usize) -> String {
    let mut block: Vec<String> = Vec::new();
    while *i < lines.len() {
        let next = strip_line_chrome(lines[*i]);
        if !next.is_empty() && is_new_field_line(&next) {
            break;
        }
        *i += 1;
        if next.is_empty() && block.is_empty() {
            continue;
        }
        block.push(next);
    }
    while block.last().is_some_and(|l| l.is_empty()) {
        block.pop();
    }
    block.join("\n").trim().to_string()
}

/// Collects fields from a fence body, including YAML block scalars.
pub(crate) fn parse_kv_block(body: &str) -> Fields {
    let mut fields = Fields::new();
    let text = lf(body);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_line_chrome(lines[i]);
        i += 1;
        if line.is_empty() || line.starts_with('#') || line == "---" {
            continue;
        }
        if has_pipe_kv(&line) {
            fields.extend(parse_kv_line(&line));
            continue;
        }
        let Some((key, value)) = split_kv(&line) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if is_block_scalar(&value) {
            let joined = take_block_scalar(&lines, &mut i);
            if !joined.is_empty() {
                fields.insert(key, joined);
            }
            continue;
        }
        if !value.is_empty() {
            fields.insert(key, value);
        }
    }
    fields
}

/// `str.split(/^---[ \t]*$/m)`.
pub(crate) fn split_hr(s: &str) -> Vec<&str> {
    HR.split(s).collect()
}

/// Header before the first `---` line, and the remainder after it, if any.
pub(crate) fn split_header(s: &str) -> (&str, Option<&str>) {
    match HR.find(s) {
        Some(m) => {
            let rest = &s[m.start()..];
            let rest = HR_PREFIX.find(rest).map_or(rest, |p| &rest[p.end()..]);
            (&s[..m.start()], Some(rest))
        }
        None => (s, None),
    }
}

/// Stable, de-duplicated `prefix-slug` ids.
#[derive(Default)]
pub(crate) struct Ids(HashMap<String, usize>);

impl Ids {
    pub(crate) fn slug(&mut self, label: &str, prefix: &str, index: usize) -> String {
        let lower = label.to_lowercase();
        let base = SLUG_STRIP.replace_all(&lower, "-");
        let base = base.trim_matches('-');
        let stem = if base.is_empty() {
            format!("{prefix}-{index}")
        } else {
            format!("{prefix}-{base}")
        };
        self.claim(stem)
    }

    pub(crate) fn claim(&mut self, stem: String) -> String {
        let n = self.0.entry(stem.clone()).or_insert(0);
        let id = if *n == 0 { stem } else { format!("{stem}-{n}") };
        *n += 1;
        id
    }
}

/// Cells of a `| a | b |` row, or empty when the line is not a row.
pub(crate) fn split_pipe_cells(line: &str) -> Vec<String> {
    let s = strip_line_chrome(line);
    let Some(s) = s.strip_prefix('|') else {
        return Vec::new();
    };
    let s = s.strip_suffix('|').unwrap_or(s);
    s.split('|').map(|c| c.trim().to_string()).collect()
}

pub(crate) fn is_gfm_separator(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let t: String = cell.chars().filter(|c| !c.is_whitespace()).collect();
            t.is_empty() || GFM_SEP.is_match(&t)
        })
}

/// Splits a body into KV preamble lines and the GFM table that follows.
pub(crate) fn split_table(body: &str) -> (String, Vec<Vec<String>>) {
    let text = lf(body);
    let mut preamble = Vec::new();
    let mut table = Vec::new();
    let mut in_table = false;
    for raw in text.split('\n') {
        let stripped = strip_line_chrome(raw);
        if !in_table && stripped.starts_with('|') {
            in_table = true;
        }
        if in_table {
            if !stripped.is_empty() {
                table.push(raw);
            }
        } else {
            preamble.push(raw);
        }
    }
    if table.len() < 2 {
        return (preamble.join("\n"), Vec::new());
    }
    let rows = table
        .into_iter()
        .map(split_pipe_cells)
        .filter(|c| !c.is_empty() && !is_gfm_separator(c))
        .collect();
    (preamble.join("\n"), rows)
}

pub(crate) fn bool_loose(raw: Option<&String>) -> Option<bool> {
    match raw?.trim().to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" | "open" => Some(true),
        "false" | "no" | "0" | "off" | "closed" => Some(false),
        _ => None,
    }
}

/// Leading `key:` lines (accepted by `accept`) up to the first blank line or
/// other line; returns the preamble and the remaining body.
pub(crate) fn leading_preamble(text: &str, accept: impl Fn(&str) -> bool) -> (String, String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut preamble = Vec::new();
    let mut start = 0;
    for (i, raw) in lines.iter().enumerate() {
        let stripped = strip_line_chrome(raw);
        if stripped.is_empty() {
            if !preamble.is_empty() {
                start = i + 1;
                break;
            }
            continue;
        }
        if accept(&stripped) {
            preamble.push(*raw);
            start = i + 1;
            continue;
        }
        start = i;
        break;
    }
    (
        preamble.join("\n"),
        lines[start.min(lines.len())..].join("\n"),
    )
}

/// First present key, trimmed.
pub(crate) fn first(fields: &Fields, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| fields.get(*k))
        .map(|v| v.trim().to_string())
}
