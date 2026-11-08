//! The editing state machine. It reads plain per-frame input in view
//! coordinates, previews gestures on a draft copy of the nodes, and saves
//! each finished gesture as one transaction through a [`CanvasStore`].
//!
//! Nothing here needs a window, so behavior can be driven and checked
//! headlessly; [`crate::view`] adapts `rndr_core` input to [`EditorInput`].

use rndr_canvas::geometry::set_line_endpoints;
use rndr_canvas::layout::SnappedMove;
use rndr_canvas::selection::{Endpoint, Handle, SelectionFrame};
use rndr_canvas::text::TextMeasure;
use rndr_canvas::{
    apply_operations, Brush, BrushKind, Camera, CanvasError, CanvasStore, Content, Guide,
    HistoryRequest, InkPoint, InputKind, Line, LinePoint, Mutation, Node, Operation, Rect,
    SceneIndex, ShapeKind, Snapping, Transaction, Transform, Vec2,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// The active tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Hand,
    Pen,
    Eraser,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Text,
    Frame,
}

impl Tool {
    pub const ALL: [Tool; 10] = [
        Tool::Select,
        Tool::Hand,
        Tool::Pen,
        Tool::Eraser,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Line,
        Tool::Arrow,
        Tool::Text,
        Tool::Frame,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Hand => "Hand",
            Tool::Pen => "Pen",
            Tool::Eraser => "Eraser",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line",
            Tool::Arrow => "Arrow",
            Tool::Text => "Text",
            Tool::Frame => "Frame",
        }
    }

    /// Single-letter shortcut.
    pub fn key(self) -> char {
        match self {
            Tool::Select => 'v',
            Tool::Hand => 'h',
            Tool::Pen => 'p',
            Tool::Eraser => 'e',
            Tool::Rectangle => 'r',
            Tool::Ellipse => 'o',
            Tool::Line => 'l',
            Tool::Arrow => 'a',
            Tool::Text => 't',
            Tool::Frame => 'f',
        }
    }
}

/// What made a pointer contact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PointerKind {
    #[default]
    Mouse,
    Touch,
    Pen,
    /// The eraser end of a stylus; erases whatever the tool.
    Eraser,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Button {
    #[default]
    Primary,
    Middle,
    Secondary,
}

/// One pointer sample in view coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    pub position: [f32; 2],
    pub pressure: Option<f32>,
    pub tilt: Option<[f32; 2]>,
    /// Milliseconds on any steady clock.
    pub time_ms: f64,
}

/// The primary pointer this frame, in view coordinates (relative to the
/// canvas surface's top-left).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointerInput {
    pub position: [f32; 2],
    pub pressed: bool,
    pub down: bool,
    pub released: bool,
    pub button: Button,
    pub kind: PointerKind,
    /// Samples since the previous frame, oldest first; may be empty.
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    /// Cmd on macOS, Ctrl elsewhere.
    pub primary: bool,
}

/// Keys the editor understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    /// A letter or symbol, lowercase, for shortcuts.
    Char(char),
}

/// Everything the editor reads in one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorInput {
    /// `None` when the pointer is outside the canvas and not captured.
    pub pointer: Option<PointerInput>,
    /// Scroll delta in view points. Positive Y moves content down.
    pub scroll: [f32; 2],
    pub modifiers: Modifiers,
    /// Key presses this frame with the modifiers held for each.
    pub keys: Vec<(Key, Modifiers)>,
    /// Typed text this frame, for text editing.
    pub text: String,
    /// Size of the canvas surface in view points.
    pub viewport: [f32; 2],
    /// Milliseconds on a steady clock, for double clicks.
    pub time_ms: f64,
}

/// Defaults for new nodes, and what the inspector edits.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub stroke: String,
    /// `transparent` for outlines only.
    pub fill: String,
    pub stroke_width: f64,
    pub ink_size: f64,
    pub font_family: String,
    pub font_size: f64,
    pub opacity: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke: "#1e1e1e".into(),
            fill: "transparent".into(),
            stroke_width: 2.0,
            ink_size: 4.0,
            font_family: "Inter".into(),
            font_size: 24.0,
            opacity: 1.0,
        }
    }
}

/// A change to the style of the selection (and of new nodes).
#[derive(Debug, Clone, PartialEq)]
pub enum StyleChange {
    Stroke(String),
    Fill(String),
    StrokeWidth(f64),
    FontSize(f64),
    Opacity(f64),
}

/// The part of the frame's interaction the overlay draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    pub marquee: Option<Rect>,
    /// Scene points.
    pub lasso: Vec<Vec2>,
    pub guides: Vec<Guide>,
    /// Scene points of the eraser path.
    pub eraser: Vec<Vec2>,
    pub hover: Option<String>,
    /// Frame to draw during a rotation, which follows the gesture.
    pub rotating: Option<SelectionFrame>,
}

/// What a handle under the pointer does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandleKind {
    Resize(Handle),
    Rotate,
    LineEnd(Endpoint),
}

/// Pointer shape the host should show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorHint {
    #[default]
    Default,
    Grab,
    Crosshair,
    Text,
    Move,
}

/// The result of one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Response {
    /// Something visible changed; paint again.
    pub repaint: bool,
    /// The editor wants pointer motion even without a button held.
    pub hover: bool,
    pub cursor: CursorHint,
}

#[derive(Debug, Clone)]
struct TextEdit {
    id: String,
    /// The saved node, or `None` for a node created by this edit.
    original: Option<Node>,
    node: Node,
}

#[derive(Debug, Clone)]
enum Gesture {
    Idle,
    Pan {
        from: [f32; 2],
        camera: Camera,
    },
    /// Pressed on a selected node; moves once past the drag threshold.
    Move {
        from: Vec2,
        bounds: Rect,
        targets: Vec<Rect>,
        moved: bool,
    },
    Handle {
        kind: HandleKind,
        frame: SelectionFrame,
        from: Vec2,
    },
    Marquee {
        from: Vec2,
        additive: Vec<String>,
    },
    Lasso,
    Draw {
        points: Vec<InkPoint>,
        input: InputKind,
        start_ms: f64,
    },
    Erase,
    Place {
        tool: Tool,
        from: Vec2,
        parent: Option<String>,
    },
}

/// Pixels a press may travel before it counts as a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// Handle size and pick tolerance, in view points.
pub const HANDLE_SIZE: f32 = 8.0;
/// Distance of the rotation handle above the frame, in view points.
pub const ROTATE_OFFSET: f32 = 24.0;

/// Edits one document in a [`CanvasStore`].
pub struct CanvasEditor {
    store: Arc<dyn CanvasStore + Send + Sync>,
    current: Mutation,
    draft: Option<Vec<Node>>,
    pub camera: Camera,
    tool: Tool,
    selection: Vec<String>,
    /// The group whose children clicks select, after double-clicking it.
    scope: Option<String>,
    pub style: Style,
    pub snapping: Snapping,
    gesture: Gesture,
    text: Option<TextEdit>,
    overlay: Overlay,
    last_error: Option<CanvasError>,
    seed: u64,
    counter: u64,
    last_click: Option<(f64, [f32; 2])>,
    erase_path: Vec<Vec2>,
}

fn now_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

fn v(p: [f32; 2]) -> Vec2 {
    Vec2::new(f64::from(p[0]), f64::from(p[1]))
}

impl CanvasEditor {
    /// Opens document `id` in `store`.
    pub fn open(store: Arc<dyn CanvasStore + Send + Sync>, id: &str) -> Result<Self, CanvasError> {
        let current = store.get(id)?;
        Ok(Self {
            store,
            current,
            draft: None,
            camera: Camera::default(),
            tool: Tool::Select,
            selection: Vec::new(),
            scope: None,
            style: Style::default(),
            snapping: Snapping::default(),
            gesture: Gesture::Idle,
            text: None,
            overlay: Overlay::default(),
            last_error: None,
            seed: now_seed() & 0xffff_ffff,
            counter: 0,
            last_click: None,
            erase_path: Vec::new(),
        })
    }

    /// The saved document and what history allows.
    pub fn mutation(&self) -> &Mutation {
        &self.current
    }

    /// Nodes to paint: the gesture preview while one is in progress, with
    /// the text being edited.
    pub fn nodes(&self) -> Vec<Node> {
        let mut nodes = self
            .draft
            .clone()
            .unwrap_or_else(|| self.current.document.scene.nodes.clone());
        if let Some(edit) = &self.text {
            match nodes.iter_mut().find(|n| n.id == edit.id) {
                Some(slot) => *slot = edit.node.clone(),
                None => nodes.push(edit.node.clone()),
            }
        }
        nodes
    }

    /// The scene to paint, with [`CanvasEditor::nodes`].
    pub fn scene(&self) -> rndr_canvas::Scene {
        let mut scene = self.current.document.scene.clone();
        scene.nodes = self.nodes();
        scene
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn selection(&self) -> &[String] {
        &self.selection
    }

    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// Id of the text node being typed into.
    pub fn editing_text(&self) -> Option<&str> {
        self.text.as_ref().map(|t| t.id.as_str())
    }

    /// The last refused save, cleared by the next successful one.
    pub fn last_error(&self) -> Option<&CanvasError> {
        self.last_error.as_ref()
    }

    /// A fresh id for a node created in this editor.
    pub fn new_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}_{:08x}{:04x}", self.seed, self.counter)
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.finish_text();
        self.cancel_gesture();
        self.tool = tool;
    }

    /// Replaces the selection with editable nodes from `ids`.
    pub fn select(&mut self, ids: &[String]) {
        let nodes = &self.current.document.scene.nodes;
        let index = SceneIndex::new(nodes);
        self.selection = ids
            .iter()
            .filter(|id| index.get(id).is_some_and(|n| index.editable(n)))
            .cloned()
            .collect();
    }

    /// Re-reads the document from the store, dropping any preview.
    pub fn reload(&mut self) -> Result<(), CanvasError> {
        self.current = self.store.get(&self.current.document.id)?;
        self.draft = None;
        self.prune_selection();
        Ok(())
    }

    fn prune_selection(&mut self) {
        let ids: BTreeSet<&str> = self
            .current
            .document
            .scene
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        self.selection.retain(|id| ids.contains(id.as_str()));
        if self.scope.as_deref().is_some_and(|s| !ids.contains(s)) {
            self.scope = None;
        }
    }

    /// Saves `operations` as one undo step. Empty edits are skipped. A
    /// stale revision reloads the document; any refusal is kept in
    /// [`CanvasEditor::last_error`].
    pub fn commit(&mut self, label: &str, operations: Vec<Operation>) -> bool {
        self.draft = None;
        if operations.is_empty() {
            return false;
        }
        let transaction = Transaction {
            id: self.new_id("tx"),
            expected_revision: self.current.document.revision,
            label: label.into(),
            operations,
        };
        let result = self.store.transact(&self.current.document.id, transaction);
        self.settle(result)
    }

    fn settle(&mut self, result: Result<Mutation, CanvasError>) -> bool {
        match result {
            Ok(mutation) => {
                self.current = mutation;
                self.last_error = None;
                self.prune_selection();
                true
            }
            Err(error) => {
                if matches!(error, CanvasError::Conflict(_)) {
                    let _ = self.reload();
                }
                self.last_error = Some(error);
                false
            }
        }
    }

    fn history(&mut self, redo: bool) -> bool {
        self.finish_text();
        self.cancel_gesture();
        if (redo && !self.current.can_redo) || (!redo && !self.current.can_undo) {
            return false;
        }
        let request = HistoryRequest {
            id: self.new_id("h"),
            expected_revision: self.current.document.revision,
            redo,
        };
        let result = self.store.history(&self.current.document.id, request);
        self.settle(result)
    }

    pub fn undo(&mut self) -> bool {
        self.history(false)
    }

    pub fn redo(&mut self) -> bool {
        self.history(true)
    }

    fn index_nodes(&self) -> &[Node] {
        &self.current.document.scene.nodes
    }

    /// Saves the difference between the document and `after` (a full node
    /// list in the same order) as one step.
    fn commit_nodes(&mut self, label: &str, after: Vec<Node>) -> bool {
        let before: BTreeMap<&str, &Node> = self
            .index_nodes()
            .iter()
            .map(|n| (n.id.as_str(), n))
            .collect();
        let operations = after
            .iter()
            .filter(|n| before.get(n.id.as_str()) != Some(n))
            .map(|n| Operation::put(n.clone()))
            .collect();
        self.commit(label, operations)
    }

    pub fn delete_selection(&mut self) -> bool {
        let index = SceneIndex::new(self.index_nodes());
        let operations: Vec<_> = index
            .selection_roots(&self.selection)
            .iter()
            .map(|n| Operation::remove(n.id.clone()))
            .collect();
        let done = self.commit("Delete", operations);
        if done {
            self.selection.clear();
        }
        done
    }

    /// Copies the selection, with descendants, 16 units down and right.
    pub fn duplicate(&mut self) -> bool {
        let nodes = self.index_nodes().to_vec();
        let index = SceneIndex::new(&nodes);
        let roots: Vec<String> = index
            .selection_roots(&self.selection)
            .iter()
            .map(|n| n.id.clone())
            .collect();
        let copied = rndr_canvas::geometry::descendants(&nodes, roots.iter().map(String::as_str));
        let mut renamed = BTreeMap::new();
        for node in nodes.iter().filter(|n| copied.contains(&n.id)) {
            renamed.insert(node.id.clone(), self.new_id("n"));
        }
        let mut operations = Vec::new();
        let mut selected = Vec::new();
        for node in nodes.iter().filter(|n| copied.contains(&n.id)) {
            let mut copy = node.clone();
            copy.id = renamed[&node.id].clone();
            copy.locked = false;
            if let Some(parent) = &node.parent_id {
                if let Some(new_parent) = renamed.get(parent) {
                    copy.parent_id = Some(new_parent.clone());
                }
            }
            if roots.contains(&node.id) {
                let moved = index.translate(std::slice::from_ref(&node.id), 16.0, 16.0);
                if let Some(Operation::PutNode { node: moved }) = moved.first() {
                    copy.transform = moved.transform;
                }
                selected.push(copy.id.clone());
            }
            operations.push(Operation::put(copy));
        }
        let done = self.commit("Duplicate", operations);
        if done {
            self.selection = selected;
        }
        done
    }

    pub fn group(&mut self) -> bool {
        let id = self.new_id("g");
        let index = SceneIndex::new(self.index_nodes());
        match index.group(&self.selection, &id) {
            Ok(edit) => {
                let done = self.commit("Group", edit.operations);
                if done {
                    self.selection = edit.selected;
                }
                done
            }
            Err(error) => {
                self.last_error = Some(error);
                false
            }
        }
    }

    pub fn ungroup(&mut self) -> bool {
        let index = SceneIndex::new(self.index_nodes());
        match index.ungroup(&self.selection) {
            Ok(edit) => {
                let done = self.commit("Ungroup", edit.operations);
                if done {
                    self.selection = edit.selected;
                }
                done
            }
            Err(error) => {
                self.last_error = Some(error);
                false
            }
        }
    }

    /// Moves the selection one step forward or backward in paint order.
    pub fn arrange(&mut self, forward: bool) -> bool {
        let ops = SceneIndex::new(self.index_nodes()).arrange(&self.selection, forward);
        self.commit(
            if forward {
                "Bring forward"
            } else {
                "Send backward"
            },
            ops,
        )
    }

    pub fn align(&mut self, alignment: rndr_canvas::Alignment) -> bool {
        let ops = SceneIndex::new(self.index_nodes()).align(&self.selection, alignment);
        self.commit("Align", ops)
    }

    pub fn nudge(&mut self, dx: f64, dy: f64) -> bool {
        let ops = SceneIndex::new(self.index_nodes()).translate(&self.selection, dx, dy);
        self.commit("Nudge", ops)
    }

    /// Shows or hides a node.
    pub fn toggle_visible(&mut self, id: &str) -> bool {
        let Some(mut node) = SceneIndex::new(self.index_nodes()).get(id).cloned() else {
            return false;
        };
        node.visible = !node.visible;
        let label = if node.visible { "Show" } else { "Hide" };
        self.commit(label, vec![Operation::put(node)])
    }

    /// Locks or unlocks a node.
    pub fn toggle_locked(&mut self, id: &str) -> bool {
        let Some(mut node) = SceneIndex::new(self.index_nodes()).get(id).cloned() else {
            return false;
        };
        node.locked = !node.locked;
        let label = if node.locked { "Lock" } else { "Unlock" };
        self.commit(label, vec![Operation::put(node)])
    }

    /// Updates [`CanvasEditor::style`] and applies the change to the
    /// selection's roots and their contents.
    pub fn apply_style(&mut self, change: StyleChange) -> bool {
        match &change {
            StyleChange::Stroke(c) => self.style.stroke = c.clone(),
            StyleChange::Fill(c) => self.style.fill = c.clone(),
            StyleChange::StrokeWidth(w) => {
                self.style.stroke_width = *w;
                self.style.ink_size = *w * 2.0;
            }
            StyleChange::FontSize(s) => self.style.font_size = *s,
            StyleChange::Opacity(o) => self.style.opacity = *o,
        }
        let nodes = self.index_nodes().to_vec();
        let index = SceneIndex::new(&nodes);
        let roots: Vec<&str> = index
            .selection_roots(&self.selection)
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        let affected = rndr_canvas::geometry::descendants(&nodes, roots.iter().copied());
        let mut operations = Vec::new();
        for node in nodes.iter().filter(|n| affected.contains(&n.id)) {
            let mut next = node.clone();
            let is_root = roots.contains(&node.id.as_str());
            match (&change, &mut next.content) {
                (StyleChange::Opacity(o), _) if is_root => next.opacity = o.clamp(0.0, 1.0),
                (StyleChange::Stroke(c), Content::Shape { stroke, .. }) => *stroke = c.clone(),
                (StyleChange::Stroke(c), Content::Ink { color, .. }) => *color = c.clone(),
                (StyleChange::Stroke(c), Content::Text { color, .. }) => *color = c.clone(),
                (StyleChange::Fill(c), Content::Shape { shape, fill, .. })
                    if *shape != ShapeKind::Line =>
                {
                    *fill = c.clone()
                }
                (StyleChange::Fill(c), Content::Frame { background, .. }) => {
                    *background = c.clone()
                }
                (StyleChange::StrokeWidth(w), Content::Shape { stroke_width, .. }) => {
                    *stroke_width = w.clamp(0.0, 1000.0)
                }
                (StyleChange::StrokeWidth(w), Content::Ink { brush, .. }) => {
                    brush.size = (w * 2.0).clamp(0.25, 160.0)
                }
                (StyleChange::FontSize(s), Content::Text { font_size, .. }) => {
                    *font_size = s.clamp(1.0, 4096.0)
                }
                _ => {}
            }
            if next != *node {
                operations.push(Operation::put(next));
            }
        }
        self.commit("Style", operations)
    }

    fn cancel_gesture(&mut self) {
        self.gesture = Gesture::Idle;
        self.draft = None;
        self.overlay = Overlay {
            hover: self.overlay.hover.take(),
            ..Overlay::default()
        };
        self.erase_path.clear();
    }

    /// Saves the text being edited. Empty text removes a node created by the
    /// edit, or deletes an existing one.
    pub fn finish_text(&mut self) -> bool {
        let Some(edit) = self.text.take() else {
            return false;
        };
        let empty =
            matches!(&edit.node.content, Content::Text { text, .. } if text.trim().is_empty());
        match (empty, &edit.original) {
            (true, None) => false,
            (true, Some(_)) => self.commit("Delete text", vec![Operation::remove(edit.id)]),
            (false, Some(original)) if *original == edit.node => false,
            (false, original) => {
                let label = if original.is_some() {
                    "Edit text"
                } else {
                    "Add text"
                };
                let id = edit.id.clone();
                let done = self.commit(label, vec![Operation::put(edit.node)]);
                if done {
                    self.selection = vec![id];
                }
                done
            }
        }
    }

    /// Starts typing into an existing text node.
    pub fn edit_text(&mut self, id: &str) -> bool {
        self.finish_text();
        let Some(node) = SceneIndex::new(self.index_nodes()).get(id).cloned() else {
            return false;
        };
        if !matches!(node.content, Content::Text { .. }) {
            return false;
        }
        self.text = Some(TextEdit {
            id: node.id.clone(),
            original: Some(node.clone()),
            node,
        });
        true
    }

    fn type_text(&mut self, typed: &str, keys: &[(Key, Modifiers)]) -> bool {
        let Some(edit) = &mut self.text else {
            return false;
        };
        let Content::Text { text, .. } = &mut edit.node.content else {
            return false;
        };
        let mut changed = false;
        for (key, mods) in keys {
            match key {
                Key::Backspace => {
                    changed |= text.pop().is_some();
                }
                Key::Enter if !mods.primary => {
                    text.push('\n');
                    changed = true;
                }
                _ => {}
            }
        }
        let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
        if !typed.is_empty() {
            text.push_str(&typed);
            changed = true;
        }
        if changed {
            // Spans may no longer line up with the text.
            if let Some(layout) = &mut edit.node.text_layout {
                let length = text.encode_utf16().count() as u32;
                layout.spans.retain(|s| s.end <= length);
            }
        }
        changed
    }

    /// Runs one frame. `measure` must match the painter's.
    pub fn handle(&mut self, input: &EditorInput, measure: &dyn TextMeasure) -> Response {
        let mut response = Response {
            hover: true,
            ..Response::default()
        };
        response.repaint |= self.keys(input);
        if self.text.is_some() {
            response.repaint |= self.type_text(&input.text, &input.keys);
        }
        response.repaint |= self.wheel(input);
        if let Some(pointer) = &input.pointer {
            response.repaint |= self.pointer(pointer, input, measure);
            response.cursor = self.cursor(pointer, measure);
        } else if self.overlay.hover.take().is_some() {
            response.repaint = true;
        }
        response
    }

    fn keys(&mut self, input: &EditorInput) -> bool {
        let mut changed = false;
        for &(key, mods) in &input.keys {
            if self.text.is_some() {
                if key == Key::Escape || (key == Key::Enter && mods.primary) {
                    changed |= self.finish_text() || true;
                }
                continue;
            }
            changed |= match (key, mods.primary, mods.shift) {
                (Key::Char('z'), true, false) => self.undo(),
                (Key::Char('z'), true, true) | (Key::Char('y'), true, _) => self.redo(),
                (Key::Char('d'), true, _) => self.duplicate(),
                (Key::Char('g'), true, false) => self.group(),
                (Key::Char('g'), true, true) => self.ungroup(),
                (Key::Char('a'), true, _) => {
                    let index = SceneIndex::new(self.index_nodes());
                    let ids: Vec<String> = index
                        .children(self.scope.as_deref())
                        .iter()
                        .filter(|n| index.editable(n))
                        .map(|n| n.id.clone())
                        .collect();
                    self.selection = ids;
                    true
                }
                (Key::Delete | Key::Backspace, _, _) => self.delete_selection(),
                (Key::Char(']'), _, _) => self.arrange(true),
                (Key::Char('['), _, _) => self.arrange(false),
                (Key::Left | Key::Right | Key::Up | Key::Down, false, shift) => {
                    let step = if shift { 10.0 } else { 1.0 };
                    let (dx, dy) = match key {
                        Key::Left => (-step, 0.0),
                        Key::Right => (step, 0.0),
                        Key::Up => (0.0, -step),
                        _ => (0.0, step),
                    };
                    self.nudge(dx, dy)
                }
                (Key::Escape, _, _) => {
                    if !matches!(self.gesture, Gesture::Idle) {
                        self.cancel_gesture();
                    } else if !self.selection.is_empty() {
                        self.selection.clear();
                    } else if self.scope.take().is_none() {
                        self.tool = Tool::Select;
                    }
                    true
                }
                (Key::Char(c), false, false) => match Tool::ALL.iter().find(|t| t.key() == c) {
                    Some(tool) => {
                        self.set_tool(*tool);
                        true
                    }
                    None => false,
                },
                _ => false,
            };
        }
        changed
    }

    fn wheel(&mut self, input: &EditorInput) -> bool {
        let [sx, sy] = input.scroll;
        if sx == 0.0 && sy == 0.0 {
            return false;
        }
        if input.modifiers.primary {
            let anchor = input.pointer.as_ref().map_or(
                Vec2::new(
                    f64::from(input.viewport[0]) / 2.0,
                    f64::from(input.viewport[1]) / 2.0,
                ),
                |p| v(p.position),
            );
            let factor = (f64::from(sy) * 0.01).exp();
            self.camera = self.camera.zoom_at(anchor, self.camera.zoom * factor);
        } else {
            self.camera.pan_x += f64::from(sx);
            self.camera.pan_y += f64::from(sy);
        }
        true
    }

    /// Zooms by `factor` about the view center.
    pub fn zoom_by(&mut self, factor: f64, viewport: [f32; 2]) {
        let center = Vec2::new(f64::from(viewport[0]) / 2.0, f64::from(viewport[1]) / 2.0);
        self.camera = self.camera.zoom_at(center, self.camera.zoom * factor);
    }

    /// Fits the visible scene in the view.
    pub fn zoom_to_fit(&mut self, viewport: [f32; 2], measure: &dyn TextMeasure) {
        let nodes = self.nodes();
        let index = SceneIndex::with_measure(&nodes, measure);
        let (w, h) = (f64::from(viewport[0]), f64::from(viewport[1]));
        self.camera = Camera::fit(index.scene_bounds(), w, h, (w * 0.1).min(56.0));
    }

    fn scene_point(&self, view: [f32; 2]) -> Vec2 {
        self.camera.view_to_scene(v(view))
    }

    /// The handle of the current selection under a view point.
    pub fn handle_at(&self, view: [f32; 2], measure: &dyn TextMeasure) -> Option<HandleKind> {
        if self.tool != Tool::Select || self.selection.is_empty() {
            return None;
        }
        let nodes = self.index_nodes();
        let index = SceneIndex::with_measure(nodes, measure);
        let frame = index.selection_frame(&self.selection)?;
        let reach = f64::from(HANDLE_SIZE);
        let p = v(view);
        let near = |scene: Vec2| self.camera.scene_to_view(scene).distance(p) <= reach;
        let roots = index.selection_roots(&self.selection);
        if let [node] = roots.as_slice() {
            if let Some(line) = rndr_canvas::LineGeometry::of(node) {
                let m = index.world_matrix(node);
                for (end, at) in [(Endpoint::Start, line.start), (Endpoint::End, line.end)] {
                    if near(m.apply(at)) {
                        return Some(HandleKind::LineEnd(end));
                    }
                }
                return None;
            }
        }
        for (handle, point) in handle_points(&frame) {
            if near(point) {
                return Some(HandleKind::Resize(handle));
            }
        }
        if near(rotate_point(&frame, self.camera.zoom)) {
            return Some(HandleKind::Rotate);
        }
        None
    }

    fn cursor(&self, pointer: &PointerInput, measure: &dyn TextMeasure) -> CursorHint {
        match self.tool {
            Tool::Hand => CursorHint::Grab,
            Tool::Text => CursorHint::Text,
            Tool::Select => match (&self.gesture, self.handle_at(pointer.position, measure)) {
                (Gesture::Pan { .. }, _) => CursorHint::Grab,
                (Gesture::Move { moved: true, .. }, _) | (_, Some(_)) => CursorHint::Move,
                _ => CursorHint::Default,
            },
            _ => CursorHint::Crosshair,
        }
    }

    /// The innermost editable frame under a scene point, for parenting new
    /// nodes.
    fn frame_at(&self, point: Vec2, measure: &dyn TextMeasure) -> Option<String> {
        let nodes = self.index_nodes();
        let index = SceneIndex::with_measure(nodes, measure);
        index
            .layer_rows(|_| false)
            .into_iter()
            .map(|row| row.node)
            .filter(|n| matches!(n.content, Content::Frame { .. }) && index.editable(n))
            .find(|n| {
                let local = index.world_matrix(n).inverse().apply(point);
                Rect::new(0.0, 0.0, n.transform.width, n.transform.height).contains(local)
            })
            .map(|n| n.id.clone())
    }

    fn pointer(
        &mut self,
        pointer: &PointerInput,
        input: &EditorInput,
        measure: &dyn TextMeasure,
    ) -> bool {
        let mut changed = false;
        if pointer.pressed {
            changed |= self.press(pointer, input, measure);
        }
        if pointer.down && !pointer.pressed {
            changed |= self.drag(pointer, input, measure);
        } else if pointer.down && pointer.pressed {
            // A press may arrive with motion samples already.
            changed |= self.drag(pointer, input, measure);
        }
        if pointer.released {
            changed |= self.drag(pointer, input, measure);
            changed |= self.release(pointer, input, measure);
        }
        if !pointer.down && !pointer.released && matches!(self.gesture, Gesture::Idle) {
            let hover = if self.tool == Tool::Select {
                let point = self.scene_point(pointer.position);
                let tolerance = f64::from(HANDLE_SIZE / 2.0) / self.camera.zoom;
                let nodes = self.index_nodes();
                let index = SceneIndex::with_measure(nodes, measure);
                index
                    .hit(point, tolerance)
                    .map(|n| index.selection_target(&n.id, self.scope.as_deref(), &self.selection))
            } else {
                None
            };
            if hover != self.overlay.hover {
                self.overlay.hover = hover;
                changed = true;
            }
        }
        changed
    }

    fn press(
        &mut self,
        pointer: &PointerInput,
        input: &EditorInput,
        measure: &dyn TextMeasure,
    ) -> bool {
        let point = self.scene_point(pointer.position);
        let mods = input.modifiers;
        let double = self.last_click.is_some_and(|(t, at)| {
            input.time_ms - t < 400.0
                && (at[0] - pointer.position[0]).hypot(at[1] - pointer.position[1]) < 6.0
        });
        self.last_click = Some((input.time_ms, pointer.position));
        if self.text.is_some() {
            let inside = self.text.as_ref().is_some_and(|edit| {
                let nodes = self.nodes();
                let index = SceneIndex::with_measure(&nodes, measure);
                index
                    .get(&edit.id)
                    .is_some_and(|n| index.bounds(n).contains(point))
            });
            if inside {
                return false;
            }
            self.finish_text();
        }
        if pointer.button == Button::Middle || self.tool == Tool::Hand {
            self.gesture = Gesture::Pan {
                from: pointer.position,
                camera: self.camera,
            };
            return false;
        }
        if pointer.button == Button::Secondary {
            return false;
        }
        if pointer.kind == PointerKind::Eraser || self.tool == Tool::Eraser {
            self.gesture = Gesture::Erase;
            self.erase_path = vec![point];
            return true;
        }
        match self.tool {
            Tool::Select => self.press_select(point, pointer, mods, double, measure),
            Tool::Pen => {
                self.gesture = Gesture::Draw {
                    points: Vec::new(),
                    input: match pointer.kind {
                        PointerKind::Pen => InputKind::Pen,
                        PointerKind::Touch => InputKind::Touch,
                        PointerKind::Mouse => InputKind::Mouse,
                        PointerKind::Eraser => InputKind::Unknown,
                    },
                    start_ms: pointer.samples.first().map_or(input.time_ms, |s| s.time_ms),
                };
                self.selection.clear();
                true
            }
            Tool::Text => {
                let id = self.new_id("t");
                let parent = self.frame_at(point, measure);
                let nodes = self.index_nodes();
                let index = SceneIndex::with_measure(nodes, measure);
                let local = match parent.as_deref().and_then(|p| index.get(p)) {
                    Some(frame) => index.world_matrix(frame).inverse().apply(point),
                    None => point,
                };
                let mut node = Node::new(
                    id.clone(),
                    Content::Text {
                        text: String::new(),
                        color: self.style.stroke.clone(),
                        font_family: self.style.font_family.clone(),
                        font_size: self.style.font_size,
                        font_weight: None,
                        font_style: None,
                    },
                    Transform {
                        x: local.x,
                        y: local.y - self.style.font_size * 0.65,
                        width: 1.0,
                        height: self.style.font_size * 1.3,
                        rotation: 0.0,
                    },
                );
                node.parent_id = parent;
                node.opacity = self.style.opacity;
                self.text = Some(TextEdit {
                    id,
                    original: None,
                    node,
                });
                self.selection.clear();
                true
            }
            tool => {
                self.gesture = Gesture::Place {
                    tool,
                    from: point,
                    parent: if tool == Tool::Frame {
                        None
                    } else {
                        self.frame_at(point, measure)
                    },
                };
                self.selection.clear();
                true
            }
        }
    }

    fn press_select(
        &mut self,
        point: Vec2,
        pointer: &PointerInput,
        mods: Modifiers,
        double: bool,
        measure: &dyn TextMeasure,
    ) -> bool {
        if let Some(kind) = self.handle_at(pointer.position, measure) {
            let index = SceneIndex::with_measure(self.index_nodes(), measure);
            if let Some(frame) = index.selection_frame(&self.selection) {
                self.gesture = Gesture::Handle {
                    kind,
                    frame,
                    from: point,
                };
                return false;
            }
        }
        let tolerance = f64::from(HANDLE_SIZE / 2.0) / self.camera.zoom;
        let nodes = self.index_nodes().to_vec();
        let index = SceneIndex::with_measure(&nodes, measure);
        let Some(hit) = index.hit(point, tolerance) else {
            if !mods.shift {
                self.selection.clear();
            }
            if double {
                self.scope = None;
            }
            self.gesture = if mods.alt {
                Gesture::Lasso
            } else {
                Gesture::Marquee {
                    from: point,
                    additive: if mods.shift {
                        self.selection.clone()
                    } else {
                        Vec::new()
                    },
                }
            };
            if mods.alt {
                self.overlay.lasso = vec![point];
            }
            return true;
        };
        let target = index.selection_target(&hit.id, self.scope.as_deref(), &self.selection);
        if double {
            let picked = index.get(&target);
            if picked.is_some_and(|n| matches!(n.content, Content::Group)) {
                self.scope = Some(target.clone());
                let inner = index.selection_target(&hit.id, self.scope.as_deref(), &[]);
                self.selection = vec![inner];
                return true;
            }
            if picked.is_some_and(|n| matches!(n.content, Content::Text { .. })) {
                self.selection = vec![target.clone()];
                return self.edit_text(&target);
            }
        }
        if mods.shift {
            if let Some(i) = self.selection.iter().position(|id| *id == target) {
                self.selection.remove(i);
                return true;
            }
            self.selection.push(target);
        } else if !self.selection.contains(&target) {
            self.selection = vec![target];
        }
        let bounds = index
            .selection_object_bounds(&self.selection)
            .unwrap_or_default();
        self.gesture = Gesture::Move {
            from: point,
            bounds,
            targets: index.snap_targets(&self.selection),
            moved: false,
        };
        true
    }

    fn drag(
        &mut self,
        pointer: &PointerInput,
        input: &EditorInput,
        measure: &dyn TextMeasure,
    ) -> bool {
        let point = self.scene_point(pointer.position);
        let mods = input.modifiers;
        match self.gesture.clone() {
            Gesture::Idle => false,
            Gesture::Pan { from, camera } => {
                self.camera = Camera {
                    pan_x: camera.pan_x + f64::from(pointer.position[0] - from[0]),
                    pan_y: camera.pan_y + f64::from(pointer.position[1] - from[1]),
                    ..camera
                };
                true
            }
            Gesture::Move {
                from,
                bounds,
                targets,
                moved,
            } => {
                let start = self.camera.scene_to_view(from);
                let travelled = (start.x as f32 - pointer.position[0])
                    .hypot(start.y as f32 - pointer.position[1]);
                if !moved && travelled < DRAG_THRESHOLD {
                    return false;
                }
                let delta = Vec2::new(point.x - from.x, point.y - from.y);
                let SnappedMove { delta, guides } =
                    self.snapping
                        .snap_move(bounds, &targets, delta, self.camera.zoom, mods.alt);
                let index = SceneIndex::with_measure(self.index_nodes(), measure);
                let ops = index.translate(&self.selection, delta.x, delta.y);
                self.draft = apply_operations(&self.current.document, &ops)
                    .ok()
                    .map(|d| d.scene.nodes);
                self.overlay.guides = guides;
                self.gesture = Gesture::Move {
                    from,
                    bounds,
                    targets,
                    moved: true,
                };
                true
            }
            Gesture::Handle { kind, frame, from } => {
                let saved = self.index_nodes().to_vec();
                let index = SceneIndex::with_measure(&saved, measure);
                let nodes = match kind {
                    HandleKind::Resize(handle) => {
                        index.resize_from_handle(&self.selection, handle, point, mods.shift)
                    }
                    HandleKind::Rotate => {
                        let angle = |p: Vec2| (p.y - frame.center.y).atan2(p.x - frame.center.x);
                        let mut degrees = (angle(point) - angle(from)).to_degrees();
                        if mods.shift {
                            let target = ((frame.angle + degrees) / 15.0).round() * 15.0;
                            degrees = target - frame.angle;
                        }
                        self.overlay.rotating = Some(frame.rotated(degrees));
                        index.rotate(&self.selection, frame.center, degrees)
                    }
                    HandleKind::LineEnd(end) => {
                        let Some(node) = self.selection.first().and_then(|id| index.get(id)) else {
                            return false;
                        };
                        let moved = index.move_line_endpoint(node, end, point, mods.shift);
                        index
                            .nodes()
                            .iter()
                            .map(|n| {
                                if n.id == moved.id {
                                    moved.clone()
                                } else {
                                    n.clone()
                                }
                            })
                            .collect()
                    }
                };
                self.draft = Some(nodes);
                true
            }
            Gesture::Marquee { from, additive } => {
                let rect = Rect::around([from, point]).unwrap_or_default();
                self.overlay.marquee = Some(rect);
                let index = SceneIndex::with_measure(self.index_nodes(), measure);
                self.selection = index.marquee(rect, &additive);
                true
            }
            Gesture::Lasso => {
                self.overlay.lasso.push(point);
                true
            }
            Gesture::Draw {
                mut points,
                input: kind,
                start_ms,
            } => {
                let samples: Vec<Sample> = if pointer.samples.is_empty() {
                    vec![Sample {
                        position: pointer.position,
                        time_ms: input.time_ms,
                        ..Sample::default()
                    }]
                } else {
                    pointer.samples.clone()
                };
                for s in samples {
                    let at = self.scene_point(s.position);
                    if points
                        .last()
                        .is_some_and(|p| Vec2::new(p.x, p.y).distance(at) < 0.5 / self.camera.zoom)
                    {
                        continue;
                    }
                    points.push(InkPoint {
                        x: at.x,
                        y: at.y,
                        pressure: s.pressure.map(|p| f64::from(p).clamp(0.0, 1.0)),
                        elapsed_ms: Some((s.time_ms - start_ms).clamp(0.0, 86_400_000.0)),
                        tilt_x: s.tilt.map(|t| f64::from(t[0]).clamp(-90.0, 90.0)),
                        tilt_y: s.tilt.map(|t| f64::from(t[1]).clamp(-90.0, 90.0)),
                    });
                }
                let preview = self.ink_node("preview", &points, kind);
                let mut nodes = self.index_nodes().to_vec();
                nodes.extend(preview);
                self.draft = Some(nodes);
                self.gesture = Gesture::Draw {
                    points,
                    input: kind,
                    start_ms,
                };
                true
            }
            Gesture::Erase => {
                if self.erase_path.last() != Some(&point) {
                    self.erase_path.push(point);
                }
                self.overlay.eraser = self.erase_path.clone();
                let ops = self.erase_ops(measure);
                self.draft = if ops.is_empty() {
                    None
                } else {
                    apply_operations(&self.current.document, &ops)
                        .ok()
                        .map(|d| d.scene.nodes)
                };
                true
            }
            Gesture::Place { tool, from, parent } => {
                if let Some(node) =
                    self.placed(tool, from, point, parent.as_deref(), mods.shift, measure)
                {
                    let mut nodes = self.index_nodes().to_vec();
                    nodes.push(node);
                    self.draft = Some(nodes);
                }
                true
            }
        }
    }

    fn erase_ops(&mut self, measure: &dyn TextMeasure) -> Vec<Operation> {
        let radius = (self.style.ink_size / 2.0).max(4.0 / self.camera.zoom);
        let path = self.erase_path.clone();
        let nodes = self.current.document.scene.nodes.clone();
        let index = SceneIndex::with_measure(&nodes, measure);
        let mut ops = index.erase_ink(&path, radius, || {
            self.counter += 1;
            format!("n_{:08x}{:04x}", self.seed, self.counter)
        });
        let inked: BTreeSet<String> = ops
            .iter()
            .filter_map(|op| match op {
                Operation::PutNode { node } => Some(node.id.clone()),
                Operation::RemoveNode { id } => Some(id.clone()),
                _ => None,
            })
            .collect();
        ops.extend(
            index
                .erase_nodes(&path, radius)
                .into_iter()
                .filter(|op| !matches!(op, Operation::RemoveNode { id } if inked.contains(id)))
                .filter(|op| match op {
                    Operation::RemoveNode { id } => index
                        .get(id)
                        .is_some_and(|n| !matches!(n.content, Content::Ink { .. })),
                    _ => true,
                }),
        );
        ops
    }

    fn release(
        &mut self,
        pointer: &PointerInput,
        input: &EditorInput,
        measure: &dyn TextMeasure,
    ) -> bool {
        let point = self.scene_point(pointer.position);
        let gesture = std::mem::replace(&mut self.gesture, Gesture::Idle);
        let changed = match gesture {
            Gesture::Idle | Gesture::Pan { .. } => false,
            Gesture::Move { moved, .. } => {
                let draft = self.draft.take();
                moved && draft.is_some_and(|nodes| self.commit_nodes("Move", nodes))
            }
            Gesture::Handle { kind, .. } => {
                let label = match kind {
                    HandleKind::Resize(_) => "Resize",
                    HandleKind::Rotate => "Rotate",
                    HandleKind::LineEnd(_) => "Move endpoint",
                };
                let draft = self.draft.take();
                draft.is_some_and(|nodes| self.commit_nodes(label, nodes))
            }
            Gesture::Marquee { .. } => true,
            Gesture::Lasso => {
                let index = SceneIndex::with_measure(self.index_nodes(), measure);
                let mut picked = index.lasso(&self.overlay.lasso);
                if input.modifiers.shift {
                    picked.extend(self.selection.iter().cloned());
                }
                self.selection = index
                    .selection_roots(&picked)
                    .iter()
                    .map(|n| n.id.clone())
                    .collect();
                true
            }
            Gesture::Draw {
                points,
                input: kind,
                ..
            } => {
                let id = self.new_id("n");
                let parent = points
                    .first()
                    .and_then(|p| self.frame_at(Vec2::new(p.x, p.y), measure));
                match self.ink_node(&id, &points, kind) {
                    Some(node) => {
                        let node = self.reparent(node, parent.as_deref(), measure);
                        self.commit("Draw", vec![Operation::put(node)])
                    }
                    None => false,
                }
            }
            Gesture::Erase => {
                let ops = self.erase_ops(measure);
                self.erase_path.clear();
                self.commit("Erase", ops)
            }
            Gesture::Place { tool, from, parent } => {
                let dragged = self
                    .camera
                    .scene_to_view(from)
                    .distance(self.camera.scene_to_view(point))
                    >= f64::from(DRAG_THRESHOLD);
                let end = if dragged {
                    point
                } else {
                    let (w, h) = match tool {
                        Tool::Frame => (400.0, 300.0),
                        Tool::Line | Tool::Arrow => (160.0, 0.0),
                        _ => (160.0, 100.0),
                    };
                    Vec2::new(from.x + w, from.y + h)
                };
                match self.placed(
                    tool,
                    from,
                    end,
                    parent.as_deref(),
                    input.modifiers.shift,
                    measure,
                ) {
                    Some(node) => {
                        let id = node.id.clone();
                        let done = self.commit(
                            &format!("Add {}", tool.label().to_lowercase()),
                            vec![Operation::put(node)],
                        );
                        if done {
                            self.selection = vec![id];
                            self.tool = Tool::Select;
                        }
                        done
                    }
                    None => false,
                }
            }
        };
        self.draft = None;
        self.overlay = Overlay {
            hover: self.overlay.hover.take(),
            ..Overlay::default()
        };
        changed || true
    }

    /// `node` in scene coordinates, moved under `parent`.
    fn reparent(&self, mut node: Node, parent: Option<&str>, measure: &dyn TextMeasure) -> Node {
        let Some(parent) = parent else {
            return node;
        };
        let index = SceneIndex::with_measure(self.index_nodes(), measure);
        let Some(frame) = index.get(parent) else {
            return node;
        };
        let to_local = index.world_matrix(frame).inverse();
        let m = to_local.then(rndr_canvas::geometry::local_matrix(&node.transform));
        node.parent_id = Some(parent.to_string());
        node.transform.rotation = m.rotation();
        node.transform.x = 0.0;
        node.transform.y = 0.0;
        let origin = rndr_canvas::geometry::local_matrix(&node.transform).0;
        node.transform.x = m.0[4] - origin[4];
        node.transform.y = m.0[5] - origin[5];
        node
    }

    fn ink_node(&self, id: &str, points: &[InkPoint], input: InputKind) -> Option<Node> {
        let first = points.first()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        let local = points
            .iter()
            .map(|p| InkPoint {
                x: p.x - x0,
                y: p.y - y0,
                ..*p
            })
            .collect();
        let mut node = Node::new(
            id,
            Content::Ink {
                color: self.style.stroke.clone(),
                input,
                brush: Brush {
                    kind: BrushKind::Pen,
                    size: self.style.ink_size.clamp(0.25, 160.0),
                    ..Brush::default()
                },
                points: local,
            },
            Transform {
                x: x0,
                y: y0,
                width: (x1 - x0).max(1.0),
                height: (y1 - y0).max(1.0),
                rotation: 0.0,
            },
        );
        node.opacity = self.style.opacity;
        Some(node)
    }

    fn placed(
        &mut self,
        tool: Tool,
        from: Vec2,
        to: Vec2,
        parent: Option<&str>,
        constrain: bool,
        measure: &dyn TextMeasure,
    ) -> Option<Node> {
        let id = match &self.draft {
            Some(nodes) => nodes
                .last()
                .filter(|n| {
                    n.id.starts_with("n_") && !self.index_nodes().iter().any(|o| o.id == n.id)
                })
                .map(|n| n.id.clone()),
            None => None,
        }
        .unwrap_or_else(|| self.new_id("n"));
        let style = self.style.clone();
        let mut node = match tool {
            Tool::Line | Tool::Arrow => {
                let end = rndr_canvas::geometry::snap_line_point(from, to, constrain);
                let seed = Node::new(
                    id,
                    Content::Shape {
                        shape: ShapeKind::Line,
                        fill: "transparent".into(),
                        stroke: style.stroke.clone(),
                        stroke_width: style.stroke_width.max(0.5),
                        line: Some(Line {
                            start: LinePoint::default(),
                            end: LinePoint::default(),
                            start_arrow: false,
                            end_arrow: tool == Tool::Arrow,
                            dashed: false,
                        }),
                    },
                    Transform::default(),
                );
                set_line_endpoints(&seed, from, end)
            }
            _ => {
                let mut w = (to.x - from.x).abs();
                let mut h = (to.y - from.y).abs();
                if constrain {
                    w = w.max(h);
                    h = w;
                }
                let x = if to.x < from.x { from.x - w } else { from.x };
                let y = if to.y < from.y { from.y - h } else { from.y };
                let transform = Transform {
                    x,
                    y,
                    width: w.max(1.0),
                    height: h.max(1.0),
                    rotation: 0.0,
                };
                let content = match tool {
                    Tool::Frame => {
                        let frames = self
                            .index_nodes()
                            .iter()
                            .filter(|n| matches!(n.content, Content::Frame { .. }))
                            .count();
                        Content::Frame {
                            name: format!("Frame {}", frames + 1),
                            background: if style.fill == "transparent" {
                                "#ffffff".into()
                            } else {
                                style.fill.clone()
                            },
                        }
                    }
                    _ => Content::Shape {
                        shape: if tool == Tool::Ellipse {
                            ShapeKind::Ellipse
                        } else {
                            ShapeKind::Rectangle
                        },
                        fill: style.fill.clone(),
                        stroke: style.stroke.clone(),
                        stroke_width: style.stroke_width,
                        line: None,
                    },
                };
                Node::new(id, content, transform)
            }
        };
        node.opacity = style.opacity;
        Some(self.reparent(node, parent, measure))
    }
}

/// Resize handle positions of a frame in scene coordinates.
pub fn handle_points(frame: &SelectionFrame) -> Vec<(Handle, Vec2)> {
    let (w, h) = (frame.width, frame.height);
    let at = |x: f64, y: f64| frame.matrix.apply(Vec2::new(x, y));
    let mut points = vec![
        (Handle::Nw, at(0.0, 0.0)),
        (Handle::Ne, at(w, 0.0)),
        (Handle::Se, at(w, h)),
        (Handle::Sw, at(0.0, h)),
    ];
    if h > 0.0 {
        points.extend([
            (Handle::N, at(w / 2.0, 0.0)),
            (Handle::E, at(w, h / 2.0)),
            (Handle::S, at(w / 2.0, h)),
            (Handle::W, at(0.0, h / 2.0)),
        ]);
    }
    points
}

/// The rotation handle, [`ROTATE_OFFSET`] view points above the frame.
pub fn rotate_point(frame: &SelectionFrame, zoom: f64) -> Vec2 {
    frame.matrix.apply(Vec2::new(
        frame.width / 2.0,
        -f64::from(ROTATE_OFFSET) / zoom,
    ))
}
