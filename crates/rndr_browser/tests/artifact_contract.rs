//! Admission contracts without a native engine. This test is not a sandbox
//! certification; examples/artifact_policy_harness exercises real WebKit.
use raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};
use rndr_browser::{ArtifactError, ArtifactPreview, MAX_ARTIFACT_HTML_BYTES};
use std::sync::atomic::{AtomicUsize, Ordering};

struct NoParent(AtomicUsize);
impl HasWindowHandle for NoParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(HandleError::Unavailable)
    }
}

#[test]
fn oversized_input_is_refused_before_touching_the_platform() {
    let parent = NoParent(AtomicUsize::new(0));
    let result = ArtifactPreview::attach(&parent, &"x".repeat(MAX_ARTIFACT_HTML_BYTES + 1));
    assert!(matches!(result, Err(ArtifactError::TooLarge)));
    assert_eq!(parent.0.load(Ordering::SeqCst), 0);
}

#[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
#[test]
fn other_backends_explicitly_refuse_without_opening_an_ordinary_browser() {
    let parent = NoParent(AtomicUsize::new(0));
    assert!(matches!(
        ArtifactPreview::attach(&parent, "<script>1</script>"),
        Err(ArtifactError::Unsupported)
    ));
    assert_eq!(parent.0.load(Ordering::SeqCst), 0);
}

#[test]
fn errors_are_specific_not_a_silent_browser_fallback() {
    assert!(ArtifactError::TooLarge.to_string().contains("UTF-8 bytes"));
    assert!(ArtifactError::Unsupported
        .to_string()
        .contains("unsupported"));
    assert!(ArtifactError::Policy("missing gate".into())
        .to_string()
        .contains("missing gate"));
}
