//! Lossless markdown concrete syntax tree.
//!
//! The bytes of a document are the authority. [`parse`] builds a green tree
//! whose concatenation is those bytes, including delimiter spelling, line
//! endings, escapes, and Liquid fences. A typed view and a journal sit on top
//! of that tree; neither one is a second copy of the file.
//!
//! This crate is unpublished. It does not depend on `rndr_ui`, `rndr_paint`,
//! or `rndr_render`, and `rndr_core` does not depend on it.

#![deny(missing_docs)]

mod edit;
mod green;
mod inline;
mod journal;
mod kind;
mod parse;
mod session;
mod view;

pub use edit::{apply_text, apply_view, print_liquid, Applied, Command, EditError, Splice};
pub use green::{Green, RedNode, RedTree};
pub use journal::{hash_bytes, recover, Conflict, Frame, Hash, Journal, JournalError};
pub use kind::SyntaxKind;
pub use parse::parse;
pub use session::{floor_grapheme, Session, ViewMode};
pub use view::{Field, LiquidFence, TypedBlock, TypedView};

/// Parse `src` and return the red tree as well as the green root.
pub fn parse_red(src: &str) -> (Green, RedTree) {
    let green = parse(src);
    let red = RedTree::from_green(&green);
    (green, red)
}
