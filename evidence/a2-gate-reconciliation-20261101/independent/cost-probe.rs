use rndr_core::*; use rndr_ui::{*,controls::{self,*}}; use std::{sync::{Arc,atomic::{AtomicUsize,Ordering}},any::Any};
struct Count {inner:BuiltinText,calls:AtomicUsize,unrelated:AtomicUsize}
impl TextEngine for Count {
 fn as_any(&self)->&dyn Any{self}
 fn layout(&self,t:&str,s:&TextStyle,r:&[TextSpan],w:f32)->TextLayout{self.calls.fetch_add(1,Ordering::Relaxed);if t.starts_with("UNRELATED"){self.unrelated.fetch_add(1,Ordering::Relaxed);}self.inner.layout(t,s,r,w)}
 fn measure(&self,t:&str,s:&TextStyle,r:&[TextSpan],w:f32)->TextMeasure{self.inner.measure(t,s,r,w)}
 fn line_height(&self,s:&TextStyle)->f32{self.inner.line_height(s)} fn unit_advance(&self,s:&TextStyle)->f32{self.inner.unit_advance(s)} fn solid_uv(&self,s:&TextStyle)->[f32;4]{self.inner.solid_uv(s)}
 fn coverage_atlas(&self)->Option<Arc<FontAtlas>>{self.inner.coverage_atlas()} fn color_atlas(&self)->Option<Arc<ColorAtlas>>{self.inner.color_atlas()}
 fn fallbacks(&self,s:&str)->Vec<String>{self.inner.fallbacks(s)} fn set_fallbacks(&self,s:&str,f:&[String]){self.inner.set_fallbacks(s,f)} fn register_font(&self,s:&str,f:Arc<dyn Font>){self.inner.register_font(s,f)}
 fn begin_frame(&self){self.inner.begin_frame()} fn shape_count(&self)->u64{self.inner.shape_count()} fn cache_epoch(&self)->u64{self.inner.cache_epoch()}
}
#[test] fn counts_whole_world_passes_not_native_time(){
 for reject in [false,true] {for n in [0,1,8,32] {
 let mut a=App::new();a.add_plugin(UiPlugin);a.world_mut().insert_resource(Viewport::new(10000.,50000.));let c=Arc::new(Count{inner:BuiltinText::new(),calls:AtomicUsize::new(0),unrelated:AtomicUsize::new(0)});let engine:Arc<dyn TextEngine>=c.clone();a.world_mut().insert_resource(engine);
 for i in 0..100 {a.world_mut().spawn((Layout{width:Size::Px(500.),..Layout::column()},Label::new(format!("UNRELATED-{i}"))));}
 let p=a.world_mut().spawn((Layout::column(),));
 for i in 0..n {let t=Toolbar::mount(a.world_mut(),p,UiId(i*3),if reject{1696.}else{2000.}).unwrap();for (j,s) in ["This is a very long but valid command label with many words","Go"].iter().enumerate(){controls::Button::mount(a.world_mut(),t.root,UiId(i*3+1+j as u64),ButtonProps::new(*s,HostIntent::Command(1))).unwrap();}}
 a.update();let u=c.unrelated.load(Ordering::Relaxed);println!("reject={reject} candidates={n} unrelated_layout_calls={u} all_layout_calls={} shapes={}",c.calls.load(Ordering::Relaxed),c.shape_count());assert_eq!(u,100*if n==0{1}else if reject{3}else{2});
 }}
}
