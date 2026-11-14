//! Accessibility tree of a markdown document.
//!
//! The tree follows the concrete syntax, not a markdown dump and not one text
//! value. Offscreen blocks are still nodes. Their bounds are zero because they
//! were not shaped. [`AccessCommand::SetValue`] stays unsupported: the core
//! dispatcher already returns [`AccessCommandResult::UnsupportedSetValue`] for
//! a node that is not a scrollbar or a numeric control.
//!
//! A selection action carries the document id and revision from
//! [`access_snapshot`]. A mismatch refuses. VoiceOver, NVDA, and Orca are not
//! exercised here.

use std::ops::Range;

use rndr_core::{AccessCommandResult, AccessNode, AccessOverlay, AccessRole, Rect, World};
use rndr_markdown::{Green, SyntaxKind};

use crate::document::MarkdownId;
use crate::surface::root_document;

/// Published tree plus the revision a later selection action must echo.
#[derive(Clone, Debug)]
pub struct MarkdownAccessSnapshot {
    /// Document the tree was read from.
    pub document: MarkdownId,
    /// [`rndr_markdown::Session::revision`] at publication.
    pub revision: u64,
    /// Anchor and head, in source bytes.
    pub selection: (usize, usize),
    /// Block nodes, in preorder. The surface root is their parent in
    /// [`rndr_core::access_tree`] and is not repeated here.
    pub nodes: Vec<AccessNode>,
}

/// Read the tree currently published on `entity`.
pub fn access_snapshot(world: &World, entity: rndr_core::Entity) -> Option<MarkdownAccessSnapshot> {
    let document = root_document(world, entity)?;
    let session = world
        .resource::<crate::MarkdownDocuments>()?
        .get(document)?;
    let nodes = world.get::<AccessOverlay>(entity)?.nodes.clone();
    Some(MarkdownAccessSnapshot {
        document,
        revision: session.revision(),
        selection: (session.anchor(), session.caret()),
        nodes,
    })
}

/// Move the caret when `snapshot` is still the published revision.
///
/// A stale revision, a different document, or an open composition refuses.
/// The bytes are unchanged on refusal.
pub fn set_access_selection(
    world: &mut World,
    entity: rndr_core::Entity,
    snapshot: &MarkdownAccessSnapshot,
    anchor: usize,
    head: usize,
) -> AccessCommandResult {
    let Some(document) = root_document(world, entity) else {
        return AccessCommandResult::Rejected;
    };
    if document != snapshot.document {
        return AccessCommandResult::Rejected;
    }
    let Some(docs) = world.resource_mut::<crate::MarkdownDocuments>() else {
        return AccessCommandResult::Rejected;
    };
    let Some(session) = docs.get_mut(document) else {
        return AccessCommandResult::Rejected;
    };
    if session.revision() != snapshot.revision || session.is_composing() {
        return AccessCommandResult::Rejected;
    }
    if session.move_caret(anchor, head).is_err() {
        return AccessCommandResult::Rejected;
    }
    AccessCommandResult::Applied
}

/// Build overlay nodes for `root`. `bounds` maps a realized source span to a
/// window rect. Spans that are not listed get a zero rect.
pub fn build_overlay(
    bytes: &str,
    root: &Green,
    bounds: &[(Range<usize>, Rect)],
    clip: Rect,
) -> Vec<AccessNode> {
    let mut builder = Builder {
        nodes: Vec::new(),
        next: 0,
        bounds,
        clip,
        bytes,
    };
    let mut child_at = 0;
    let mut _children = Vec::new();
    for child in root.children() {
        if let Some(id) = builder.block(child, child_at) {
            _children.push(id);
        }
        child_at += child.width();
    }
    builder.nodes
}

struct Builder<'a> {
    nodes: Vec<AccessNode>,
    next: u64,
    bounds: &'a [(Range<usize>, Rect)],
    clip: Rect,
    bytes: &'a str,
}

impl Builder<'_> {
    fn ident(&mut self) -> u64 {
        self.next += 1;
        (1_u64 << 63) | self.next
    }

    fn push(
        &mut self,
        role: AccessRole,
        name: String,
        value: String,
        level: Option<usize>,
        span: Range<usize>,
        children: Vec<u64>,
    ) -> u64 {
        let id = self.ident();
        self.nodes.push(AccessNode {
            id,
            role,
            name,
            value,
            bounds: clipped(self.lookup(span), self.clip),
            children,
            focusable: false,
            focused: false,
            selected: false,
            enabled: true,
            expanded: None,
            level,
        });
        id
    }

    fn lookup(&self, span: Range<usize>) -> Rect {
        self.bounds
            .iter()
            .find(|(range, _)| range.start <= span.start && range.end >= span.end)
            .map(|(_, rect)| *rect)
            .unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            })
    }

    fn block(&mut self, node: &Green, at: usize) -> Option<u64> {
        let span = at..at + node.width();
        let text = plain(self.bytes, node, at);
        match node.kind() {
            SyntaxKind::BlockGap | SyntaxKind::Bom => None,
            SyntaxKind::Heading => {
                let level = heading_level(node);
                let links = self.inlines(node, at);
                Some(self.push(
                    AccessRole::Heading,
                    text,
                    String::new(),
                    Some(level as usize),
                    span,
                    links,
                ))
            }
            SyntaxKind::Paragraph => {
                let links = self.inlines(node, at);
                Some(self.push(
                    AccessRole::Paragraph,
                    String::new(),
                    text,
                    None,
                    span,
                    links,
                ))
            }
            SyntaxKind::List => {
                let mut child_at = at;
                let mut children = Vec::new();
                for child in node.children() {
                    if let Some(id) = self.block(child, child_at) {
                        children.push(id);
                    }
                    child_at += child.width();
                }
                Some(self.push(
                    AccessRole::List,
                    String::new(),
                    String::new(),
                    None,
                    span,
                    children,
                ))
            }
            SyntaxKind::ListItem => {
                let mut child_at = at;
                let mut children = Vec::new();
                for child in node.children() {
                    if child.kind() != SyntaxKind::ListMarker {
                        if let Some(id) = self.block(child, child_at) {
                            children.push(id);
                        }
                    }
                    child_at += child.width();
                }
                Some(self.push(
                    AccessRole::ListItem,
                    text,
                    String::new(),
                    None,
                    span,
                    children,
                ))
            }
            SyntaxKind::BlockQuote => {
                let mut child_at = at;
                let mut children = Vec::new();
                for child in node.children() {
                    if let Some(id) = self.block(child, child_at) {
                        children.push(id);
                    }
                    child_at += child.width();
                }
                Some(self.push(
                    AccessRole::Group,
                    "Block quote".into(),
                    String::new(),
                    None,
                    span,
                    children,
                ))
            }
            SyntaxKind::Table => Some(self.table(node, at)),
            SyntaxKind::FencedCode => Some(self.push(
                AccessRole::Code,
                fence_info_of(node),
                text,
                None,
                span,
                Vec::new(),
            )),
            SyntaxKind::LiquidFence => Some(self.liquid(node, at)),
            SyntaxKind::ThematicBreak => Some(self.push(
                AccessRole::Group,
                "Thematic break".into(),
                String::new(),
                None,
                span,
                Vec::new(),
            )),
            SyntaxKind::Image => Some(self.push(
                AccessRole::Image,
                text,
                String::new(),
                None,
                span,
                Vec::new(),
            )),
            _ => Some(self.push(
                AccessRole::Group,
                String::new(),
                text,
                None,
                span,
                Vec::new(),
            )),
        }
    }

    fn inlines(&mut self, node: &Green, at: usize) -> Vec<u64> {
        let mut child_at = at;
        let mut ids = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::Link {
                let label = plain(self.bytes, child, child_at);
                let dest = link_dest(child);
                ids.push(self.push(
                    AccessRole::Link,
                    label,
                    dest,
                    None,
                    child_at..child_at + child.width(),
                    Vec::new(),
                ));
            } else if child.kind() == SyntaxKind::Image {
                ids.push(self.push(
                    AccessRole::Image,
                    plain(self.bytes, child, child_at),
                    String::new(),
                    None,
                    child_at..child_at + child.width(),
                    Vec::new(),
                ));
            } else if !child.children().is_empty() {
                ids.extend(self.inlines(child, child_at));
            }
            child_at += child.width();
        }
        ids
    }

    fn table(&mut self, node: &Green, at: usize) -> u64 {
        let mut child_at = at;
        let mut rows = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::TableRow {
                let mut cell_at = child_at;
                let mut cells = Vec::new();
                for cell in child.children() {
                    if cell.kind() == SyntaxKind::TableCell {
                        cells.push(self.push(
                            AccessRole::Cell,
                            String::new(),
                            plain(self.bytes, cell, cell_at),
                            None,
                            cell_at..cell_at + cell.width(),
                            Vec::new(),
                        ));
                    }
                    cell_at += cell.width();
                }
                rows.push(self.push(
                    AccessRole::Row,
                    String::new(),
                    String::new(),
                    None,
                    child_at..child_at + child.width(),
                    cells,
                ));
            }
            child_at += child.width();
        }
        self.push(
            AccessRole::Table,
            String::new(),
            String::new(),
            None,
            at..at + node.width(),
            rows,
        )
    }

    fn liquid(&mut self, node: &Green, at: usize) -> u64 {
        let info = fence_info_of(node);
        let lowered = info.to_lowercase();
        let canonical = match lowered.as_str() {
            "action_row" => "actions",
            "chip_group" => "chips",
            "procedure" => "recipe",
            other => other,
        };
        let mut child_at = at;
        let mut fields = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::Field {
                let (key, value) = field_text(child);
                fields.push(self.push(
                    AccessRole::Label,
                    key,
                    value,
                    None,
                    child_at..child_at + child.width(),
                    Vec::new(),
                ));
            }
            child_at += child.width();
        }
        let name = if canonical.is_empty() {
            "Liquid".to_string()
        } else {
            canonical.to_string()
        };
        self.push(
            AccessRole::Group,
            name,
            String::new(),
            None,
            at..at + node.width(),
            fields,
        )
    }
}

fn clipped(rect: Rect, clip: Rect) -> Rect {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    }
    let x1 = rect.x.max(clip.x);
    let y1 = rect.y.max(clip.y);
    let x2 = (rect.x + rect.w).min(clip.x + clip.w);
    let y2 = (rect.y + rect.h).min(clip.y + clip.h);
    if x2 <= x1 || y2 <= y1 || clip.w <= 0.0 || clip.h <= 0.0 {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    } else {
        Rect {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }
}

fn plain(bytes: &str, node: &Green, at: usize) -> String {
    let end = at + node.width();
    bytes
        .get(at..end)
        .unwrap_or("")
        .trim_end_matches(['\n', '\r'])
        .trim()
        .to_string()
}

fn heading_level(node: &Green) -> u8 {
    let marker = node
        .children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::AtxMarker)
        .map(Green::text)
        .unwrap_or("");
    let hashes = marker.bytes().filter(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) {
        hashes as u8
    } else {
        1
    }
}

fn fence_info_of(node: &Green) -> String {
    node.children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::FenceLine)
        .map(|line| {
            let line = line.text().trim_end_matches(['\n', '\r']).trim();
            let line = line.trim_start_matches(['`', '~']);
            line.split_whitespace().next().unwrap_or("").to_string()
        })
        .unwrap_or_default()
}

fn field_text(node: &Green) -> (String, String) {
    let mut key = String::new();
    let mut value = String::new();
    for child in node.children() {
        if child.kind() == SyntaxKind::FieldKey {
            key = child.text().trim().to_string();
        }
        if child.kind() == SyntaxKind::FieldValue {
            value = child.text().to_string();
        }
    }
    (key, value)
}

fn link_dest(node: &Green) -> String {
    let mut after = false;
    let mut dest = String::new();
    for child in node.children() {
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" {
            after = true;
            continue;
        }
        if after {
            dest.push_str(child.text());
        }
    }
    dest.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']')
        .to_string()
}
