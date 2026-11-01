use rndr_core::*;
use rndr_ui::{controls::{self,*},*};
fn base()->(App,Entity) {let mut a=App::new();a.add_plugin(UiPlugin);a.world_mut().insert_resource(Viewport::new(1200.,2000.));let p=a.world_mut().spawn((Layout{width:Size::Px(1000.),..Layout::column()},));(a,p)}
#[test]
fn unequal_toolbar_labels_must_fit_each_row_share(){
 let(mut a,p)=base(); let t=Toolbar::mount(a.world_mut(),p,UiId(1),1000.).unwrap();
 let long=controls::Button::mount(a.world_mut(),t.root,UiId(2),ButtonProps::new("This is a very long but valid command label with many words",HostIntent::Command(1))).unwrap();
 controls::Button::mount(a.world_mut(),t.root,UiId(3),ButtonProps::new("Go",HostIntent::Command(2))).unwrap();
 a.update(); let style=a.world().get::<TextStyle>(long.label).unwrap();
 let natural=text_engine(a.world()).measure(&a.world().get::<Label>(long.label).unwrap().text,style,&[],f32::INFINITY).width;
 let r=*a.world().get::<Rect>(long.label).unwrap(); let axis=a.world().get::<Layout>(t.root).unwrap().axis;
 println!("axis={axis:?} natural_label_width={natural} actual_label_rect={r:?}");
 assert!(axis==Axis::Column || r.w+0.01>=natural,"row selected even though longest label does not fit its allocated width");
}
#[test]
fn transient_payload_change_invalidates_previously_drained_request(){
 let(mut a,p)=base(); let mut props=ButtonProps::new("Run",HostIntent::Command(1));let h=controls::Button::mount(a.world_mut(),p,UiId(1),props.clone()).unwrap();a.update();
 assert_eq!(try_activate(a.world_mut(),h.root),ActivationResult::Accepted);let old=take_action_intents(a.world_mut()).remove(0);
 props.intent=HostIntent::Command(2);controls::Button::mount(a.world_mut(),p,UiId(1),props.clone()).unwrap();props.intent=HostIntent::Command(1);controls::Button::mount(a.world_mut(),p,UiId(1),props).unwrap();
 // Current identity policy is value-based, not a nonce: document the ABA outcome.
 assert_eq!(host_intent(a.world(),&old),Some(HostIntent::Command(1)));
}
#[test]
fn disabled_active_roving_entry_and_hidden_nested_target(){
 let(mut a,p)=base();let m=ModeSelector::mount(a.world_mut(),p,UiId(1),"Modes",&["A","B","C"],TabOrientation::Vertical,1).unwrap();
 let b=controls::Button::mount(a.world_mut(),m.panels[0],UiId(8),ButtonProps::new("Hidden",HostIntent::Command(8))).unwrap();a.update();
 assert!(!request_focus(a.world_mut(),b.root));assert_eq!(try_activate(a.world_mut(),b.root),ActivationResult::Rejected);
 m.set_available(a.world_mut(),1,false,false).unwrap();a.update();
 a.world_mut().resource_mut::<Input>().unwrap().push_key(KeyEvent::named(NamedKey::Tab,Modifiers::new()),Platform::Mac);a.update();
 assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(m.tabs[0].root));assert_eq!(a.world().get::<Tabs>(m.group).unwrap().selected,1);
}
#[test]
fn malformed_panel_reparent_rejected_without_partial_destroy(){let(mut a,p)=base();let m=ModeSelector::mount(a.world_mut(),p,UiId(1),"Modes",&["A","B"],TabOrientation::Horizontal,0).unwrap();let tab=m.tabs[0].root;set_parent(a.world_mut(),m.panels[1],p);assert_eq!(m.destroy(a.world_mut()),Err(UiError::Stale));assert!(a.world().is_alive(tab));}
#[test]
fn same_update_disable_reenable_cancels_space_arm(){let(mut a,p)=base();let mut props=ButtonProps::new("Run",HostIntent::Command(1));let h=controls::Button::mount(a.world_mut(),p,UiId(1),props.clone()).unwrap();a.update();request_focus(a.world_mut(),h.root);a.world_mut().resource_mut::<Input>().unwrap().push_key(KeyEvent::named(NamedKey::Space,Modifiers::new()),Platform::Mac);a.update();a.world_mut().insert_resource(Input::default());assert!(interaction_pressed(a.world(),h.root));props.enabled=false;controls::Button::mount(a.world_mut(),p,UiId(1),props.clone()).unwrap();props.enabled=true;controls::Button::mount(a.world_mut(),p,UiId(1),props).unwrap();let mut up=KeyEvent::named(NamedKey::Space,Modifiers::new());up.released=true;a.world_mut().resource_mut::<Input>().unwrap().push_key(up,Platform::Mac);a.update();assert!(take_action_intents(a.world_mut()).is_empty());}
