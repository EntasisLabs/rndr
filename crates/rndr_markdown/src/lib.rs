//! Lossless markdown concrete syntax tree.
//!
//! The bytes of a document are the authority. [`parse`] builds a green tree
//! whose concatenation is those bytes. The typed view borrows that tree:
//! Liquid fields are spans over the fence text, not a second document.

#![deny(missing_docs)]

mod edit;
mod green;
mod inline;
mod kind;
mod parse;
mod view;

pub use edit::{apply_text, apply_view, print_liquid, Applied, Command, EditError, Splice};
pub use green::{Green, RedNode, RedTree};
pub use kind::SyntaxKind;
pub use parse::parse;
pub use view::{Field, LiquidFence, TypedBlock, TypedView};

/// Parse `src` and return the red tree as well as the green root.
pub fn parse_red(src: &str) -> (Green, RedTree) {
    let green = parse(src);
    let red = RedTree::from_green(&green);
    (green, red)
}
