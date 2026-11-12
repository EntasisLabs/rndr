//! Conservative correspondence must not guess among equal occurrences.

#[test]
fn f0_explicit_operations_table_mounted_and_offscreen() {
    for offscreen in [false, true] {
        for keyed in [false, true] {
            for operation in [
                "empty_append",
                "complete_pending",
                "reset_block",
                "reset_surface",
                "remove_pump_reinsert",
                "destroy_recreate",
            ] {
                let filler: String = (0..80).map(|i| format!("Paragraph {i}.\n\n")).collect();
                let source = format!("{A}\n{filler}");
                let (mut app, mut view) = boot(&source);
                let mut surface = liquid_surface_handle(app.world(), view).unwrap();
                let map = LiquidKeyMap {
                    keys: vec![Some("a".into())],
                };
                if keyed {
                    set_liquid_source_keyed(
                        app.world_mut(),
                        surface,
                        LiquidSourceRevision::ZERO,
                        &source,
                        &map,
                    )
                    .unwrap();
                }
                let old_surface = surface;
                let old = liquid_component_handle(app.world(), surface, 0).unwrap();
                let record = put_liquid_component_state_for(
                    app.world_mut(),
                    old,
                    0,
                    LiquidComponentState::Accordion { open: vec![0] },
                )
                .unwrap();
                for _ in 0..2 {
                    rndr_paint::frame(&mut app);
                }
                if offscreen {
                    let bottom = (app.world().get::<Rect>(view).unwrap().h
                        - app.world().get::<Scroll>(view).unwrap().content[1])
                        .min(0.0);
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
                let before = liquid_source_revision(app.world(), surface).unwrap();
                let fresh = match operation {
                    "empty_append" => {
                        push_liquid_str(app.world_mut(), view, "");
                        assert_eq!(
                            liquid_source_revision(app.world(), surface).unwrap(),
                            before
                        );
                        old
                    }
                    "complete_pending" => {
                        push_liquid_str(
                            app.world_mut(),
                            view,
                            "\n```accordion\ntitle: Pending\n---\nlabel: New\nbody: New body\n",
                        );
                        assert_eq!(
                            liquid_component_handle(app.world(), surface, 0).unwrap(),
                            old
                        );
                        assert!(liquid_component_handle(app.world(), surface, 1).is_err());
                        push_liquid_str(app.world_mut(), view, "```\n");
                        let new = liquid_component_handle(app.world(), surface, 1).unwrap();
                        assert_ne!(new, old);
                        assert_eq!(liquid_component_state_for(app.world(), new).unwrap(), None);
                        assert_ne!(
                            liquid_source_revision(app.world(), surface).unwrap(),
                            before
                        );
                        old
                    }
                    "reset_block" => {
                        let new = reset_liquid_block(app.world_mut(), old, 1).unwrap();
                        assert_ne!(
                            liquid_source_revision(app.world(), surface).unwrap(),
                            before
                        );
                        new
                    }
                    "reset_surface" => {
                        surface = reset_liquid_surface(app.world_mut(), surface, before).unwrap();
                        assert_ne!(surface, old_surface);
                        assert_eq!(
                            liquid_source_revision(app.world(), surface).unwrap(),
                            LiquidSourceRevision::ZERO
                        );
                        liquid_component_handle(app.world(), surface, 0).unwrap()
                    }
                    "remove_pump_reinsert" => {
                        if keyed {
                            set_liquid_source_keyed(
                                app.world_mut(),
                                surface,
                                before,
                                &filler,
                                &LiquidKeyMap::default(),
                            )
                            .unwrap();
                        } else {
                            set_liquid_source(app.world_mut(), view, &filler);
                        }
                        for _ in 0..2 {
                            rndr_paint::frame(&mut app);
                        }
                        assert!(liquid_component_state_for(app.world(), old).is_err());
                        if keyed {
                            let rev = liquid_source_revision(app.world(), surface).unwrap();
                            set_liquid_source_keyed(app.world_mut(), surface, rev, &source, &map)
                                .unwrap();
                        } else {
                            set_liquid_source(app.world_mut(), view, &source);
                        }
                        liquid_component_handle(app.world(), surface, 0).unwrap()
                    }
                    "destroy_recreate" => {
                        app.world_mut().despawn_entity(view);
                        let replacement = liquid_view(app.world_mut(), &source);
                        assert_ne!(replacement, view);
                        view = replacement;
                        app.world_mut()
                            .get_mut::<MarkdownView>(view)
                            .unwrap()
                            .set_follow(false);
                        surface = liquid_surface_handle(app.world(), view).unwrap();
                        assert_ne!(surface, old_surface);
                        if keyed {
                            set_liquid_source_keyed(
                                app.world_mut(),
                                surface,
                                LiquidSourceRevision::ZERO,
                                &source,
                                &map,
                            )
                            .unwrap();
                        }
                        liquid_component_handle(app.world(), surface, 0).unwrap()
                    }
                    _ => unreachable!(),
                };
                let preserved = matches!(operation, "empty_append" | "complete_pending");
                if preserved {
                    assert_eq!(
                        fresh, old,
                        "{operation} keyed={keyed} offscreen={offscreen}"
                    );
                    assert_eq!(
                        liquid_component_state_for(app.world(), fresh).unwrap(),
                        Some(record)
                    );
                } else {
                    assert_ne!(
                        fresh, old,
                        "{operation} keyed={keyed} offscreen={offscreen}"
                    );
                    assert!(liquid_component_state_for(app.world(), old).is_err());
                    assert_eq!(
                        liquid_component_state_for(app.world(), fresh).unwrap(),
                        None
                    );
                }
                assert!(liquid_timers(app.world(), view).is_empty());
                app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
                for _ in 0..3 {
                    rndr_paint::frame(&mut app);
                }
                assert_eq!(
                    liquid_component_handle(app.world(), surface, 0).unwrap(),
                    fresh
                );
                if !preserved {
                    assert_eq!(
                        liquid_component_state_for(app.world(), fresh).unwrap(),
                        None
                    );
                }
                app.world_mut().despawn_entity(view);
                for _ in 0..3 {
                    rndr_paint::frame(&mut app);
                }
                assert!(liquid_source_revision(app.world(), surface).is_err());
                assert!(liquid_component_states(app.world(), view).is_empty());
            }
        }
    }
}

#[test]
fn f0_identity_table_mounted_and_offscreen() {
    #[derive(Debug)]
    struct Row {
        name: &'static str,
        old: String,
        old_keys: Option<Vec<Option<&'static str>>>,
        new: String,
        new_keys: Option<Vec<Option<&'static str>>>,
        from: Vec<Option<usize>>,
        reset: bool,
    }
    let b = A.replace("FAQ", "Other");
    let changed = A.replace("Body", "Edited");
    let two = format!("{A}\n{A}");
    let rows = vec![
        Row {
            name: "exact noop duplicates",
            old: two.clone(),
            old_keys: None,
            new: two.clone(),
            new_keys: None,
            from: vec![Some(0), Some(1)],
            reset: false,
        },
        Row {
            name: "true append duplicates",
            old: two.clone(),
            old_keys: None,
            new: format!("{two}\n{A}"),
            new_keys: None,
            from: vec![Some(0), Some(1), None],
            reset: false,
        },
        Row {
            name: "nonappend prefix duplicates",
            old: two.clone(),
            old_keys: None,
            new: format!("Prefix\n\n{two}"),
            new_keys: None,
            from: vec![None, None],
            reset: false,
        },
        Row {
            name: "duplicate contraction",
            old: two.clone(),
            old_keys: None,
            new: A.into(),
            new_keys: None,
            from: vec![None],
            reset: false,
        },
        Row {
            name: "duplicate expansion nonappend",
            old: A.into(),
            old_keys: None,
            new: format!("Prefix\n\n{two}"),
            new_keys: None,
            from: vec![None, None],
            reset: false,
        },
        Row {
            name: "unique reorder",
            old: format!("{A}\n{b}"),
            old_keys: None,
            new: format!("{b}\n{A}"),
            new_keys: None,
            from: vec![Some(1), Some(0)],
            reset: false,
        },
        Row {
            name: "unique prefix",
            old: A.into(),
            old_keys: None,
            new: format!("Prefix\n\n{A}"),
            new_keys: None,
            from: vec![Some(0)],
            reset: false,
        },
        Row {
            name: "semantic edit fallback",
            old: A.into(),
            old_keys: None,
            new: changed.clone(),
            new_keys: None,
            from: vec![None],
            reset: false,
        },
        Row {
            name: "duplicate and unique mixed reorder",
            old: format!("{two}\n{b}"),
            old_keys: None,
            new: format!("{b}\n{two}"),
            new_keys: None,
            from: vec![Some(2), None, None],
            reset: false,
        },
        Row {
            name: "keyed duplicates reorder",
            old: two.clone(),
            old_keys: Some(vec![Some("a"), Some("b")]),
            new: format!("Prefix\n\n{two}"),
            new_keys: Some(vec![Some("b"), Some("a")]),
            from: vec![Some(1), Some(0)],
            reset: false,
        },
        Row {
            name: "keyed child schema edit",
            old: A.into(),
            old_keys: Some(vec![Some("a")]),
            new: changed,
            new_keys: Some(vec![Some("a")]),
            from: vec![Some(0)],
            reset: true,
        },
        Row {
            name: "keyed title edit",
            old: A.into(),
            old_keys: Some(vec![Some("a")]),
            new: b.clone(),
            new_keys: Some(vec![Some("a")]),
            from: vec![Some(0)],
            reset: false,
        },
        Row {
            name: "keyed to unkeyed",
            old: A.into(),
            old_keys: Some(vec![Some("a")]),
            new: A.into(),
            new_keys: Some(vec![None]),
            from: vec![None],
            reset: false,
        },
        Row {
            name: "unkeyed to keyed",
            old: A.into(),
            old_keys: None,
            new: A.into(),
            new_keys: Some(vec![Some("a")]),
            from: vec![None],
            reset: false,
        },
        Row {
            name: "keyed kind change",
            old: A.into(),
            old_keys: Some(vec![Some("a")]),
            new: "```feed\nid: news\ndatatype: text\n```\n".into(),
            new_keys: Some(vec![Some("a")]),
            from: vec![None],
            reset: false,
        },
        Row {
            name: "rekey same payload",
            old: A.into(),
            old_keys: Some(vec![Some("a")]),
            new: A.into(),
            new_keys: Some(vec![Some("b")]),
            from: vec![None],
            reset: false,
        },
        Row {
            name: "pending fence append",
            old: A.into(),
            old_keys: None,
            new: format!("{A}\n```accordion\ntitle: Pending\n"),
            new_keys: None,
            from: vec![Some(0)],
            reset: false,
        },
        Row {
            name: "CRLF normalized noop",
            old: A.into(),
            old_keys: None,
            new: A.replace('\n', "\r\n"),
            new_keys: None,
            from: vec![Some(0)],
            reset: false,
        },
    ];
    for offscreen in [false, true] {
        for row in &rows {
            let filler: String = (0..80).map(|i| format!("Paragraph {i}.\n\n")).collect();
            let initial = format!("{}\n{filler}", row.old);
            let (mut app, view) = boot(&initial);
            let s = liquid_surface_handle(app.world(), view).unwrap();
            let map = |keys: &Vec<Option<&str>>| LiquidKeyMap {
                keys: keys.iter().map(|k| k.map(str::to_owned)).collect(),
            };
            if let Some(keys) = &row.old_keys {
                let revision = liquid_source_revision(app.world(), s).unwrap();
                set_liquid_source_keyed(app.world_mut(), s, revision, &initial, &map(keys))
                    .unwrap();
            }
            let count = inspect_liquid_occurrences(&initial).unwrap().len();
            let handles: Vec<_> = (0..count)
                .map(|i| liquid_component_handle(app.world(), s, i).unwrap())
                .collect();
            for h in &handles {
                put_liquid_component_state_for(
                    app.world_mut(),
                    *h,
                    0,
                    LiquidComponentState::Accordion { open: vec![0] },
                )
                .unwrap();
            }
            for _ in 0..2 {
                rndr_paint::frame(&mut app);
            }
            if offscreen {
                let scroll = app.world().get::<Scroll>(view).unwrap();
                let height = app.world().get::<Rect>(view).unwrap().h;
                app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] =
                    (height - scroll.content[1]).min(0.0);
                for _ in 0..2 {
                    rndr_paint::frame(&mut app);
                }
                assert!(
                    app.world()
                        .get::<LiquidView>(view)
                        .unwrap()
                        .mounted()
                        .is_empty(),
                    "{row:?}"
                );
            }
            let next = if row.name == "true append duplicates" {
                format!("{initial}\n{A}")
            } else if row.name == "pending fence append" {
                format!("{initial}\n```accordion\ntitle: Pending\n")
            } else {
                format!("{}\n{filler}", row.new)
            };
            let before_rev = liquid_source_revision(app.world(), s).unwrap();
            if let Some(keys) = &row.new_keys {
                set_liquid_source_keyed(app.world_mut(), s, before_rev, &next, &map(keys)).unwrap();
            } else {
                set_liquid_source(app.world_mut(), view, &next);
            }
            let after_rev = liquid_source_revision(app.world(), s).unwrap();
            assert_eq!(
                before_rev == after_rev,
                initial == next.replace("\r\n", "\n") && row.old_keys == row.new_keys,
                "revision {row:?}"
            );
            for (n, prior) in row.from.iter().enumerate() {
                let current = liquid_component_handle(app.world(), s, n).unwrap();
                let record = liquid_component_state_for(app.world(), current).unwrap();
                if let Some(p) = prior {
                    assert_eq!(
                        current, handles[*p],
                        "serial {row:?}, offscreen={offscreen}"
                    );
                    let record = record.expect("preserved or reset materialized record");
                    assert_eq!(record.revision, if row.reset { 2 } else { 1 }, "{row:?}");
                    assert_eq!(
                        record.state,
                        LiquidComponentState::Accordion {
                            open: if row.reset { vec![] } else { vec![0] }
                        },
                        "{row:?}"
                    );
                } else {
                    assert!(!handles.contains(&current), "fresh {row:?}");
                    assert_eq!(record, None, "no inherited state {row:?}");
                }
            }
            for (i, old) in handles.iter().enumerate() {
                if !row.from.contains(&Some(i)) {
                    assert!(
                        liquid_component_state_for(app.world(), *old).is_err(),
                        "retirement {row:?}"
                    );
                }
            }
            assert!(liquid_timers(app.world(), view).is_empty());
            app.world_mut().despawn_entity(view);
            for _ in 0..3 {
                rndr_paint::frame(&mut app);
            }
            assert!(liquid_source_revision(app.world(), s).is_err());
            assert!(liquid_component_states(app.world(), view).is_empty());
        }
    }
}

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
