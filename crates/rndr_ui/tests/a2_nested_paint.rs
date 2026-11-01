use rndr_core::*;
use rndr_ui::{controls::*, *};
#[test]
fn nested_source_focus_real_glyph_clip_and_hit_geometry_agree() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let outer = a.world_mut().spawn((
        Layout {
            width: Size::Px(300.0),
            height: Size::Px(140.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let spacer = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(220.0),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), spacer, outer);
    let inner = a.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(80.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(a.world_mut(), inner, outer);
    let filler = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(300.0),
        ..Layout::column()
    },));
    set_parent(a.world_mut(), filler, inner);
    let h = SourceAction::mount(
        a.world_mut(),
        inner,
        UiId(1),
        SourceLocator {
            document: 1,
            revision: 2,
            anchor: 3,
        },
        true,
    )
    .unwrap();
    a.update();
    assert!(request_focus(a.world_mut(), h.root));
    let frame = rndr_paint::frame(&mut a);
    let r = *a.world().get::<Rect>(h.root).unwrap();
    let lr = *a.world().get::<Rect>(h.label).unwrap();
    let outer = *a.world().get::<Rect>(outer).unwrap();
    let inner = *a.world().get::<Rect>(inner).unwrap();
    assert!(r.y >= outer.y && r.y + r.h <= outer.y + outer.h + 0.01);
    assert!(r.y >= inner.y && r.y + r.h <= inner.y + inner.h + 0.01);
    let glyphs: Vec<_> = frame
        .quads
        .iter()
        .filter(|q| {
            q.uv != rndr_paint::solid_uv()
                && q.rect.x >= lr.x
                && q.rect.y >= lr.y
                && q.rect.x + q.rect.w <= lr.x + lr.w + 0.01
                && q.rect.y + q.rect.h <= lr.y + lr.h + 0.01
        })
        .collect();
    assert_eq!(
        glyphs.len(),
        10,
        "actual complete Show source glyphs, no World-only visibility claim"
    );
    for q in glyphs {
        for port in [outer, inner] {
            assert!(
                q.clip.x >= port.x
                    && q.clip.y >= port.y
                    && q.clip.x + q.clip.w <= port.x + port.w + 0.01
                    && q.clip.y + q.clip.h <= port.y + port.h + 0.01
            );
        }
    }
    let hit = hit_test(a.world(), lr.x + 12.0, lr.y + 12.0).unwrap();
    assert_eq!(
        ancestor_with::<rndr_core::Button>(a.world(), hit),
        Some(h.root)
    );
}
