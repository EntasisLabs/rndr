use rndr_core::*;
use rndr_ui::{collections::*, data::*, UiId, UiPlugin};
use rndr_ui::collections::Item;
fn app(width:f32) -> (App,Entity) {
    let mut a=App::new(); a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(width,500.));
    let r=a.world_mut().spawn((Layout {width:Size::Fill,height:Size::Fill,..Layout::column()},));
    (a,r)
}
#[test]
fn table_column_boundaries_share_one_grid_in_both_densities_and_after_scroll() {
    for compact in [false,true] { for width in [800.,220.] {
        let (mut a,r)=app(width);
        a.world_mut().insert_resource(rndr_ui::UiTheme {density:if compact {rndr_ui::Density::Compact} else {rndr_ui::Density::Comfortable},..Default::default()});
        let cols:Vec<_>=[130.,160.,90.].iter().enumerate().map(|(i,w)|Column{id:ColumnId(i as u64+1),label:format!("Column {i}"),width:*w,alignment:CellAlignment::Start,sortable:true}).collect();
        let h=DataTable::mount(a.world_mut(),r,UiId(1),TableProps::new("Grid"),&cols,&[Item::new(1,"Record")],|w,slot,_,_| {rndr_ui::text(w,slot.content,"Value",rndr_ui::Typography::Label).unwrap();}).unwrap();
        rndr_paint::frame(&mut a);
        for scroll in [0.,-100.] {
            a.world_mut().get_mut::<Scroll>(h.root).unwrap().offset[0]=scroll;
            rndr_paint::frame(&mut a);
            for c in &cols {
                let header=*a.world().get::<Rect>(h.header(a.world(),c.id).unwrap()).unwrap();
                let cell=*a.world().get::<Rect>(h.cell(a.world(),ItemId(1),c.id).unwrap().root).unwrap();
                assert!((header.x-cell.x).abs()<0.01,"density {compact} width {width}: header {header:?} cell {cell:?}");
                assert!((header.w-cell.w).abs()<0.01,"header/cell widths {header:?} {cell:?}");
            }
        }
    }}
}
fn nested() -> (App,Entity,Entity) {
    let (mut a,outer)=app(200.);
    a.world_mut().get_mut::<Layout>(outer).unwrap().height=Size::Px(150.);
    a.world_mut().insert(outer,Scroll::default());
    let inner=a.world_mut().spawn((Layout{width:Size::Px(180.),height:Size::Px(100.),..Layout::column()},Scroll::default()));
    set_parent(a.world_mut(),inner,outer);
    let content=a.world_mut().spawn((Layout{width:Size::Px(400.),height:Size::Px(300.),..Layout::column()},));
    set_parent(a.world_mut(),content,inner);
    let tail=a.world_mut().spawn((Layout{width:Size::Px(180.),height:Size::Px(300.),..Layout::column()},));
    set_parent(a.world_mut(),tail,outer);
    rndr_paint::frame(&mut a);
    (a,outer,inner)
}
fn wheel(a:&mut App,p:[f32;2],delta:[f32;2]) {
    let i=a.world_mut().resource_mut::<Input>().unwrap(); i.pointer.position=p;i.scroll=delta;
    apply_scroll(a.world_mut());
}
#[test]
fn nested_wheel_consumes_only_available_distance_and_chains_each_axis() {
    let (mut a,o,i)=nested();
    wheel(&mut a,[50.,50.],[-30.,-230.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset,[-30.,-200.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset,[0.,-30.]);
    wheel(&mut a,[50.,20.],[0.,50.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset[1],-150.);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset[1],-30.);
    assert_eq!(a.world().resource::<Input>().unwrap().scroll,[0.,0.]);
}
#[test]
fn outside_or_clipped_content_never_targets_an_unrelated_scrollport() {
    let (mut a,o,i)=nested();
    wheel(&mut a,[250.,50.],[0.,-70.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset,[0.,0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset,[0.,0.]);
    wheel(&mut a,[50.,125.],[0.,-70.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset,[0.,0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset,[0.,-70.]);
}
#[test]
fn non_scrollable_inner_axis_chains_without_remapping() {
    let (mut a,o,i)=nested();
    let child=children_of(a.world(),i)[0];
    a.world_mut().get_mut::<Layout>(child).unwrap().height=Size::Px(80.);
    layout_propagate(a.world_mut());
    wheel(&mut a,[50.,50.],[-20.,-40.]);
    assert_eq!(a.world().get::<Scroll>(i).unwrap().offset,[-20.,0.]);
    assert_eq!(a.world().get::<Scroll>(o).unwrap().offset,[0.,-40.]);
}
