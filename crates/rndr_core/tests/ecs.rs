use rndr_core::{App, Component, Entity, Schedule, World};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tag {
    n: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Pos {
    x: f32,
}

impl Component for Tag {}
impl Component for Pos {}

#[test]
fn spawn_query_and_generation_reuse() {
    let mut world = World::new();
    let a = world.spawn(Tag { n: 1 });
    let b = world.spawn((Tag { n: 2 }, Pos { x: 4.0 }));
    assert!(world.is_alive(a));
    assert_eq!(world.get::<Tag>(a).unwrap().n, 1);
    assert_eq!(world.get::<Pos>(b).unwrap().x, 4.0);
    assert_eq!(world.query::<Tag>().len(), 2);

    world.despawn_entity(a);
    assert!(!world.is_alive(a));
    assert!(world.get::<Tag>(a).is_none());

    let c = world.spawn(Tag { n: 3 });
    assert_ne!(a, c);
    assert_eq!(c.index(), a.index());
    assert_ne!(c.generation(), a.generation());
    assert!(world.is_alive(c));
    assert!(!world.is_alive(a));
    assert_eq!(world.get::<Tag>(c).unwrap().n, 3);
}

#[test]
fn stale_handle_after_recycle() {
    let mut world = World::new();
    let first = world.spawn(Tag { n: 1 });
    world.despawn_entity(first);
    let _second = world.spawn(Tag { n: 2 });
    world.insert(first, Tag { n: 99 });
    assert!(world.get::<Tag>(first).is_none());
}

#[test]
fn headless_app_runs_startup_once_then_update() {
    fn startup(world: &mut World) {
        world.spawn(Tag { n: 1 });
    }
    fn tick(world: &mut World) {
        let entity = world.entities_with::<Tag>()[0];
        world.get_mut::<Tag>(entity).unwrap().n += 1;
    }

    let mut app = App::new();
    app.add_system(Schedule::Startup, startup)
        .add_system(Schedule::Update, tick);
    app.update();
    app.update();
    let entity: Entity = app.world().entities_with::<Tag>()[0];
    assert_eq!(app.world().get::<Tag>(entity).unwrap().n, 3);
}
