//! Opens links that [`rndr_core::request_open_url`] already checked.

/// Hand `url` to the system's default handler.
#[cfg(target_arch = "wasm32")]
pub(crate) fn open(url: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("no browser window")?;
    window
        .open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer")
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

#[cfg(target_os = "macos")]
pub(crate) fn open(url: &str) -> Result<(), String> {
    spawn("/usr/bin/open", url)
}

#[cfg(target_os = "windows")]
pub(crate) fn open(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, target) = (wide("open"), wide(url));
    // SAFETY: both strings are NUL-terminated and outlive the call; the other
    // pointers may be null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    if result as usize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW returned {}", result as usize))
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub(crate) fn open(url: &str) -> Result<(), String> {
    spawn("xdg-open", url)
}

#[cfg(not(any(
    target_arch = "wasm32",
    target_os = "macos",
    target_os = "windows",
    all(unix, not(any(target_os = "ios", target_os = "android")))
)))]
pub(crate) fn open(_: &str) -> Result<(), String> {
    Err("this platform can't open links".into())
}

/// Run `program url` detached. The checked link starts with its scheme, so
/// it can't be read as an option.
#[cfg(any(
    target_os = "macos",
    all(unix, not(any(target_os = "ios", target_os = "android")))
))]
fn spawn(program: &str, url: &str) -> Result<(), String> {
    debug_assert!(!url.starts_with('-'));
    let mut child = std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    // Reap it so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
