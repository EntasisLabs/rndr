use rndr_core::*;
use rndr_ui::{*,controls::{self,*}};
const LONG:&str="This is a very long but valid command label with many words";
fn button(a:&mut App,t:ToolbarHandle,id:u64,s:&str)->ControlHandle { controls::Button::mount(a.world_mut(),t.root,UiId(id),ButtonProps::new(s,HostIntent::Command(id))).unwrap() }
fn verify(a:&App,t:ToolbarHandle,hs:&[ControlHandle]) {
 if a.world().get::<Layout>(t.root).unwrap().axis!=Axis::Row{return}
 for h in hs {let w=a.world();let l=w.get::<Label>(h.label).unwrap();let st=w.get::<TextStyle>(h.label).unwrap();let natural=text_engine(w).layout(&l.text,st,&[],f32::INFINITY);let actual=w.get::<TextLayout>(h.label).unwrap();assert_eq!(actual.lines,natural.lines);assert_eq!(actual.glyphs,natural.glyphs);assert!(actual.lines.len()<=1);}
}
fn scene()->(App,Entity) {let mut a=App::new();a.add_plugin(UiPlugin);a.world_mut().insert_resource(Viewport::new(12000.,30000.));let p=a.world_mut().spawn((Layout{width:Size::Px(6000.),..Layout::column()},));(a,p)}
#[test] fn siblings_nested_unknown_parent_and_creation_order_converge(){
 for reverse in [false,true] {
  let(mut a,p)=scene();let outer=Toolbar::mount(a.world_mut(),p,UiId(100),6000.).unwrap();let mut rows=vec![];
  for i in 0..8u64 {let j=if reverse {7-i}else{i};let parent=if j%2==0{outer.root}else{p};let t=Toolbar::mount(a.world_mut(),parent,UiId(200+j),1744.).unwrap();let h=button(&mut a,t,300+j*2,LONG);let s=button(&mut a,t,301+j*2,"Go");rows.push((j,parent,t,h,s));}
  for n in 0..16 {let scale=[1.,1.25,1.75,2.][n%4];a.world_mut().resource_mut::<Viewport>().unwrap().scale=scale;a.world_mut().insert_resource(UiTheme{density:if n%2==0{Density::Compact}else{Density::Comfortable},text_scale:if n%3==0{2.}else{1.},..UiTheme::default()});
   for (j,parent,t,h,_) in &rows {Toolbar::mount(a.world_mut(),*parent,UiId(200+j),if n%2==0{4000.}else{1696.}).unwrap();button(&mut a,*t,300+j*2,if n%5==0{"Go"}else{LONG});assert!(h.validate(a.world()).is_ok());}
   a.update();assert_eq!(a.world().get::<Layout>(outer.root).unwrap().axis,Axis::Column);
   for (_,_,t,h,s) in &rows {verify(&a,*t,&[*h,*s]);}
   let before:Vec<_>=rows.iter().map(|(_,_,t,h,_)|(*a.world().get::<Rect>(h.label).unwrap(),a.world().get::<Layout>(t.root).unwrap().axis)).collect();a.update();let after:Vec<_>=rows.iter().map(|(_,_,t,h,_)|(*a.world().get::<Rect>(h.label).unwrap(),a.world().get::<Layout>(t.root).unwrap().axis)).collect();assert_eq!(before,after);
  }
 }
}
#[test] fn unrelated_scroll_and_layout_cache_equivalent_to_final_authoritative_walk(){
 let(mut a,p)=scene();let t=Toolbar::mount(a.world_mut(),p,UiId(1),1696.).unwrap();let h=button(&mut a,t,2,LONG);let s=button(&mut a,t,3,"Go");
 let q=a.world_mut().spawn((Layout{width:Size::Px(250.),height:Size::Px(50.),..Layout::column()},Scroll::default()));let l=a.world_mut().spawn((Layout{width:Size::Fill,..Layout::column()},Label::new("Unrelated wrapped text unrelated wrapped text unrelated wrapped text unrelated wrapped text")));set_parent(a.world_mut(),l,q);
 for n in 0..10 {Toolbar::mount(a.world_mut(),p,UiId(1),if n%2==0{1696.}else{4000.}).unwrap();a.world_mut().get_mut::<Scroll>(q).unwrap().offset=[0.,-10000.];a.update();verify(&a,t,&[h,s]);let scroll=*a.world().get::<Scroll>(q).unwrap();let rect=*a.world().get::<Rect>(l).unwrap();let clip=*a.world().get::<Clip>(l).unwrap();let text=a.world().get::<TextLayout>(l).unwrap().clone();layout_propagate(a.world_mut());apply_text_layout(a.world_mut());assert_eq!(*a.world().get::<Scroll>(q).unwrap(),scroll);assert_eq!(*a.world().get::<Rect>(l).unwrap(),rect);assert_eq!(*a.world().get::<Clip>(l).unwrap(),clip);assert_eq!(a.world().get::<TextLayout>(l).unwrap().glyphs,text.glyphs);assert_eq!(a.world().get::<TextLayout>(l).unwrap().lines,text.lines);}
}
#[test] fn hidden_shown_revealed_and_missing_geometry_recover(){
 let(mut a,p)=scene();let t=Toolbar::mount(a.world_mut(),p,UiId(1),1744.).unwrap();let h=button(&mut a,t,2,LONG);let s=button(&mut a,t,3,"Go");a.world_mut().insert(p,Hidden);a.update();a.world_mut().remove::<Hidden>(p);a.update();verify(&a,t,&[h,s]);a.world_mut().insert(p,Hidden);Toolbar::mount(a.world_mut(),p,UiId(1),1696.).unwrap();a.update();a.world_mut().remove::<Hidden>(p);a.update();assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis,Axis::Column);a.world_mut().remove::<Rect>(h.label);a.world_mut().remove::<TextLayout>(h.label);Toolbar::mount(a.world_mut(),p,UiId(1),4000.).unwrap();a.update();assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis,Axis::Row);verify(&a,t,&[h,s]);
}
#[test] fn malformed_geometry_and_nonfinite_resize_do_not_adopt_or_panic(){
 let(mut a,p)=scene();let t=Toolbar::mount(a.world_mut(),p,UiId(1),1744.).unwrap();let h=button(&mut a,t,2,LONG);let s=button(&mut a,t,3,"Go");a.update();let old=*a.world().get::<Rect>(t.root).unwrap();for x in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY,-1.,0.] {assert_eq!(Toolbar::mount(a.world_mut(),p,UiId(1),x),Err(UiError::InvalidProps));a.update();assert_eq!(*a.world().get::<Rect>(t.root).unwrap(),old);}
 for x in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY] {a.world_mut().insert(h.label,Rect{x,y:x,w:x,h:x});a.update();verify(&a,t,&[h,s]);assert!(a.world().get::<Rect>(h.label).unwrap().w.is_finite());}
 let layout=a.world_mut().remove::<Layout>(h.label).unwrap();a.update();assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis,Axis::Column);a.world_mut().insert(h.label,layout);a.update();verify(&a,t,&[h,s]);
}
#[path="operating.rs"] mod operating;
#[test] fn fresh_reflow_preserves_actual_editor_history_selection_focus_and_source_payload(){
 let mut a=operating::create(800.,600.,UiTheme::default(),false);let d=a.world().resource::<operating::Demo>().unwrap().clone();request_focus(a.world_mut(),d.editor);a.world_mut().insert_resource(Platform::Windows);let before=a.world().get::<Buffer>(d.editor).unwrap().text();a.world_mut().resource_mut::<Input>().unwrap().push_key(KeyEvent::character('!',Modifiers::new()),Platform::Windows);a.update();a.world_mut().insert_resource(Input::default());let source=a.world().get::<Buffer>(d.editor).unwrap().text();assert_ne!(before,source);{let b=a.world_mut().get_mut::<Buffer>(d.editor).unwrap();b.caret=3;b.anchor=1;}
 let p=a.world_mut().spawn((Layout::column(),));let t=Toolbar::mount(a.world_mut(),p,UiId(900),1696.).unwrap();let h=button(&mut a,t,901,LONG);let s=button(&mut a,t,902,"Go");let locator=SourceLocator{document:19,revision:23,anchor:42};let action=SourceAction::mount(a.world_mut(),p,UiId(903),locator,true).unwrap();
 for n in 0..16 {Toolbar::mount(a.world_mut(),p,UiId(900),if n%2==0{1696.}else{6000.}).unwrap();a.world_mut().insert_resource(UiTheme{density:if n%2==0{Density::Compact}else{Density::Comfortable},text_scale:if n%3==0{2.}else{1.},..UiTheme::default()});a.world_mut().resource_mut::<Viewport>().unwrap().scale=[1.,1.25,1.75,2.][n%4];a.update();verify(&a,t,&[h,s]);let b=a.world().get::<Buffer>(d.editor).unwrap();assert_eq!((b.caret,b.anchor),(3,1));assert_eq!(b.text(),source);assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(d.editor));assert_eq!(try_activate(a.world_mut(),action.root),ActivationResult::Accepted);let req=take_action_intents(a.world_mut());assert_eq!(req.len(),1);assert_eq!(host_intent(a.world(),&req[0]),Some(HostIntent::RevealSource(locator)));}
 let mut mods=Modifiers::new();mods.ctrl=true;for (ch,expected) in [('z',before),('y',source)] {a.world_mut().resource_mut::<Input>().unwrap().push_key(KeyEvent::character(ch,mods),Platform::Windows);a.update();a.world_mut().insert_resource(Input::default());assert_eq!(a.world().get::<Buffer>(d.editor).unwrap().text(),expected);}
}
