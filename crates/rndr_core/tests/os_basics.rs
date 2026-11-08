use std::time::Duration;

use rndr_core::motion::{self, MotionSettings, Prop, Spring};
use rndr_core::{
    check_url, events, publish_system_appearance, request_open_url, system_appearance,
    take_url_requests, Color, Layout, OpenUrlError, Sim, Size, SystemAppearance,
    SystemAppearanceChanged, Transform, Viewport, World,
};

#[test]
fn only_web_and_mail_links_are_queued() {
    assert_eq!(
        check_url("  https://example.com/a?b=1#c ").as_deref(),
        Ok("https://example.com/a?b=1#c")
    );
    assert_eq!(
        check_url("HTTP://example.com").as_deref(),
        Ok("http://example.com")
    );
    assert_eq!(
        check_url("https://[::1]:8080/x").as_deref(),
        Ok("https://[::1]:8080/x")
    );
    assert_eq!(
        check_url("mailto:someone@example.com?subject=Hi").as_deref(),
        Ok("mailto:someone@example.com?subject=Hi")
    );
    assert_eq!(
        check_url("https://user@example.com:443").as_deref(),
        Ok("https://user@example.com:443")
    );
    for (url, scheme) in [
        ("file:///etc/passwd", "file"),
        ("JavaScript:alert(1)", "javascript"),
        ("myapp://do/thing", "myapp"),
    ] {
        assert_eq!(
            check_url(url),
            Err(OpenUrlError::UnsupportedScheme(scheme.into()))
        );
    }
    assert_eq!(check_url(""), Err(OpenUrlError::Empty));
    assert_eq!(check_url("   "), Err(OpenUrlError::Empty));
    for bad in [
        "example.com",
        "https:",
        "https://",
        "https:///x",
        "https://a b",
        "https://a\n.com",
        "mailto:",
        "mailto://example.com",
        "1http://x",
        "://x",
        "-https://x",
        "https://\\evil.com",
        "https://example.com/\"--flag",
        "https://example.com/%ZZ",
        "https://example.com/%00",
        "https://example.com/%2",
        "https://@/",
        "https://:80/",
        "https://example.com:abc/",
        "https://example.com:99999/",
        "https://evil%2f.com/",
        "https://.example.com",
        "https://a..b.com",
        "https://example.com/a[0]",
        "https://[not-ipv6]/",
        "https://[::1]abc/",
    ] {
        assert_eq!(check_url(bad), Err(OpenUrlError::Invalid), "{bad:?}");
    }
    assert_eq!(
        check_url(&format!("https://example.com/{}", "a".repeat(9000))),
        Err(OpenUrlError::Invalid)
    );
}

#[test]
fn requests_queue_in_order_and_refusals_queue_nothing() {
    let mut world = World::new();
    assert!(take_url_requests(&mut world).is_empty());
    assert_eq!(
        request_open_url(&mut world, "https://example.com/docs"),
        Ok(())
    );
    assert_eq!(
        request_open_url(&mut world, "file:///etc/hosts"),
        Err(OpenUrlError::UnsupportedScheme("file".into()))
    );
    assert_eq!(
        request_open_url(&mut world, " MAILTO:a@example.com"),
        Ok(())
    );
    assert_eq!(
        take_url_requests(&mut world),
        ["https://example.com/docs", "mailto:a@example.com"]
    );
    assert!(take_url_requests(&mut world).is_empty());
}

#[test]
fn publishing_appearance_sends_only_changes() {
    let mut sim = Sim::new();
    assert_eq!(system_appearance(sim.world()), SystemAppearance::default());
    let dark = SystemAppearance {
        dark: Some(true),
        accent: Some(Color::rgb(1.0, 0.5, 0.0)),
        reduced_motion: false,
    };
    assert!(publish_system_appearance(sim.world(), dark));
    assert!(!publish_system_appearance(sim.world(), dark));
    assert_eq!(system_appearance(sim.world()), dark);
    let light = SystemAppearance {
        dark: Some(false),
        ..dark
    };
    assert!(publish_system_appearance(sim.world(), light));
    sim.update();
    let sent: Vec<_> = events::<SystemAppearanceChanged>(sim.world())
        .iter()
        .map(|e| e.0)
        .collect();
    assert_eq!(sent, [dark, light]);
}

#[test]
fn system_reduced_motion_lands_motion_immediately() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let card = sim.world().spawn((Layout {
        width: Size::Px(40.0),
        height: Size::Px(20.0),
        ..Layout::column()
    },));
    sim.update();
    publish_system_appearance(
        sim.world(),
        SystemAppearance {
            reduced_motion: true,
            ..SystemAppearance::default()
        },
    );
    assert!(!sim
        .world()
        .resource::<MotionSettings>()
        .is_some_and(|s| s.reduced_motion));
    motion::animate(sim.world(), card, Prop::X, 80.0, Spring::SMOOTH);
    sim.advance(Duration::from_millis(16));
    sim.update();
    let x = sim.world().get::<Transform>(card).unwrap().translate[0];
    assert_eq!(x, 80.0);

    publish_system_appearance(sim.world(), SystemAppearance::default());
    motion::animate(sim.world(), card, Prop::X, 0.0, Spring::SMOOTH);
    sim.advance(Duration::from_millis(16));
    sim.update();
    let x = sim.world().get::<Transform>(card).unwrap().translate[0];
    assert!(x > 0.0 && x < 80.0, "{x}");
}
