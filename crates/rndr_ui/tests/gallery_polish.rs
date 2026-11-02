use rndr_core::*;
use rndr_ui::*;
#[path = "../examples/support/operating.rs"]
mod operating;

#[test]
fn workspace_reflows_without_rebuilding_or_horizontal_control_overflow() {
    for density in [Density::Comfortable, Density::Compact] {
        for scale in [1.0, 2.0] {
            let theme = UiTheme {
                density,
                text_scale: scale,
                reading_scale: scale,
                ..UiTheme::default()
            };
            let mut app = operating::create(1000.0, 700.0, theme, false);
            let d = app.world().resource::<operating::Demo>().unwrap().clone();
            let original = app.world().get::<Buffer>(d.editor).unwrap().text();
            for width in [1000.0, 320.0, 280.0, 1600.0, 760.0, 1000.0] {
                app.world_mut().resource_mut::<Viewport>().unwrap().width = width;
                app.update();
                let w = app.world();
                assert_eq!(
                    w.get::<Layout>(d.workspace).unwrap().axis,
                    if width >= 760.0 * scale {
                        Axis::Row
                    } else {
                        Axis::Column
                    }
                );
                assert_eq!(w.get::<Buffer>(d.editor).unwrap().text(), original);
                assert!(w.has::<Scroll>(d.editor));
                let editor = *w.get::<Rect>(d.editor).unwrap();
                assert!(editor.h <= 180.0 && editor.h >= 76.0);
                for h in d
                    .commands
                    .iter()
                    .chain(d.nav.iter())
                    .chain(d.selector.tabs.iter())
                    .chain([d.source].iter())
                {
                    h.validate(w).unwrap();
                    let rect = *w.get::<Rect>(h.root).unwrap();
                    assert!(
                        rect.x >= 0.0 && rect.x + rect.w <= width + 0.1,
                        "width={width} scale={scale} rect={rect:?}"
                    );
                    assert!(rect.h >= w.resource::<UiTheme>().unwrap().target_min());
                    let label = *w.get::<Rect>(h.label).unwrap();
                    assert!(label.x >= rect.x && label.x + label.w <= rect.x + rect.w + 0.1);
                }
                if width >= 760.0 * scale {
                    let nav = *w.get::<Rect>(d.navigation).unwrap();
                    assert!(
                        editor.x > nav.x + nav.w,
                        "document is separate from navigation"
                    );
                }
                let source = *w.get::<Rect>(d.source.root).unwrap();
                let card = *w.get::<Rect>(d.card.content).unwrap();
                assert!(source.x + source.w <= card.x + card.w + 0.1);
                if width >= 1000.0 {
                    assert!(
                        source.w < card.w * 0.75,
                        "source command should not resemble a full-width field"
                    );
                }
                assert_eq!(
                    *w.get::<TextStyle>(d.source.label).unwrap(),
                    w.resource::<UiTheme>()
                        .unwrap()
                        .typography(Typography::Emphasis)
                );
            }
        }
    }
}
