//! Headless fallback-engine characterization, not native/GPU/IME latency proof.
//! cargo test -p rndr_core --release --test editor_scale_cost -- --ignored --nocapture
use rndr_core::editing::*;
use rndr_core::*;
use std::time::Instant;

fn fixture(bytes: usize) -> String {
    let unit = "alpha beta gamma delta epsilon zeta eta theta 0123456789\n";
    unit.repeat(bytes.div_ceil(unit.len()))
}

#[test]
#[ignore = "opt-in release scale characterization; no timing threshold assertions"]
fn editor_scale_costs() {
    measure(None, |app| app.update());
}

pub fn measure(engine: Option<std::sync::Arc<dyn TextEngine>>, frame: fn(&mut App)) {
    println!("bytes,cold_us,idle_us_p50,idle_us_p95,edit_layout_us_p50,edit_layout_us_p95,resize_us_p50,resize_us_p95,paragraph_cache_bytes,row_geometry_bytes,visible_glyphs");
    for size in [10_240, 1_048_576, 10_485_760] {
        let source = fixture(size);
        let mut app = App::new();
        app.add_plugin(Chrome);
        if let Some(engine) = &engine {
            app.world_mut().insert_resource(engine.clone());
        }
        let mut docs = SourceDocuments::default();
        let doc = docs.open(&source);
        app.world_mut().insert_resource(docs);
        let e = app.world_mut().spawn((
            Layout {
                width: Size::Px(600.0),
                height: Size::Px(400.0),
                ..Layout::column()
            },
            Label::new(""),
            Buffer::from_str(""),
            Editor::default(),
            CapturesKeys,
        ));
        attach_source(app.world_mut(), e, doc).unwrap();
        app.world_mut().insert_resource(Viewport::new(620.0, 420.0));
        let cold = Instant::now();
        for _ in 0..3 {
            frame(&mut app);
        }
        let cold = cold.elapsed().as_micros();
        let baseline = *app.world().get::<EditorLayoutStats>(e).unwrap();
        let mut idle = Vec::new();
        for i in 0..21 {
            app.world_mut().get_mut::<Editor>(e).unwrap().offset = (i * 40) as f32;
            let start = Instant::now();
            frame(&mut app);
            idle.push(start.elapsed().as_micros());
        }
        let after_idle = *app.world().get::<EditorLayoutStats>(e).unwrap();
        assert_eq!(baseline.assemblies, after_idle.assemblies);
        assert_eq!(baseline.bytes_shaped, after_idle.bytes_shaped);
        let mut edit = Vec::new();
        let mut resize = Vec::new();
        for i in 0..21 {
            let start = Instant::now();
            let session = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(doc)
                .unwrap();
            session
                .commit(SourceBatch {
                    document: doc,
                    base: session.revision(),
                    origin: EditOrigin::Host(1),
                    changes: vec![SourceChange {
                        range: 0..1,
                        replacement: if i % 2 == 0 { "A" } else { "a" }.into(),
                    }],
                })
                .unwrap();
            frame(&mut app);
            edit.push(start.elapsed().as_micros());
            app.world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(doc)
                .unwrap()
                .drain_events()
                .for_each(drop);
            app.world_mut().get_mut::<Layout>(e).unwrap().width =
                Size::Px(if i % 2 == 0 { 500.0 } else { 600.0 });
            let start = Instant::now();
            for _ in 0..2 {
                frame(&mut app);
            }
            resize.push(start.elapsed().as_micros());
        }
        idle.sort_unstable();
        edit.sort_unstable();
        resize.sort_unstable();
        let stats = *app.world().get::<EditorLayoutStats>(e).unwrap();
        let visible = app.world().get::<TextLayout>(e).unwrap().glyphs.len();
        assert!(stats.paragraph_cache_bytes <= 8 * 1024 * 1024);
        assert!(app.world().get::<Label>(e).unwrap().text.len() < 10_000);
        assert!(visible < 10_000);
        let session = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(doc)
            .unwrap();
        assert_eq!(&session.text()[1..], &source[1..]);
        println!(
            "{},{cold},{},{},{},{},{},{},{},{},{visible}",
            source.len(),
            idle[10],
            idle[19],
            edit[10],
            edit[19],
            resize[10],
            resize[19],
            stats.paragraph_cache_bytes,
            stats.row_geometry_bytes
        );
    }
}
