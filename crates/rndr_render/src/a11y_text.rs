//! Experimental editor text bridge. Full logical text is retained, while shaped
//! visual lines supply bounds. Mixed/RTL geometry is deliberately conservative:
//! character arrays are emitted only for unambiguous monotone LTR caret edges.
use accesskit::{ActionData, Node, NodeId, Rect, Role, TextPosition, TextSelection};
use rndr_core::editing::SourceSelection;
use rndr_core::{AccessCommand, AccessTextSnapshot, Entity, World};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(super) struct PublishedText {
    pub editors: HashMap<NodeId, EditorText>,
}
pub(super) struct Run {
    pub id: NodeId,
    start: usize,
    end: usize,
    offsets: Vec<usize>,
}
pub(super) struct EditorText {
    pub snapshot: AccessTextSnapshot,
    pub runs: Vec<Run>,
    pub nodes: Vec<(NodeId, Node)>,
}
impl EditorText {
    fn position(&self, byte: usize) -> Option<TextPosition> {
        // At a wrap/hard-break boundary use the following line. At document end
        // use the trailing empty run when present.
        let run = self
            .runs
            .iter()
            .rev()
            .find(|r| r.start <= byte && byte <= r.end)?;
        Some(TextPosition {
            node: run.id,
            character_index: run.offsets.binary_search(&byte).ok()?,
        })
    }
    fn byte(&self, position: TextPosition) -> Option<usize> {
        let run = self.runs.iter().find(|r| r.id == position.node)?;
        run.offsets.get(position.character_index).copied()
    }
    pub fn selection(&self) -> Option<TextSelection> {
        Some(TextSelection {
            anchor: self.position(self.snapshot.selection.anchor)?,
            // A wrap endpoint has two TextRun representations. Keep the
            // moving caret on the same shaped row as the retained editor.
            focus: self
                .runs
                .get(self.snapshot.caret_row)
                .filter(|run| {
                    run.start <= self.snapshot.selection.head
                        && self.snapshot.selection.head <= run.end
                })
                .and_then(|run| {
                    Some(TextPosition {
                        node: run.id,
                        character_index: run
                            .offsets
                            .binary_search(&self.snapshot.selection.head)
                            .ok()?,
                    })
                })
                .or_else(|| self.position(self.snapshot.selection.head))?,
        })
    }
}
impl PublishedText {
    pub fn command(
        &self,
        world: &World,
        entity: Entity,
        request: &accesskit::ActionRequest,
    ) -> Option<AccessCommand> {
        let editor = self.editors.get(&request.target_node)?;
        let current = rndr_core::access_text(world, entity)?;
        if !current.selectable || current.state != editor.snapshot.state {
            return None;
        }
        let Some(ActionData::SetTextSelection(selection)) = &request.data else {
            return None;
        };
        Some(AccessCommand::SetTextSelection {
            entity,
            state: editor.snapshot.state.clone(),
            selection: SourceSelection {
                anchor: editor.byte(selection.anchor)?,
                head: editor.byte(selection.focus)?,
            },
        })
    }
}

pub(super) fn publish(
    _owner: NodeId,
    snapshot: AccessTextSnapshot,
    old: Option<&EditorText>,
    mut allocate: impl FnMut() -> NodeId,
) -> Option<EditorText> {
    let text = snapshot.state.text();
    let boundaries: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    // AccessKit lengths are u8 UTF-8 byte counts. Never split a selectable unit
    // or silently wrap its length; unsupported clusters fail the whole bridge.
    if boundaries
        .windows(2)
        .any(|w| w[1] - w[0] > u8::MAX as usize)
    {
        return None;
    }
    if snapshot.layout.lines.is_empty() {
        return None;
    }
    let mut runs = Vec::new();
    let mut nodes = Vec::new();
    let reusable = old.filter(|old| old.snapshot.state == snapshot.state);
    for (index, line) in snapshot.layout.lines.iter().enumerate() {
        let start = line.start;
        let end = snapshot
            .layout
            .lines
            .get(index + 1)
            .map_or(text.len(), |l| l.start);
        if start > end || !boundaries.contains(&start) || !boundaries.contains(&end) {
            return None;
        }
        if index == 0 && start != 0 {
            return None;
        }
        let offsets: Vec<_> = boundaries
            .iter()
            .copied()
            .filter(|b| *b >= start && *b <= end)
            .collect();
        let lengths: Vec<u8> = offsets.windows(2).map(|w| (w[1] - w[0]) as u8).collect();
        let id = reusable
            .and_then(|old| old.runs.get(index))
            .filter(|r| r.start == start && r.end == end)
            .map_or_else(&mut allocate, |r| r.id);
        let mut node = Node::new(Role::TextRun);
        node.set_value(text[start..end].to_owned());
        node.set_character_lengths(lengths);
        let min_x = line.edges.iter().map(|(_, x)| *x).fold(0.0_f32, f32::min);
        let max_x = line
            .edges
            .iter()
            .map(|(_, x)| *x)
            .fold(line.width, f32::max);
        node.set_bounds(Rect {
            x0: (snapshot.origin.0 + min_x) as f64,
            y0: (snapshot.origin.1 + line.y) as f64,
            x1: (snapshot.origin.0 + max_x) as f64,
            y1: (snapshot.origin.1 + line.y + line.h) as f64,
        });
        // Repeated logical caret edges can represent bidi affinities. Do not
        // guess direction or expose fabricated per-character positions there.
        let mut edges = line.edges.clone();
        edges.sort_by_key(|(byte, _)| *byte);
        let monotone = edges
            .windows(2)
            .all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1);
        let xs: Option<Vec<f32>> = offsets
            .iter()
            .map(|b| {
                edges
                    .iter()
                    .find(|(byte, _)| byte == b)
                    .map(|(_, x)| *x)
                    .or_else(|| {
                        if *b == end && text[start..end].ends_with('\n') {
                            Some(line.width)
                        } else {
                            None
                        }
                    })
            })
            .collect();
        if let Some(xs) = xs.filter(|xs| {
            monotone && xs.iter().all(|x| x.is_finite()) && xs.windows(2).all(|w| w[0] <= w[1])
        }) {
            node.set_text_direction(accesskit::TextDirection::LeftToRight);
            node.set_character_positions(
                xs.iter()
                    .take(xs.len().saturating_sub(1))
                    .map(|x| *x - min_x)
                    .collect::<Vec<_>>(),
            );
            node.set_character_widths(xs.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
        }
        nodes.push((id, node));
        runs.push(Run {
            id,
            start,
            end,
            offsets,
        });
    }
    Some(EditorText {
        snapshot,
        runs,
        nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y::Bridge;
    use accesskit::{Action, ActionRequest, TreeId, TreeUpdate};
    use rndr_core::editing::{attach_source, SourceDocuments};
    use rndr_core::{App, Buffer, Chrome, Editor, Focus, Label};

    fn boot(text: &str) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugin(Chrome);
        let e = app.world_mut().spawn((
            Buffer::from_str(text),
            Editor::default(),
            Label::new(""),
            rndr_core::Rect {
                x: 20.0,
                y: 30.0,
                w: 500.0,
                h: 60.0,
            },
        ));
        app.world_mut().insert_resource(Focus { entity: Some(e) });
        app.update();
        (app, e)
    }
    fn owner(e: Entity) -> NodeId {
        NodeId(rndr_core::access_id(e))
    }
    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
        &update.nodes.iter().find(|(n, _)| *n == id).unwrap().1
    }
    fn request(target: NodeId, anchor: TextPosition, focus: TextPosition) -> ActionRequest {
        ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor,
                focus,
            })),
        }
    }
    fn position(node: NodeId, character_index: usize) -> TextPosition {
        TextPosition {
            node,
            character_index,
        }
    }
    #[test]
    fn empty_and_only_hard_break_documents_keep_addressable_endpoints() {
        for text in ["", "\n", "\r\n"] {
            let (mut app, e) = boot(text);
            let mut bridge = Bridge::default();
            let update = bridge.update(app.world());
            let editor = node(&update, owner(e));
            assert!(editor.supports_action(Action::SetTextSelection));
            assert_eq!(
                editor
                    .children()
                    .iter()
                    .map(|id| node(&update, *id).value().unwrap())
                    .collect::<String>(),
                text
            );
            let selection = editor.text_selection().unwrap();
            assert_eq!(selection.anchor, selection.focus);
            let command = bridge
                .command(
                    app.world(),
                    &request(owner(e), selection.anchor, selection.focus),
                )
                .unwrap();
            assert_eq!(
                rndr_core::try_apply_access(app.world_mut(), command),
                rndr_core::AccessCommandResult::Applied
            );
            assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), text);
        }
    }

    #[test]
    fn unicode_bytes_directed_selection_and_hard_breaks_roundtrip() {
        let text = "e\u{301}👩‍👩‍👧‍👦🇺🇳\r\nlast\n";
        let (mut app, e) = boot(text);
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let native = node(&update, owner(e));
        assert_eq!(native.role(), Role::MultilineTextInput);
        assert!(native.supports_action(Action::SetTextSelection));
        let runs: Vec<_> = native
            .children()
            .iter()
            .map(|id| node(&update, *id))
            .collect();
        assert_eq!(
            runs.iter().map(|n| n.value().unwrap()).collect::<String>(),
            text
        );
        assert_eq!(runs[0].character_lengths(), &[3, 25, 8, 2]);
        assert_eq!(runs.last().unwrap().value(), Some(""));
        for run in runs {
            assert_eq!(
                run.character_lengths()
                    .iter()
                    .map(|n| *n as usize)
                    .sum::<usize>(),
                run.value().unwrap().len()
            );
        }
        let first = native.children()[0];
        let cmd = bridge
            .command(
                app.world(),
                &request(owner(e), position(first, 3), position(first, 1)),
            )
            .unwrap();
        assert_eq!(
            rndr_core::try_apply_access(app.world_mut(), cmd),
            rndr_core::AccessCommandResult::Applied
        );
        let b = app.world().get::<Buffer>(e).unwrap();
        assert_eq!((b.anchor, b.caret), (36, 3));
        let update = bridge.update(app.world());
        let selection = node(&update, owner(e)).text_selection().unwrap();
        assert_eq!(selection.anchor, position(first, 3));
        assert_eq!(selection.focus, position(first, 1));
    }
    #[test]
    fn rejects_wrong_tree_owner_out_of_range_and_unpublished_endpoints() {
        let (mut app, e) = boot("abc\n");
        let other = app.world_mut().spawn((
            Buffer::from_str("other"),
            Editor::default(),
            Label::new(""),
            rndr_core::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
            },
        ));
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let first = node(&update, owner(e)).children()[0];
        let foreign = node(&update, owner(other)).children()[0];
        for endpoint in [
            position(foreign, 0),
            position(owner(e), 0),
            position(first, usize::MAX),
            position(NodeId(0), 0),
        ] {
            assert!(bridge
                .command(
                    app.world(),
                    &request(owner(e), position(first, 0), endpoint)
                )
                .is_none());
        }
        let mut req = request(owner(e), position(first, 0), position(first, 1));
        req.target_tree = TreeId("01010101-0101-0101-0101-010101010101".parse().unwrap());
        assert!(bridge.command(app.world(), &req).is_none());
        req.target_tree = TreeId::ROOT;
        req.target_node = first;
        assert!(bridge.command(app.world(), &req).is_none());
    }
    #[test]
    fn source_switch_invalidates_even_identical_text_and_never_reinterprets_old_runs() {
        let (mut app, e) = boot("abc");
        let mut docs = SourceDocuments::default();
        let a = docs.open("abc");
        let b = docs.open("abc");
        app.world_mut().insert_resource(docs);
        attach_source(app.world_mut(), e, a).unwrap();
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let old = node(&update, owner(e)).children()[0];
        let req = request(owner(e), position(old, 0), position(old, 2));
        assert!(bridge.command(app.world(), &req).is_some());
        rndr_core::editing::detach_source(app.world_mut(), e).unwrap();
        attach_source(app.world_mut(), e, b).unwrap();
        assert!(bridge.command(app.world(), &req).is_none());
        let update = bridge.update(app.world());
        assert_ne!(node(&update, owner(e)).children()[0], old);
        assert!(bridge.command(app.world(), &req).is_none());
    }
    #[test]
    fn edited_then_undone_source_still_rejects_published_revision() {
        use rndr_core::editing::{EditOrigin, Revision, SourceBatch, SourceChange};
        let (mut app, e) = boot("abc");
        let mut docs = SourceDocuments::default();
        let id = docs.open("abc");
        app.world_mut().insert_resource(docs);
        attach_source(app.world_mut(), e, id).unwrap();
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let old = node(&update, owner(e)).children()[0];
        let req = request(owner(e), position(old, 0), position(old, 2));
        let doc = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap();
        doc.commit(SourceBatch {
            document: id,
            base: Revision(0),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: 0..1,
                replacement: "x".into(),
            }],
        })
        .unwrap();
        assert!(
            bridge.command(app.world(), &req).is_none(),
            "unprojected source changes cannot use old text"
        );
        app.world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap()
            .undo()
            .unwrap();
        app.update();
        assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), "abc");
        assert!(
            bridge.command(app.world(), &req).is_none(),
            "undo does not revive a source revision"
        );
        let update = bridge.update(app.world());
        assert_ne!(node(&update, owner(e)).children()[0], old);
        assert!(bridge.command(app.world(), &req).is_none());
    }
    #[test]
    fn readonly_selection_and_focus_composition_availability() {
        let (mut app, e) = boot("abc");
        app.world_mut().insert(
            e,
            rndr_core::Semantics {
                readonly: true,
                ..Default::default()
            },
        );
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let editor = node(&update, owner(e));
        assert!(editor.is_read_only());
        assert!(!editor.supports_action(Action::SetValue));
        let first = editor.children()[0];
        let req = request(owner(e), position(first, 0), position(first, 2));
        let cmd = bridge.command(app.world(), &req).unwrap();
        assert_eq!(
            rndr_core::try_apply_access(app.world_mut(), cmd),
            rndr_core::AccessCommandResult::Applied
        );
        app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
        assert!(bridge.command(app.world(), &req).is_none());
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
        app.world_mut()
            .resource_mut::<rndr_core::Input>()
            .unwrap()
            .ime_preedit = "x".into();
        assert!(bridge.command(app.world(), &req).is_none());
        let update = bridge.update(app.world());
        assert!(!node(&update, owner(e)).supports_action(Action::SetTextSelection));
    }
    #[test]
    fn scroll_updates_geometry_with_stable_ids_resize_rebuilds_logical_partition() {
        let text = "abcdefghijk\n".repeat(20);
        let (mut app, e) = boot(&text);
        let mut bridge = Bridge::default();
        let initial = bridge.update(app.world());
        let ids = node(&initial, owner(e)).children().to_vec();
        app.world_mut().get_mut::<Editor>(e).unwrap().offset = 100.0;
        let scrolled = bridge.update(app.world());
        assert_eq!(node(&scrolled, owner(e)).children(), ids.as_slice());
        assert!(
            node(&scrolled, ids[0]).bounds().unwrap().y0
                < node(&initial, ids[0]).bounds().unwrap().y0
        );
        assert!(node(&scrolled, owner(e)).clips_children());
        app.world_mut().get_mut::<rndr_core::Rect>(e).unwrap().w = 45.0;
        let wrapped = bridge.update(app.world());
        let children = node(&wrapped, owner(e)).children();
        assert!(children.len() > ids.len());
        assert_eq!(
            children
                .iter()
                .map(|id| node(&wrapped, *id).value().unwrap())
                .collect::<String>(),
            text
        );
    }
    #[cfg(feature = "parley")]
    #[test]
    fn bundled_shaped_unicode_runs_preserve_source_and_refresh_geometry() {
        use std::sync::Arc;
        let text = "AV fi café e\u{301} مرحبا 👩‍💻 alpha beta gamma\r\nnext\n";
        let (mut app, e) = boot(text);
        let engine: Arc<dyn rndr_core::TextEngine> = Arc::new(crate::ParleyText::bundled());
        app.world_mut().insert_resource(engine);
        app.world_mut().get_mut::<rndr_core::Rect>(e).unwrap().w = 180.0;
        app.world_mut().get_mut::<Editor>(e).unwrap().offset = 0.0;
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        let editor = node(&update, owner(e));
        assert!(editor.supports_action(Action::SetTextSelection));
        assert_eq!(
            editor
                .children()
                .iter()
                .map(|id| node(&update, *id).value().unwrap())
                .collect::<String>(),
            text
        );
        assert!(editor.children().len() > 3);
        let pos = editor.text_selection().unwrap().focus;
        let command = bridge
            .command(app.world(), &request(owner(e), pos, pos))
            .unwrap();
        assert_eq!(
            rndr_core::try_apply_access(app.world_mut(), command),
            rndr_core::AccessCommandResult::Applied
        );
        let old = editor.children().to_vec();
        let second_row = rndr_core::access_text(app.world(), e).unwrap().layout.lines[1].y;
        app.world_mut().get_mut::<Editor>(e).unwrap().offset = second_row;
        let scrolled = bridge.update(app.world());
        assert_eq!(node(&scrolled, owner(e)).children(), old.as_slice());
        assert!(
            node(&scrolled, old[0]).bounds().unwrap().y0
                < node(&update, old[0]).bounds().unwrap().y0
        );
    }

    #[test]
    fn published_focus_preserves_upstream_wrap_row_affinity() {
        let (mut app, e) = boot("abcdefghijk");
        app.world_mut().get_mut::<rndr_core::Rect>(e).unwrap().w = 45.0;
        let mut snapshot = rndr_core::access_text(app.world(), e).unwrap();
        assert!(snapshot.layout.lines.len() > 1);
        snapshot.selection = SourceSelection {
            anchor: 0,
            head: snapshot.layout.lines[1].start,
        };
        snapshot.caret_row = 0;
        let mut next = 1000;
        let published = publish(owner(e), snapshot, None, || {
            next += 1;
            NodeId(next)
        })
        .unwrap();
        let selected = published.selection().unwrap();
        assert_eq!(selected.focus.node, published.runs[0].id);
        assert_eq!(
            selected.focus.character_index,
            published.runs[0].offsets.len() - 1
        );
        assert_eq!(
            published.byte(selected.focus),
            Some(published.runs[1].start)
        );
    }

    #[test]
    fn overlong_grapheme_fails_closed_and_bidi_does_not_invent_arrays() {
        let (app, e) = boot(&format!("e{}", "\u{301}".repeat(200)));
        let mut bridge = Bridge::default();
        let update = bridge.update(app.world());
        assert!(!node(&update, owner(e)).supports_action(Action::SetTextSelection));
        let (app, e) = boot("abc");
        let mut snapshot = rndr_core::access_text(app.world(), e).unwrap();
        snapshot.layout.lines[0].edges = vec![(0, 0.0), (1, 30.0), (2, 10.0), (3, 40.0)];
        let published = publish(owner(e), snapshot, None, || NodeId(1000)).unwrap();
        assert!(published.nodes[0].1.character_positions().is_none());
        assert!(published.nodes[0].1.character_widths().is_none());
    }
}
