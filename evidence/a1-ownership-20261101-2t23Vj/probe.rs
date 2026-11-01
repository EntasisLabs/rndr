use rndr_core::*;
fn key(app:&mut App,k:KeyEvent){app.world_mut().resource_mut::<Input>().unwrap().push_key(k,Platform::Mac);}
fn main(){
 let mut app=App::new();app.add_plugin(GenericInteraction);
 let b=app.world_mut().spawn((Layout::column(),Button::new("go"),ReleaseActivation));
 app.update();
 key(&mut app,KeyEvent::named(NamedKey::Tab,Modifiers::new()));
 key(&mut app,KeyEvent::named(NamedKey::Space,Modifiers::new()));
 key(&mut app,KeyEvent::named(NamedKey::Space,Modifiers::new()).released());
 app.update();let f=app.world().resource::<Focus>().unwrap().entity;println!("Tab+Space down+up focus={:?} target={:?} intents={:?}",f,b,take_action_intents(app.world_mut()));
 let input=app.world_mut().resource_mut::<Input>().unwrap();input.keys.clear();input.key=None;
 key(&mut app,KeyEvent::named(NamedKey::Space,Modifiers::new()).released());app.update();println!("subsequent unarmed release intents={:?}",take_action_intents(app.world_mut()));
}
