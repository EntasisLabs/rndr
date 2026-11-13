//! Headless gallery for slider, range, stepper, tooltip, and toast.
//!
//! Run: `cargo run -p rndr_ui --example inputs_gallery`
//!
//! The host applies each request on the next frame. That is the controlled
//! contract: the control does not publish a value until `mount` is called again.
use rndr_core::*;
use rndr_ui::controls::{Button, ButtonProps, HostIntent};
use rndr_ui::inputs::*;
use rndr_ui::{UiId, UiPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(360.0),
        gap: 12.0,
        padding: 16.0,
        ..Layout::column()
    },));
    let mut volume = 25.0;
    let span = (20.0, 70.0);
    let mut count = 2.0;
    let volume_props = SliderProps::new("Volume");
    let span_props = SliderProps::new("Span");
    let count_props = NumberProps::new("Count");
    let slider =
        Slider::mount(app.world_mut(), root, UiId(1), volume_props.clone(), volume).unwrap();
    let range = Range::mount(
        app.world_mut(),
        root,
        UiId(2),
        span_props.clone(),
        span.0,
        span.1,
    )
    .unwrap();
    let number =
        NumberField::mount(app.world_mut(), root, UiId(3), count_props.clone(), count).unwrap();
    let save = Button::mount(
        app.world_mut(),
        root,
        UiId(4),
        ButtonProps::new("Save", HostIntent::Command(1)),
    )
    .unwrap();
    Tooltip::attach(app.world_mut(), save.root, "Save the current values").unwrap();
    let toast = Toast::mount(
        app.world_mut(),
        root,
        UiId(5),
        ToastProps::timed("Gallery ready", std::time::Duration::from_secs(4)),
    )
    .unwrap();
    app.update();
    request_focus(app.world_mut(), slider.root);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
        KeyEvent::named(NamedKey::PageUp, Modifiers::default()),
    ];
    app.update();
    for change in events::<SliderChange>(app.world()) {
        volume = change.value;
    }
    Slider::mount(app.world_mut(), root, UiId(1), volume_props, volume).unwrap();
    request_focus(app.world_mut(), number.field);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::default())];
    app.update();
    for change in events::<NumberChange>(app.world()) {
        count = change.value;
    }
    NumberField::mount(app.world_mut(), root, UiId(3), count_props, count).unwrap();
    app.update();
    println!("volume {volume}");
    println!("span {} {}", span.0, span.1);
    println!("count {count}");
    println!("toast {}", access_id(toast.root));
    println!("range {}", access_id(range.root));
    println!(
        "slider role {:?}",
        access_tree(app.world())
            .nodes
            .iter()
            .find(|node| node.id == access_id(slider.root))
            .map(|node| node.role)
    );
}
