//! Whole-document preprocessing: finds Liquid fences (innermost first),
//! shields documentation fences, unwraps mistaken prose fences and
//! normalizes `{{icon:…}}` shortcodes.

use crate::block::{parse_fence, LiquidBlock, FENCE_LANGS};
use crate::icons::normalize_icon_id;
use crate::kv::re;

/// Delimiter around a block reference in [`LiquidDocument::markdown`].
pub const BLOCK_TOKEN_DELIMITER: char = '\u{E001}';
const DOC_SLOT: char = '\u{E000}';

pub(crate) const PROSE_MISTAKEN_LANGS: &[&str] = &[
    "code",
    "text",
    "plaintext",
    "plain",
    "markdown",
    "md",
    "output",
    "response",
    "answer",
    "txt",
];

re!(SOURCE_WHISPER, r"(?mi)^\|\s*Source:\s*(.+?)\s*\|\s*$");
re!(
    CODE_LEAD,
    r#"(?m)^(import |export |from ['"]|const |let |var |function |class |def |package |using |#include|<\?php|#!/)"#
);
re!(CODEY_LINE, r"^\s*[{}\]);]|^\s*//|^\s*/\*|=>|:=|;\s*$");
re!(
    EMPHASIS,
    r"\*\*[^*]+\*\*|(^|[^*])\*[^*\n]+\*([^*]|$)|_[^_\n]+_"
);
re!(SENTENCE_END, r#"[.!?]["'”’)]?\s*$"#);
re!(SENTENCE_BREAK, r#"[.!?][\s"'”’]"#);
re!(FENCE_OPEN, r"^(```+)([a-zA-Z0-9_-]*)[ \t]*$");
re!(FENCE_CLOSE, r"^(```+)[ \t]*$");
re!(BODY_HAS_OPENER, r"(?m)^```+[a-zA-Z0-9_-]*[ \t]*$");
re!(ICON, r"\{\{icon:([a-zA-Z0-9_-]+)\}\}");
re!(INNER_OPEN, r"^```([a-zA-Z0-9_-]*)[ \t]*$");
re!(INNER_CLOSE, r"^```[ \t]*$");

/// Markdown with Liquid fences replaced by block references.
///
/// Each parsed block is replaced by its own line, `\u{E001}N\u{E001}`, where
/// `N` indexes [`blocks`](Self::blocks). Block bodies that hold Markdown
/// (reports, tabs, sections, …) may themselves contain such references to
/// nested blocks; [`segments`] splits any such text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiquidDocument {
    /// Markdown with block references.
    pub markdown: String,
    /// Parsed blocks in creation order (innermost fences first).
    pub blocks: Vec<LiquidBlock>,
}

/// A run of Markdown or a block reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment<'a> {
    /// Plain Markdown.
    Markdown(&'a str),
    /// Index into [`LiquidDocument::blocks`].
    Block(usize),
}

impl LiquidDocument {
    /// Preprocesses `source`.
    pub fn parse(source: &str) -> Self {
        let mut blocks = Vec::new();
        let markdown = preprocess(source, &mut blocks);
        Self { markdown, blocks }
    }

    /// Block at `index`.
    pub fn block(&self, index: usize) -> Option<&LiquidBlock> {
        self.blocks.get(index)
    }

    /// Top-level segments of [`markdown`](Self::markdown).
    pub fn segments(&self) -> Vec<Segment<'_>> {
        segments(&self.markdown)
    }
}

/// The reference text for block `index`.
pub fn block_token(index: usize) -> String {
    format!("{BLOCK_TOKEN_DELIMITER}{index}{BLOCK_TOKEN_DELIMITER}")
}

/// Splits text into Markdown runs and block references.
pub fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(BLOCK_TOKEN_DELIMITER) {
        let after = &rest[start + BLOCK_TOKEN_DELIMITER.len_utf8()..];
        let Some(end) = after.find(BLOCK_TOKEN_DELIMITER) else {
            break;
        };
        let Ok(index) = after[..end].parse::<usize>() else {
            out.push(Segment::Markdown(
                &rest[..start + BLOCK_TOKEN_DELIMITER.len_utf8()],
            ));
            rest = after;
            continue;
        };
        if start > 0 {
            out.push(Segment::Markdown(&rest[..start]));
        }
        out.push(Segment::Block(index));
        rest = &after[end + BLOCK_TOKEN_DELIMITER.len_utf8()..];
    }
    if !rest.is_empty() {
        out.push(Segment::Markdown(rest));
    }
    out
}

/// True when a fenced body reads as narrative prose rather than source code.
pub fn looks_like_prose(body: &str) -> bool {
    let normalized = body.replace("\r\n", "\n");
    let t = normalized.trim();
    let len = t.encode_utf16().count();
    if len < 20 || t.contains("```") || CODE_LEAD.is_match(t) {
        return false;
    }
    if t.split('\n').filter(|l| CODEY_LINE.is_match(l)).count() >= 3 {
        return false;
    }
    EMPHASIS.is_match(t)
        || (SENTENCE_END.is_match(t) && len >= 40)
        || (SENTENCE_BREAK.is_match(t) && len > 48)
}

pub(crate) fn body_has_opener(body: &str) -> bool {
    BODY_HAS_OPENER.is_match(body)
}

pub(crate) fn is_liquid_lang(lang: &str) -> bool {
    FENCE_LANGS.contains(&lang)
}

pub(crate) fn fence_open(line: &str) -> Option<(usize, String)> {
    let caps = FENCE_OPEN.captures(line)?;
    Some((caps[1].len(), caps[2].to_lowercase()))
}

pub(crate) fn fence_close(line: &str, ticks: usize) -> bool {
    FENCE_CLOSE
        .captures(line)
        .is_some_and(|c| c[1].len() >= ticks)
}

struct TopFence {
    open: usize,
    close: usize,
    lang: String,
    body: String,
}

fn top_level_fences(lines: &[&str]) -> Vec<TopFence> {
    let mut fences = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((ticks, lang)) = fence_open(lines[i]) else {
            i += 1;
            continue;
        };
        let mut depth = 1;
        let mut j = i + 1;
        while j < lines.len() && depth > 0 {
            if let Some((t, l)) = fence_open(lines[j]) {
                if t >= ticks && !l.is_empty() {
                    depth += 1;
                    j += 1;
                    continue;
                }
            }
            if fence_close(lines[j], ticks) {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            j += 1;
        }
        if j >= lines.len() || depth != 0 {
            break;
        }
        let body = lines[i + 1..j].join("\n");
        let mut close = j;
        if !is_liquid_lang(&lang)
            && BODY_HAS_OPENER.is_match(&body)
            && j + 1 < lines.len()
            && fence_close(lines[j + 1], ticks)
        {
            close = j + 1;
        }
        fences.push(TopFence {
            open: i,
            close,
            lang,
            body,
        });
        i = close + 1;
    }
    fences
}

fn shield_documentation_fences(source: &str, slots: &mut Vec<String>) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let fences = top_level_fences(&lines);
    if fences.is_empty() {
        return source.to_string();
    }
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    for fence in fences.iter().rev() {
        if is_liquid_lang(&fence.lang) {
            continue;
        }
        let range = fence.open..fence.close + 1;
        let mistaken = (fence.lang.is_empty()
            || PROSE_MISTAKEN_LANGS.contains(&fence.lang.as_str()))
            && !BODY_HAS_OPENER.is_match(&fence.body)
            && looks_like_prose(&fence.body);
        if mistaken {
            let body: Vec<String> = if fence.body.is_empty() {
                Vec::new()
            } else {
                fence.body.split('\n').map(String::from).collect()
            };
            out.splice(range, body);
            continue;
        }
        let token = format!("{DOC_SLOT}{}{DOC_SLOT}", slots.len());
        slots.push(lines[range.clone()].join("\n"));
        out.splice(range, [token]);
    }
    out.join("\n")
}

fn restore_slots(source: &str, slots: &[String]) -> String {
    if slots.is_empty() {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find(DOC_SLOT) {
        let after = &rest[start + DOC_SLOT.len_utf8()..];
        let slot = after
            .find(DOC_SLOT)
            .and_then(|end| Some((end, after[..end].parse::<usize>().ok()?)));
        match slot {
            Some((end, index)) => {
                out.push_str(&rest[..start]);
                out.push_str(slots.get(index).map_or("", String::as_str));
                rest = &after[end + DOC_SLOT.len_utf8()..];
            }
            None => {
                out.push_str(&rest[..start + DOC_SLOT.len_utf8()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Replacement text for one innermost fence, or `None` to keep it.
fn replace_fence(lang: &str, body: &str, blocks: &mut Vec<LiquidBlock>) -> Option<String> {
    let lang = lang.trim().to_lowercase();
    let mut emit = |block: LiquidBlock| {
        blocks.push(block);
        Some(format!("\n{}\n", block_token(blocks.len() - 1)))
    };
    if lang == "code" {
        if let Some(block) = parse_fence("code", body) {
            return emit(block);
        }
        return looks_like_prose(body).then(|| format!("\n{}\n", body.trim()));
    }
    if (lang.is_empty() || PROSE_MISTAKEN_LANGS.contains(&lang.as_str())) && looks_like_prose(body)
    {
        return Some(format!("\n{}\n", body.trim()));
    }
    parse_fence(&lang, body).and_then(emit)
}

/// One pass over innermost fences (no line inside starts with ```).
/// Returns the first replacement made, as `(start, end, text)` byte offsets.
fn first_innermost(text: &str, blocks: &mut Vec<LiquidBlock>) -> Option<(usize, usize, String)> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let line_at = |n: usize| {
        let s = starts[n];
        let e = starts.get(n + 1).map_or(text.len(), |&e| e - 1);
        &text[s..e]
    };
    let mut n = 0;
    while n < starts.len() {
        let line = line_at(n);
        let Some(caps) = INNER_OPEN.captures(line).filter(|_| n + 1 < starts.len()) else {
            n += 1;
            continue;
        };
        let Some(close) = (n + 1..starts.len()).find(|&k| line_at(k).starts_with("```")) else {
            n += 1;
            continue;
        };
        if !INNER_CLOSE.is_match(line_at(close)) {
            n += 1;
            continue;
        }
        let body_start = starts[n + 1];
        let body_end = starts[close];
        let end = starts[close] + line_at(close).len();
        if let Some(rep) = replace_fence(&caps[1], &text[body_start..body_end], blocks) {
            return Some((starts[n], end, rep));
        }
        n = close + 1;
    }
    None
}

fn process_nested(text: String, blocks: &mut Vec<LiquidBlock>) -> String {
    let mut out = text;
    while let Some((start, end, rep)) = first_innermost(&out, blocks) {
        out.replace_range(start..end, &rep);
    }
    out
}

fn repair_trailing_prose_fence(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut open: Option<(usize, usize, String)> = None;
    for (i, line) in lines.iter().enumerate() {
        match &open {
            None => {
                if let Some((ticks, lang)) = fence_open(line) {
                    open = Some((i, ticks, lang));
                }
            }
            Some((_, ticks, _)) => {
                if fence_close(line, *ticks) {
                    open = None;
                }
            }
        }
    }
    let Some((line, _, lang)) = open else {
        return source.to_string();
    };
    let after = lines[line + 1..].join("\n");
    if after.split('\n').any(|l| l.starts_with("```")) || is_liquid_lang(&lang) {
        return source.to_string();
    }
    if after.trim().is_empty() {
        return lines[..line].join("\n");
    }
    if lang.is_empty() || PROSE_MISTAKEN_LANGS.contains(&lang.as_str()) || looks_like_prose(&after)
    {
        let mut kept = lines[..line].to_vec();
        kept.extend_from_slice(&lines[line + 1..]);
        return kept.join("\n");
    }
    source.to_string()
}

fn normalize_icons(source: &str) -> String {
    let mut in_fence = false;
    source
        .split('\n')
        .map(|line| {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                return line.to_string();
            }
            if in_fence {
                return line.to_string();
            }
            ICON.replace_all(line, |caps: &regex::Captures| {
                match normalize_icon_id(&caps[1]) {
                    Some(id) => format!("{{{{icon:{id}}}}}"),
                    None => String::new(),
                }
            })
            .into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn preprocess(source: &str, blocks: &mut Vec<LiquidBlock>) -> String {
    let normalized = source.replace("\r\n", "\n");
    let out = SOURCE_WHISPER.replace_all(&normalized, |caps: &regex::Captures| {
        format!("*Source: {}*", caps[1].trim())
    });
    let mut slots = Vec::new();
    let out = shield_documentation_fences(&out, &mut slots);
    let out = process_nested(out, blocks);
    let out = restore_slots(&out, &slots);
    let out = repair_trailing_prose_fence(&out);
    normalize_icons(&out)
}
