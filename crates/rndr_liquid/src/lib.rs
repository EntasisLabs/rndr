//! Liquid Markdown: paste-first UI fences for Markdown documents.
//!
//! A Liquid block is an ordinary fenced code block whose language names a
//! component — ` ```card `, ` ```chart `, ` ```tabs `, ` ```plan `, … — and
//! whose body is forgiving `key: value` text, `---`-separated items, or a GFM
//! table. Anything that does not parse stays an ordinary code fence, so a
//! document always degrades to readable Markdown.
//!
//! This crate is the headless grammar. It owns no Markdown engine, no
//! renderer, no navigation and no network access: [`parse_fence`] turns one
//! fence into a typed [`LiquidBlock`], and [`LiquidDocument::parse`] rewrites a
//! whole document, resolving nested fences innermost-first and leaving block
//! references in the Markdown for a host to mount.
//!
//! Intents, feed ids, image paths and URLs are carried as data; acting on them
//! is the host's decision.
//!
//! ```
//! use rndr_liquid::{LiquidBlock, LiquidDocument, Segment};
//!
//! let doc = LiquidDocument::parse(
//!     "Intro\n\n```chart\ntype: line\n| Month | Users |\n|---|---|\n| Jan | 10 |\n| Feb | 14 |\n```\n",
//! );
//! let blocks: Vec<_> = doc.segments().into_iter().filter_map(|s| match s {
//!     Segment::Block(i) => doc.block(i),
//!     Segment::Markdown(_) => None,
//! }).collect();
//! let LiquidBlock::Chart(chart) = blocks[0] else { panic!() };
//! assert_eq!(chart.series[0].values, [10.0, 14.0]);
//! ```

#![deny(missing_docs)]

mod block;
pub mod chart;
mod document;
mod icons;
mod kanban;
mod kv;
mod parse;
mod props;
mod slides;
mod styled;

pub use block::{parse_fence, LiquidBlock, FENCE_LANGS};
pub use document::{
    block_token, looks_like_prose, segments, LiquidDocument, Segment, BLOCK_TOKEN_DELIMITER,
};
pub use icons::{normalize_icon_id, split_icons, InlineIcon, ICON_ALLOWLIST};
pub use props::*;
pub use slides::split_section_breaks;
pub use styled::{block_font_size, block_line_height, BlockFont};
