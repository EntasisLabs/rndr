//! Headless checks for the visual-fidelity slice: styled prose, code tokens
//! and the code role, the chart tooltip overlay, and slide image layers.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};

fn setup(source: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
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
    for child in children_of(world, root) {
        out.push(child);
        under(world, child, out);
    }
}

fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut all = Vec::new();
    under(world, root, &mut all);
    all
}

fn label_entity(world: &World, root: Entity, text: &str) -> Entity {
    descendants(world, root)
        .into_iter()
        .find(|e| world.get::<Label>(*e).is_some_and(|l| l.text == text))
        .unwrap_or_else(|| {
            let texts: Vec<_> = descendants(world, root)
                .iter()
                .filter_map(|e| world.get::<Label>(*e).map(|l| l.text.clone()))
                .collect();
            panic!("no {text:?} in {texts:?}")
        })
}

#[test]
fn styled_block_sets_family_size_alignment_and_line_height() {
    let (app, view) = setup(
        "\
```block
font: mono
size: lg
align: center
spacing: tight
---
Hello styled
```
",
    );
    let entity = label_entity(app.world(), view, "Hello styled");
    let style = app.world().get::<TextStyle>(entity).unwrap();
    assert_eq!(style.family, "monospace");
    assert_eq!(style.size, 18.0);
    assert_eq!(style.align, TextAlign::Center);
    assert_eq!(style.line_height, Some(1.25));
}

#[test]
fn styled_block_justify_uses_the_start_edge() {
    let (app, view) = setup(
        "\
```block
font: serif
size: sm
align: justify
---
Hello justified
```
",
    );
    let entity = label_entity(app.world(), view, "Hello justified");
    let style = app.world().get::<TextStyle>(entity).unwrap();
    assert_eq!(style.family, "serif");
    assert_eq!(style.size, 14.0);
    assert_eq!(style.align, TextAlign::Start);
    assert_eq!(style.line_height, None);
}

#[test]
fn code_block_colors_tokens_and_exposes_a_code_role() {
    let source = "\
```code
lang: rust
---
fn main() {
    let n = 1; // count
}
```
";
    let (app, view) = setup(source);
    let world = app.world();
    let code = descendants(world, view)
        .into_iter()
        .find(|e| {
            world
                .get::<AccessMark>(*e)
                .is_some_and(|mark| mark.0 == AccessRole::Code)
        })
        .expect("code role");
    let text = world.get::<Label>(code).unwrap().text.clone();
    assert!(text.starts_with("fn main()"), "{text}");
    let theme = MarkdownTheme::default();
    let spans = &world.get::<Spans>(code).unwrap().runs;
    let slice = |span: &TextSpan| text.get(span.start..span.end);
    assert!(
        spans
            .iter()
            .any(|span| slice(span) == Some("fn") && span.color == theme.keyword),
        "{spans:?}"
    );
    assert!(spans
        .iter()
        .any(|span| slice(span) == Some("1") && span.color == theme.number));
    assert!(spans.iter().any(|span| {
        slice(span).is_some_and(|part| part.starts_with("//")) && span.color == theme.comment
    }));
    let node = access_tree(world)
        .nodes
        .into_iter()
        .find(|node| node.id == access_id(code))
        .expect("code in the access tree");
    assert_eq!(node.role, AccessRole::Code);
    assert_eq!(node.name, "rust");
    assert_eq!(node.value, text);
}

#[test]
fn slide_image_layer_is_pinned_in_the_stage_and_named() {
    let (app, view) = setup(
        "\
```slides
title: Deck

---
label: One
layer: photo
  src: pic.png
  x: 0.25
  y: 0.5
  w: 0.5
  h: 0.25

Hello slide
```
",
    );
    let world = app.world();
    let layer = descendants(world, view)
        .into_iter()
        .find(|e| {
            world
                .get::<AccessMark>(*e)
                .is_some_and(|mark| mark.0 == AccessRole::Image)
        })
        .expect("image layer");
    let stage = world.get::<Parent>(layer).unwrap().entity;
    let stage_rect = *world.get::<Rect>(stage).unwrap();
    let rect = *world.get::<Rect>(layer).unwrap();
    assert!(
        (stage_rect.w - 640.0).abs() < 0.5 && (stage_rect.h - 360.0).abs() < 0.5,
        "{stage_rect:?}"
    );
    assert!(
        (rect.x - (stage_rect.x + 160.0)).abs() < 0.5,
        "{rect:?} parent {stage_rect:?}"
    );
    assert!(
        (rect.y - (stage_rect.y + 180.0)).abs() < 0.5,
        "{rect:?} parent {stage_rect:?}"
    );
    assert!((rect.w - 320.0).abs() < 0.5, "{rect:?}");
    assert!((rect.h - 90.0).abs() < 0.5, "{rect:?}");
    let node = access_tree(world)
        .nodes
        .into_iter()
        .find(|node| node.id == access_id(layer))
        .expect("image in the access tree");
    assert_eq!(node.role, AccessRole::Image);
    assert_eq!(node.name, "photo");
    assert!(descendants(world, view).iter().any(|e| {
        world
            .get::<Label>(*e)
            .is_some_and(|label| label.text == "Hello slide" || label.text == "One")
    }));
}

#[test]
fn chart_tooltip_overlays_the_plot_and_describes_it() {
    let (mut app, view) = setup(
        "\
```chart
type: bar
title: Weekly

| Day | Visits |
|---|---|
| Mon | 3 |
| Tue | 7 |
| Wed | 5 |
```
",
    );
    let tip = descendants(app.world(), view)
        .into_iter()
        .find(|e| {
            app.world()
                .get::<AccessMark>(*e)
                .is_some_and(|mark| mark.0 == AccessRole::Tooltip)
        })
        .expect("tooltip");
    let plot = app.world().get::<Parent>(tip).unwrap().entity;
    assert!(app.world().has::<SurfaceSlot>(plot));
    assert!(app.world().has::<Hidden>(tip));
    assert!(app.world().get::<DescribedBy>(plot).is_none());
    assert!(app.world().get::<Absolute>(tip).is_some());
    let before = access_tree(app.world());
    assert!(before.nodes.iter().any(|node| {
        node.id == access_id(plot) && node.role == AccessRole::Group && node.name == "Weekly"
    }));
    assert!(!before
        .nodes
        .iter()
        .any(|node| node.role == AccessRole::Tooltip));

    let rect = *app.world().get::<Rect>(plot).unwrap();
    let mut found = false;
    'scan: for y in (0..rect.h as i32).step_by(6) {
        for x in (0..rect.w as i32).step_by(6) {
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .pointer
                .position = [rect.x + x as f32, rect.y + y as f32];
            rndr_paint::frame(&mut app);
            let text = app
                .world()
                .get::<Label>(tip)
                .map(|label| label.text.clone())
                .unwrap_or_default();
            if text.contains("Visits: ") {
                found = true;
                break 'scan;
            }
        }
    }
    assert!(found, "hovering a bar shows its tooltip");
    let world = app.world();
    assert!(!world.has::<Hidden>(tip));
    assert_eq!(world.get::<DescribedBy>(plot).unwrap().0, vec![tip]);
    let pin = world.get::<Absolute>(tip).unwrap();
    let left = pin.left.expect("tooltip left");
    let top = pin.top.expect("tooltip top");
    assert!((0.0..=rect.w).contains(&left), "{left} in {}", rect.w);
    assert!((0.0..=rect.h).contains(&top), "{top} in {}", rect.h);
    let tree = access_tree(world);
    let tip_node = tree
        .nodes
        .iter()
        .find(|node| node.id == access_id(tip))
        .expect("visible tooltip");
    assert_eq!(tip_node.role, AccessRole::Tooltip);
    assert!(tip_node.name.contains("Visits: "), "{}", tip_node.name);
    let plot_node = tree
        .nodes
        .iter()
        .find(|node| node.id == access_id(plot))
        .unwrap();
    assert!(plot_node.children.contains(&access_id(tip)));
}
