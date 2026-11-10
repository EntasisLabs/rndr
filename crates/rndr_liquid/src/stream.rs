//! Append-only Liquid documents.
//!
//! [`LiquidStream`] keeps the markdown and blocks for a stable prefix of the
//! source and reparses only the tail. An unclosed Liquid fence is not emitted
//! as a code fence; [`LiquidStream::pending_lang`] names it until the close
//! arrives.

use crate::block::FENCE_LANGS;
use crate::document::{
    body_has_opener, fence_close, fence_open, is_liquid_lang, preprocess, PROSE_MISTAKEN_LANGS,
};
use crate::LiquidBlock;

/// How [`LiquidStream::push_str`] changed the markdown the host should show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownWrite {
    /// Bytes to append. Empty when the chunk did not settle any markdown,
    /// which is the case while an unclosed Liquid fence grows.
    Append(String),
    /// The markdown was rebuilt from the source. Replace the host document.
    Replace(String),
}

/// One streaming Liquid document.
///
/// [`Self::push_str`] reparses only the source that became stable since the
/// previous call. Finished fences keep their block index. A finished document
/// matches [`crate::LiquidDocument::parse`] of the same source.
#[derive(Clone, Debug, Default)]
pub struct LiquidStream {
    source: String,
    /// `source[..parsed]` has been folded into `markdown` and `blocks`.
    parsed: usize,
    /// Bytes of `source[parsed..]` already copied verbatim into `markdown`.
    raw: usize,
    markdown: String,
    blocks: Vec<LiquidBlock>,
    pending: Option<String>,
}

impl LiquidStream {
    /// Stream `source` from the start.
    pub fn new(source: &str) -> Self {
        let mut stream = Self::default();
        stream.push_str(source);
        stream
    }

    /// Raw source, with CRLF normalized to LF.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Markdown for the settled prefix, plus any open non-Liquid fence copied
    /// through as code.
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    /// Blocks closed so far, in creation order.
    pub fn blocks(&self) -> &[LiquidBlock] {
        &self.blocks
    }

    /// Byte offset of the source prefix that will not be parsed again.
    ///
    /// An unclosed fence starts here. A finished source reports
    /// [`source`](Self::source)`.len()`.
    pub fn stable_prefix(&self) -> usize {
        self.parsed
    }

    /// Language of the unclosed Liquid fence at the end of the source.
    ///
    /// `Some("")` means the opening line is still being typed and may become
    /// a Liquid fence. `None` means the tail is settled or is ordinary code.
    pub fn pending_lang(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    /// Append `chunk`.
    ///
    /// An empty chunk is a no-op. The returned write is empty while the new
    /// bytes only extend an unclosed Liquid fence.
    pub fn push_str(&mut self, chunk: &str) -> MarkdownWrite {
        if chunk.is_empty() {
            return MarkdownWrite::Append(String::new());
        }
        let before = self.markdown.len();
        self.source.push_str(&chunk.replace("\r\n", "\n"));
        if self.recompute() {
            MarkdownWrite::Replace(self.markdown.clone())
        } else {
            MarkdownWrite::Append(self.markdown[before..].to_string())
        }
    }

    fn recompute(&mut self) -> bool {
        let cut = stream_cut(&self.source, self.parsed);
        if cut.stable < self.parsed {
            self.blocks.clear();
            self.markdown.clear();
            self.parsed = 0;
            self.raw = 0;
            self.apply(stream_cut(&self.source, 0));
            return true;
        }
        self.apply(cut);
        false
    }

    fn apply(&mut self, cut: StreamCut) {
        if cut.stable > self.parsed {
            if self.raw > 0 {
                let start = self.parsed + self.raw;
                if cut.stable > start {
                    self.markdown.push_str(&self.source[start..cut.stable]);
                }
            } else {
                let slice = &self.source[self.parsed..cut.stable];
                if !slice.is_empty() {
                    let markdown = preprocess(slice, &mut self.blocks);
                    self.markdown.push_str(&markdown);
                }
            }
            self.parsed = cut.stable;
            self.raw = 0;
        }
        self.pending.clone_from(&cut.pending);
        if cut.raw {
            let tail = &self.source[self.parsed..];
            if tail.len() > self.raw {
                self.markdown.push_str(&tail[self.raw..]);
                self.raw = tail.len();
            }
        }
    }
}

struct StreamCut {
    stable: usize,
    pending: Option<String>,
    raw: bool,
}

struct Open {
    start: usize,
    body_at: usize,
    ticks: usize,
    lang: String,
    liquid: bool,
}

fn stream_cut(source: &str, from: usize) -> StreamCut {
    let bytes = source.as_bytes();
    // A safe partial line can leave `from` mid-line. Resume at that line so a
    // later chunk cannot be mistaken for a fence opener.
    let resume = if from == 0 || bytes.get(from - 1) == Some(&b'\n') {
        from
    } else {
        bytes[..from]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(0)
    };
    let mut i = resume;
    let mut stable = from;
    let mut stack: Vec<Open> = Vec::new();
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
        let body = &source[start..i];
        let complete = i < bytes.len();
        let next = if complete { i + 1 } else { i };
        let top_ticks = stack.last().map(|top| top.ticks);
        if let Some(top_ticks) = top_ticks {
            if complete {
                if let Some((ticks, lang)) = fence_open(body) {
                    if ticks >= top_ticks && !lang.is_empty() {
                        let liquid = is_liquid_lang(&lang);
                        stack.push(Open {
                            start,
                            body_at: next,
                            ticks,
                            lang,
                            liquid,
                        });
                        i = next;
                        continue;
                    }
                }
                if fence_close(body, top_ticks) {
                    let open = stack.pop().expect("close matched an open fence");
                    let mut end = next;
                    if stack.is_empty() && !open.liquid {
                        let fenced = &source[open.body_at..start];
                        if body_has_opener(fenced) {
                            if let Some(extra) = extra_close(source, next, open.ticks) {
                                end = extra;
                            }
                        }
                    }
                    if stack.is_empty() {
                        stable = end;
                    }
                    i = end;
                    continue;
                }
            }
            i = next;
            if !complete {
                break;
            }
            continue;
        }
        if let Some((ticks, lang)) = fence_open(body) {
            if !complete {
                return partial_opener(start, lang);
            }
            let liquid = is_liquid_lang(&lang);
            stack.push(Open {
                start,
                body_at: next,
                ticks,
                lang,
                liquid,
            });
            i = next;
            continue;
        }
        if !complete && !safe_partial(body) {
            return StreamCut {
                stable,
                pending: None,
                raw: false,
            };
        }
        stable = next;
        i = next;
    }
    if let Some(open) = stack.first() {
        return cut_open(open);
    }
    StreamCut {
        stable,
        pending: None,
        raw: false,
    }
}

fn partial_opener(start: usize, lang: String) -> StreamCut {
    if could_be_liquid(&lang) {
        StreamCut {
            stable: start,
            pending: Some(lang),
            raw: false,
        }
    } else {
        StreamCut {
            stable: start,
            pending: None,
            raw: true,
        }
    }
}

fn cut_open(open: &Open) -> StreamCut {
    if open.liquid {
        StreamCut {
            stable: open.start,
            pending: Some(open.lang.clone()),
            raw: false,
        }
    } else if open.lang.is_empty() || PROSE_MISTAKEN_LANGS.contains(&open.lang.as_str()) {
        StreamCut {
            stable: open.start,
            pending: None,
            raw: false,
        }
    } else {
        StreamCut {
            stable: open.start,
            pending: None,
            raw: true,
        }
    }
}

fn extra_close(source: &str, at: usize, ticks: usize) -> Option<usize> {
    if at >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut i = at;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    (i < bytes.len() && fence_close(&source[at..i], ticks)).then_some(i + 1)
}

fn could_be_liquid(lang: &str) -> bool {
    lang.is_empty() || FENCE_LANGS.iter().any(|known| known.starts_with(lang))
}

fn safe_partial(body: &str) -> bool {
    // `{` can still grow into `{{icon:…}}`, which is rewritten when the line
    // finishes. A committed brace cannot be rewritten later.
    !body.trim_start().starts_with('`') && !body.contains('{') && !body.starts_with('|')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiquidDocument;

    fn feed(source: &str, size: usize) -> LiquidStream {
        let mut stream = LiquidStream::new("");
        let mut at = 0;
        while at < source.len() {
            let mut end = (at + size).min(source.len());
            while !source.is_char_boundary(end) {
                end += 1;
            }
            stream.push_str(&source[at..end]);
            at = end;
        }
        stream
    }

    fn assert_matches_parse(source: &str) {
        for size in [1usize, 3, 7, 64, 4096] {
            let stream = feed(source, size);
            let doc = LiquidDocument::parse(source);
            assert_eq!(stream.blocks(), doc.blocks.as_slice(), "{source:?} @{size}");
            assert_eq!(stream.markdown(), doc.markdown, "{source:?} @{size}");
            assert_eq!(stream.pending_lang(), None, "{source:?} @{size}");
            assert_eq!(stream.stable_prefix(), stream.source().len());
        }
    }

    #[test]
    fn finished_chunks_match_a_full_parse() {
        assert_matches_parse("Intro\n\n```callout\ntone: tip\nbody: Hi\n```\n\nTail\n");
        assert_matches_parse(
            "```report\ntitle: Weekly\n\nOpening.\n\n```callout\ntone: tip\nbody: Inner\n```\n\n## Metrics\n\nMore.\n```\n",
        );
        assert_matches_parse("````markdown\n```card\ntitle: Example\n```\n````\n");
        assert_matches_parse("Go {{icon:Home}} {{icon:skull}}!\n```\n{{icon:home}}\n```\n");
        assert_matches_parse("```rust\nfn main() {}\n```\n\n```card\n```\n");
        assert_matches_parse("Hello `inline` and [a](https://example.com).\n\nNext.\n");
    }

    #[test]
    fn an_open_liquid_fence_is_pending_and_not_markdown() {
        let mut stream = LiquidStream::new("Before\n\n```callout\ntone: tip\nbody: Hi\n```\n\n");
        let settled = stream.markdown().to_string();
        let blocks = stream.blocks().to_vec();
        let write = stream.push_str("```callout\ntone: tip\nbody: Still");
        assert_eq!(write, MarkdownWrite::Append(String::new()));
        assert_eq!(stream.markdown(), settled);
        assert_eq!(stream.blocks(), blocks.as_slice());
        assert_eq!(stream.pending_lang(), Some("callout"));
        assert!(!stream.markdown().contains("```callout"));
        assert!(stream.stable_prefix() < stream.source().len());
        stream.push_str(" open\n```\n");
        let doc = LiquidDocument::parse(stream.source());
        assert_eq!(stream.blocks(), doc.blocks.as_slice());
        assert_eq!(stream.markdown(), doc.markdown);
        assert_eq!(stream.pending_lang(), None);
    }

    #[test]
    fn an_open_code_fence_stays_a_code_fence() {
        let source = "See\n\n```rust\nfn a() {}\n";
        let stream = LiquidStream::new(source);
        let doc = LiquidDocument::parse(source);
        assert_eq!(stream.markdown(), doc.markdown);
        assert!(stream.pending_lang().is_none());
        assert!(stream.markdown().contains("```rust"));
    }
}
