use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Recorder {
    events: Arc<Mutex<Vec<LiquidEvent>>>,
    feed: Vec<FeedEntry>,
}

impl LiquidHost for Recorder {
    fn event(&mut self, _: &mut World, event: LiquidEvent) {
        self.events.lock().unwrap().push(event);
    }
    fn feed(&mut self, _: &mut World, feed_id: &str, _: &str) -> Vec<FeedEntry> {
        if feed_id == "news" {
            self.feed.clone()
        } else {
            Vec::new()
        }
    }
    fn tile(&mut self, _: &mut World, feed: &str, field: Option<&str>) -> Option<String> {
        (feed == "metrics" && field == Some("users")).then(|| "1,204".into())
    }
}

fn setup(source: &str, host: Recorder) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(host));
    app.world_mut().insert_resource(Viewport::new(720., 2000.));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    (app, view)
}

fn under(world: &World, root: Entity, out: &mut Vec<Entity>) {
    for c in children_of(world, root) {
        out.push(c);
        under(world, c, out);
    }
}

fn texts(world: &World, root: Entity) -> Vec<String> {
    let mut all = Vec::new();
    under(world, root, &mut all);
    all.iter()
        .filter_map(|e| world.get::<Label>(*e).map(|l| l.text.clone()))
        .collect()
}

fn button(world: &World, root: Entity, label: &str) -> Entity {
    let mut all = Vec::new();
    under(world, root, &mut all);
    let label = all
        .into_iter()
        .find(|e| {
            world
                .get::<Label>(*e)
                .is_some_and(|l| rndr_core::strip_icon_scalars(&l.text) == label)
        })
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", texts(world, root)));
    label
}

fn press(app: &mut App, root: Entity, label: &str) -> bool {
    let mut at = button(app.world(), root, label);
    while try_activate(app.world_mut(), at) != ActivationResult::Accepted {
        at = app
            .world()
            .get::<Parent>(at)
            .expect("an activatable ancestor")
            .entity;
    }
    let mut handled = false;
    for request in take_action_intents(app.world_mut()) {
        handled |= dispatch(app.world_mut(), &request);
    }
    rndr_paint::frame(app);
    handled
}

const DOC: &str = "\
# Title

Plain paragraph.

```callout
tone: warn
title: Careful
body: Hot **surface**.
```

```actions
Approve | approve
{{icon:x}} Reject
```

Between.

```code
lang: rust
---
fn main() {}
```
";

#[test]
fn blocks_mount_in_place_of_their_references() {
    let (app, view) = setup(DOC, Recorder::default());
    let liquid = app.world().get::<LiquidView>(view).unwrap();
    assert_eq!(liquid.blocks().len(), 3);
    assert_eq!(liquid.mounted(), vec![0, 1, 2]);
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t.ends_with("Careful")), "{all:?}");
    for want in ["Approve", "Reject", "Copy", "fn main() {}", "Between."] {
        assert!(
            all.iter().any(|t| rndr_core::strip_icon_scalars(t) == want),
            "{want:?} missing from {all:?}"
        );
    }
    assert!(
        all.iter()
            .any(|t| t.contains(rndr_core::Icon::X.scalar()) && t.contains("Reject")),
        "reject should draw the x glyph, got {all:?}"
    );
    assert!(!all.iter().any(|t| t.contains('\u{E001}')), "{all:?}");
    let reject = button(app.world(), view, "Reject");
    let mut named = reject;
    let name = loop {
        if let Some(semantics) = app.world().get::<Semantics>(named) {
            break semantics.name.clone();
        }
        named = app
            .world()
            .get::<Parent>(named)
            .expect("button root")
            .entity;
    };
    assert_eq!(name.as_deref(), Some("Reject"));
    let md = app.world().get::<MarkdownView>(view).unwrap();
    let ys: Vec<f32> = children_of(app.world(), view)
        .into_iter()
        .map(|c| app.world().get::<Rect>(c).unwrap().y)
        .collect();
    assert!(ys.windows(2).all(|w| w[0] <= w[1]), "{ys:?}");
    assert_eq!(md.blocks().len(), 6);
}

#[test]
fn every_allowlisted_icon_has_a_baked_glyph() {
    for id in rndr_liquid::ICON_ALLOWLIST {
        let canonical = rndr_liquid::normalize_icon_id(id).unwrap_or_else(|| panic!("{id}"));
        assert!(
            rndr_core::Icon::from_name(&canonical).is_some(),
            "{id} -> {canonical} has no glyph"
        );
    }
}

#[test]
fn nested_markdown_bodies_render_inside_the_block() {
    let (app, view) = setup(DOC, Recorder::default());
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t == "Hot surface."), "{all:?}");
}

#[test]
fn host_events_carry_the_document_intent() {
    let host = Recorder::default();
    let events = Arc::clone(&host.events);
    let (mut app, view) = setup(DOC, host);
    assert!(press(&mut app, view, "Approve"));
    assert!(press(&mut app, view, "Reject"));
    assert!(press(&mut app, view, "Copy"));
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            LiquidEvent::Action {
                label: "Approve".into(),
                intent: Some("approve".into())
            },
            LiquidEvent::Action {
                label: "Reject".into(),
                intent: None
            },
            LiquidEvent::Copy {
                text: "fn main() {}".into()
            },
        ]
    );
}

#[test]
fn accordion_and_carousel_state_is_local_and_survives_rebuild() {
    let src = "\
```accordion
title: FAQ

---
label: One
body: First body
---
label: Two
body: Second body
```

```carousel
---
title: Alpha
---
title: Beta
```
";
    let host = Recorder::default();
    let events = Arc::clone(&host.events);
    let (mut app, view) = setup(src, host);
    let has = |app: &App, t: &str| texts(app.world(), view).iter().any(|x| x == t);
    assert!(!has(&app, "First body"));
    assert!(press(&mut app, view, "+ One"));
    assert!(has(&app, "First body"));
    assert!(press(&mut app, view, "+ Two"));
    assert!(has(&app, "Second body") && !has(&app, "First body"));
    assert!(has(&app, "Alpha") && !has(&app, "Beta"));
    assert!(press(&mut app, view, "Next"));
    assert!(has(&app, "Beta") && has(&app, "2 / 2"));
    refresh(app.world_mut());
    rndr_paint::frame(&mut app);
    assert!(has(&app, "Beta") && has(&app, "Second body"));
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn source_replacement_remounts_and_foreign_intents_are_ignored() {
    let (mut app, view) = setup(DOC, Recorder::default());
    set_liquid_source(
        app.world_mut(),
        view,
        "Only text\n\n```chips\n- label: Rust\n- label: Fast\n```\n",
    );
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t == "Rust"), "{all:?}");
    assert!(!all.iter().any(|t| t == "Careful"), "{all:?}");
    assert_eq!(
        app.world().get::<LiquidView>(view).unwrap().mounted(),
        vec![0]
    );
    let stray = app.world_mut().spawn((Layout::column(),));
    let request = ActionIntent {
        entity: stray,
        name: "activate".into(),
    };
    assert!(!dispatch(app.world_mut(), &request));
}

fn label_entity(world: &World, root: Entity, text: &str) -> Entity {
    let mut all = Vec::new();
    under(world, root, &mut all);
    all.into_iter()
        .find(|e| world.get::<Label>(*e).is_some_and(|l| l.text == text))
        .unwrap_or_else(|| panic!("no {text:?}"))
}

const STATE: &str = "\
```accordion
title: FAQ

---
label: One
body: First body
---
label: Two
body: Second body
```

```carousel
---
title: Alpha
---
title: Beta
```

```slides
---
label: Deck one
body: Slide one
---
label: Deck two
body: Slide two
```

```steps
title: Build

---
label: Mix
body: Stir
---
label: Bake
body: Heat
```

```tabs
---
label: One
body: First panel
---
label: Two
body: Second panel
```
";

#[test]
fn reader_state_survives_appends_and_unchanged_blocks() {
    let (mut app, view) = setup(STATE, Recorder::default());
    let has = |app: &App, text: &str| texts(app.world(), view).iter().any(|t| t == text);
    assert!(press(&mut app, view, "+ Two"));
    assert!(press(&mut app, view, "Next"));
    assert!(press(&mut app, view, "Next"));
    assert!(press(&mut app, view, "[ ]"));
    let tab = label_entity(app.world(), view, "Two");
    assert!(has(&app, "Second body"));
    assert!(has(&app, "Beta"));
    assert!(has(&app, "Deck two"));
    assert!(has(&app, "[x]"));
    let body = label_entity(app.world(), view, "Second body");
    let beta = label_entity(app.world(), view, "Beta");
    let slide = label_entity(app.world(), view, "Deck two");
    let checked = label_entity(app.world(), view, "[x]");
    let panel = label_entity(app.world(), view, "Second panel");
    assert!(app.world().is_alive(tab));
    push_liquid_str(app.world_mut(), view, "\n\nTail after the blocks.\n");
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert!(app.world().is_alive(body));
    assert!(app.world().is_alive(beta));
    assert!(app.world().is_alive(slide));
    assert!(app.world().is_alive(checked));
    assert!(app.world().is_alive(panel));
    assert!(app.world().is_alive(tab));
    assert!(has(&app, "Second body") && has(&app, "Beta") && has(&app, "Deck two"));
    assert!(has(&app, "Tail after the blocks."));
    let rewritten = STATE.replacen("FAQ", "Questions", 1);
    set_liquid_source(app.world_mut(), view, &rewritten);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    // F1 provisional whole-parsed-value correspondence: a title edit retires
    // the old accordion lifetime, unlike the unchanged appended prefix above.
    assert!(
        !app.world().is_alive(body),
        "retired body projection survived"
    );
    assert!(
        !has(&app, "Second body"),
        "changed accordion inherited open state"
    );
    assert!(has(&app, "+ Two"), "new lifetime did not start closed");
    assert!(has(&app, "Beta"));
    assert!(has(&app, "Deck two"));
    assert!(has(&app, "[x]"));
    assert!(has(&app, "Second panel"));
}

#[test]
fn an_unclosed_fence_is_pending_until_it_closes() {
    let (mut app, view) = setup("Before the fence.\n\n", Recorder::default());
    push_liquid_str(app.world_mut(), view, "```callout\ntone: tip\nbody: Soon\n");
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let md = app.world().get::<MarkdownView>(view).unwrap();
    assert!(
        md.blocks().iter().all(|block| {
            !(matches!(block.kind, MarkdownKind::Code) && block.language == "callout")
        }),
        "open fence rendered as code: {:?}",
        md.blocks()
    );
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t == "Pending callout"), "{all:?}");
    assert!(!all.iter().any(|t| t == "Soon"), "{all:?}");
    push_liquid_str(app.world_mut(), view, "```\n");
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t == "Soon"), "{all:?}");
    assert!(!all.iter().any(|t| t == "Pending callout"), "{all:?}");
    assert_eq!(
        app.world().get::<LiquidView>(view).unwrap().mounted(),
        vec![0]
    );
}

#[test]
fn chart_paint_slots_are_not_leaked_across_rebuilds() {
    let chart = "\
```chart
type: bar
title: Users
| Month | Users |
|---|---|
| Jan | 3 |
| Feb | 4 |
```
";
    let (mut app, view) = setup(chart, Recorder::default());
    let slots = |app: &App| {
        app.world()
            .resource::<rndr_paint::CanvasRegistry>()
            .map(|registry| registry.len())
            .unwrap_or(0)
    };
    assert_eq!(slots(&app), 1);
    for _ in 0..12 {
        push_liquid_str(app.world_mut(), view, " more");
        rndr_paint::frame(&mut app);
        refresh(app.world_mut());
        rndr_paint::frame(&mut app);
    }
    assert_eq!(slots(&app), 1, "append and refresh grew chart slots");
    set_liquid_source(app.world_mut(), view, "No chart left.\n");
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert_eq!(slots(&app), 0, "removing the chart left its paint slot");
    set_liquid_source(app.world_mut(), view, chart);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    assert_eq!(slots(&app), 1, "a new chart did not reuse the freed slot");
}

#[test]
fn feeds_and_tiles_come_from_the_host() {
    let host = Recorder {
        feed: vec![FeedEntry {
            title: "Launch".into(),
            detail: Some("Shipped today".into()),
            meta: None,
        }],
        ..Recorder::default()
    };
    let events = Arc::clone(&host.events);
    let src = "\
```feed
id: news
datatype: md
title: News
```

```dashboard
title: Ops

---
label: Users
value: 10
feed: metrics
field: users
---
label: Errors
value: 3
```
";
    let (mut app, view) = setup(src, host);
    let all = texts(app.world(), view);
    for want in ["News", "Launch", "Shipped today", "1,204", "3"] {
        assert!(
            all.iter().any(|t| t == want),
            "{want:?} missing from {all:?}"
        );
    }
    assert!(press(&mut app, view, "Refresh"));
    assert_eq!(
        *events.lock().unwrap(),
        vec![LiquidEvent::RefreshFeed {
            feed_id: "news".into()
        }]
    );
}

#[test]
fn charts_paint_through_a_surface_slot_and_report_hover() {
    let src = "\
```chart
type: bar
title: Weekly

| Day | Visits |
|---|---|
| Mon | 3 |
| Tue | 7 |
| Wed | 5 |
```
";
    let (mut app, view) = setup(src, Recorder::default());
    let all = texts(app.world(), view);
    assert!(all.iter().any(|t| t == "Weekly"), "{all:?}");
    let mut nodes = Vec::new();
    under(app.world(), view, &mut nodes);
    let plot = nodes
        .into_iter()
        .find(|e| app.world().has::<SurfaceSlot>(*e))
        .expect("plot surface");
    let rect = *app.world().get::<Rect>(plot).unwrap();
    assert!(rect.w > 100. && rect.h > 100., "{rect:?}");
    assert!(app.world().resource::<MotionWatch>().unwrap().active);
    let scene = rndr_paint::frame(&mut app);
    assert!(!scene.draws.is_empty(), "chart painted nothing");
    let mut found = None;
    'scan: for y in (0..rect.h as i32).step_by(6) {
        for x in (0..rect.w as i32).step_by(6) {
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .pointer
                .position = [rect.x + x as f32, rect.y + y as f32];
            rndr_paint::frame(&mut app);
            let all = texts(app.world(), view);
            if let Some(t) = all.iter().find(|t| t.contains("Visits: ")) {
                found = Some(t.clone());
                break 'scan;
            }
        }
    }
    let tip = found.expect("hovering a bar shows its tooltip row");
    assert!(
        tip.starts_with("Mon") || tip.starts_with("Tue") || tip.starts_with("Wed"),
        "{tip}"
    );
}
