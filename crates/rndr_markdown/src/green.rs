//! Immutable green nodes and the red tree rebuilt over them.
//!
//! Green nodes own their text and have no parents. Unchanged nodes are shared
//! across edits, so a graft copies the path it replaces and not the rest of
//! the file. Red nodes are the session's view of the same bytes: offsets,
//! parents, and widths. Session ids are not stored here.

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use crate::kind::SyntaxKind;

thread_local! {
    /// Source of the parse currently running. Leaves cut from it share the
    /// buffer instead of copying each token.
    static PARSE_SOURCE: RefCell<Option<Arc<str>>> = const { RefCell::new(None) };
}

struct ParseSource {
    prev: Option<Arc<str>>,
}

impl ParseSource {
    fn enter(src: &str) -> (Arc<str>, Self) {
        let shared: Arc<str> = Arc::from(src);
        let prev = PARSE_SOURCE.with(|slot| slot.borrow_mut().replace(shared.clone()));
        (shared, Self { prev })
    }
}

impl Drop for ParseSource {
    fn drop(&mut self) {
        PARSE_SOURCE.with(|slot| *slot.borrow_mut() = self.prev.take());
    }
}

/// Run `f` on a shared copy of `src` so new leaves can alias that buffer.
pub(crate) fn with_parse_source<T>(src: &str, f: impl FnOnce(&str) -> T) -> T {
    let (shared, _guard) = ParseSource::enter(src);
    f(shared.as_ref())
}

#[derive(Clone, Eq)]
struct ByteSpan {
    buf: Arc<str>,
    start: usize,
    end: usize,
}

impl PartialEq for ByteSpan {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl ByteSpan {
    fn empty() -> Self {
        static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
        let buf = EMPTY.get_or_init(|| Arc::from("")).clone();
        Self {
            buf,
            start: 0,
            end: 0,
        }
    }

    fn owned(text: &str) -> Self {
        let buf: Arc<str> = Arc::from(text);
        let end = buf.len();
        Self { buf, start: 0, end }
    }

    fn shared(buf: Arc<str>, start: usize, end: usize) -> Self {
        Self { buf, start, end }
    }

    fn as_str(&self) -> &str {
        &self.buf[self.start..self.end]
    }
}

struct GreenInner {
    kind: SyntaxKind,
    text: ByteSpan,
    children: Vec<Green>,
    width: usize,
}

/// One immutable green node. Leaves own [`Self::text`]; branches own children.
/// The concatenation of every leaf is the source the node was parsed from.
///
/// Cloning a node is cheap: the bytes and children are shared until a new
/// parent is built.
#[derive(Clone)]
pub struct Green {
    inner: Arc<GreenInner>,
}

impl Eq for Green {}

impl PartialEq for Green {
    fn eq(&self, other: &Self) -> bool {
        self.inner.kind == other.inner.kind
            && self.inner.text == other.inner.text
            && self.inner.children == other.inner.children
    }
}

impl std::fmt::Debug for Green {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Green")
            .field("kind", &self.inner.kind)
            .field("text", &self.inner.text.as_str())
            .field("children", &self.inner.children)
            .finish()
    }
}

impl Green {
    /// A leaf. `text` is the exact source bytes of this token.
    pub fn leaf(kind: SyntaxKind, text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        let span = if text.is_empty() {
            ByteSpan::empty()
        } else if let Some(span) = PARSE_SOURCE.with(|slot| {
            slot.borrow().as_ref().and_then(|buf| {
                subslice_offset(buf, text)
                    .map(|start| ByteSpan::shared(buf.clone(), start, start + text.len()))
            })
        }) {
            span
        } else {
            ByteSpan::owned(text)
        };
        let width = span.as_str().len();
        Self {
            inner: Arc::new(GreenInner {
                kind,
                text: span,
                children: Vec::new(),
                width,
            }),
        }
    }

    /// A branch. The node's bytes are the concatenation of `children`.
    pub fn branch(kind: SyntaxKind, children: Vec<Green>) -> Self {
        let width = children.iter().map(Green::width).sum();
        Self {
            inner: Arc::new(GreenInner {
                kind,
                text: ByteSpan::empty(),
                children,
                width,
            }),
        }
    }

    /// Replace children `lo..hi` when this node is uniquely owned.
    /// Returns the window if another owner still holds it.
    pub(crate) fn splice_children(
        &mut self,
        lo: usize,
        hi: usize,
        window: Vec<Green>,
    ) -> Result<(), Vec<Green>> {
        let Some(inner) = Arc::get_mut(&mut self.inner) else {
            return Err(window);
        };
        inner.children.splice(lo..hi, window);
        inner.width = inner.children.iter().map(Green::width).sum();
        Ok(())
    }

    /// This node's kind.
    pub fn kind(&self) -> SyntaxKind {
        self.inner.kind
    }

    /// Leaf text. Empty for a branch.
    pub fn text(&self) -> &str {
        self.inner.text.as_str()
    }

    /// Child nodes. Empty for a leaf.
    pub fn children(&self) -> &[Green] {
        &self.inner.children
    }

    /// Byte length of the source this node covers.
    pub fn width(&self) -> usize {
        self.inner.width
    }

    /// The source bytes of this node.
    pub fn concat(&self) -> String {
        let mut out = String::with_capacity(self.width());
        self.write_concat(&mut out);
        out
    }

    fn write_concat(&self, out: &mut String) {
        if self.children().is_empty() {
            out.push_str(self.text());
        } else {
            for child in self.children() {
                child.write_concat(out);
            }
        }
    }
}

fn subslice_offset(buf: &str, text: &str) -> Option<usize> {
    let buf_start = buf.as_ptr() as usize;
    let text_start = text.as_ptr() as usize;
    let text_end = text_start + text.len();
    let buf_end = buf_start + buf.len();
    if text_start >= buf_start && text_end <= buf_end {
        Some(text_start - buf_start)
    } else {
        None
    }
}

/// A red node. `parent` and `children` are indexes into [`RedTree::nodes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedNode {
    /// Syntax kind.
    pub kind: SyntaxKind,
    /// Byte offset from the parent. The root is zero, so a top-level block's
    /// offset is its document position. A nested node's document position is
    /// the sum of offsets from the root.
    pub offset: usize,
    /// Byte length.
    pub width: usize,
    /// Parent index. The root has none.
    pub parent: Option<u32>,
    /// Child indexes, in source order.
    pub children: Vec<u32>,
    /// Leaf text. Empty for a branch. Shared with the green leaf when the
    /// bytes already live in a parse buffer.
    pub text: TokenText,
}

/// Leaf bytes of a red node. Dereferences to the text.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenText {
    buf: Arc<str>,
    start: usize,
    end: usize,
}

impl TokenText {
    fn from_green(green: &Green) -> Self {
        let span = &green.inner.text;
        Self {
            buf: span.buf.clone(),
            start: span.start,
            end: span.end,
        }
    }

    fn as_str(&self) -> &str {
        &self.buf[self.start..self.end]
    }
}

impl std::ops::Deref for TokenText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for TokenText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for TokenText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for TokenText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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

    /// Keep this red tree when `new_root` shares the children it did not
    /// reparse and those children kept their shape.
    ///
    /// Offsets are relative to the parent. Only the root's later children
    /// move when an earlier block changes width; nodes inside an untouched
    /// block stay put.
    pub(crate) fn reuse_graft(
        &mut self,
        new_root: &Green,
        replaced: std::ops::Range<usize>,
    ) -> bool {
        let new_children = new_root.children();
        if self.nodes.is_empty() || new_children.len() != self.nodes[0].children.len() {
            return false;
        }
        if replaced.end > new_children.len() {
            return false;
        }
        let root_children = self.nodes[0].children.clone();
        for index in replaced.clone() {
            if !same_shape(&new_children[index], self, root_children[index]) {
                return false;
            }
        }
        let last = replaced.end.checked_sub(1);
        let mut cursor = 0usize;
        for (index, child) in new_children.iter().enumerate() {
            let red_index = root_children[index];
            if replaced.contains(&index) {
                update_shape(self, red_index, child, cursor);
            } else if last.is_some_and(|end| index > end)
                && self.nodes[red_index as usize].offset != cursor
            {
                self.nodes[red_index as usize].offset = cursor;
            }
            cursor += child.width();
        }
        self.nodes[0].width = new_root.width();
        true
    }
}

fn same_shape(green: &Green, red: &RedTree, index: u32) -> bool {
    let Some(node) = red.get(index) else {
        return false;
    };
    if node.kind != green.kind() || node.children.len() != green.children().len() {
        return false;
    }
    green
        .children()
        .iter()
        .zip(&node.children)
        .all(|(child, &child_index)| same_shape(child, red, child_index))
}

fn update_shape(red: &mut RedTree, index: u32, green: &Green, relative: usize) {
    let children = red.nodes[index as usize].children.clone();
    {
        let node = &mut red.nodes[index as usize];
        node.offset = relative;
        node.width = green.width();
        node.text = TokenText::from_green(green);
    }
    let mut local = 0usize;
    for (child, &child_index) in green.children().iter().zip(&children) {
        update_shape(red, child_index, child, local);
        local += child.width();
    }
}

fn build(nodes: &mut Vec<RedNode>, green: &Green, relative: usize, parent: Option<u32>) -> u32 {
    let index = nodes.len() as u32;
    nodes.push(RedNode {
        kind: green.kind(),
        offset: relative,
        width: green.width(),
        parent,
        children: Vec::new(),
        text: TokenText::from_green(green),
    });
    if green.children().is_empty() {
        return index;
    }
    let mut local = 0usize;
    let mut children = Vec::new();
    for child in green.children() {
        let child_index = build(nodes, child, local, Some(index));
        local += child.width();
        children.push(child_index);
    }
    nodes[index as usize].children = children;
    debug_assert_eq!(local, green.width());
    index
}
