use rndr_core::*; use rndr_ui::*;
#[path="/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library/crates/rndr_ui/examples/support/operating.rs"] mod operating;
#[test] fn independent_geometry_semantics_and_panel_switch_128(){
 let mut count=0;
 for appearance in [Appearance::Light,Appearance::Dark] {for contrast in [Contrast::Standard,Contrast::High] {for density in [Density::Comfortable,Density::Compact] {for text_scale in [1.,2.] {for width in [320.,640.] {for long in [false,true] {
 let theme=UiTheme{appearance,contrast,density,text_scale,reading_scale:text_scale,..UiTheme::default()};
 let mut a=operating::create(width,30000.,theme.clone(),long);let d=a.world().resource::<operating::Demo>().unwrap().clone();assert!(request_focus(a.world_mut(),d.nav[0].root));let f=rndr_paint::frame(&mut a);
 for h in d.commands.iter().chain(d.nav.iter()).chain(d.selector.tabs.iter()).chain([d.source].iter()) {
 let r=*a.world().get::<Rect>(h.root).unwrap();let lr=*a.world().get::<Rect>(h.label).unwrap();let t=a.world().get::<TextLayout>(h.label).unwrap();
 assert!(r.w>=theme.target_min()-0.02&&r.h>=theme.target_min()-0.02); assert!(r.x>=-0.02&&r.x+r.w<=width+0.02,"{r:?} width={width}");
 assert!(lr.x>=r.x-0.02&&lr.y>=r.y-0.02&&lr.x+lr.w<=r.x+r.w+0.02&&lr.y+lr.h<=r.y+r.h+0.02);assert!(t.height<=lr.h+0.02);
 let expected:String=a.world().get::<Label>(h.label).unwrap().text.chars().filter(|c|c.is_ascii_graphic()).collect();let actual:String=t.glyphs.iter().map(|g|g.ch).filter(|c|c.is_ascii_graphic()).collect();assert_eq!(actual,expected);
 assert!(f.quads.iter().any(|q|q.uv!=rndr_paint::solid_uv()&&q.rect.x>=lr.x-0.02&&q.rect.y>=lr.y-0.02&&q.rect.x+q.rect.w<=lr.x+lr.w+0.02&&q.rect.y+q.rect.h<=lr.y+lr.h+0.02));
 }
 let nodes=access_tree(a.world()).nodes;let nav=nodes.iter().find(|n|n.id==access_id(d.nav[0].root)).unwrap();assert!(nav.focused&&nav.selected);assert!(!access_details(a.world(),d.card.root).activate);assert!(!access_details(a.world(),d.callout.root).focus);
 assert_eq!(try_activate(a.world_mut(),d.selector.tabs[1].root),ActivationResult::Accepted);a.update();let nodes=access_tree(a.world()).nodes;assert!(!nodes.iter().any(|n|n.id==access_id(d.editor)));assert!(!interaction_visible(a.world(),d.editor));assert!(!request_focus(a.world_mut(),d.editor));assert!(a.world().get::<Buffer>(d.editor).is_some());
 count+=1;
 }}}}}} assert_eq!(count,128);println!("independent geometry/semantics combinations={count}");
}
