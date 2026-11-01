"""Compile exact observation + paint source independently; only target scratch writes.
No implementation files are modified. Existing dependency artifacts are reused.
"""
from pathlib import Path
import subprocess, hashlib
root=Path.cwd(); out=root/'target/native-observation-review'; out.mkdir(exist_ok=True)
p=Path('evidence/native-observation-v1-review')
source=Path('crates/rndr_render/src/observe.rs').read_text()
source=source.replace('#[path = "observe_tests.rs"]', '#[path = "'+str((out/'tests.rs').resolve())+'"]')
(out/'observe.rs').write_text(source)
(out/'paint.rs').write_text(Path('crates/rndr_paint/src/lib.rs').read_text())
# Extra probes run inside the copied original tests module, preserving access to
# the author's fixtures without changing the sealed source.
extra=r'''
// Delegating engine with a distinct runtime type, not a different font algorithm.
struct ReviewEngine(BuiltinText);
macro_rules! delegate_engine {
    ($(fn $n:ident(&self $(, $a:ident : $t:ty)*) $(-> $r:ty)?;)*) => {
        $(fn $n(&self $(, $a : $t)*) $(-> $r)? { self.0.$n($($a),*) })*
    }
}
impl TextEngine for ReviewEngine {
    fn as_any(&self) -> &dyn std::any::Any { self }
    delegate_engine! {
        fn layout(&self, text: &str, style: &TextStyle, runs: &[TextSpan], width: f32) -> TextLayout;
        fn measure(&self, text: &str, style: &TextStyle, runs: &[TextSpan], width: f32) -> TextMeasure;
        fn line_height(&self, style: &TextStyle) -> f32;
        fn unit_advance(&self, style: &TextStyle) -> f32;
        fn solid_uv(&self, style: &TextStyle) -> [f32;4];
        fn coverage_atlas(&self) -> Option<Arc<FontAtlas>>;
        fn color_atlas(&self) -> Option<Arc<ColorAtlas>>;
        fn fallbacks(&self, family: &str) -> Vec<String>;
        fn set_fallbacks(&self, family: &str, faces: &[String]);
        fn register_font(&self, name: &str, font: Arc<dyn Font>);
        fn begin_frame(&self);
        fn shape_count(&self) -> u64;
    }
}
#[test]
fn review_post_paint_text_engine_replacement_is_accepted() {
    let (mut app, send) = fixture();
    rndr_core::ensure_text_engine(app.world_mut());
    app.world_mut().insert_resource(ObservationContent::Public);
    let original = rndr_core::text_engine(app.world());
    app.add_system(Schedule::Render, |world| {
        let replacement: Arc<dyn TextEngine> = Arc::new(ReviewEngine(BuiltinText::new()));
        world.insert_resource(replacement);
    });
    let recv = request(&send, 0);
    app.world_mut().spawn((Label::new("painted with builtin"),));
    let (_, capture) = crate::fresh_paint::frame_with_snapshot(&mut app, |w| begin(w, 2, 2, 1.));
    let current = rndr_core::text_engine(app.world());
    assert!(!Arc::ptr_eq(&original, &current));
    assert!(original.shape_count() > 0, "paint actually used the old engine");
    assert_eq!(text_engine_kind(app.world()), "custom");
    let capture=validate(app.world(), capture.unwrap(), 1.).unwrap();
    complete(capture,Ok(image(2,2)),true,json!({"text_engine":text_engine_kind(app.world())}));
    let (meta,_) = recv.recv().unwrap();
    assert_eq!(meta["render_provenance"]["text_engine"],"custom");
    println!("GAP: replaced text engine after paint accepted; identity/epoch/fonts are not bound");
}
#[test]
fn review_revocation_in_render_denies_without_either_channel() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.add_system(Schedule::Render, |w| w.resource::<ObservationSession>().unwrap().shutdown());
    let recv = request(&send, 0);
    let (_, capture) = crate::fresh_paint::frame_with_snapshot(&mut app, |w| begin(w,2,2,1.));
    assert!(validate(app.world(),capture.unwrap(),1.).is_none());
    assert_error(recv,"session_revoked");
}
#[test]
fn review_sensitive_removed_during_render_still_denies() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.world_mut().spawn((ObservationSensitive,));
    app.add_system(Schedule::Render, |w| {
        for e in w.entities_with::<ObservationSensitive>() { w.despawn_entity(e); }
    });
    let recv = request(&send, 0);
    let (_, capture) = crate::fresh_paint::frame_with_snapshot(&mut app, |w| begin(w,2,2,1.));
    assert!(capture.is_none()); assert_error(recv,"sensitive_frame");
}
'''
(out/'tests.rs').write_text(Path('crates/rndr_render/src/observe_tests.rs').read_text()+extra)
(out/'harness.rs').write_text('pub use rndr_render::{RgbaImage, encode_png};\nextern crate rndr_paint_review as fresh_paint;\n#[path="observe.rs"] mod observe;\n')
import shlex
line=next(l for l in Path('evidence/native-observation-v1/final-build-native.log').read_text().splitlines() if '--crate-name rndr_render ' in l)
args=shlex.split(line)
libs={}
for i,a in enumerate(args):
 if a=='--extern':
  name,path=args[i+1].split('=',1)
  if name in ['rndr_core','rndr_paint','serde_json','getrandom','png']: libs[name]=str(Path(path).with_suffix('.rlib'))
libs['rndr_render']=str(root/'target/debug/deps/librndr_render-98344a67c1d1af4d.rlib')
paintcmd=['rustc','--crate-name','rndr_paint_review','--crate-type','rlib','--edition=2021','-C','debuginfo=0','crates/rndr_paint/src/lib.rs','-L','dependency=target/debug/deps','--extern','rndr_core='+libs['rndr_core'],'-o',str(out/'libfresh_paint.rlib')]
paintcmd+=['--extern','lyon='+str(next(Path('target/debug/deps').glob('liblyon-*.rlib')))]
print('PAINT_COMMAND',repr(paintcmd),flush=True)
subprocess.run(paintcmd,check=True)
libs['rndr_paint_review']=str(out/'libfresh_paint.rlib')
cmd=['rustc','--test','--edition=2021','-C','debuginfo=0',str(out/'harness.rs'),'-L','dependency=target/debug/deps','-o',str(out/'probe-tests')]
for name,lib in libs.items(): cmd+=['--extern',f'{name}={lib}']
print('COMMAND',repr(cmd),flush=True)
for f in ['crates/rndr_render/src/observe.rs','crates/rndr_render/src/observe_tests.rs','crates/rndr_paint/src/lib.rs']:
 print('SOURCE',f,hashlib.sha256(Path(f).read_bytes()).hexdigest(),flush=True)
subprocess.run(cmd,check=True)
subprocess.run([str(out/'probe-tests'),'--nocapture','--test-threads=2'],check=True)
