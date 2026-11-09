use rndr_core::{
    events, is_link, launch_links, push_opened_url, push_reopened, Reopened, Sim, UrlOpened,
};

#[test]
fn links_are_scheme_urls_and_nothing_else() {
    for link in [
        "myapp://open?id=1",
        "web+app://x",
        "com.example.app://path",
        "https://example.com",
    ] {
        assert!(is_link(link), "{link}");
    }
    for arg in [
        "C:\\Users\\me\\file.txt",
        "c://drive",
        "/home/me/file",
        "--flag",
        "1app://x",
        "my app://x",
        "",
    ] {
        assert!(!is_link(arg), "{arg}");
    }
}

#[test]
fn launch_links_keep_their_order_and_skip_other_arguments() {
    let args = ["--verbose", "myapp://one", "file.txt", "myapp://two"];
    assert_eq!(launch_links(args), vec!["myapp://one", "myapp://two"]);
}

#[test]
fn links_and_reopens_arrive_in_order_after_the_frame_flip() {
    let mut sim = Sim::new();
    push_opened_url(sim.world(), "myapp://first");
    push_reopened(sim.world());
    push_opened_url(sim.world(), "myapp://second");
    assert!(
        events::<UrlOpened>(sim.world()).is_empty(),
        "not readable before the frame"
    );
    sim.update();
    let urls: Vec<_> = events::<UrlOpened>(sim.world())
        .iter()
        .map(|u| u.0.clone())
        .collect();
    assert_eq!(urls, ["myapp://first", "myapp://second"]);
    assert_eq!(events::<Reopened>(sim.world()), [Reopened]);
    sim.update();
    assert!(
        events::<UrlOpened>(sim.world()).is_empty(),
        "each link arrives once"
    );
}
