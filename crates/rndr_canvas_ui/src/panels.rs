//! Toolbar, layers list and inspector built from `rndr_ui` buttons. Each
//! button carries a [`HostIntent::Command`]; [`dispatch`] maps it back to a
//! [`PanelAction`] and applies it to the installed [`CanvasView`].

use crate::editor::{StyleChange, Tool};
use crate::view::CanvasView;
use rndr_canvas::SceneIndex;
use rndr_core::{set_parent, ActionIntent, Entity, Layout, World};
use rndr_ui::controls::{
    Button, ButtonProps, ButtonVariant, ControlHandle, HostIntent, Toolbar, ToolbarContentWidth,
};
use rndr_ui::{Typography, UiId};
use std::collections::HashMap;

/// What a panel button does.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelAction {
    Tool(Tool),
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Delete,
    Duplicate,
    Group,
    Ungroup,
    Forward,
    Backward,
    Select(String),
    ToggleVisible(String),
    ToggleLocked(String),
    Style(StyleChange),
}

/// Swatches offered by the inspector.
pub const SWATCHES: [(&str, &str); 8] = [
    ("Ink", "#1e1e1e"),
    ("Grey", "#868e96"),
    ("Red", "#e03131"),
    ("Orange", "#f08c00"),
    ("Green", "#2f9e44"),
    ("Blue", "#1971c2"),
    ("Violet", "#7048e8"),
    ("White", "#ffffff"),
];

/// Retained panel state. Insert with [`mount`].
pub struct Panels {
    toolbar: Entity,
    side: Entity,
    handles: Vec<ControlHandle>,
    actions: HashMap<u64, PanelAction>,
    shown: Option<String>,
    next: u64,
    widths: [f32; 2],
}

/// Mounts the toolbar under `toolbar` and the layers list and inspector
/// under `side`; `widths` are their logical widths, used to lay rows out.
/// Run [`refresh`] and [`dispatch`] each frame.
pub fn mount(world: &mut World, toolbar: Entity, side: Entity, widths: [f32; 2]) {
    world.insert_resource(Panels {
        toolbar,
        side,
        handles: Vec::new(),
        actions: HashMap::new(),
        shown: None,
        next: 1,
        widths,
    });
    refresh(world);
}

fn signature(view: &CanvasView) -> String {
    let doc = &view.editor.mutation().document;
    format!(
        "{}|{:?}|{:?}|{}|{}|{}",
        doc.revision,
        view.editor.tool(),
        view.editor.selection(),
        view.editor.style.stroke,
        view.editor.style.fill,
        view.editor.mutation().can_undo as u8 + 2 * view.editor.mutation().can_redo as u8,
    )
}

struct Builder<'a> {
    world: &'a mut World,
    panels: &'a mut Panels,
}

impl Builder<'_> {
    fn row(&mut self, parent: Entity) -> Entity {
        let id = self.panels.next;
        self.panels.next += 1;
        let available = if parent == self.panels.toolbar {
            self.panels.widths[0]
        } else {
            self.panels.widths[1]
        };
        match Toolbar::mount(self.world, parent, UiId(id), available.max(32.0)) {
            Ok(handle) => handle.root,
            Err(_) => {
                let row = self.world.spawn((Layout {
                    gap: 4.0,
                    ..Layout::row()
                },));
                set_parent(self.world, row, parent);
                row
            }
        }
    }

    fn heading(&mut self, parent: Entity, value: &str) {
        let _ = rndr_ui::text(self.world, parent, value, Typography::Emphasis);
    }

    fn button(
        &mut self,
        parent: Entity,
        label: &str,
        action: PanelAction,
        active: bool,
        enabled: bool,
    ) {
        let id = self.panels.next;
        self.panels.next += 1;
        let mut props = ButtonProps::new(label, HostIntent::Command(id));
        props.variant = if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        };
        props.enabled = enabled;
        if let Ok(handle) = Button::mount(self.world, parent, UiId(id), props) {
            self.world.insert(handle.root, ToolbarContentWidth);
            self.panels.handles.push(handle);
            self.panels.actions.insert(id, action);
        }
    }
}

/// Rebuilds the panels when the document, selection, tool or style changed.
pub fn refresh(world: &mut World) {
    let Some(view) = world.resource::<CanvasView>() else {
        return;
    };
    let shown = signature(view);
    let nodes = view.editor.mutation().document.scene.nodes.clone();
    let selection = view.editor.selection().to_vec();
    let tool = view.editor.tool();
    let style = view.editor.style.clone();
    let (can_undo, can_redo) = (
        view.editor.mutation().can_undo,
        view.editor.mutation().can_redo,
    );
    let Some(state) = world.resource_mut::<Panels>() else {
        return;
    };
    if state.shown.as_deref() == Some(shown.as_str()) {
        return;
    }
    let mut panels = Panels {
        toolbar: state.toolbar,
        side: state.side,
        handles: std::mem::take(&mut state.handles),
        actions: HashMap::new(),
        shown: Some(shown),
        next: 1,
        widths: state.widths,
    };
    for handle in panels.handles.drain(..) {
        let _ = handle.destroy(world);
    }
    for root in [panels.toolbar, panels.side] {
        for child in rndr_core::children_of(world, root) {
            world.despawn_entity(child);
        }
    }
    let (toolbar, side) = (panels.toolbar, panels.side);
    let mut b = Builder {
        world,
        panels: &mut panels,
    };
    let tools = b.row(toolbar);
    for t in Tool::ALL {
        b.button(tools, t.label(), PanelAction::Tool(t), t == tool, true);
    }
    b.button(tools, "Undo", PanelAction::Undo, false, can_undo);
    b.button(tools, "Redo", PanelAction::Redo, false, can_redo);
    b.button(tools, "Zoom in", PanelAction::ZoomIn, false, true);
    b.button(tools, "Zoom out", PanelAction::ZoomOut, false, true);

    let has = !selection.is_empty();
    b.heading(side, "Arrange");
    let edit = b.row(side);
    b.button(edit, "Delete", PanelAction::Delete, false, has);
    b.button(edit, "Duplicate", PanelAction::Duplicate, false, has);
    b.button(
        edit,
        "Group",
        PanelAction::Group,
        false,
        selection.len() > 1,
    );
    b.button(edit, "Ungroup", PanelAction::Ungroup, false, has);
    let order = b.row(side);
    b.button(order, "Forward", PanelAction::Forward, false, has);
    b.button(order, "Backward", PanelAction::Backward, false, has);
    for (title, fill) in [("Stroke", false), ("Fill", true)] {
        b.heading(side, title);
        let current = if fill { &style.fill } else { &style.stroke };
        let mut row = b.row(side);
        if fill {
            let none = current == "transparent";
            b.button(
                row,
                "None",
                PanelAction::Style(StyleChange::Fill("transparent".into())),
                none,
                true,
            );
        }
        for (i, (name, hex)) in SWATCHES.into_iter().enumerate() {
            if i == 4 {
                row = b.row(side);
            }
            let change = if fill {
                StyleChange::Fill(hex.into())
            } else {
                StyleChange::Stroke(hex.into())
            };
            b.button(row, name, PanelAction::Style(change), current == hex, true);
        }
    }
    b.heading(side, "Width");
    let sizes = b.row(side);
    for (name, width) in [
        ("Thin", 1.0),
        ("Medium", 2.0),
        ("Bold", 4.0),
        ("Heavy", 8.0),
    ] {
        let active = (style.stroke_width - width).abs() < 1e-9;
        b.button(
            sizes,
            name,
            PanelAction::Style(StyleChange::StrokeWidth(width)),
            active,
            true,
        );
    }
    b.heading(side, "Text size");
    let fonts = b.row(side);
    for size in [16.0, 24.0, 36.0, 64.0] {
        let active = (style.font_size - size).abs() < 1e-9;
        b.button(
            fonts,
            &format!("{size} pt"),
            PanelAction::Style(StyleChange::FontSize(size)),
            active,
            true,
        );
    }
    b.heading(side, "Layers");
    let index = SceneIndex::new(&nodes);
    for row in index.layer_rows(|_| false) {
        let node = row.node;
        let line = b.row(side);
        let indent = "  ".repeat(row.depth);
        let selected = selection.contains(&node.id);
        b.button(
            line,
            &format!("{indent}{}", capitalized(&node.label())),
            PanelAction::Select(node.id.clone()),
            selected,
            index.editable(node) || !node.visible || node.locked,
        );
        b.button(
            line,
            if node.visible { "Hide" } else { "Show" },
            PanelAction::ToggleVisible(node.id.clone()),
            false,
            true,
        );
        b.button(
            line,
            if node.locked { "Unlock" } else { "Lock" },
            PanelAction::ToggleLocked(node.id.clone()),
            false,
            true,
        );
    }
    world.insert_resource(panels);
}

/// Applies a panel button's action. Returns `false` for intents that are
/// not from the panels.
pub fn dispatch(world: &mut World, request: &ActionIntent) -> bool {
    let Some(HostIntent::Command(id)) = rndr_ui::controls::host_intent(world, request) else {
        return false;
    };
    let Some(action) = world
        .resource::<Panels>()
        .and_then(|p| p.actions.get(&id).cloned())
    else {
        return false;
    };
    let slot = world.resource::<CanvasView>().map(|v| v.slot);
    let viewport = slot
        .and_then(|slot| crate::view::slot_rect(world, slot))
        .map_or([800.0, 600.0], |r| [r.w, r.h]);
    let Some(view) = world.resource_mut::<CanvasView>() else {
        return false;
    };
    let editor = &mut view.editor;
    match action {
        PanelAction::Tool(t) => editor.set_tool(t),
        PanelAction::Undo => {
            editor.undo();
        }
        PanelAction::Redo => {
            editor.redo();
        }
        PanelAction::ZoomIn => editor.zoom_by(1.25, viewport),
        PanelAction::ZoomOut => editor.zoom_by(0.8, viewport),
        PanelAction::Delete => {
            editor.delete_selection();
        }
        PanelAction::Duplicate => {
            editor.duplicate();
        }
        PanelAction::Group => {
            editor.group();
        }
        PanelAction::Ungroup => {
            editor.ungroup();
        }
        PanelAction::Forward => {
            editor.arrange(true);
        }
        PanelAction::Backward => {
            editor.arrange(false);
        }
        PanelAction::Select(id) => editor.select(&[id]),
        PanelAction::ToggleVisible(id) => {
            editor.toggle_visible(&id);
        }
        PanelAction::ToggleLocked(id) => {
            editor.toggle_locked(&id);
        }
        PanelAction::Style(change) => {
            editor.apply_style(change);
        }
    }
    true
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
