//! Optional nonmodal completion display. Keyboard admission remains in core's chronological loop.
use rndr_core::source_extensions::SourceExtensions;
use rndr_core::*;

/// Passive completion popup owned by a source view, not a second focus/input owner.
#[derive(Clone, Copy)]
pub struct CompletionPopup {
    /// Detached floating root, automatically removed when the anchor is destroyed.
    pub root: Entity,
    /// Source editor whose live results are displayed.
    pub editor: Entity,
}
impl Component for CompletionPopup {}
impl CompletionPopup {
    /// Mount a hidden popup. Call [`apply_completion_popups`] in Update after publishing results.
    pub fn mount(world: &mut World, editor: Entity) -> Option<Self> {
        rndr_core::editing::attached_source(world, editor)?;
        let root = world.spawn((
            Layout {
                width: Size::Px(320.0),
                padding: 8.0,
                ..Layout::column()
            },
            Hidden,
            PassivePopup,
            Layer(10),
            Color::rgb(0.12, 0.15, 0.20),
            Label::new(""),
        ));
        world.insert(
            root,
            Semantics {
                name: Some("Completion suggestions".into()),
                ..Default::default()
            },
        );
        world.insert(
            root,
            AnchoredPopup {
                anchor: editor,
                max_height: 220.0,
                closed: |_, _| {},
            },
        );
        let popup = Self { root, editor };
        world.insert(root, popup);
        Some(popup)
    }
}
/// Reconcile live registry results without shifting focus, applying edits or capturing keys.
/// IME/blur/readonly/revision/identity guards are rechecked by the registry on every read.
/// Rows are informational; Up/Down and Enter/Tab are admitted by the source key boundary.
pub fn apply_completion_popups(world: &mut World) {
    for root in world.entities_with::<CompletionPopup>() {
        let popup = *world.get::<CompletionPopup>(root).unwrap();
        if !world.is_alive(popup.editor) {
            despawn(world, root);
            continue;
        }
        let text = world.resource::<SourceExtensions>().and_then(|extensions| {
            let selected = extensions.completion_index(popup.editor);
            extensions
                .completions(world, popup.editor)
                .map(|(_, items)| {
                    // Keep the selected row visible for arbitrarily many provider items.
                    let first = selected.saturating_sub(7);
                    items
                        .iter()
                        .enumerate()
                        .skip(first)
                        .take(8)
                        .map(|(i, item)| {
                            format!("{} {}", if i == selected { "›" } else { " " }, item.label)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        });
        if let Some(text) = text.filter(|s| !s.is_empty()) {
            world.remove::<Hidden>(root);
            if let Some(label) = world.get_mut::<Label>(root) {
                label.text = text;
            }
        } else {
            world.insert(root, Hidden);
        }
    }
}
