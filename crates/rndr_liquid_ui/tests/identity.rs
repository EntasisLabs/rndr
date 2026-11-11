//! Conservative correspondence must not guess among equal occurrences.
use rndr_core::*;
use rndr_liquid_ui::*;
use rndr_ui::{apply_theme_bridge, UiTheme};

const A: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: Body\n```\n";

fn boot(source: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(NoHost));
    app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    let view = liquid_view(app.world_mut(), source);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    for _ in 0..2 {
        rndr_paint::frame(&mut app);
    }
    (app, view)
}

fn open_first(app: &mut App, view: Entity) {
    let source = app.world().get::<MarkdownView>(view).unwrap().source();
    assert!(!source.is_empty());
    let mut at = app
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|e| {
            app.world()
                .get::<Label>(*e)
                .is_some_and(|l| l.text == "+ One")
        })
        .expect("accordion control");
    while try_activate(app.world_mut(), at) != ActivationResult::Accepted {
        at = app.world().get::<Parent>(at).unwrap().entity;
    }
    for request in take_action_intents(app.world_mut()) {
        dispatch(app.world_mut(), &request);
    }
    assert_eq!(liquid_component_states(app.world(), view).len(), 1);
}

fn ambiguous_replacement(offscreen: bool) {
    let filler: String = (0..80)
        .map(|i| format!("Paragraph {i} alpha beta gamma delta.\n\n"))
        .collect();
    let source = format!("{A}\n{A}\n{filler}");
    let (mut app, view) = boot(&source);
    open_first(&mut app, view);
    if offscreen {
        let scroll = app.world().get::<Scroll>(view).unwrap();
        let height = app.world().get::<Rect>(view).unwrap().h;
        let bottom = (height - scroll.content[1]).min(0.0);
        app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = bottom;
        for _ in 0..2 {
            rndr_paint::frame(&mut app);
        }
        assert!(app
            .world()
            .get::<LiquidView>(view)
            .unwrap()
            .mounted()
            .is_empty());
    }
    set_liquid_source(app.world_mut(), view, &format!("Prefix\n\n{source}"));
    assert!(
        liquid_component_states(app.world(), view).is_empty(),
        "ambiguous group inherited an old record (offscreen={offscreen})"
    );
}

#[test]
fn ambiguous_equal_group_resets_when_mounted() {
    ambiguous_replacement(false);
}

#[test]
fn ambiguous_equal_group_resets_when_offscreen() {
    ambiguous_replacement(true);
}

// F1's public enum is exhaustively matchable. Adding any F2 discriminant or
// marking it non_exhaustive breaks this unchanged downstream source pattern.
#[test]
fn legacy_update_error_remains_exhaustively_matchable() {
    fn classify(error: LiquidUpdateError) -> u8 {
        match error {
            LiquidUpdateError::StaleOwner => 0,
            LiquidUpdateError::Capacity => 1,
            LiquidUpdateError::Exhausted => 2,
        }
    }
    assert_eq!(classify(LiquidUpdateError::StaleOwner), 0);
    assert_eq!(classify(LiquidUpdateError::Capacity), 1);
    assert_eq!(classify(LiquidUpdateError::Exhausted), 2);
}

#[test]
fn ambiguous_group_preserves_on_exact_noop_and_true_append() {
    let source = format!("{A}\n{A}");
    let (mut app, view) = boot(&source);
    open_first(&mut app, view);
    let before = liquid_component_states(app.world(), view);
    set_liquid_source(app.world_mut(), view, &source);
    assert_eq!(liquid_component_states(app.world(), view), before);
    push_liquid_str(app.world_mut(), view, "\nTail\n");
    assert_eq!(liquid_component_states(app.world(), view), before);
}
