//! Handle and transaction contract tests; legacy public layouts remain unchanged.

#[test]
fn split_crlf_stream_then_normalized_noop_preserves_completed_occurrences() {
    for keyed in [false, true] {
        let doc = format!("{A}\n{A}");
        let (mut world, view, surface) = setup(&doc);
        let map = if keyed {
            keys(&[Some("one"), Some("two")])
        } else {
            keys(&[None, None])
        };
        if keyed {
            bind(&mut world, surface, &doc, &map);
        }
        let handles: Vec<_> = (0..2)
            .map(|i| liquid_component_handle(&world, surface, i).unwrap())
            .collect();
        for h in &handles {
            put_liquid_component_state_for(
                &mut world,
                *h,
                0,
                LiquidComponentState::Accordion { open: vec![0] },
            )
            .unwrap();
        }
        push_liquid_str(&mut world, view, "\r");
        push_liquid_str(&mut world, view, "\n");
        let revision = liquid_source_revision(&world, surface).unwrap();
        let normalized = format!("{doc}\n");
        assert_eq!(
            inspect_liquid_occurrences(&normalized.replace('\n', "\r\n"))
                .unwrap()
                .len(),
            2
        );
        let result =
            set_liquid_source_keyed(&mut world, surface, revision, &normalized, &map).unwrap();
        assert_eq!(result, revision, "a normalized exact no-op, keyed={keyed}");
        for (i, h) in handles.iter().enumerate() {
            assert_eq!(liquid_component_handle(&world, surface, i).unwrap(), *h);
            let record = liquid_component_state_for(&world, *h).unwrap().unwrap();
            assert_eq!(record.revision, 1);
            assert_eq!(
                record.state,
                LiquidComponentState::Accordion { open: vec![0] }
            );
        }
    }
}

use rndr_core::*;
use rndr_liquid_ui::*;

const A: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: Body\n```\n";
fn keys(names: &[Option<&str>]) -> LiquidKeyMap {
    LiquidKeyMap {
        keys: names.iter().map(|s| s.map(str::to_owned)).collect(),
    }
}
fn setup(source: &str) -> (World, Entity, LiquidSurfaceHandle) {
    let mut w = World::new();
    let v = liquid_view(&mut w, source);
    let s = liquid_surface_handle(&w, v).unwrap();
    (w, v, s)
}
fn bind(
    w: &mut World,
    s: LiquidSurfaceHandle,
    source: &str,
    k: &LiquidKeyMap,
) -> LiquidSourceRevision {
    let rev = liquid_source_revision(w, s).unwrap();
    set_liquid_source_keyed(w, s, rev, source, k).unwrap()
}
#[test]
fn complete_reading_order_keys_and_invalid_transactions_are_atomic() {
    let doc = format!("{A}\n{A}");
    let (mut w, v, s) = setup(&doc);
    let r = bind(&mut w, s, &doc, &keys(&[Some("one"), Some("two")]));
    let h = liquid_component_handle(&w, s, 0).unwrap();
    let record = put_liquid_component_state_for(
        &mut w,
        h,
        0,
        LiquidComponentState::Accordion { open: vec![0] },
    )
    .unwrap();
    for bad in [
        keys(&[]),
        keys(&[Some("x"), Some("x")]),
        keys(&[Some(""), None]),
        keys(&[Some("x\n"), None]),
        keys(&[Some(&"x".repeat(257)), None]),
    ] {
        assert_eq!(
            set_liquid_source_keyed(&mut w, s, r, &doc, &bad),
            Err(LiquidF2Error::InvalidKeys)
        );
        assert_eq!(liquid_source_revision(&w, s).unwrap(), r);
        assert_eq!(liquid_component_handle(&w, s, 0).unwrap(), h);
        assert_eq!(
            liquid_component_state_for(&w, h).unwrap(),
            Some(record.clone())
        );
    }
    let moved = format!("Prefix\n\n{doc}");
    let r2 =
        set_liquid_source_keyed(&mut w, s, r, &moved, &keys(&[Some("two"), Some("one")])).unwrap();
    assert_ne!(r, r2);
    assert_eq!(liquid_component_handle(&w, s, 1).unwrap(), h);
    assert_eq!(
        liquid_component_state_for(&w, h).unwrap().unwrap().revision,
        1
    );
    assert!(matches!(
        set_liquid_source_keyed(&mut w, s, r, &doc, &keys(&[None, None])),
        Err(LiquidF2Error::Revision { .. })
    ));
    assert_eq!(liquid_source_revision(&w, s).unwrap(), r2);
    assert_eq!(liquid_component_states(&w, v).len(), 1);
}
#[test]
fn reset_remove_reinsert_and_foreign_world_never_resurrect_handles() {
    let (mut w, _, s) = setup(A);
    bind(&mut w, s, A, &keys(&[Some("a")]));
    let h = liquid_component_handle(&w, s, 0).unwrap();
    let before = liquid_source_revision(&w, s).unwrap();
    let new = reset_liquid_block(&mut w, h, 0).unwrap();
    assert_ne!(h, new);
    assert_ne!(liquid_source_revision(&w, s).unwrap(), before);
    assert_eq!(
        liquid_component_state_for(&w, h),
        Err(LiquidF2Error::StaleBlock)
    );
    assert_eq!(liquid_component_state_for(&w, new).unwrap(), None);
    bind(&mut w, s, "", &keys(&[]));
    bind(&mut w, s, A, &keys(&[Some("a")]));
    assert_ne!(liquid_component_handle(&w, s, 0).unwrap(), new);
    let rev = liquid_source_revision(&w, s).unwrap();
    let fresh = reset_liquid_surface(&mut w, s, rev).unwrap();
    assert_ne!(s, fresh);
    assert_eq!(
        liquid_source_revision(&w, s),
        Err(LiquidF2Error::StaleOwner)
    );
    assert_eq!(
        liquid_source_revision(&w, fresh).unwrap(),
        LiquidSourceRevision::ZERO
    );
    let (other, _, _) = setup(A);
    assert_eq!(
        liquid_component_state_for(&other, new),
        Err(LiquidF2Error::StaleOwner)
    );
}
#[test]
fn keyed_child_edit_resets_but_title_edit_preserves_state_and_serial() {
    let (mut w, _, s) = setup(A);
    bind(&mut w, s, A, &keys(&[Some("a")]));
    let h = liquid_component_handle(&w, s, 0).unwrap();
    put_liquid_component_state_for(
        &mut w,
        h,
        0,
        LiquidComponentState::Accordion { open: vec![0] },
    )
    .unwrap();
    bind(&mut w, s, &A.replace("FAQ", "Title"), &keys(&[Some("a")]));
    assert_eq!(liquid_component_handle(&w, s, 0).unwrap(), h);
    assert_eq!(
        liquid_component_state_for(&w, h).unwrap().unwrap().revision,
        1
    );
    bind(
        &mut w,
        s,
        &A.replace("Body", "Changed"),
        &keys(&[Some("a")]),
    );
    let reset = liquid_component_state_for(&w, h).unwrap().unwrap();
    assert_eq!(reset.revision, 2);
    assert_eq!(
        reset.state,
        LiquidComponentState::Accordion { open: vec![] }
    );
    assert!(matches!(
        put_liquid_component_state_for(
            &mut w,
            h,
            1,
            LiquidComponentState::Accordion { open: vec![0] }
        ),
        Err(LiquidF2Error::Revision { .. })
    ));
    assert_eq!(liquid_component_state_for(&w, h).unwrap(), Some(reset));
}
#[test]
fn kinds_and_index_domains_are_checked_without_writes() {
    let (mut w, _, s) = setup(A);
    let h = liquid_component_handle(&w, s, 0).unwrap();
    for state in [
        LiquidComponentState::Tabs { selected: 0 },
        LiquidComponentState::Accordion { open: vec![1] },
    ] {
        assert_eq!(
            put_liquid_component_state_for(&mut w, h, 0, state),
            Err(LiquidF2Error::Kind)
        );
        assert_eq!(liquid_component_state_for(&w, h).unwrap(), None);
    }
}
