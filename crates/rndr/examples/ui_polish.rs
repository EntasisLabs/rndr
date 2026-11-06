//! Synthetic UI polish reproduction. Headless pixels are not native input certification.
#[path = "../../rndr_ui/examples/support/data_gallery.rs"]
mod data;
#[path = "../../rndr_ui/examples/support/data_editors_gallery.rs"]
mod editors;
#[path = "../../rndr_ui/examples/support/selection_gallery.rs"]
mod selection;
use rndr_core::*;
use rndr_ui::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let value = |s: &str| {
        args.iter()
            .position(|a| a == s)
            .and_then(|i| args.get(i + 1))
    };
    let width = if has("--narrow") { 380. } else { 1000. };
    let height = 960.;
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        ..Default::default()
    };
    let scene = value("--scene").map(String::as_str).unwrap_or("selection");
    let mut app = match scene {
        "data" => data::create(width, height, theme),
        "editors" => editors::create(width, height, theme),
        "scroll" => nested(width, height, theme),
        _ => selection::create(width, height, theme, &args),
    };
    #[cfg(feature = "parley")]
    if !has("--builtin") {
        let engine: std::sync::Arc<dyn TextEngine> =
            std::sync::Arc::new(rndr_render::ParleyText::bundled());
        app.world_mut().insert_resource(engine);
    }
    if has("--compact") {
        app.world_mut().resource_mut::<UiTheme>().unwrap().density = Density::Compact;
    }
    if has("--2x") {
        app.world_mut().resource_mut::<Viewport>().unwrap().scale = 2.;
    }
    rndr_paint::frame(&mut app);
    if has("--focus-view") {
        let e = match scene {
            "data" => app.world().resource::<data::Gallery>().unwrap().tree.root,
            "editors" => {
                app.world()
                    .resource::<editors::Gallery>()
                    .unwrap()
                    .table
                    .unwrap()
                    .rows
                    .root
            }
            "scroll" => app.world().resource::<Nested>().unwrap().inner,
            _ => {
                app.world()
                    .resource::<selection::Gallery>()
                    .unwrap()
                    .checkbox
                    .unwrap()
                    .root
            }
        };
        request_focus(app.world_mut(), e);
    }
    if has("--focus") {
        if let Some(e) = app
            .world()
            .entities_with::<Focusable>()
            .into_iter()
            .find(|e| access_details(app.world(), *e).focus)
        {
            request_focus(app.world_mut(), e);
        }
    }
    if let Some(label) = value("--focus-label") {
        if let Some(e) = app
            .world()
            .entities_with::<Label>()
            .into_iter()
            .find(|e| {
                app.world()
                    .get::<Label>(*e)
                    .is_some_and(|l| &l.text == label)
            })
            .and_then(|e| ancestor_with::<Focusable>(app.world(), e))
        {
            request_focus(app.world_mut(), e);
        } else {
            return Err("focus label not found".into());
        }
    }
    if has("--wheel-inner-end") {
        let inner = app
            .world()
            .resource::<Nested>()
            .ok_or("wheel probe requires --scene scroll")?
            .inner;
        rndr_paint::frame(&mut app);
        let rect = *app.world().get::<Rect>(inner).unwrap();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
        input.scroll = [0., -500.];
        apply_scroll(app.world_mut());
        let root = app.world().resource::<Nested>().unwrap().root;
        println!(
            "bounded wheel probe: inner {:?}, outer {:?}",
            app.world().get::<Scroll>(inner).unwrap().offset,
            app.world().get::<Scroll>(root).unwrap().offset
        );
        assert!(app.world().get::<Scroll>(root).unwrap().offset[1] < 0.);
    }
    if let Some(prefix) = value("--capture-prefix") {
        #[cfg(not(feature = "observe"))]
        let _ = prefix;
        #[cfg(feature = "observe")]
        {
            use rndr_render::{
                capture::{capture, Target},
                encode_png,
                observe::ObservationContent,
                Exporter,
            };
            use std::io::Write;
            app.world_mut().insert_resource(ObservationContent::Public);
            let root = match scene {
                "data" => app.world().resource::<data::Gallery>().unwrap().root,
                "editors" => app.world().resource::<editors::Gallery>().unwrap().root,
                "scroll" => app.world().resource::<Nested>().unwrap().root,
                _ => app.world().resource::<selection::Gallery>().unwrap().root,
            };
            let mut exporter = Exporter::new()?;
            let result = capture(&mut app, &mut exporter, Target::Entity(root), |w, e| {
                identity(w, e).map(|id| id.0)
            })?;
            for (ext, bytes) in [
                ("png", encode_png(&result.image)?),
                ("json", result.metadata.to_json().to_string().into_bytes()),
            ] {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(format!("{prefix}.{ext}"))?
                    .write_all(&bytes)?;
            }
            println!(
                "headless {scene}: {}x{} scale {}",
                result.image.width, result.image.height, result.metadata.scale
            );
            return Ok(());
        }
        #[cfg(not(feature = "observe"))]
        return Err("capture requires observe".into());
    }
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / UI polish retest".into(),
        size: LogicalSize::new(width, height),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}

#[derive(Clone, Copy)]
struct Nested {
    root: Entity,
    inner: Entity,
}
fn nested(width: f32, height: f32, theme: UiTheme) -> App {
    use rndr_ui::collections::{Item, ItemId, ListView, ViewProps};
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(Viewport::new(width, height));
    app.world_mut().insert_resource(theme);
    let w = app.world_mut();
    let root = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 16.,
            gap: 12.,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    w.insert(
        root,
        UiStyle {
            background: Some(Token::Canvas.into()),
            ..Default::default()
        },
    );
    text(w, root, "Nested scrolling retest", Typography::Title).unwrap();
    text(w,root,"Over the inner region: it scrolls first. At its boundary, only leftover movement goes to this parent. Horizontal input is never converted to vertical. Thumbs show each overflowing region.",Typography::Label).unwrap();
    text(
        w,
        root,
        "Parent-only region — wheel here to move only the outer document",
        Typography::Label,
    )
    .unwrap();
    let items: Vec<_> = (0..10)
        .map(|i| Item::new(i + 1, format!("Inner record {}", i + 1)))
        .collect();
    let inner = ListView::mount(
        w,
        root,
        UiId(100),
        ViewProps::new("Inner scroll region"),
        &items,
        |w, slot, item| {
            text(w, slot.content, &item.name, Typography::Label).unwrap();
        },
    )
    .unwrap();
    w.get_mut::<Layout>(inner.root).unwrap().height = Size::Px(180.);
    inner.select(w, Some(ItemId(1))).unwrap();
    text(w,root,"Parent-only region — continue upward/downward across the inner boundary; no need to find a tiny edge gutter",Typography::Label).unwrap();
    for i in 0..24 {
        text(
            w,
            root,
            &format!("Outer document record {} — parent scroll content", i + 1),
            Typography::Label,
        )
        .unwrap();
    }
    w.insert_resource(Nested {
        root,
        inner: inner.root,
    });
    app
}
