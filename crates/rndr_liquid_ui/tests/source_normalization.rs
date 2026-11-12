//! Complete-source setters must not reuse a chunk-sensitive raw parser projection.
use rndr_core::*;
use rndr_liquid::LiquidStream;
use rndr_liquid_ui::*;

const A: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: First\n  Second\n```\n";

#[test]
fn normalized_setter_repairs_opening_crlf_split() {
    let wire = A.replace('\n', "\r\n");
    let mut world = World::new();
    let view = liquid_view(&mut world, &wire[..13]);
    push_liquid_str(&mut world, view, &wire[13..]);
    let surface = liquid_surface_handle(&world, view).unwrap();
    let before = liquid_source_revision(&world, surface).unwrap();
    set_liquid_source(&mut world, view, A);
    assert_ne!(liquid_source_revision(&world, surface).unwrap(), before);
    assert_eq!(
        world.get::<LiquidView>(view).unwrap().blocks(),
        LiquidStream::new(A).blocks()
    );
    assert!(liquid_component_handle(&world, surface, 0).is_ok());
}

#[test]
fn normalized_prefix_setter_repairs_opening_crlf_split() {
    let wire = A.replace('\n', "\r\n");
    let mut world = World::new();
    let view = liquid_view(&mut world, &wire[..13]);
    push_liquid_str(&mut world, view, &wire[13..]);
    let extended = format!("{A}\n{A}");
    set_liquid_source(&mut world, view, &extended);
    assert_eq!(
        world.get::<LiquidView>(view).unwrap().blocks(),
        LiquidStream::new(&extended).blocks()
    );
}

#[test]
fn canonical_setters_cover_every_crlf_boundary_and_pending_fences() {
    // Every opening/body/closing CRLF split, not only CRLF after a finished fence.
    // Two equal completed blocks make accidental ambiguity resets observable.
    let mut cases = 0;
    for pending in [false, true] {
        let tail = if pending {
            A.strip_suffix("```\n").unwrap()
        } else {
            A
        };
        let wire = tail.replace('\n', "\r\n");
        for split in 0..=wire.len() {
            for keyed in [false, true] {
                for append in [false, true] {
                    let prefix = format!("{A}\n{A}\n");
                    let initial = format!("{prefix}{}", &wire[..split]);
                    let mut w = World::new();
                    let v = liquid_view(&mut w, &initial);
                    let s = liquid_surface_handle(&w, v).unwrap();
                    if keyed {
                        let r = liquid_source_revision(&w, s).unwrap();
                        set_liquid_source_keyed(
                            &mut w,
                            s,
                            r,
                            &initial,
                            &LiquidKeyMap {
                                keys: ["one", "two", "three"]
                                    .into_iter()
                                    .take(inspect_liquid_occurrences(&initial).unwrap().len())
                                    .map(|s| Some(s.to_owned()))
                                    .collect(),
                            },
                        )
                        .unwrap();
                    }
                    let handles: Vec<_> = (0..2)
                        .map(|i| liquid_component_handle(&w, s, i).unwrap())
                        .collect();
                    let records: Vec<_> = handles
                        .iter()
                        .map(|h| {
                            put_liquid_component_state_for(
                                &mut w,
                                *h,
                                0,
                                LiquidComponentState::Accordion { open: vec![0] },
                            )
                            .unwrap()
                        })
                        .collect();
                    push_liquid_str(&mut w, v, &wire[split..]);
                    let before = liquid_source_revision(&w, s).unwrap();
                    let normalized = format!("{prefix}{tail}");
                    // Prefix extension must stage from complete canonical source too.
                    let target = if append {
                        if pending {
                            format!("{normalized}```\n\n{A}")
                        } else {
                            format!("{normalized}\n{A}")
                        }
                    } else {
                        normalized
                    };
                    let expected = LiquidStream::new(&target);
                    let count = inspect_liquid_occurrences(&target).unwrap().len();
                    let map = LiquidKeyMap {
                        keys: (0..count)
                            .map(|i| Some(["one", "two", "three", "four"][i].to_owned()))
                            .collect(),
                    };
                    if keyed {
                        // Invalid keys must not incidentally repair a bad projection.
                        let blocks = w.get::<LiquidView>(v).unwrap().blocks().to_vec();
                        let states = liquid_component_states(&w, v);
                        let markdown = w.get::<MarkdownView>(v).unwrap().source().to_owned();
                        assert_eq!(
                            set_liquid_source_keyed(
                                &mut w,
                                s,
                                before,
                                &target,
                                &LiquidKeyMap { keys: vec![] }
                            ),
                            Err(LiquidF2Error::InvalidKeys)
                        );
                        assert_eq!(w.get::<LiquidView>(v).unwrap().blocks(), blocks);
                        assert_eq!(liquid_component_states(&w, v), states);
                        assert_eq!(w.get::<MarkdownView>(v).unwrap().source(), markdown);
                        assert_eq!(liquid_source_revision(&w, s).unwrap(), before);
                        set_liquid_source_keyed(&mut w, s, before, &target, &map).unwrap();
                    } else {
                        set_liquid_source(&mut w, v, &target);
                    }
                    assert_eq!(
                        w.get::<LiquidView>(v).unwrap().blocks(),
                        expected.blocks(),
                        "pending={pending} split={split} keyed={keyed} append={append}"
                    );
                    assert_eq!(
                        w.get::<MarkdownView>(v).unwrap().source(),
                        expected.markdown()
                    );
                    for (i, h) in handles.iter().enumerate() {
                        assert_eq!(liquid_component_handle(&w, s, i).unwrap(), *h);
                        assert_eq!(
                            liquid_component_state_for(&w, *h).unwrap(),
                            Some(records[i].clone())
                        );
                    }
                    for i in 2..count {
                        let h = liquid_component_handle(&w, s, i).unwrap();
                        assert!(!handles.contains(&h));
                        assert_eq!(liquid_component_state_for(&w, h).unwrap(), None);
                    }
                    assert!(liquid_component_handle(&w, s, count).is_err());
                    if append {
                        assert_ne!(liquid_source_revision(&w, s).unwrap(), before);
                    }
                    // The repaired canonical projection is now a genuine normalized
                    // no-op: serials, records and source revision must all survive.
                    let revision = liquid_source_revision(&w, s).unwrap();
                    let all: Vec<_> = (0..count)
                        .map(|i| liquid_component_handle(&w, s, i).unwrap())
                        .collect();
                    let crlf = target.replace('\n', "\r\n");
                    if keyed {
                        assert_eq!(
                            set_liquid_source_keyed(&mut w, s, revision, &crlf, &map).unwrap(),
                            revision
                        );
                    } else {
                        set_liquid_source(&mut w, v, &crlf);
                    }
                    assert_eq!(liquid_source_revision(&w, s).unwrap(), revision);
                    for (i, h) in all.iter().enumerate() {
                        assert_eq!(liquid_component_handle(&w, s, i).unwrap(), *h);
                    }
                    // Complete the pending fence through the *public append* path.
                    // A setter repair must also install correct internal parse state.
                    cases += 1;
                    if pending && !append {
                        push_liquid_str(&mut w, v, "```\n");
                        assert_eq!(
                            w.get::<LiquidView>(v).unwrap().blocks(),
                            LiquidStream::new(&format!("{target}```\n")).blocks()
                        );
                        assert!(liquid_component_handle(&w, s, count).is_ok());
                        for (i, h) in handles.iter().enumerate() {
                            assert_eq!(liquid_component_handle(&w, s, i).unwrap(), *h);
                        }
                    }
                }
            }
        }
    }
    eprintln!("canonical setter boundary cases={cases}: all byte splits, keyed/unkeyed, equal/prefix, settled/pending");
}

#[test]
fn canonical_repair_keeps_mounted_and_saved_duplicate_state() {
    use rndr_ui::{apply_theme_bridge, UiPlugin, UiTheme};
    for offscreen in [false, true] {
        for keyed in [false, true] {
            let mut app = App::new();
            app.add_plugin(UiPlugin);
            app.add_plugin(MarkdownPlugin);
            app.add_plugin(LiquidUiPlugin::new(NoHost));
            app.world_mut().insert_resource(Viewport::new(960.0, 640.0));
            apply_theme_bridge(app.world_mut(), UiTheme::default());
            let filler: String = (0..80).map(|i| format!("Paragraph {i}.\n\n")).collect();
            let prefix = format!("{A}\n{A}\n{filler}");
            let view = liquid_view(app.world_mut(), &prefix);
            app.world_mut()
                .get_mut::<MarkdownView>(view)
                .unwrap()
                .set_follow(false);
            let surface = liquid_surface_handle(app.world(), view).unwrap();
            if keyed {
                set_liquid_source_keyed(
                    app.world_mut(),
                    surface,
                    LiquidSourceRevision::ZERO,
                    &prefix,
                    &LiquidKeyMap {
                        keys: vec![Some("one".into()), Some("two".into())],
                    },
                )
                .unwrap();
            }
            let handles: Vec<_> = (0..2)
                .map(|i| liquid_component_handle(app.world(), surface, i).unwrap())
                .collect();
            let records: Vec<_> = handles
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    put_liquid_component_state_for(
                        app.world_mut(),
                        *h,
                        0,
                        LiquidComponentState::Accordion {
                            open: if i == 0 { vec![0] } else { vec![] },
                        },
                    )
                    .unwrap()
                })
                .collect();
            for _ in 0..2 {
                rndr_paint::frame(&mut app);
            }
            assert_eq!(
                app.world().get::<LiquidView>(view).unwrap().mounted().len(),
                2
            );
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
            let wire = A.replace('\n', "\r\n");
            push_liquid_str(app.world_mut(), view, &wire[..13]);
            push_liquid_str(app.world_mut(), view, &wire[13..]);
            let target = format!("{prefix}{A}");
            let revision = liquid_source_revision(app.world(), surface).unwrap();
            if keyed {
                set_liquid_source_keyed(
                    app.world_mut(),
                    surface,
                    revision,
                    &target,
                    &LiquidKeyMap {
                        keys: vec![Some("one".into()), Some("two".into()), Some("three".into())],
                    },
                )
                .unwrap();
            } else {
                set_liquid_source(app.world_mut(), view, &target);
            }
            assert_eq!(
                app.world().get::<LiquidView>(view).unwrap().blocks(),
                LiquidStream::new(&target).blocks()
            );
            for _ in 0..2 {
                rndr_paint::frame(&mut app);
            }
            app.world_mut().get_mut::<Scroll>(view).unwrap().offset[1] = 0.0;
            for _ in 0..2 {
                rndr_paint::frame(&mut app);
            }
            assert_eq!(
                app.world().get::<LiquidView>(view).unwrap().mounted().len(),
                2,
                "offscreen={offscreen} keyed={keyed} window={:?} scroll={:?} markdown={:?}",
                markdown_realized_blocks(app.world(), view),
                app.world()
                    .get::<Scroll>(view)
                    .map(|s| (s.offset, s.content)),
                app.world().get::<MarkdownView>(view).unwrap().source()
            );
            for (i, h) in handles.iter().enumerate() {
                assert_eq!(
                    liquid_component_handle(app.world(), surface, i).unwrap(),
                    *h
                );
                assert_eq!(
                    liquid_component_state_for(app.world(), *h).unwrap(),
                    Some(records[i].clone())
                );
            }
        }
    }
}
