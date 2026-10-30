use rndr_core::{App, CloseGuard, CloseState, Input, Schedule, WindowConfig};
use std::sync::{Arc, atomic::{AtomicBool,Ordering}};
fn main() {
    let built=Arc::new(AtomicBool::new(false));
    let dropped=Arc::new(AtomicBool::new(false));
    struct Draft { text:String, dropped:Arc<AtomicBool> }
    impl Drop for Draft {fn drop(&mut self) {self.dropped.store(true,Ordering::SeqCst);eprintln!("QUEUED_DRAFT_DROPPED {}",self.text);}}
    let mut app=App::new();
    app.world_mut().insert_resource(CloseGuard::default());
    app.world_mut().resource_mut::<Input>().unwrap().quit=true;
    app.world_mut().insert_resource((built.clone(),dropped.clone()));
    app.add_system(Schedule::Update,|world| {
        let state=world.resource::<CloseGuard>().unwrap().state().clone();
        if let CloseState::Pending(r)=state {
            let (seen,gone)=world.resource::<(Arc<AtomicBool>,Arc<AtomicBool>)>().unwrap().clone();
            let draft=Draft{text:"synthetic unsaved late window bytes".into(),dropped:gone.clone()};
            let mark=seen.clone();
            rndr_core::desktop(world).unwrap().open(WindowConfig::default(), move |new| {
                mark.store(true,Ordering::SeqCst);
                eprintln!("LATE_WINDOW_BUILT");
                new.world_mut().insert_resource(CloseGuard::default());
                new.world_mut().insert_resource(draft);
                new.add_system(Schedule::Update,|w| eprintln!("LATE_GUARD_STATE {:?}", w.resource::<CloseGuard>().unwrap().state()));
            });
            eprintln!("OPEN_QUEUED_IN_PENDING_FRAME");
            world.resource_mut::<CloseGuard>().unwrap().complete(r,Ok(()));
        }
    });
    rndr_render::run(app);
    eprintln!("RUN_RETURNED late_built={} queued_draft_dropped={}",built.load(Ordering::SeqCst),dropped.load(Ordering::SeqCst));
    panic!("BUG: run returned although late guard never authorized closure");
}
