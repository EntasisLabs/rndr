//! Identical fixture for exact A1 source and current-source rlibs.
use rndr_core::*;
#[test]
fn tab_release_on_its_label_descendant_must_select() {
    let mut a=App::new();a.add_plugin(GenericInteraction);
    let root=a.world_mut().spawn((Layout{width:Size::Px(300.0),..Layout::column()},));
    let group=a.world_mut().spawn((Layout::row(),Tabs{selected:0}));set_parent(a.world_mut(),group,root);
    let mut targets=vec![];
    for id in 0..2 {let panel=a.world_mut().spawn((Layout::column(),TabPanel));set_parent(a.world_mut(),panel,root);let tab=a.world_mut().spawn((Layout{width:Size::Px(100.0),height:Size::Px(50.0),..Layout::column()},Tab{id},ReleaseActivation,Focusable));set_parent(a.world_mut(),tab,group);let label=a.world_mut().spawn((Layout{width:Size::Fill,height:Size::Fill,..Layout::column()},Label::new("named tab")));set_parent(a.world_mut(),label,tab);link_tab_panel(a.world_mut(),tab,group,panel).unwrap();targets.push(tab);}
    a.update();let r=*a.world().get::<Rect>(targets[1]).unwrap();
    for down in [true,false] {let p=&mut a.world_mut().resource_mut::<Input>().unwrap().pointer;p.position=[r.x+12.0,r.y+12.0];p.press_at=Some(p.position);p.pressed=down;p.released=!down;p.down=down;a.update();}
    assert_eq!(a.world().get::<Tabs>(group).unwrap().selected,1,"release over the tab label descendant must select that tab");
}
