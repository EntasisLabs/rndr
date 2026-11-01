use rndr_core::*;
struct Hotkeys;
impl hotkey::HotkeyBackend for Hotkeys {
 fn register(&mut self,_:hotkey::HotkeyId,_:&hotkey::PlatformAccel)->Result<(),hotkey::HotkeyFailure>{Ok(())}
 fn unregister(&mut self,_:hotkey::HotkeyId){}
 fn poll(&mut self)->Vec<hotkey::HotkeyId>{vec![]}
}
fn frame(a:&mut App, events:Vec<KeyEvent>){let i=a.world_mut().resource_mut::<Input>().unwrap();i.keys.clear();i.key=None;i.chars.clear();for e in events{i.push_key(e,Platform::Mac);}a.update();}
fn space()->KeyEvent{KeyEvent::named(NamedKey::Space,Modifiers::new())}
fn check(same_frame:bool, registered:bool) {
 let mut a=App::new();a.add_plugin(GenericInteraction);
 hotkey::install_backend(a.world_mut(),Hotkeys);
 let binding=if registered {Some(hotkey::register(a.world_mut(),"Alt+Space").unwrap())}else{None};
 let b=a.world_mut().spawn((Layout::column(),Button::new("go"),ReleaseActivation));a.update();assert!(request_focus(a.world_mut(),b));
 if same_frame{frame(&mut a,vec![space(),space().released()]);}else{frame(&mut a,vec![space()]);frame(&mut a,vec![space().released()]);}
 let got=take_action_intents(a.world_mut());assert_eq!(got.len(),1,"same_frame={same_frame}, registered={registered}, plain Space was not hotkey-owned");drop(binding);
}
#[test] fn plain_space_no_hotkey_same_frame(){check(true,false)}
#[test] fn plain_space_no_hotkey_split_frame(){check(false,false)}
#[test] fn unrelated_modified_hotkey_same_frame(){check(true,true)}
#[test] fn unrelated_modified_hotkey_split_frame(){check(false,true)}
