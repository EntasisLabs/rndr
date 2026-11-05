//! E7 source-preserving staged editor, not a general inline Markdown editor.
//! Select rendered text -> Capture -> compose a replacement -> Apply.
//! One source session owns every accepted edit and Undo. The native draft field
//! owns only uncommitted replacement text; unsupported operations reveal source.
use rndr_core::editing::*;
use rndr_core::source_projection::*;
use rndr_core::*;
use rndr_ui::controls::{host_intent, Button as UiButton, ButtonProps, ButtonVariant, HostIntent};
use rndr_ui::editing::{EditorSurface, EditorSurfaceProps};
use rndr_ui::{
    apply_theme_bridge, Corner, Token, Typography, UiId, UiPlugin, UiRecipe, UiStyle, UiTheme,
};
use std::sync::Arc;

const INITIAL: &str = "\u{feff}A *quiet* document  \r\n\r\n- [ ] Try a source-preserving edit\r\n- [X] Keep the original delimiters\r\n\r\nMixed text: café, مرحبا and 👩‍💻.\r\n\r\n```unknown\r\n  opaque bytes stay here  \t\r\n```\r\n<custom data=\"untouched\">\r\n";
#[derive(Clone)]
struct Demo {
    document: DocumentId,
    root: Entity,
    source: Entity,
    rendered: Entity,
    draft: Entity,
    status: Entity,
    detail: Entity,
    projection: Projection,
    target: Option<ProjectionTarget>,
    preview: bool,
    message: String,
}
fn column(world: &mut World, parent: Entity, fill: bool, padding: f32, gap: f32) -> Entity {
    let e = world.spawn((Layout {
        width: Size::Fill,
        height: if fill { Size::Fill } else { Size::Hug },
        padding,
        gap,
        ..Layout::column()
    },));
    set_parent(world, e, parent);
    e
}
fn label(
    world: &mut World,
    parent: Entity,
    text: &str,
    typography: Typography,
    secondary: bool,
) -> Entity {
    let e = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new(text),
        LabelBox::CONTENT,
        UiRecipe(UiStyle {
            foreground: Some(
                if secondary {
                    Token::Secondary
                } else {
                    Token::Text
                }
                .into(),
            ),
            typography: Some(typography),
            ..Default::default()
        }),
    ));
    set_parent(world, e, parent);
    e
}
fn setup(world: &mut World) {
    apply_theme_bridge(
        world,
        UiTheme {
            reading_scale: 16.0 / 13.0,
            ..Default::default()
        },
    );
    let mut docs = SourceDocuments::default();
    let document = docs.open(INITIAL);
    let projection = Projection::build(&docs.get(document).unwrap().snapshot());
    let a = projection.text().find("quiet").unwrap();
    let target = projection
        .capture(
            docs.get(document).unwrap(),
            ProjectionSelection::inward(a, a + 5),
        )
        .unwrap();
    world.insert_resource(docs);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 22.0,
            gap: 12.0,
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::Canvas.into()),
            ..Default::default()
        }),
    ));
    label(
        world,
        root,
        "SOURCE PRESERVATION  /  E7",
        Typography::Emphasis,
        true,
    );
    label(
        world,
        root,
        "Rendered edits. Original source.",
        Typography::Title,
        false,
    );
    label(world, root, "Select rendered text → Capture target → compose below → Apply. Unsupported edits reveal source; nothing is serialized.", Typography::Secondary, true);
    let row = world.spawn((Layout {
        width: Size::Fill,
        gap: 8.0,
        ..Layout::row()
    },));
    set_parent(world, row, root);
    for (id, text) in [
        (1, "Capture target"),
        (2, "Apply replacement"),
        (3, "Toggle task"),
        (4, "Reveal source"),
        (5, "Undo source"),
        (6, "Redo source"),
        (7, "Read preview"),
    ] {
        let mut props = ButtonProps::new(text, HostIntent::Command(id));
        props.variant = if id == 2 {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        };
        UiButton::mount(world, row, UiId(id), props).unwrap();
    }
    let panes = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 14.0,
        ..Layout::row()
    },));
    set_parent(world, panes, root);
    let left = column(world, panes, true, 14.0, 10.0);
    let right = column(world, panes, true, 14.0, 10.0);
    for e in [left, right] {
        world.insert(
            e,
            UiRecipe(UiStyle {
                background: Some(Token::Content.into()),
                corner: Some(Corner::Group),
                ..Default::default()
            }),
        );
    }
    label(
        world,
        left,
        "Rendered • select a supported region",
        Typography::Emphasis,
        false,
    );
    label(
        world,
        left,
        "Copy uses display text; copy raw bytes in Source",
        Typography::Secondary,
        true,
    );
    let mut buffer = Buffer::from_str(projection.text());
    let a = projection.text().find("quiet").unwrap();
    buffer.anchor = a;
    buffer.caret = a + 5;
    let rendered = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Label::new(""),
        buffer,
        Editor::default(),
        CapturesKeys,
        Focusable,
    ));
    world.insert(
        rendered,
        Semantics {
            name: Some("Rendered source preview; select then Capture target".into()),
            readonly: true,
            ..Default::default()
        },
    );
    world.insert(
        rendered,
        UiRecipe(UiStyle {
            typography: Some(Typography::Reading),
            foreground: Some(Token::Text.into()),
            ..Default::default()
        }),
    );
    set_parent(world, rendered, left);
    label(
        world,
        right,
        "Source • the only document authority",
        Typography::Emphasis,
        false,
    );
    label(
        world,
        right,
        "BOM + CRLF + spacing + unknown syntax retained",
        Typography::Secondary,
        true,
    );
    let mut props = EditorSurfaceProps::new(document, "Exact source editor");
    props.gutter = false;
    let source = EditorSurface::mount(world, right, UiId(100), props)
        .unwrap()
        .editor;
    let edit = column(world, root, false, 14.0, 7.0);
    world.insert(
        edit,
        UiRecipe(UiStyle {
            background: Some(Token::Content.into()),
            corner: Some(Corner::Group),
            ..Default::default()
        }),
    );
    label(
        world,
        edit,
        "Replacement draft  •  native typing / IME; Apply is one source transaction",
        Typography::Emphasis,
        false,
    );
    let draft = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(38.0),
            padding: 8.0,
            ..Layout::column()
        },
        Label::new("calm"),
        TextField,
        CapturesKeys,
        Focusable,
        FieldTransactions(draft_transaction),
    ));
    world.insert(
        draft,
        Semantics {
            name: Some("Replacement text for captured rendered selection".into()),
            ..Default::default()
        },
    );
    world.insert(draft, UiRecipe(UiStyle::editor()));
    set_parent(world, draft, edit);
    let detail = label(world, edit, "", Typography::Secondary, true);
    let status = label(world, root, "", Typography::Secondary, true);
    world.insert_resource(Demo {document,root,source,rendered,draft,status,detail,projection:projection.clone(),target:Some(target),preview:false,message:"Ready: “quiet” is captured. Replace it, toggle a task, then undo in the source history.".into()});
}
fn draft_transaction(_world: &mut World, _entity: Entity, _signal: FieldTransaction) {}
fn composition_active(world: &World, d: &Demo) -> bool {
    world
        .resource::<Input>()
        .is_some_and(|i| !i.ime_preedit.is_empty())
        || world
            .get::<ImePreview>(d.draft)
            .is_some_and(|i| !i.text.is_empty())
        || world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(d.document)
            .unwrap()
            .is_composing()
}
fn reveal(
    world: &mut World,
    d: &Demo,
    range: std::ops::Range<usize>,
) -> Result<(), ProjectionError> {
    let doc = world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(d.document)
        .unwrap();
    doc.set_view_selection(
        d.source,
        doc.revision(),
        SourceSelection {
            anchor: range.start,
            head: range.end,
        },
    )?;
    world.resource_mut::<Focus>().unwrap().entity = Some(d.source);
    Ok(())
}
fn selected(world: &World, d: &Demo) -> ProjectionSelection {
    let b = world.get::<Buffer>(d.rendered).unwrap();
    ProjectionSelection::inward(b.anchor, b.caret)
}
fn command(world: &mut World, d: &mut Demo, id: u64) -> Result<(), ProjectionError> {
    if composition_active(world, d) {
        return Err(EditError::Composing.into());
    }
    if d.preview && matches!(id, 2 | 3 | 5 | 6) {
        return Err(EditError::Readonly.into());
    }
    match id {
        1 => {
            let s = selected(world, d);
            let target = d.projection.capture(
                world
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(d.document)
                    .unwrap(),
                s,
            )?;
            reset_field_draft(world, d.draft, target.original_text());
            d.target = Some(target);
            world.resource_mut::<Focus>().unwrap().entity = Some(d.draft);
            d.message =
                "Captured a revision-bound target. Compose the replacement, then Apply.".into();
        }
        2 => {
            let t = d.target.as_ref().ok_or(ProjectionError::InvalidPoint)?;
            let text = world.get::<Label>(d.draft).unwrap().text.clone();
            t.replace(
                world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(d.document)
                    .unwrap(),
                &text,
                EditOrigin::Host(700),
            )?;
            d.target = None;
            d.message =
                "Applied exact source splice. Undo source restores the original bytes.".into();
        }
        3 => {
            let b = world.get::<Buffer>(d.rendered).unwrap();
            let offset = d
                .projection
                .segments()
                .iter()
                .filter(|s| s.kind == SegmentKind::Task)
                .find(|s| {
                    s.display.start <= b.caret
                        && b.caret
                            <= d.projection.text()[s.display.end..]
                                .find('\n')
                                .map_or(d.projection.text().len(), |n| s.display.end + n)
                })
                .or_else(|| {
                    d.projection
                        .segments()
                        .iter()
                        .find(|s| s.kind == SegmentKind::Task)
                })
                .map(|s| s.display.start)
                .ok_or(ProjectionError::InvalidPoint)?;
            d.projection.toggle_task(
                world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(d.document)
                    .unwrap(),
                offset,
                EditOrigin::Host(701),
            )?;
            d.message =
                "Toggled only the captured task marker byte; no block normalization.".into();
        }
        4 => {
            let s = match d.projection.selection_to_source(selected(world, d)) {
                Ok(s) => s,
                Err(ProjectionError::RevealSource(r)) => SourceSelection {
                    anchor: r.start,
                    head: r.end,
                },
                Err(e) => return Err(e),
            };
            reveal(world, d, s.anchor.min(s.head)..s.anchor.max(s.head))?;
            d.target = None;
            d.message =
                "Source revealed and selected. The source session/history never changed owners."
                    .into();
        }
        5 | 6 => {
            let doc = world
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(d.document)
                .unwrap();
            if id == 5 {
                doc.undo()?;
            } else {
                doc.redo()?;
            }
            d.message =
                "Used the shared source history; previous captured targets remain stale.".into();
        }
        7 => {
            world
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(d.document)
                .unwrap()
                .set_readonly(!d.preview)?;
            d.preview = !d.preview;
            d.target = None;
            d.message = if d.preview {
                "Read-preview mode. Navigation/copy only; source and rendered edits are refused."
            } else {
                "Editing enabled. Capture a fresh target to edit rendered text."
            }
            .into();
        }
        _ => {}
    }
    Ok(())
}
fn refresh(world: &mut World, d: &mut Demo) {
    let snapshot = world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(d.document)
        .unwrap()
        .snapshot();
    if d.projection.revision() != snapshot.revision {
        d.projection = Projection::build(&snapshot);
        let old = world.get::<Buffer>(d.rendered).unwrap();
        let (a, h) = (
            old.anchor.min(d.projection.text().len()),
            old.caret.min(d.projection.text().len()),
        );
        let mut b = Buffer::from_str(d.projection.text());
        // Source revisions change logical positions: use safe scalar boundaries;
        // a host with a richer selection bridge can map via the source event.
        let floor = |mut p: usize| {
            while !d.projection.text().is_char_boundary(p) {
                p -= 1;
            }
            p
        };
        b.anchor = floor(a);
        b.caret = floor(h);
        world.insert(d.rendered, b);
    }
    let mut emphasis = false;
    let mut runs = Vec::new();
    for s in d.projection.segments() {
        match s.kind {
            SegmentKind::Hidden if snapshot.text[s.source.clone()] == *"*" => emphasis = !emphasis,
            SegmentKind::Text if emphasis => runs.push(
                TextSpan::new(
                    s.display.start,
                    s.display.end,
                    world
                        .get::<Color>(d.rendered)
                        .copied()
                        .unwrap_or(Color::rgb(0.18, 0.19, 0.21)),
                )
                .italic(),
            ),
            SegmentKind::Opaque => runs.push(TextSpan::new(
                s.display.start,
                s.display.end,
                Color::rgb(0.42, 0.44, 0.48),
            )),
            _ => {}
        }
    }
    world.insert(d.rendered, Spans { runs });
    let detail = if let Some(t) = &d.target {
        format!(
            "Captured r{} · display {}..{} · current r{}{}",
            t.revision().0,
            t.selection().anchor.offset,
            t.selection().head.offset,
            snapshot.revision.0,
            if t.revision() != snapshot.revision {
                " — stale: recapture before applying"
            } else {
                ""
            }
        )
    } else {
        "No captured target. Select rendered text and Capture; unsupported regions reveal source."
            .into()
    };
    world.get_mut::<Label>(d.detail).unwrap().text = detail;
    world.get_mut::<Label>(d.status).unwrap().text = format!(
        "r{} · {} source bytes · {} display bytes · {}\n{}",
        snapshot.revision.0,
        snapshot.text.len(),
        d.projection.text().len(),
        if d.preview {
            "READ PREVIEW"
        } else {
            "SOURCE-AUTHORITATIVE"
        },
        d.message
    );
    // The demo is the only source event consumer; no unbounded queue accumulation.
    world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(d.document)
        .unwrap()
        .drain_events()
        .for_each(drop);
}
fn update(world: &mut World) {
    let Some(mut d) = world.resource::<Demo>().cloned() else {
        return;
    };
    refresh(world, &mut d);
    let intents = take_action_intents(world)
        .iter()
        .filter_map(|r| host_intent(world, r))
        .collect::<Vec<_>>();
    for intent in intents {
        if let HostIntent::Command(id) = intent {
            if let Err(error) = command(world, &mut d, id) {
                if let ProjectionError::RevealSource(range) = &error {
                    let _ = reveal(world, &d, range.clone());
                    d.target = None;
                }
                d.message = format!("Refused without changing source: {error:?}");
            }
            refresh(world, &mut d);
        }
    }
    world.insert_resource(d);
}
fn demo_app() -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(WindowConfig {
        title: "Source projection — rndr".into(),
        size: LogicalSize::new(1200.0, 820.0),
        min_size: Some(LogicalSize::new(1000.0, 720.0)),
        ..Default::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}
fn main() {
    let mut args = std::env::args().skip(1);
    let output = match args.next().as_deref() {
        None => None,
        Some("--capture") => Some(args.next().expect("capture needs OUTPUT.png")),
        _ => panic!("usage: projection [--capture OUTPUT.png]"),
    };
    assert!(args.next().is_none());
    let mut app = demo_app();
    if let Some(output) = output {
        app.world_mut()
            .insert_resource(rndr_render::observe::ObservationContent::Public);
        app.world_mut()
            .insert_resource(Viewport::new(1200.0, 820.0));
        let _ = rndr_paint::frame(&mut app);
        let root = app.world().resource::<Demo>().unwrap().root;
        let mut exporter = rndr_render::Exporter::new().expect("offscreen adapter");
        let capture = rndr_render::capture::capture(
            &mut app,
            &mut exporter,
            rndr_render::capture::Target::Entity(root),
            |w, e| rndr_ui::identity(w, e).map(|id| id.0),
        )
        .expect("public E7 fixture");
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap()
            .write_all(&rndr_render::encode_png(&capture.image).unwrap())
            .unwrap();
    } else {
        rndr_render::run(app);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::source_folding::{source_position_box, source_range_boxes};
    fn boot() -> (App, Demo) {
        let mut app = demo_app();
        app.world_mut()
            .insert_resource(Viewport::new(1200.0, 820.0));
        let _ = rndr_paint::frame(&mut app);
        let d = app.world().resource::<Demo>().unwrap().clone();
        (app, d)
    }
    fn text(app: &App, d: &Demo) -> String {
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(d.document)
            .unwrap()
            .text()
            .into()
    }
    #[test]
    fn mount_apply_undo_preview_and_stale_target_use_one_history() {
        let (mut app, mut d) = boot();
        assert_eq!(text(&app, &d), INITIAL);
        assert_eq!(d.projection.revision(), Revision(0));
        command(app.world_mut(), &mut d, 2).unwrap();
        refresh(app.world_mut(), &mut d);
        assert_eq!(text(&app, &d), INITIAL.replacen("quiet", "calm", 1));
        command(app.world_mut(), &mut d, 5).unwrap();
        refresh(app.world_mut(), &mut d);
        assert_eq!(text(&app, &d), INITIAL);
        command(app.world_mut(), &mut d, 1).unwrap();
        command(app.world_mut(), &mut d, 3).unwrap();
        refresh(app.world_mut(), &mut d);
        assert_eq!(
            command(app.world_mut(), &mut d, 2),
            Err(ProjectionError::Source(EditError::StaleRevision))
        );
        command(app.world_mut(), &mut d, 7).unwrap();
        assert_eq!(
            command(app.world_mut(), &mut d, 3),
            Err(ProjectionError::Source(EditError::Readonly))
        );
        command(app.world_mut(), &mut d, 7).unwrap();
    }
    #[test]
    fn native_replacement_field_ime_preedit_is_local_commit_then_apply_is_one_source_edit() {
        let (mut app, mut d) = boot();
        command(app.world_mut(), &mut d, 1).unwrap();
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
        app.update();
        assert_eq!(text(&app, &d), INITIAL);
        assert_eq!(
            command(app.world_mut(), &mut d, 2),
            Err(ProjectionError::Source(EditError::Composing))
        );
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit.clear();
        input.ime_commit = "日本".into();
        input.ime_enabled = true;
        // Replace the captured draft field text using its real native selection.
        app.world_mut().insert(
            d.draft,
            Caret {
                anchor: 0,
                index: 5,
            },
        );
        app.update();
        assert_eq!(text(&app, &d), INITIAL);
        command(app.world_mut(), &mut d, 2).unwrap();
        assert_eq!(text(&app, &d), INITIAL.replacen("quiet", "日本", 1));
        command(app.world_mut(), &mut d, 5).unwrap();
        assert_eq!(text(&app, &d), INITIAL);
    }
    #[test]
    fn reveal_opaque_selects_exact_raw_range_and_changes_no_bytes() {
        let (mut app, mut d) = boot();
        let s = d
            .projection
            .segments()
            .iter()
            .find(|s| s.kind == SegmentKind::Opaque)
            .unwrap()
            .clone();
        let b = app.world_mut().get_mut::<Buffer>(d.rendered).unwrap();
        b.anchor = s.display.start + 1;
        b.caret = s.display.start + 2;
        command(app.world_mut(), &mut d, 4).unwrap();
        let doc = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(d.document)
            .unwrap();
        assert_eq!(
            doc.view_selection(d.source),
            Some(SourceSelection {
                anchor: s.source.start,
                head: s.source.end
            })
        );
        assert_eq!(doc.text(), INITIAL);
        assert_eq!(doc.revision(), Revision(0));
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(d.source)
        );
    }
    #[test]
    fn external_source_edit_during_native_draft_composition_makes_apply_stale() {
        let (mut app, mut d) = boot();
        command(app.world_mut(), &mut d, 1).unwrap();
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
        app.update();
        let doc = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(d.document)
            .unwrap();
        doc.commit(SourceBatch {
            document: d.document,
            base: doc.revision(),
            origin: EditOrigin::Host(99),
            changes: vec![SourceChange {
                range: 3..4,
                replacement: "The".into(),
            }],
        })
        .unwrap();
        let changed = text(&app, &d);
        // Mode/reveal changes refuse while native draft composition is active.
        assert_eq!(
            command(app.world_mut(), &mut d, 4),
            Err(ProjectionError::Source(EditError::Composing))
        );
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit.clear();
        input.ime_commit = "日本".into();
        input.ime_enabled = true;
        app.update();
        assert_eq!(
            command(app.world_mut(), &mut d, 2),
            Err(ProjectionError::Source(EditError::StaleRevision))
        );
        assert_eq!(text(&app, &d), changed);
    }
    #[test]
    fn resize_and_bidi_shape_never_change_logical_mapping_or_dirty_source() {
        let (mut app, d) = boot();
        let expected = d.projection.text().to_owned();
        let at = expected.find("مرحبا").unwrap();
        let selection = ProjectionSelection::inward(at + "مرحبا".len(), at);
        let mapped = d.projection.selection_to_source(selection).unwrap();
        for (w, h) in [(1200.0, 820.0), (1000.0, 720.0)] {
            app.world_mut().insert_resource(Viewport::new(w, h));
            let _ = rndr_paint::frame(&mut app);
            assert_eq!(d.projection.selection_to_source(selection).unwrap(), mapped);
            // Resolve a real shaped interior Arabic caret, not a source offset
            // inferred from visual order. Resize may rewrap but cannot change
            // the logical map. A run interior avoids bidi-boundary ambiguity.
            let logical = at + "مر".len();
            let caret = source_position_box(app.world(), d.rendered, logical).unwrap();
            let layout = app.world().get::<TextLayout>(d.rendered).unwrap();
            let hit = layout.index_at(caret.x, caret.y + caret.h * 0.5);
            assert_eq!(hit, logical);
            let expected_source = INITIAL.find("مرحبا").unwrap() + "مر".len();
            assert_eq!(
                d.projection
                    .point_to_source(ProjectionPoint {
                        offset: hit,
                        affinity: Affinity::After
                    })
                    .unwrap(),
                expected_source
            );
            assert!(
                !source_range_boxes(app.world(), d.rendered, at, at + "مرحبا".len()).is_empty()
            );
            assert_eq!(text(&app, &d), INITIAL);
            assert!(app.world().get::<Rect>(d.draft).unwrap().h >= 30.0);
            assert!(app.world().get::<Rect>(d.rendered).unwrap().h > 150.0);
        }
    }
}
