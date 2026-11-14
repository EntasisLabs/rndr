//! Immutable green nodes and the red tree rebuilt over them.
//!
//! Green nodes own their text and have no parents. Red nodes are the session's
//! view of the same bytes: offsets, parents, and widths. A graft builds a new
//! green root and a new red tree. Session ids are not stored here.

use crate::kind::SyntaxKind;

/// One immutable green node. Leaves own [`Self::text`]; branches own children.
/// The concatenation of every leaf is the source the node was parsed from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Green {
    kind: SyntaxKind,
    text: String,
    children: Vec<Green>,
}

impl Green {
    /// A leaf. `text` is the exact source bytes of this token.
    pub fn leaf(kind: SyntaxKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            children: Vec::new(),
        }
    }

    /// A branch. The node's bytes are the concatenation of `children`.
    pub fn branch(kind: SyntaxKind, children: Vec<Green>) -> Self {
        Self {
            kind,
            text: String::new(),
            children,
        }
    }

    /// This node's kind.
    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// Leaf text. Empty for a branch.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Child nodes. Empty for a leaf.
    pub fn children(&self) -> &[Green] {
        &self.children
    }

    /// Byte length of the source this node covers.
    pub fn width(&self) -> usize {
        if self.children.is_empty() {
            self.text.len()
        } else {
            self.children.iter().map(Green::width).sum()
        }
    }

    /// The source bytes of this node.
    pub fn concat(&self) -> String {
        let mut out = String::new();
        self.write_concat(&mut out);
        out
    }

    fn write_concat(&self, out: &mut String) {
        if self.children.is_empty() {
            out.push_str(&self.text);
        } else {
            for child in &self.children {
                child.write_concat(out);
            }
        }
    }
}

/// A red node. `parent` and `children` are indexes into [`RedTree::nodes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedNode {
    /// Syntax kind.
    pub kind: SyntaxKind,
    /// Byte offset from the start of the document.
    pub offset: usize,
    /// Byte length.
    pub width: usize,
    /// Parent index. The root has none.
    pub parent: Option<u32>,
    /// Child indexes, in source order.
    pub children: Vec<u32>,
    /// Leaf text. Empty for a branch.
    pub text: String,
}

/// Parent-linked view of a [`Green`] tree. Rebuilt after every graft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedTree {
    nodes: Vec<RedNode>,
}

impl RedTree {
    /// Index of the root.
    pub const ROOT: u32 = 0;

    /// Build red nodes in preorder. Offsets are byte offsets from the root.
    pub fn from_green(green: &Green) -> Self {
        let mut nodes = Vec::new();
        build(&mut nodes, green, 0, None);
        Self { nodes }
    }

    /// Every node, root first.
    pub fn nodes(&self) -> &[RedNode] {
        &self.nodes
    }

    /// The root node.
    pub fn root(&self) -> &RedNode {
        &self.nodes[0]
    }

    /// Borrow one node.
    pub fn get(&self, index: u32) -> Option<&RedNode> {
        self.nodes.get(index as usize)
    }
}

fn build(nodes: &mut Vec<RedNode>, green: &Green, offset: usize, parent: Option<u32>) -> u32 {
    let index = nodes.len() as u32;
    nodes.push(RedNode {
        kind: green.kind(),
        offset,
        width: 0,
        parent,
        children: Vec::new(),
        text: green.text().to_string(),
    });
    if green.children().is_empty() {
        nodes[index as usize].width = green.text().len();
        return index;
    }
    let mut cursor = offset;
    let mut children = Vec::new();
    for child in green.children() {
        let child_index = build(nodes, child, cursor, Some(index));
        cursor += nodes[child_index as usize].width;
        children.push(child_index);
    }
    nodes[index as usize].width = cursor - offset;
    nodes[index as usize].children = children;
    index
}
