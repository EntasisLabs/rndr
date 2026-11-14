//! Lossless markdown concrete syntax tree.
//!
//! The bytes of a document are the authority. [`parse`] builds a green tree
//! whose concatenation is those bytes, including delimiter spelling, line
//! endings, escapes, and Liquid fences.

#![deny(missing_docs)]

mod green;
mod inline;
mod kind;
mod parse;

pub use green::{Green, RedNode, RedTree};
pub use kind::SyntaxKind;
pub use parse::parse;

/// Parse `src` and return the red tree as well as the green root.
pub fn parse_red(src: &str) -> (Green, RedTree) {
    let green = parse(src);
    let red = RedTree::from_green(&green);
    (green, red)
}
