"""Fresh reviewer probes, exact observer/fresh paint with pinned existing rlibs.
Writes only target/native-observation-rereview; no product-source mutation.
"""
from pathlib import Path
extra = r'''
#[test]
fn rereview_same_custom_type_replacement_denies() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let old: Arc<dyn TextEngine> = Arc::new(CorrectionEngine(BuiltinText::new(), AtomicU64::new(0)));
    app.world_mut().insert_resource(old.clone());
    app.world_mut().spawn((Label::new("actual old custom paint"),));
    app.add_system(Schedule::Render, |w| {
        w.insert_resource(Arc::new(CorrectionEngine(BuiltinText::new(), AtomicU64::new(0))) as Arc<dyn TextEngine>);
    });
    let recv=request(&send,0);
    let (_,capture)=crate::fresh_paint::frame_with_snapshot(&mut app,|w|begin(w,2,2,1.));
    assert!(old.shape_count()>0);
    assert!(!Arc::ptr_eq(&old,&text_engine(app.world())));
    assert!(validate(app.world(),capture.unwrap(),1.).is_none());
    assert_error(recv,"post_paint_text_engine_mutation");
}
#[test]
fn rereview_stable_builtin_epoch_and_cache_across_frames() {
    let (mut app,send)=fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.world_mut().spawn((Label::new("stable glyph cache"),));
    let mut epoch=None;
    for _ in 0..3 {
        let recv=request(&send,0);
        let (_,capture)=crate::fresh_paint::frame_with_snapshot(&mut app,|w|begin(w,2,2,1.));
        let engine=text_engine(app.world());
        let before=engine.cache_epoch();
        let current=engine.fallbacks("bitmap");
        engine.set_fallbacks("bitmap",&current);
        assert_eq!(engine.cache_epoch(),before,"identical fallback is no invalidation");
        if let Some(old)=epoch { assert_eq!(old,before); }
        epoch=Some(before);
        let capture=validate(app.world(),capture.unwrap(),1.).unwrap();
        complete(capture,Ok(image(2,2)),true,json!({"text_engine":"builtin_atlas"}));
        let (meta,png)=recv.recv().unwrap();
        assert!(!png.is_empty()); assert_eq!(meta["presented_confirmation"],false);
    }
}
#[test]
fn rereview_replacement_of_existing_registered_font_invalidates() {
    let (mut app,send)=fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    ensure_text_engine(app.world_mut());
    text_engine(app.world()).register_font("another",Arc::new(BitmapFont));
    app.world_mut().spawn((Label::new("old paint"),));
    app.add_system(Schedule::Render,|w| {
        text_engine(w).register_font("another",Arc::new(BitmapFont));
    });
    let recv=request(&send,0);
    let (_,capture)=crate::fresh_paint::frame_with_snapshot(&mut app,|w|begin(w,2,2,1.));
    assert!(validate(app.world(),capture.unwrap(),1.).is_none());
    assert_error(recv,"post_paint_text_engine_mutation");
}
'''
recipe=Path('evidence/native-observation-v1-review/run_rust_probes.py').read_text()
start=recipe.index("extra=r'''"); end=recipe.index("(out/'tests.rs')",start)
recipe=recipe[:start]+'extra='+repr(extra)+'\n'+recipe[end:]
recipe=recipe.replace('target/native-observation-review','target/native-observation-rereview')
recipe=recipe.replace("Path('crates/rndr_render/src/observe_tests.rs').read_text()+extra", "Path('crates/rndr_render/src/observe_tests.rs').read_text().replace('rndr_paint::', 'crate::fresh_paint::')+extra")
exec(compile(recipe,'<fresh reviewer harness>','exec'))
