//! Native source-session exercise. No filesystem or pretend persistence.
use rndr_core::editing::*;
use rndr_core::source_extensions::*;
use rndr_core::source_folding::*;
use rndr_core::source_selection::SourceSelections;
use rndr_core::*;
use rndr_ui::controls::{host_intent, Button as UiButton, ButtonProps, ButtonVariant, HostIntent};
use rndr_ui::editing::{EditorSurface, EditorSurfaceProps};
use rndr_ui::{
    apply_theme_bridge, Corner, Token, Typography, UiId, UiPlugin, UiRecipe, UiStyle, UiTheme,
};
use std::sync::Arc;

const INITIAL: &str = "\u{feff}---\r\ntitle: Source session\r\n---\r\n# Exact source\r\n\r\n- [ ] é مرحبا 👩‍💻\r\n```unknown\r\nopaque bytes stay opaque\r\n```\r\n";
/// Intentionally fake host journal: captures exact bytes and revision, then
/// acknowledges that capture later, never overwriting subsequent local edits.
struct FakeHost {
    document: DocumentId,
    saved: SourceSnapshot,
    pending: Option<SourceSnapshot>,
    root: Entity,
    status: Entity,
    detail: Entity,
    events: usize,
    message: String,
}
// Native and offscreen rendering deliberately share the same shaped-font path.
// Installing TextFace here would opt out of the renderer’s Parley default.
#[derive(Clone, Copy)]
struct SplitView(bool);
struct ExtensionDemo;
#[derive(Clone, Copy)]
struct DeliverFakeCompletion;
impl Event for DeliverFakeCompletion {}
const EXTENSION_SOURCE: &str = "// Fake providers: no language server or filesystem\r\nfn demo() {\r\n    let value = 42;\r\n    let other = value; // TODO\r\n    println(\"value\");\r\n}\r\n";
struct ExtensionState {
    editor: Entity,
    provider: ProviderId,
    clock: u64,
    transport: FakeDelayedProvider,
    syntax_revision: Option<Revision>,
}

fn demo_app() -> App {
    demo_app_with_split(false)
}
fn demo_app_with_split(split: bool) -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(SplitView(split));
    app.add_plugin(UiPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(WindowConfig {
        title: "Source session — rndr".into(),
        min_size: Some(LogicalSize::new(640.0, 540.0)),
        ..Default::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, rndr_ui::source_tools::apply_find_bars);
    app.add_system(Schedule::Update, host);
    app.add_system(Schedule::Update, extension_update);
    app.add_system(
        Schedule::Update,
        rndr_ui::completion::apply_completion_popups,
    );
    app
}

/// The optional tool workspace needs more room than the base save-host demo.
/// Apply before native runner creation, also shared by offscreen captures/tests.
fn enable_extensions(app: &mut App) {
    app.world_mut().insert_resource(ExtensionDemo);
    let config = app.world_mut().resource_mut::<WindowConfig>().unwrap();
    config.size = LogicalSize::new(1000.0, 800.0);
    config.min_size = Some(LogicalSize::new(800.0, 720.0));
}

fn column(world: &mut World, parent: Entity, padding: f32, gap: f32, fill: bool) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Fill,
        height: if fill { Size::Fill } else { Size::Hug },
        padding,
        gap,
        ..Layout::column()
    },));
    set_parent(world, entity, parent);
    entity
}

fn label(
    world: &mut World,
    parent: Entity,
    text: &str,
    typography: Typography,
    color: Token,
) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new(text),
        LabelBox::CONTENT,
        UiRecipe(UiStyle {
            foreground: Some(color.into()),
            typography: Some(typography),
            ..Default::default()
        }),
    ));
    set_parent(world, entity, parent);
    entity
}

fn setup(world: &mut World) {
    apply_theme_bridge(
        world,
        UiTheme {
            reading_scale: 15.0 / 13.0,
            code_family: if cfg!(target_os = "macos") {
                "Menlo"
            } else {
                "monospace"
            }
            .into(),
            ..Default::default()
        },
    );
    let mut docs = SourceDocuments::default();
    let document = docs.open(if world.has_resource::<ExtensionDemo>() {
        EXTENSION_SOURCE
    } else {
        INITIAL
    });
    let saved = docs.get(document).unwrap().snapshot();
    world.insert_resource(docs);
    let extensions = world.has_resource::<ExtensionDemo>();
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: if extensions { 8.0 } else { 16.0 },
        padding: if extensions { 12.0 } else { 20.0 },
        ..Layout::column()
    },));
    world.insert(
        root,
        rndr_ui::UiRecipe(rndr_ui::UiStyle {
            background: Some(rndr_ui::Token::Canvas.into()),
            ..Default::default()
        }),
    );
    let header = column(world, root, 0.0, 4.0, false);
    if !extensions {
        label(
            world,
            header,
            "RNDR  /  NATIVE EDITING",
            Typography::Emphasis,
            Token::Secondary,
        );
    }
    label(
        world,
        header,
        "Source session",
        Typography::Heading,
        Token::Text,
    );

    let paper = column(world, root, 0.0, 0.0, true);
    world.insert(
        paper,
        UiRecipe(UiStyle {
            background: Some(Token::Content.into()),
            corner: Some(Corner::Group),
            ..Default::default()
        }),
    );
    let document_header = column(world, paper, 12.0, 4.0, false);
    label(
        world,
        document_header,
        "source.md",
        Typography::Emphasis,
        Token::Text,
    );
    let status = label(
        world,
        document_header,
        "",
        Typography::Secondary,
        Token::Secondary,
    );
    let divider = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(1.0),
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::SubtleStroke.into()),
            ..Default::default()
        }),
    ));
    set_parent(world, divider, paper);
    let split = world.resource::<SplitView>().is_some_and(|s| s.0);
    let panes = if split {
        let panes = world.spawn((Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.0,
            ..Layout::row()
        },));
        set_parent(world, panes, paper);
        panes
    } else {
        paper
    };
    let view = EditorSurface::mount(
        world,
        panes,
        UiId(100),
        EditorSurfaceProps::new(document, "Exact source document"),
    )
    .unwrap();
    if world.has_resource::<ExtensionDemo>() {
        rndr_ui::source_tools::FindBar::mount(world, document_header, view.editor).unwrap();
        rndr_ui::completion::CompletionPopup::mount(world, view.editor).unwrap();
        let mut extensions = SourceExtensions::default();
        let provider = extensions.register(10);
        world.insert_resource(extensions);
        world.insert_resource(ExtensionState {
            editor: view.editor,
            provider,
            clock: 0,
            transport: FakeDelayedProvider::default(),
            syntax_revision: None,
        });
    }
    // Keep the shared surface’s interaction and history untouched. Only the
    // demo’s spacing changes; gutter and text use the same type role and inset.
    world.get_mut::<Layout>(view.editor).unwrap().padding = 16.0;
    for child in children_of(world, view.root) {
        if world.has::<Gutter>(child) {
            let layout = world.get_mut::<Layout>(child).unwrap();
            layout.padding = 16.0;
            layout.width = Size::Px(80.0);
        }
    }
    if split {
        let peer = EditorSurface::mount(
            world,
            panes,
            UiId(101),
            EditorSurfaceProps::new(document, "Exact source document — second pane"),
        )
        .unwrap();
        world.get_mut::<Layout>(peer.editor).unwrap().padding = 16.0;
        for child in children_of(world, peer.root) {
            if world.has::<Gutter>(child) {
                let layout = world.get_mut::<Layout>(child).unwrap();
                layout.padding = 16.0;
                layout.width = Size::Px(64.0);
            }
        }
    }
    let hint = column(world, paper, 12.0, 0.0, false);
    label(
        world,
        hint,
        if split {
            "Two views  ·  Independent selections and scroll  ·  One undo history"
        } else {
            "Exact source  ·  One shared undo history"
        },
        Typography::Secondary,
        Token::Secondary,
    );

    let tools = column(world, root, 0.0, 8.0, false);
    label(
        world,
        tools,
        "In-memory host  /  Nothing is saved to disk",
        Typography::Emphasis,
        Token::Secondary,
    );
    let commands = world.spawn((Layout {
        width: Size::Fill,
        gap: 4.0,
        ..Layout::row()
    },));
    set_parent(world, commands, tools);
    for (id, text, variant) in [
        (1, "Host splice", ButtonVariant::Quiet),
        (2, "Capture save", ButtonVariant::Primary),
        (3, "Ack save", ButtonVariant::Secondary),
        (4, "Reload saved", ButtonVariant::Quiet),
        (5, "Toggle readonly", ButtonVariant::Quiet),
    ] {
        let mut props = ButtonProps::new(text, HostIntent::Command(id));
        props.variant = variant;
        UiButton::mount(world, commands, UiId(id), props).unwrap();
    }
    if world.has_resource::<ExtensionDemo>() {
        // Two compact rows keep the optional command groups easy to scan.
        for row in [
            vec![
                (200, "Select value"),
                (206, "Rectangle"),
                (201, "Fold / unfold"),
                (205, "Match brace"),
            ],
            vec![(202, "Complete"), (203, "Diagnose"), (204, "Fix TODO")],
        ] {
            let commands = world.spawn((Layout {
                width: Size::Fill,
                gap: 4.0,
                ..Layout::row()
            },));
            set_parent(world, commands, tools);
            for (id, text) in row {
                let mut props = ButtonProps::new(text, HostIntent::Command(id));
                props.variant = ButtonVariant::Quiet;
                UiButton::mount(world, commands, UiId(id), props).unwrap();
            }
        }
    }
    let detail = label(world, tools, "", Typography::Secondary, Token::Secondary);
    // Reserve two lines so a host message does not move the editor while typing.
    world.get_mut::<Layout>(detail).unwrap().height = Size::Px(36.0);
    world.resource_mut::<Focus>().unwrap().entity = Some(view.editor);
    world.insert_resource(FakeHost {
        document,
        saved,
        pending: None,
        root,
        status,
        detail,
        events: 0,
        message:
            "Try delayed save: Capture save, keep typing, then Ack save. Tab reaches controls."
                .into(),
    });
}
fn host(world: &mut World) {
    if world.has_resource::<ExtensionState>() {
        let intents = take_action_intents_matching(world, |w, intent| {
            rndr_ui::identity(w, intent.entity).is_some_and(|id| (200..=207).contains(&id.0))
        });
        for intent in intents {
            if let Some(HostIntent::Command(id)) = host_intent(world, &intent) {
                let message = extension_command(world, id);
                world.resource_mut::<FakeHost>().unwrap().message = message;
            }
        }
    }
    let commands: Vec<_> = take_action_intents_matching(world, |w, intent| {
        // Only demo commands; optional source tools own their own FIFO requests.
        w.get::<Parent>(intent.entity).is_some_and(|parent| {
            children_of(w, parent.entity)
                .iter()
                .any(|e| rndr_ui::identity(w, *e) == Some(UiId(5)))
        })
    })
    .iter()
    .filter_map(|intent| host_intent(world, intent))
    .collect();
    for command in commands {
        let HostIntent::Command(command) = command else {
            continue;
        };
        let host = world.resource::<FakeHost>().unwrap();
        let document = host.document;
        let saved = host.saved.clone();
        let pending = host.pending.clone();
        let session = world
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(document)
            .unwrap();
        let message = match command {
            1 => {
                let end = session.text().len();
                match session.commit(SourceBatch {
                    document,
                    base: session.revision(),
                    origin: EditOrigin::Host(1),
                    changes: vec![SourceChange {
                        range: end..end,
                        replacement: "\r\n// host splice\r\n".into(),
                    }],
                }) {
                    Ok(_) => "Host text appended. Undo uses the same document history.".into(),
                    Err(error) => format!("Host splice rejected: {error:?}"),
                }
            }
            2 => {
                let snapshot = session.snapshot();
                let message = format!(
                    "Save captured revision {}; edit before Ack to test delayed completion",
                    snapshot.revision.0
                );
                world.resource_mut::<FakeHost>().unwrap().pending = Some(snapshot);
                message
            }
            3 => {
                if let Some(snapshot) = pending {
                    let message = format!(
                        "Acknowledged captured revision {} (local text untouched)",
                        snapshot.revision.0
                    );
                    let host = world.resource_mut::<FakeHost>().unwrap();
                    host.saved = snapshot;
                    host.pending = None;
                    message
                } else {
                    "No pending save capture".into()
                }
            }
            4 => match session.reload(session.revision(), saved.text) {
                Ok(_) => "Reloaded the acknowledged snapshot into the current session.".into(),
                Err(error) => format!("Reload rejected: {error:?}"),
            },
            5 => match session.set_readonly(!session.is_readonly()) {
                Ok(_) => {
                    if session.is_readonly() {
                        "Read only. Selection, copy and navigation remain available.".into()
                    } else {
                        "Editing enabled.".into()
                    }
                }
                Err(error) => format!("Permission change rejected: {error:?}"),
            },
            _ => continue,
        };
        world.resource_mut::<FakeHost>().unwrap().message = message;
    }
    let host = world.resource::<FakeHost>().unwrap();
    let document = host.document;
    let saved_text = host.saved.text.clone();
    let session = world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    let events = session.drain_events().count();
    let dirty = session.text() != saved_text;
    let revision = session.revision().0;
    let readonly = session.is_readonly();
    let host = world.resource_mut::<FakeHost>().unwrap();
    host.events += events;
    let state = if dirty {
        "Unsaved changes"
    } else {
        "Snapshot matches"
    };
    let mode = if readonly { "Read only" } else { "Editable" };
    let pending = host.pending.as_ref().map_or(String::new(), |snapshot| {
        format!("  ·  Save pending: r{}", snapshot.revision.0)
    });
    let text = format!("{state}  ·  {mode}  ·  Revision {revision}{pending}");
    let detail_text = format!("{}  ·  {} change events", host.message, host.events);
    let status = host.status;
    let detail = host.detail;
    world.get_mut::<Label>(status).unwrap().text = text;
    world.get_mut::<Label>(detail).unwrap().text = detail_text;
}
// Frame ticks make the fake transport deterministic in tests. A native timer
// also advances it after 250ms, waking an otherwise idle window without moving focus.
fn extension_update(world: &mut World) {
    if !world.has_resource::<ExtensionState>() {
        return;
    }
    let (editor, provider, revision) = {
        let state = world.resource::<ExtensionState>().unwrap();
        let revision = attached_source(world, state.editor)
            .and_then(|id| world.resource::<SourceDocuments>()?.get(id))
            .map(|d| d.revision());
        (state.editor, state.provider, revision)
    };
    let mut extensions = std::mem::take(world.resource_mut::<SourceExtensions>().unwrap());
    extensions.refresh(world);
    let state = world.resource_mut::<ExtensionState>().unwrap();
    state.clock += 1;
    if !events::<DeliverFakeCompletion>(world).is_empty() {
        world.resource_mut::<ExtensionState>().unwrap().clock += 90;
    }
    let state = world.resource_mut::<ExtensionState>().unwrap();
    let clock = state.clock;
    let mut transport = std::mem::take(&mut state.transport);
    let delivered = transport.deliver(clock, &mut extensions, world);
    world.resource_mut::<ExtensionState>().unwrap().transport = transport;
    if !delivered.is_empty() {
        world.resource_mut::<FakeHost>().unwrap().message = if delivered.iter().all(Result::is_ok) {
            "Fake completion delivered. Up/Down, Enter/Tab accept; Escape dismisses.".into()
        } else {
            "Delayed result refused: source, selection, focus or policy changed.".into()
        };
    }
    if revision != world.resource::<ExtensionState>().unwrap().syntax_revision {
        if let Ok((ticket, snapshot)) =
            extensions.request(world, provider, editor, RequestKind::Decoration)
        {
            let analysis = BraceSourceHooks.analyze(&snapshot.text);
            let items = analysis
                .syntax
                .into_iter()
                .map(|span| SourceDecoration {
                    range: span.start..span.end,
                    color: span.color,
                    class: "demo syntax".into(),
                })
                .collect();
            if extensions
                .publish_decorations(
                    world,
                    &ticket,
                    items,
                    vec![GutterMarker {
                        at: 0,
                        label: "Fake brace-source provider".into(),
                    }],
                )
                .is_ok()
            {
                world
                    .resource_mut::<ExtensionState>()
                    .unwrap()
                    .syntax_revision = revision;
            }
        }
    }
    *world.resource_mut::<SourceExtensions>().unwrap() = extensions;
}

fn extension_command(world: &mut World, id: u64) -> String {
    let state = world.resource::<ExtensionState>().unwrap();
    let (editor, provider, clock) = (state.editor, state.provider, state.clock);
    // A visible host command deliberately returns focus to its requesting view.
    if world.resource::<Focus>().and_then(|f| f.entity) != Some(editor) {
        request_focus(world, editor);
    }
    project_source(world, editor);
    let mut extensions = std::mem::take(world.resource_mut::<SourceExtensions>().unwrap());
    extensions.refresh(world);
    let result = (|| -> Result<String, String> {
        let document = attached_source(world, editor).ok_or("Detached editor")?;
        let snapshot = world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .snapshot();
        match id {
            200 => {
                let ranges = snapshot
                    .text
                    .match_indices("value")
                    .map(|(at, word)| SourceSelection {
                        anchor: at,
                        head: at + word.len(),
                    })
                    .collect::<Vec<_>>();
                let count = ranges.len();
                let selections = SourceSelections::new(&snapshot.text, ranges, 0)
                    .map_err(|e| format!("{e:?}"))?;
                world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(document)
                    .unwrap()
                    .set_view_selections(editor, snapshot.revision, selections)
                    .map_err(|e| format!("{e:?}"))?;
                project_source(world, editor);
                Ok(format!(
                    "{count} literal value selections; typing replaces all in one undo step."
                ))
            }
            201 => {
                if world.has::<SourceFolds>(editor) {
                    clear_source_folds(world, editor);
                    return Ok("Expanded all source lines.".into());
                }
                let mut folds = BraceSourceHooks.analyze(&snapshot.text).folds;
                folds.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
                let mut end = 0;
                folds.retain(|r| {
                    if r.start < end {
                        false
                    } else {
                        end = r.end;
                        true
                    }
                });
                // Keep the selection on the opening line, outside the hidden bytes.
                world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(document)
                    .unwrap()
                    .set_view_selection(
                        editor,
                        snapshot.revision,
                        SourceSelection { anchor: 0, head: 0 },
                    )
                    .map_err(|e| format!("{e:?}"))?;
                project_source(world, editor);
                set_source_folds(world, editor, document, snapshot.revision, folds)
                    .map_err(|e| format!("{e:?}"))?;
                Ok("Collapsed brace-body lines; hidden bytes remain in source and undo.".into())
            }
            202 => {
                let (ticket, _) = extensions
                    .request(world, provider, editor, RequestKind::Completion)
                    .map_err(|e| e.to_string())?;
                let selection = ticket.selection();
                let item = CompletionItem {
                    label: "Insert fake completion".into(),
                    changes: vec![SourceChange {
                        range: selection.anchor.min(selection.head)
                            ..selection.anchor.max(selection.head),
                        replacement: "demo_completion".into(),
                    }],
                };
                world
                    .resource_mut::<ExtensionState>()
                    .unwrap()
                    .transport
                    .enqueue(clock + 90, ticket, vec![item]);
                after(
                    world,
                    std::time::Duration::from_millis(250),
                    DeliverFakeCompletion,
                )
                .detach();
                Ok("Fake completion queued (250ms). Keep source focus; edit or move to prove stale refusal.".into())
            }
            203 => {
                let (ticket, _) = extensions
                    .request(world, provider, editor, RequestKind::Diagnostics)
                    .map_err(|e| e.to_string())?;
                let at = snapshot
                    .text
                    .find("TODO")
                    .ok_or("No TODO in current source")?;
                extensions
                    .publish_diagnostics(
                        world,
                        &ticket,
                        vec![Diagnostic {
                            range: at..at + 4,
                            message: "Fake diagnostic: resolve TODO".into(),
                            severity: DiagnosticSeverity::Warning,
                            fix: vec![SourceChange {
                                range: at..at + 4,
                                replacement: "DONE".into(),
                            }],
                        }],
                    )
                    .map_err(|e| e.to_string())?;
                Ok("Published fake TODO warning; Fix TODO uses shared undo history.".into())
            }
            204 => {
                extensions
                    .apply_diagnostic_fix(world, provider, editor, 0)
                    .map_err(|e| e.to_string())?;
                Ok("Applied revision-fenced TODO fix; Undo restores exact bytes.".into())
            }
            205 => {
                let (ticket, _) = extensions
                    .request(world, provider, editor, RequestKind::Navigation)
                    .map_err(|e| e.to_string())?;
                let (_, at) = BraceSourceHooks
                    .analyze(&snapshot.text)
                    .brackets
                    .first()
                    .copied()
                    .ok_or("No matched brace")?;
                extensions
                    .navigate(
                        world,
                        &ticket,
                        SourceSelection {
                            anchor: at,
                            head: at,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                Ok("Navigated to matching brace without changing source.".into())
            }
            207 => {
                // Explicit advancement also works when the native window is idle.
                let state = world.resource_mut::<ExtensionState>().unwrap();
                state.clock += 90;
                let now = state.clock;
                let mut transport = std::mem::take(&mut state.transport);
                let results = transport.deliver(now, &mut extensions, world);
                world.resource_mut::<ExtensionState>().unwrap().transport = transport;
                Ok(if results.is_empty() {
                    "No queued fake completion.".into()
                } else if results.iter().all(Result::is_ok) {
                    "Delivered fake completion. Enter/Tab accepts; Escape dismisses.".into()
                } else {
                    "Delayed result refused: source, selection, focus or policy changed.".into()
                })
            }
            206 => {
                let geometry = access_text(world, editor).ok_or("Wait for shaped editor layout")?;
                let rows: Vec<_> = geometry.layout.lines.iter().skip(2).take(3).collect();
                let first = rows.first().ok_or("No visible source rows")?;
                let last = rows.last().unwrap();
                let anchor = [
                    geometry.origin.0 + 32.0,
                    geometry.origin.1 + first.y + first.h / 2.0,
                ];
                let head = [
                    geometry.origin.0 + 96.0,
                    geometry.origin.1 + last.y + last.h / 2.0,
                ];
                rndr_core::source_selection::select_source_rectangle(world, editor, anchor, head)
                    .map_err(|e| format!("{e:?}"))?;
                let count = world
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(document)
                    .unwrap()
                    .view_selections(editor)
                    .unwrap()
                    .ranges()
                    .len();
                Ok(format!(
                    "{count} rectangular selections from real shaped rows (32–96pt)."
                ))
            }
            _ => Err("Unknown extension command".into()),
        }
    })();
    *world.resource_mut::<SourceExtensions>().unwrap() = extensions;
    result.unwrap_or_else(|e| format!("Extension command refused: {e}"))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut split = false;
    let mut output = None;
    let mut extensions = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--split" => split = true,
            "--extensions" => extensions = true,
            "--capture" => output = Some(args.next().expect("capture needs OUTPUT.png")),
            _ => panic!("usage: source-session [--split] [--extensions] [--capture OUTPUT.png]"),
        }
    }
    let mut app = if split {
        demo_app_with_split(true)
    } else {
        demo_app()
    };
    if extensions {
        enable_extensions(&mut app);
    }
    if let Some(output) = output {
        // Only this built-in fake fixture is attested public; never capture a
        // loaded document or native desktop. No window/event loop is started.

        app.world_mut()
            .insert_resource(rndr_render::observe::ObservationContent::Public);
        let size = app.world().resource::<WindowConfig>().unwrap().size;
        app.world_mut()
            .insert_resource(Viewport::new(size.width, size.height));
        let _ = rndr_paint::frame(&mut app);
        let root = app.world().resource::<FakeHost>().unwrap().root;
        let mut exporter = rndr_render::Exporter::new().expect("offscreen adapter");
        let capture = rndr_render::capture::capture(
            &mut app,
            &mut exporter,
            rndr_render::capture::Target::Entity(root),
            |w, e| rndr_ui::identity(w, e).map(|id| id.0),
        )
        .expect("public fake-host capture");
        let png = rndr_render::encode_png(&capture.image).expect("PNG encode");
        // Avoid silently replacing an existing artifact.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .expect("new capture path");
        file.write_all(&png).expect("PNG write");
    } else {
        rndr_render::run(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(app: &mut App, id: u64) {
        let entity = app
            .world()
            .entities_with::<rndr_core::Button>()
            .into_iter()
            .find(|&e| rndr_ui::identity(app.world(), e) == Some(UiId(id)))
            .unwrap();
        assert_eq!(
            try_activate(app.world_mut(), entity),
            ActivationResult::Accepted
        );
        host(app.world_mut());
    }

    fn extensions_app() -> App {
        let mut app = demo_app();
        app.world_mut().insert_resource(ExtensionDemo);
        setup(app.world_mut());
        app
    }

    #[test]
    fn extension_controls_select_fold_diagnose_fix_and_navigate() {
        let mut app = extensions_app();
        let world = app.world_mut();
        let editor = world.resource::<ExtensionState>().unwrap().editor;
        let document = attached_source(world, editor).unwrap();
        assert!(extension_command(world, 200).contains("3 literal"));
        assert_eq!(
            world
                .resource::<SourceDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .view_selections(editor)
                .unwrap()
                .ranges()
                .len(),
            3
        );
        let original = world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .snapshot();
        assert!(extension_command(world, 201).contains("Collapsed"));
        assert!(!world
            .get::<SourceFolds>(editor)
            .unwrap()
            .ranges()
            .is_empty());
        assert_eq!(
            world
                .resource::<SourceDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .text(),
            original.text
        );
        assert!(extension_command(world, 201).contains("Expanded"));
        assert!(extension_command(world, 203).contains("Published"));
        assert!(extension_command(world, 204).contains("Applied"));
        assert!(world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text()
            .contains("DONE"));
        assert!(extension_command(world, 205).contains("Navigated"));
        assert_eq!(world.resource::<Focus>().unwrap().entity, Some(editor));
    }

    #[test]
    fn extension_fake_request_retains_native_timer_after_handler_returns() {
        let mut app = demo_app();
        app.world_mut().insert_resource(ExtensionDemo);
        app.update();
        assert!(app.next_deadline().is_none());
        let result = extension_command(app.world_mut(), 202);
        assert!(result.contains("queued"), "{result}");
        assert!(app.next_deadline().is_some());
    }

    #[test]
    fn extension_fake_completion_delivers_only_at_deadline_and_refuses_late_source() {
        let mut app = extensions_app();
        let world = app.world_mut();
        let editor = world.resource::<ExtensionState>().unwrap().editor;
        let document = attached_source(world, editor).unwrap();
        assert!(extension_command(world, 202).contains("queued"));
        for _ in 0..89 {
            extension_update(world);
        }
        assert!(world
            .resource::<SourceExtensions>()
            .unwrap()
            .completions(world, editor)
            .is_none());
        extension_update(world);
        assert!(world
            .resource::<SourceExtensions>()
            .unwrap()
            .completions(world, editor)
            .is_some());
        assert!(extension_command(world, 202).contains("queued"));
        let doc = world
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(document)
            .unwrap();
        doc.commit(SourceBatch {
            document,
            base: doc.revision(),
            origin: EditOrigin::Host(99),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "// newer\r\n".into(),
            }],
        })
        .unwrap();
        for _ in 0..90 {
            extension_update(world);
        }
        assert!(world
            .resource::<SourceExtensions>()
            .unwrap()
            .completions(world, editor)
            .is_none());
        assert!(world
            .resource::<FakeHost>()
            .unwrap()
            .message
            .contains("refused"));
        assert!(world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text()
            .starts_with("// newer"));
    }

    #[test]
    fn extension_rectangle_uses_real_shaped_editor_rows() {
        let mut app = demo_app();
        app.world_mut().insert_resource(ExtensionDemo);
        app.world_mut()
            .insert_resource(Viewport::new(1000.0, 900.0));
        for _ in 0..3 {
            let _ = rndr_paint::frame(&mut app);
        }
        let message = extension_command(app.world_mut(), 206);
        assert!(message.contains("3 rectangular"), "{message}");
        assert!(extension_command(app.world_mut(), 202).contains("queued"));
        assert!(extension_command(app.world_mut(), 207).contains("Delivered"));
    }

    #[test]
    fn extension_syntax_is_registry_owned_and_base_demo_remains_exact() {
        let mut app = extensions_app();
        extension_update(app.world_mut());
        let world = app.world();
        let editor = world.resource::<ExtensionState>().unwrap().editor;
        assert!(!world
            .resource::<SourceExtensions>()
            .unwrap()
            .decorations(world, editor)
            .is_empty());
        let mut base = demo_app();
        setup(base.world_mut());
        assert!(!base.world().has_resource::<SourceExtensions>());
        let document = base.world().resource::<FakeHost>().unwrap().document;
        assert_eq!(
            base.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .text(),
            INITIAL
        );
    }

    #[test]
    fn split_views_share_source_but_keep_distinct_entities_and_accessible_names() {
        let mut app = demo_app_with_split(true);
        app.world_mut()
            .insert_resource(Viewport::new(1000.0, 720.0));
        for _ in 0..3 {
            let _ = rndr_paint::frame(&mut app);
        }
        let views = app.world().entities_with::<Editor>();
        assert_eq!(views.len(), 2);
        for &e in &views {
            let rect = app.world().get::<Rect>(e).unwrap();
            assert!(rect.w > 200.0 && rect.h > 180.0, "{rect:?}");
        }
        assert_ne!(
            app.world().get::<Semantics>(views[0]).unwrap().name,
            app.world().get::<Semantics>(views[1]).unwrap().name
        );
        let a = attached_source(app.world(), views[0]).unwrap();
        assert_eq!(attached_source(app.world(), views[1]), Some(a));
        let session = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap();
        session
            .commit(SourceBatch {
                document: a,
                base: session.revision(),
                origin: EditOrigin::Host(1),
                changes: vec![SourceChange {
                    range: 0..0,
                    replacement: "shared ".into(),
                }],
            })
            .unwrap();
        let _ = rndr_paint::frame(&mut app);
        for e in views {
            assert!(app
                .world()
                .get::<Buffer>(e)
                .unwrap()
                .text()
                .starts_with("shared "));
        }
    }

    #[test]
    fn native_and_capture_share_shaped_typography() {
        let app = demo_app();
        assert!(app.world().has_resource::<Arc<dyn TextEngine>>());
        assert!(!app.world().has_resource::<TextFace>());
        let engine = app.world().resource::<Arc<dyn TextEngine>>().unwrap();
        assert!(engine.as_any().is::<rndr_render::ParleyText>());
    }

    #[test]
    fn layout_keeps_editor_and_commands_visible_at_default_and_minimum_sizes() {
        for (width, height) in [(800.0, 600.0), (640.0, 540.0), (1000.0, 720.0)] {
            let mut app = demo_app();
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            for _ in 0..3 {
                let _ = rndr_paint::frame(&mut app);
            }
            let world = app.world();
            let editor = world.resource::<Focus>().unwrap().entity.unwrap();
            let rect = world.get::<Rect>(editor).unwrap();
            assert!(
                rect.h >= 180.0,
                "editor too short at {width}x{height}: {rect:?}"
            );
            let style = world.get::<TextStyle>(editor).unwrap();
            assert_eq!(style.size, 15.0);
            for entity in world.entities_with::<rndr_core::Button>() {
                let rect = world.get::<Rect>(entity).unwrap();
                assert!(rect.x >= 0.0 && rect.y >= 0.0);
                assert!(
                    rect.x + rect.w <= width && rect.y + rect.h <= height,
                    "command clipped at {width}x{height}: {rect:?}"
                );
            }
            let host = world.resource::<FakeHost>().unwrap();
            let detail = world.get::<Rect>(host.detail).unwrap();
            assert!(detail.y + detail.h <= height);
        }
    }

    #[test]
    fn extension_layout_keeps_source_and_tools_reachable_at_supported_sizes() {
        for split in [false, true] {
            for (width, height) in [(1000.0, 800.0), (800.0, 720.0), (1200.0, 900.0)] {
                let mut app = demo_app_with_split(split);
                enable_extensions(&mut app);
                let config = app.world().resource::<WindowConfig>().unwrap();
                let minimum = config.min_size.unwrap();
                assert!(width >= minimum.width && height >= minimum.height);
                app.world_mut()
                    .insert_resource(Viewport::new(width, height));
                for _ in 0..3 {
                    let _ = rndr_paint::frame(&mut app);
                }
                let world = app.world();
                for editor in world.entities_with::<Editor>() {
                    let rect = world.get::<Rect>(editor).unwrap();
                    assert!(
                        rect.h >= 180.0 && rect.w >= 180.0,
                        "extension source too small at {width}x{height}, split={split}: {rect:?}"
                    );
                }
                for button in world.entities_with::<rndr_core::Button>() {
                    let rect = world.get::<Rect>(button).unwrap();
                    assert!(
                        rect.x >= 0.0
                            && rect.y >= 0.0
                            && rect.x + rect.w <= width
                            && rect.y + rect.h <= height,
                        "extension command clipped at {width}x{height}, split={split}: {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn readonly_control_preserves_navigation_and_updates_visible_status() {
        let mut app = demo_app();
        setup(app.world_mut());
        command(&mut app, 5);
        let host = app.world().resource::<FakeHost>().unwrap();
        assert!(app
            .world()
            .get::<Label>(host.status)
            .unwrap()
            .text
            .contains("Read only"));
        assert!(app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(host.document)
            .unwrap()
            .is_readonly());
        command(&mut app, 5);
        let host = app.world().resource::<FakeHost>().unwrap();
        assert!(app
            .world()
            .get::<Label>(host.status)
            .unwrap()
            .text
            .contains("Editable"));
    }

    #[test]
    fn delayed_save_acknowledges_capture_without_replacing_later_source() {
        let mut app = App::new();
        app.add_plugin(UiPlugin);
        setup(app.world_mut());
        command(&mut app, 1); // first host edit
        command(&mut app, 2); // capture
        let captured = app
            .world()
            .resource::<FakeHost>()
            .unwrap()
            .pending
            .clone()
            .unwrap();
        command(&mut app, 1); // newer local session contents
        let document = app.world().resource::<FakeHost>().unwrap().document;
        let before_ack = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .snapshot();
        command(&mut app, 3); // delayed completion
        let host = app.world().resource::<FakeHost>().unwrap();
        assert_eq!(host.saved.text, captured.text);
        assert_eq!(host.saved.revision, captured.revision);
        assert!(host.pending.is_none());
        let session = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap();
        assert_eq!(session.text(), before_ack.text);
        assert_eq!(session.revision(), before_ack.revision);
        assert_ne!(session.text(), host.saved.text);
        assert!(app
            .world()
            .get::<Label>(host.status)
            .unwrap()
            .text
            .contains("Unsaved changes"));
        command(&mut app, 4); // explicit reload of acknowledged bytes
        let session = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap();
        assert_eq!(session.text(), captured.text);
        assert!(session.revision().0 > before_ack.revision.0);
    }
}
