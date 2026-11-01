//! Regression for independent review of 01da8acd: registration is not ownership.
use rndr_core::*;
use std::sync::{Arc, Mutex};

type Edges = Arc<Mutex<Vec<hotkey::HotkeyEdge>>>;
struct Backend(Edges);
impl hotkey::HotkeyBackend for Backend {
    fn register(
        &mut self,
        _: hotkey::HotkeyId,
        _: &hotkey::PlatformAccel,
    ) -> Result<(), hotkey::HotkeyFailure> {
        Ok(())
    }
    fn unregister(&mut self, _: hotkey::HotkeyId) {}
    fn poll(&mut self) -> Vec<hotkey::HotkeyId> {
        vec![]
    }
    fn poll_edges(&mut self) -> Vec<hotkey::HotkeyEdge> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}
fn setup() -> (App, Entity, Edges) {
    let mut a = App::new();
    a.add_plugin(GenericInteraction);
    a.world_mut().insert_resource(Platform::Mac);
    let edges = Edges::default();
    hotkey::install_backend(a.world_mut(), Backend(edges.clone()));
    let b = a
        .world_mut()
        .spawn((Layout::column(), Button::new("go"), ReleaseActivation));
    a.update();
    assert!(request_focus(a.world_mut(), b));
    (a, b, edges)
}
fn key(k: NamedKey) -> KeyEvent {
    KeyEvent::named(k, Modifiers::new())
}
fn frame(a: &mut App, keys: Vec<KeyEvent>) {
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.keys.clear();
    i.key = None;
    i.chars.clear();
    for e in keys {
        i.push_key(e, Platform::Mac);
    }
    a.update();
}
fn assert_count(a: &mut App, n: usize) {
    assert_eq!(take_action_intents(a.world_mut()).len(), n);
    assert!(a.world().resource::<Fired>().unwrap().name.is_none());
}

fn logical(k: NamedKey) -> KeyEvent { let mut e=key(k); e.code=None; e }
#[test]
fn independent_true_logical_only_matrix() {
 for platform in [Platform::Mac, Platform::Windows, Platform::X11, Platform::Wayland] {
  for k in [NamedKey::Space,NamedKey::Enter] { for split in [false,true] {
   for chord in ["Alt+Space","Ctrl+Space","Alt+Enter","Ctrl+Enter","primary+Space"] {
    let (mut a,_,_)=setup(); a.world_mut().insert_resource(platform);
    let _h=hotkey::register(a.world(),chord).unwrap();
    let d=logical(k); assert_eq!(d.code,None);
    if split { frame(&mut a,vec![d.clone()]); frame(&mut a,vec![d.released()]); }
    else { frame(&mut a,vec![d.clone(),d.released()]); }
    assert_count(&mut a,1);
   }
  }}
 }
}
#[test]
fn independent_logical_only_guard_stale_and_synthetic_up() {
 for k in [NamedKey::Space,NamedKey::Enter] {
  let (mut a,_,edges)=setup(); let h=hotkey::register(a.world(),if k==NamedKey::Space {"Alt+Space"}else{"Alt+Enter"}).unwrap();
  frame(&mut a,vec![logical(NamedKey::Space)]);
  hotkey::note_key_order(a.world_mut(),20);
  edges.lock().unwrap().push(hotkey::HotkeyEdge::Press(h.id()));
  let mut stale=logical(k).released(); stale.order=19;
  frame(&mut a,vec![stale.clone(),stale.synthetic(),logical(k).repeating()]);
  assert_count(&mut a,0);
  let mut up=logical(k).released(); up.order=21;
  frame(&mut a,vec![up]); assert_count(&mut a,0);
  frame(&mut a,vec![logical(k),logical(k).released()]); assert_count(&mut a,1);
 }
}
#[test]
fn independent_physical_binding_true_logical_input_is_not_owned() {
 for k in [NamedKey::Space,NamedKey::Enter] {
  let (mut a,_,_)=setup();
  let stroke=Keystroke::code(if k==NamedKey::Space {KeyCode::Space}else{KeyCode::Enter}).alt();
  let _h=hotkey::register(a.world(),stroke).unwrap();
  frame(&mut a,vec![logical(k),logical(k).released()]); assert_count(&mut a,1);
 }
}
#[test]
fn independent_registration_dropped_while_armed_does_not_steal_release() {
 let (mut a,_,_)=setup(); let h=hotkey::register(a.world(),"Alt+Space").unwrap();
 frame(&mut a,vec![logical(NamedKey::Space)]); drop(h);
 frame(&mut a,vec![logical(NamedKey::Space).released()]); assert_count(&mut a,1);
}
#[test]
fn independent_focus_roundtrip_and_repeated_release_never_commit() {
 let (mut a,b,_)=setup(); let _h=hotkey::register(a.world(),"Alt+Space").unwrap();
 let other=a.world_mut().spawn((Layout::column(),Button::new("other"),ReleaseActivation));
 frame(&mut a,vec![logical(NamedKey::Space)]);
 request_focus(a.world_mut(),other); request_focus(a.world_mut(),b);
 frame(&mut a,vec![logical(NamedKey::Space).released()]); assert_count(&mut a,0);
 frame(&mut a,vec![logical(NamedKey::Space),logical(NamedKey::Space).released().repeating()]); assert_count(&mut a,0);
 frame(&mut a,vec![logical(NamedKey::Space).released()]); assert_count(&mut a,0);
}
#[test]
fn independent_fifo_same_target_64_no_fanout() {
 let (mut a,b,_)=setup();
 for _ in 0..64 { assert_eq!(try_activate(a.world_mut(),b),ActivationResult::Accepted); }
 assert_eq!(try_activate(a.world_mut(),b),ActivationResult::Overflow);
 assert_eq!(a.world().resource::<ActionIntents>().unwrap().overflow,1);
 let v=take_action_intents(a.world_mut()); assert_eq!(v.len(),64);
 assert!(v.iter().all(|i| i.entity==b));
 assert_count(&mut a,0);
}
#[test]
fn independent_modified_release_consumes_arm_then_plain_release_cannot_commit() {
 let (mut a,_,_)=setup(); let _h=hotkey::register(a.world(),"Alt+Enter").unwrap();
 frame(&mut a,vec![logical(NamedKey::Space)]);
 let mut up=logical(NamedKey::Space).released();up.modifiers.ctrl=true;
 frame(&mut a,vec![up]); frame(&mut a,vec![logical(NamedKey::Space).released()]);assert_count(&mut a,0);
 frame(&mut a,vec![logical(NamedKey::Space),logical(NamedKey::Space).released()]);assert_count(&mut a,1);
}
#[test]
fn independent_nonmatching_logical_registration_preserves_editor_and_legacy() {
 let (mut a,_,_)=setup(); let _h=hotkey::register(a.world(),"Alt+Space").unwrap();
 let field=a.world_mut().spawn((Layout::column(),TextField,CapturesKeys,Label::new("")));
 request_focus(a.world_mut(),field);
 frame(&mut a,vec![logical(NamedKey::Space).with_text(" "),logical(NamedKey::Space).released()]);
 assert_eq!(a.world().get::<Label>(field).unwrap().text," "); assert_count(&mut a,0);
 let old=a.world_mut().spawn((Layout::column(),Button::new("old")));
 request_focus(a.world_mut(),old);
 frame(&mut a,vec![logical(NamedKey::Enter),logical(NamedKey::Enter).released()]);
 assert_eq!(a.world().resource::<Fired>().unwrap().name.as_deref(),Some("old"));
 assert!(take_action_intents(a.world_mut()).is_empty());
}
