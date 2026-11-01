//! Unit/synthetic protocol tests, NOT proof of native presentation.
use super::*;
use rndr_core::*;

fn fixture() -> (App, mpsc::SyncSender<Request>) {
    let mut app = App::new();
    let (send, recv) = mpsc::sync_channel(1);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "rndr-observe-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    app.world_mut().insert_resource(ObservationSession {
        requests: Mutex::new(recv),
        stop: Arc::new(AtomicBool::new(false)),
        worker: Mutex::new(None),
        directory,
        revision: AtomicU64::new(0),
        theme: "test-light".into(),
    });
    (app, send)
}
fn request(send: &mpsc::SyncSender<Request>, after: u64) -> mpsc::Receiver<(Value, Vec<u8>)> {
    let (reply, recv) = mpsc::sync_channel(1);
    send.send(Request {
        after,
        reply,
        deadline: Instant::now() + Duration::from_secs(2),
    })
    .unwrap();
    recv
}
fn image(width: u32, height: u32) -> crate::RgbaImage {
    crate::RgbaImage {
        width,
        height,
        rgba: vec![255; width as usize * height as usize * 4],
    }
}
fn assert_error(recv: mpsc::Receiver<(Value, Vec<u8>)>, code: &str) {
    let (value, png) = recv.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(value["error"], code);
    assert_eq!(value["png_bytes"], 0);
    assert!(png.is_empty());
    assert!(value.get("nodes").is_none());
    assert!(value.get("label").is_none());
}
#[test]
fn disabled_by_default_and_auth_is_exact() {
    assert!(App::new()
        .world()
        .resource::<ObservationSession>()
        .is_none());
    let token = "a".repeat(64);
    assert_eq!(
        authenticate(format!("RNDR1 {token} 0\n").as_bytes(), &token).unwrap(),
        0
    );
    for line in [
        format!("RNDR1 {} 0\n", "b".repeat(64)),
        format!("RNDR1 {token} -1\n"),
        format!("RNDR1 {token} 1 extra\n"),
        format!("EXEC {token} 0\n"),
        format!("RNDR1 {token} 18446744073709551616\n"),
        format!("RNDR1 {token} 0\r\n"),
        format!("RNDR1 {token} 0\n\n"),
        String::new(),
    ] {
        assert!(authenticate(line.as_bytes(), &token).is_err(), "{line:?}");
    }
}
#[test]
fn missing_or_deny_attestation_exposes_neither_pixels_nor_semantics() {
    let (mut app, send) = fixture();
    app.world_mut().spawn((Label::new("secret-name"),));
    for deny in [false, true] {
        if deny {
            app.world_mut().insert_resource(ObservationContent::Deny);
        }
        let recv = request(&send, 0);
        assert!(begin(app.world(), 2, 2, 1.).is_none());
        assert_error(recv, "sensitive_frame");
    }
}
#[test]
fn sensitive_hidden_or_unsemantic_entity_denies_whole_frame() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.world_mut()
        .spawn((ObservationSensitive, Hidden, Label::new("secret")));
    let recv = request(&send, 0);
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "sensitive_frame");
}
#[test]
fn stale_future_and_expired_requests_never_return_cached_frame() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let recv = request(&send, 100);
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "stale_revision");
    let (reply, recv) = mpsc::sync_channel(1);
    send.send(Request {
        after: 0,
        deadline: Instant::now() - Duration::from_secs(1),
        reply,
    })
    .unwrap();
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "expired");
}
#[test]
fn invalid_and_oversized_viewports_are_denied_before_allocation() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    for (w, h, s) in [
        (0, 1, 1.),
        (1, 0, 1.),
        (2001, 2000, 1.),
        (1, 4_000_000, 1.),
        (4_000_000, 1, 1.),
        (u32::MAX, u32::MAX, 1.),
        (2, 2, 0.),
        (2, 2, f32::NAN),
    ] {
        let recv = request(&send, 0);
        assert!(begin(app.world(), w, h, s).is_none());
        assert_error(recv, "size_limit");
    }
}
#[test]
fn semantics_reuse_actions_states_focus_and_unflattened_scroll() {
    let (mut app, send) = fixture();
    assert_eq!(text_engine_kind(app.world()), "builtin_atlas");
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(ObservationContent::Public);
    let button = app.world_mut().spawn((
        Button::new("Save \"draft\""),
        ReleaseActivation,
        Rect {
            x: 2.,
            y: 3.,
            w: 40.,
            h: 20.,
        },
        SelectedState(true),
        Semantics {
            name: Some("Save \"draft\"".into()),
            ..Semantics::default()
        },
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(button);
    let port = app.world_mut().spawn((
        Scroll {
            offset: [0., -20.],
            content: [100., 400.],
        },
        Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 100.,
        },
    ));
    let recv = request(&send, 0);
    let capture = begin(app.world(), 4, 3, 2.).unwrap();
    let capture = validate(app.world(), capture, 2.).unwrap();
    complete(capture, Ok(image(4, 3)), true, "Bgra8UnormSrgb");
    let (meta, png) = recv.recv().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(meta["png_bytes"], png.len());
    assert_eq!(meta["presented_confirmation"], false);
    assert_eq!(meta["device_scale"], 2.);
    let node = meta["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == access_id(button).to_string())
        .unwrap();
    assert_eq!(node["label"], "Save \"draft\"");
    assert_eq!(node["focused"], true);
    assert_eq!(node["selected"], true);
    assert_eq!(node["bounds"], json!([2., 3., 40., 20.]));
    assert_eq!(node["actions"], json!(["focus", "activate"]));
    assert_eq!(meta["scroll"][0]["id"], access_id(port).to_string());
    assert_eq!(meta["scroll"][0]["offset"], json!([0., -20.]));
}
#[test]
fn same_frame_resize_revision_and_image_dimensions_remain_bound() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let mut last = 0;
    for (w, h, scale) in [(2, 3, 1.), (4, 2, 2.), (1, 1, 1.25)] {
        let recv = request(&send, last);
        let capture = begin(app.world(), w, h, scale).unwrap();
        complete(capture, Ok(image(w, h)), true, "Rgba8UnormSrgb");
        let (meta, png) = recv.recv().unwrap();
        let rev = meta["frame_revision"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(rev > last);
        last = rev;
        assert_eq!(meta["width"], w);
        assert_eq!(meta["height"], h);
        assert_eq!(meta["device_scale"], scale);
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().width, w);
        assert_eq!(reader.info().height, h);
    }
}
#[test]
fn surface_loss_readback_failure_or_mismatched_image_exposes_nothing() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    for (pixels, submitted, code) in [
        (Ok(image(2, 2)), false, "surface_unavailable"),
        (Err("gpu_timeout"), true, "gpu_timeout"),
        (Ok(image(1, 1)), true, "size_mismatch"),
    ] {
        let recv = request(&send, 0);
        let capture = begin(app.world(), 2, 2, 1.).unwrap();
        complete(capture, pixels, submitted, "test");
        assert_error(recv, code);
    }
}
#[test]
fn snapshot_precedes_render_and_mutations_fail_closed() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.world_mut().spawn((Label::new("before"),));
    app.add_system(Schedule::Render, |world| {
        let e = world.entities_with::<Label>()[0];
        world.get_mut::<Label>(e).unwrap().text = "after".into();
    });
    let recv = request(&send, 0);
    let (_, capture) = rndr_paint::frame_with_snapshot(&mut app, |world| begin(world, 2, 2, 1.));
    let capture = capture.unwrap();
    assert!(capture.metadata.to_string().contains("before"));
    assert!(!capture.metadata.to_string().contains("after"));
    assert!(validate(app.world(), capture, 1.).is_none());
    assert_error(recv, "post_paint_mutation");
}
#[test]
fn post_paint_sensitive_state_scale_theme_and_scroll_are_validated() {
    for mode in 0..5 {
        let (mut app, send) = fixture();
        app.world_mut().insert_resource(ObservationContent::Public);
        let port = app.world_mut().spawn((Scroll::default(),));
        let recv = request(&send, 0);
        let capture = begin(app.world(), 2, 2, 1.).unwrap();
        match mode {
            0 => {
                app.world_mut().spawn((ObservationSensitive,));
            }
            1 => {
                app.world_mut().insert_resource(ObservationContent::Deny);
            }
            2 => {
                app.world_mut()
                    .insert_resource(ObservationTheme("dark".into()));
            }
            3 => {
                app.world_mut().get_mut::<Scroll>(port).unwrap().offset = [0., -1.];
            }
            _ => {}
        }
        assert!(validate(app.world(), capture, if mode == 4 { 2. } else { 1. }).is_none());
        assert_error(
            recv,
            if mode < 2 {
                "sensitive_frame"
            } else {
                "post_paint_mutation"
            },
        );
    }
}
#[test]
fn foreign_surface_and_oversized_node_tree_are_denied() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let foreign = app.world_mut().spawn((SurfaceSlot { id: 1 },));
    let recv = request(&send, 0);
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "foreign_surface_unsupported");
    app.world_mut().despawn_entity(foreign);
    for _ in 0..8192 {
        app.world_mut().spawn((Label::new("label"),));
    }
    let recv = request(&send, 0);
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "semantic_limit");
}
#[test]
fn supplemental_description_and_action_changes_cannot_escape_frame_binding() {
    for readonly in [false, true] {
        let (mut app, send) = fixture();
        app.add_plugin(GenericInteraction);
        app.world_mut().insert_resource(ObservationContent::Public);
        let button = app.world_mut().spawn((
            Button::new("save"),
            Semantics {
                name: Some("Save".into()),
                ..Semantics::default()
            },
        ));
        let recv = request(&send, 0);
        let capture = begin(app.world(), 2, 2, 1.).unwrap();
        let semantics = app.world_mut().get_mut::<Semantics>(button).unwrap();
        if readonly {
            semantics.readonly = true;
        } else {
            semantics.description = "new help".into();
        }
        assert!(validate(app.world(), capture, 1.).is_none());
        assert_error(recv, "post_paint_mutation");
    }
}
#[test]
fn shutdown_revokes_pending_capture_and_never_restarts_implicitly() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let recv = request(&send, 0);
    let capture = begin(app.world(), 2, 2, 1.).unwrap();
    let session = app.world().resource::<ObservationSession>().unwrap();
    let directory = session.directory.clone();
    session.shutdown();
    session.shutdown();
    assert!(!directory.exists());
    assert!(validate(app.world(), capture, 1.).is_none());
    assert_error(recv, "session_revoked");
    assert!(begin(app.world(), 2, 2, 1.).is_none());
}
#[test]
fn render_stall_expiry_is_denied_before_gpu_allocation() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    let recv = request(&send, 0);
    let mut capture = begin(app.world(), 2, 2, 1.).unwrap();
    capture.request.deadline = Instant::now() - Duration::from_secs(1);
    assert!(validate(app.world(), capture, 1.).is_none());
    assert_error(recv, "expired");
}
#[test]
fn metadata_limit_denies_without_exposing_large_or_sensitive_strings() {
    let (mut app, send) = fixture();
    app.world_mut().insert_resource(ObservationContent::Public);
    app.world_mut().spawn((Label::new("x".repeat(MAX_JSON)),));
    let recv = request(&send, 0);
    assert!(begin(app.world(), 2, 2, 1.).is_none());
    assert_error(recv, "semantic_limit");
}
#[cfg(unix)]
fn read_packet(stream: &mut std::os::unix::net::UnixStream) -> (Value, Vec<u8>) {
    use std::io::Read;
    let mut len = [0; 4];
    stream.read_exact(&mut len).unwrap();
    let n = u32::from_be_bytes(len) as usize;
    assert!(n <= MAX_JSON);
    let mut bytes = vec![0; n];
    stream.read_exact(&mut bytes).unwrap();
    let meta: Value = serde_json::from_slice(&bytes).unwrap();
    let n = meta["png_bytes"].as_u64().unwrap() as usize;
    assert!(n <= MAX_PIXELS as usize * 5 + 65536);
    let mut png = vec![0; n];
    stream.read_exact(&mut png).unwrap();
    (meta, png)
}
#[test]
#[cfg(unix)]
fn real_local_transport_permissions_bad_auth_then_coherent_packet_and_cleanup() {
    use std::io::Write;
    use std::os::unix::{fs::PermissionsExt, net::UnixStream};
    let mut app = App::new();
    app.world_mut().insert_resource(ObservationContent::Public);
    let descriptor = ObservationSession::enable(app.world_mut(), "synthetic-test").unwrap();
    assert_eq!(
        std::fs::metadata(&descriptor).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(descriptor.parent().unwrap().join("observe.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let dir = descriptor.parent().unwrap().to_path_buf();
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(ObservationSession::enable(app.world_mut(), "duplicate").is_err());
    let desc: Value = serde_json::from_slice(&std::fs::read(&descriptor).unwrap()).unwrap();
    let path = desc["socket"].as_str().unwrap().to_string();
    let token = desc["capability"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 64);
    let mut bad = UnixStream::connect(&path).unwrap();
    bad.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    bad.write_all(format!("RNDR1 {} 0\n", "0".repeat(64)).as_bytes())
        .unwrap();
    let (meta, png) = read_packet(&mut bad);
    assert_eq!(meta["error"], "invalid_request");
    assert!(png.is_empty());
    assert!(app
        .world()
        .resource::<ObservationSession>()
        .unwrap()
        .requests
        .lock()
        .unwrap()
        .try_recv()
        .is_err());
    let client = std::thread::spawn(move || {
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .write_all(format!("RNDR1 {token} 0\n").as_bytes())
            .unwrap();
        read_packet(&mut stream)
    });
    let until = Instant::now() + Duration::from_secs(1);
    let capture = loop {
        if let Some(c) = begin(app.world(), 2, 2, 1.) {
            break c;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    };
    complete(capture, Ok(image(2, 2)), true, "synthetic-unit-test");
    let (meta, png) = client.join().unwrap();
    assert!(meta.get("error").is_none());
    assert!(!png.is_empty());
    drop(app);
    assert!(!dir.exists());
}
#[test]
#[cfg(unix)]
fn truncated_oversized_and_slow_requests_are_bounded() {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    for bytes in [vec![b'x'; 129], b"RNDR1 incomplete".to_vec()] {
        let (mut server, mut client) = UnixStream::pair().unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        client.write_all(&bytes).unwrap();
        let start = Instant::now();
        assert!(read_request(&mut server).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
#[test]
#[cfg(unix)]
fn slow_reader_and_shutdown_with_pending_request_have_absolute_deadlines() {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    let (mut server, _client) = UnixStream::pair().unwrap();
    let start = Instant::now();
    assert!(write_packet(
        &mut server,
        (json!({"png_bytes":16_000_000}), vec![0; 16_000_000])
    )
    .is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    let mut app = App::new();
    let descriptor = ObservationSession::enable(app.world_mut(), "test").unwrap();
    let d: Value = serde_json::from_slice(&std::fs::read(&descriptor).unwrap()).unwrap();
    let mut stream = UnixStream::connect(d["socket"].as_str().unwrap()).unwrap();
    stream
        .write_all(format!("RNDR1 {} 0\n", d["capability"].as_str().unwrap()).as_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let start = Instant::now();
    drop(app);
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(!descriptor.exists());
}
