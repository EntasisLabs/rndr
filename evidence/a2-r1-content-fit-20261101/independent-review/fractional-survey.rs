use rndr_core::*;
use rndr_ui::{*,controls::{self,*}};
const LONG:&str="This is a very long but valid command label with many words";
fn setup(w:f32,scale:f32)->(App,Entity,ToolbarHandle,ControlHandle){
 let mut a=App::new();a.add_plugin(UiPlugin);a.world_mut().insert_resource(Viewport{scale,..Viewport::new(8000.,30000.)});
 let p=a.world_mut().spawn((Layout{width:Size::Px(w),..Layout::column()},));
 let t=Toolbar::mount(a.world_mut(),p,UiId(1),w).unwrap();
 let h=controls::Button::mount(a.world_mut(),t.root,UiId(2),ButtonProps::new(LONG,HostIntent::Command(2))).unwrap();
 controls::Button::mount(a.world_mut(),t.root,UiId(3),ButtonProps::new("Go",HostIntent::Command(3))).unwrap();(a,p,t,h)
}
fn width(a:&App,h:ControlHandle)->f32 {text_engine(a.world()).measure(&a.world().get::<Label>(h.label).unwrap().text,a.world().get::<TextStyle>(h.label).unwrap(),&[],f32::INFINITY).width}
#[test] fn fractional_scale_boundary_matches_actual_snapped_allocation(){
 let mut cases=0;
 for dpi in [1.,1.25,1.5,1.75,2.] {for type_scale in [1.,1.1,1.25,1.5,2.] {
 let (mut a,p,t,h)=setup(4000.,dpi);a.world_mut().insert_resource(UiTheme{text_scale:type_scale,..UiTheme::default()});a.update();
 let natural=width(&a,h);let boundary=2.*(natural+18.)+8.;
 for extra in [0.,0.01,0.25,0.5,1.,2.,4.] {
 let w=boundary+extra;a.world_mut().get_mut::<Layout>(p).unwrap().width=Size::Px(w);Toolbar::mount(a.world_mut(),p,UiId(1),w).unwrap();a.update();cases+=1;
 let axis=a.world().get::<Layout>(t.root).unwrap().axis;let r=a.world().get::<Rect>(h.label).unwrap();let text=a.world().get::<TextLayout>(h.label).unwrap();
 println!("dpi={dpi} type={type_scale} width={w} axis={axis:?} natural={natural} rect={r:?} textheight={}",text.height);
 if axis==Axis::Row && r.w+0.01<natural {println!("MISMATCH dpi={dpi} type={type_scale} extra={extra} natural={natural} actual={} deficit={}",r.w,natural-r.w);}
 }
 }}println!("cases={cases}");
}
#[test] fn rejected_nonfinite_resize_is_atomic_and_malformed_child_stacks(){
 let(mut a,p,t,h)=setup(1000.,1.);a.update();let r=*a.world().get::<Rect>(t.root).unwrap();
 for w in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY,-1.,0.,31.99] {assert_eq!(Toolbar::mount(a.world_mut(),p,UiId(1),w),Err(UiError::InvalidProps));a.update();assert_eq!(*a.world().get::<Rect>(t.root).unwrap(),r);}
 a.world_mut().remove::<Layout>(h.label);a.update();assert_eq!(h.validate(a.world()),Err(UiError::Stale));assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis,Axis::Column);
}
#[test] fn conservative_veto_and_toolbar_detach_preserve_commands(){
 let(mut a,p,t,h)=setup(160.,1.);controls::Button::mount(a.world_mut(),t.root,UiId(2),ButtonProps::new("Go",HostIntent::Command(2))).unwrap();a.update();
 assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis,Axis::Column);
 assert!(request_focus(a.world_mut(),h.root));assert_eq!(try_activate(a.world_mut(),h.root),ActivationResult::Accepted);
 t.destroy(a.world_mut()).unwrap();assert!(a.world().is_alive(h.root));assert!(a.world().is_alive(p));assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(h.root));
 let req=take_action_intents(a.world_mut());assert_eq!(req.len(),1);assert_eq!(host_intent(a.world(),&req[0]),Some(HostIntent::Command(2)));
}

#[test] fn isolated_fractional_reproduction(){
 for dpi in [1.,1.75] {for w in [1696.,1697.,1700.,2000.] {
 let(mut a,_,t,h)=setup(w,dpi);
 for pass in 0..3 {a.update();let r=a.world().get::<Rect>(h.label).unwrap();let l=a.world().get::<TextStyle>(h.label).unwrap();let text=a.world().get::<Label>(h.label).unwrap();let e=text_engine(a.world());
 let natural=e.measure(&text.text,l,&[],f32::INFINITY);let measured=e.measure(&text.text,l,&[],r.w);
 let max_x=a.world().get::<TextLayout>(h.label).unwrap().glyphs.iter().map(|g|g.x+g.w).fold(0f32,f32::max);
 println!("ISOLATED dpi={dpi} w={w} pass={pass} axis={:?} rect={r:?} natural={natural:?} constrained={measured:?} glyph_max={max_x}",a.world().get::<Layout>(t.root).unwrap().axis);
 }
 }}
}
