use rndr_core::*;
fn key(app:&mut App,k:KeyEvent){app.world_mut().resource_mut::<Input>().unwrap().push_key(k,Platform::Mac);}
fn main(){
 let mut app=App::new();app.add_plugin(GenericInteraction);
 let b=app.world_mut().spawn((Layout::column(),Button::new("legacy")));app.update();
 key(&mut app,KeyEvent::named(NamedKey::Tab,Modifiers::new()));key(&mut app,KeyEvent::named(NamedKey::Enter,Modifiers::new()));app.update();
 println!("Tab+Enter legacy focus={:?} target={:?} Fired={:?}",app.world().resource::<Focus>().unwrap().entity,b,app.world().resource::<Fired>().unwrap().name);
 let mut app=App::new();app.add_plugin(GenericInteraction);
 let b=app.world_mut().spawn((Layout::column(),Button::new("legacy")));app.update();request_focus(app.world_mut(),b);
 key(&mut app,KeyEvent::named(NamedKey::Enter,Modifiers::new()));app.update();println!("separate-frame Enter legacy Fired={:?}",app.world().resource::<Fired>().unwrap().name);
}
