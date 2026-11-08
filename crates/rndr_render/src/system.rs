//! The desktop's appearance and reduced-motion preferences, read for
//! [`rndr_core::SystemAppearance`].
//!
//! Windows and macOS report dark or light through winit. Linux reads the XDG
//! desktop portal (`color-scheme`, `accent-color` and GNOME's
//! `enable-animations`) on a worker thread when the `appearance` feature is
//! on, plus KDE's `AnimationDurationFactor`. Browsers answer media queries.

use rndr_core::{Color, SystemAppearance};
use winit::window::{Theme, Window};

/// What the desktop prefers now. `window` supplies winit's theme.
pub(crate) fn read(window: Option<&dyn Window>) -> SystemAppearance {
    let winit_dark = window
        .and_then(Window::theme)
        .map(|theme| theme == Theme::Dark);
    SystemAppearance {
        dark: platform_dark().or(winit_dark),
        accent: accent(),
        reduced_motion: reduced_motion(),
    }
}

/// Changes since the last [`read`] are pending: bumps when the portal or a
/// media query reports one.
pub(crate) fn generation() -> u64 {
    #[cfg(all(native_appearance, target_os = "linux"))]
    return portal::generation();
    #[allow(unreachable_code)]
    0
}

/// Redraw `window` when the desktop's preferences change.
pub(crate) fn follow(window: &std::sync::Arc<dyn Window>) {
    #[cfg(all(native_appearance, target_os = "linux"))]
    portal::follow(window);
    let _ = window;
}

fn platform_dark() -> Option<bool> {
    #[cfg(all(native_appearance, target_os = "linux"))]
    return portal::dark();
    #[cfg(target_arch = "wasm32")]
    return web::matches("(prefers-color-scheme: dark)")
        .filter(|_| web::matches("(prefers-color-scheme: light)").is_some());
    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "macos")]
fn accent() -> Option<Color> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // SAFETY: AppKit class methods on the UI thread. Each returns an
    // autoreleased colour or nil, and the component getters return CGFloat.
    unsafe {
        let color: *mut AnyObject = msg_send![class!(NSColor), controlAccentColor];
        if color.is_null() {
            return None;
        }
        let space: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
        let srgb: *mut AnyObject = msg_send![color, colorUsingColorSpace: space];
        if srgb.is_null() {
            return None;
        }
        let r: f64 = msg_send![srgb, redComponent];
        let g: f64 = msg_send![srgb, greenComponent];
        let b: f64 = msg_send![srgb, blueComponent];
        Some(Color::rgb(r as f32, g as f32, b as f32))
    }
}

#[cfg(target_os = "windows")]
fn accent() -> Option<Color> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, value) = (
        wide("Software\\Microsoft\\Windows\\DWM"),
        wide("AccentColor"),
    );
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: NUL-terminated strings and a DWORD-sized output buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    // The DWORD is 0xAABBGGRR.
    (status == 0).then(|| {
        let channel = |shift: u32| ((data >> shift) & 0xff) as f32 / 255.0;
        Color::rgb(channel(0), channel(8), channel(16))
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn accent() -> Option<Color> {
    #[cfg(all(native_appearance, target_os = "linux"))]
    return portal::accent();
    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "macos")]
fn reduced_motion() -> bool {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    // SAFETY: NSWorkspace's shared instance lives for the process; the
    // selector returns BOOL.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return false;
        }
        let reduced: Bool = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        reduced.as_bool()
    }
}

#[cfg(target_os = "windows")]
fn reduced_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };
    let mut animations: i32 = 1;
    // SAFETY: this query writes one BOOL into a valid local.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animations as *mut i32).cast(),
            0,
        ) != 0
            && animations == 0
    }
}

#[cfg(target_os = "linux")]
fn reduced_motion() -> bool {
    if let Some(factor) = kde_animation_factor() {
        return factor == 0.0;
    }
    #[cfg(native_appearance)]
    return portal::animations() == Some(false);
    #[allow(unreachable_code)]
    false
}

#[cfg(target_arch = "wasm32")]
fn reduced_motion() -> bool {
    web::matches("(prefers-reduced-motion: reduce)").unwrap_or(false)
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_arch = "wasm32"
)))]
fn reduced_motion() -> bool {
    false
}

/// KDE's animation speed from `kdeglobals`; `0` turns animation off.
#[cfg(target_os = "linux")]
fn kde_animation_factor() -> Option<f64> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop.split(':').any(|d| d.eq_ignore_ascii_case("KDE")) {
        return None;
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".config"))
        })?;
    let contents = std::fs::read_to_string(config.join("kdeglobals")).ok()?;
    kde_factor(&contents)
}

#[cfg(any(target_os = "linux", test))]
fn kde_factor(contents: &str) -> Option<f64> {
    contents
        .lines()
        .filter_map(|line| line.trim().split_once('='))
        .find(|(key, _)| key.trim() == "AnimationDurationFactor")
        .and_then(|(_, value)| value.trim().parse().ok())
}

#[cfg(target_arch = "wasm32")]
mod web {
    /// Whether the media `query` matches, or `None` when it can't be asked.
    pub(super) fn matches(query: &str) -> Option<bool> {
        let list = web_sys::window()?.match_media(query).ok()??;
        Some(list.matches())
    }
}

#[cfg(all(native_appearance, target_os = "linux"))]
mod portal {
    //! A worker thread reads the portal once and follows `SettingChanged`.
    //! Each change bumps the generation and redraws every followed window,
    //! whose next frame rereads the values.

    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::{mpsc, Arc, Mutex, OnceLock, Weak};
    use std::time::Duration;

    use futures_util::StreamExt;
    use rndr_core::Color;
    use winit::window::Window;
    use zbus::zvariant::{OwnedValue, Value};
    use zbus::{Connection, Proxy};

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const INTERFACE: &str = "org.freedesktop.portal.Settings";
    const APPEARANCE: &str = "org.freedesktop.appearance";
    const GNOME: &str = "org.gnome.desktop.interface";
    /// How long the first window waits for the portal's first answer. A
    /// slower answer redraws the windows when it arrives.
    const FIRST_ANSWER: Duration = Duration::from_millis(100);
    const RPC_TIMEOUT: Duration = Duration::from_secs(10);

    /// Portal numbering: 0 no preference, 1 dark, 2 light.
    static SCHEME: AtomicU8 = AtomicU8::new(0);
    /// 0 unknown, 1 on, 2 off.
    static ANIMATIONS: AtomicU8 = AtomicU8::new(0);
    static ACCENT: Mutex<Option<Color>> = Mutex::new(None);
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    static WATCH: OnceLock<()> = OnceLock::new();
    static WINDOWS: Mutex<Vec<Weak<dyn Window>>> = Mutex::new(Vec::new());

    pub(super) fn dark() -> Option<bool> {
        WATCH.get_or_init(start);
        scheme_dark(SCHEME.load(Ordering::Relaxed).into())
    }

    pub(super) fn accent() -> Option<Color> {
        WATCH.get_or_init(start);
        *ACCENT.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn animations() -> Option<bool> {
        WATCH.get_or_init(start);
        match ANIMATIONS.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    pub(super) fn generation() -> u64 {
        GENERATION.load(Ordering::Relaxed)
    }

    pub(super) fn follow(window: &Arc<dyn Window>) {
        WATCH.get_or_init(start);
        let mut windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
        windows.retain(|w| w.strong_count() > 0);
        windows.push(Arc::downgrade(window));
    }

    pub(super) fn scheme_dark(scheme: u32) -> Option<bool> {
        match scheme {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// The setting itself. `Read` wraps it in one more variant than `ReadOne`.
    pub(super) fn unwrap<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
        match value {
            Value::Value(inner) => unwrap(inner),
            other => other,
        }
    }

    /// `accent-color` is `(ddd)` sRGB; any channel outside 0..=1 means unset.
    pub(super) fn parse_accent(value: &Value<'_>) -> Option<Color> {
        let Value::Structure(fields) = unwrap(value) else {
            return None;
        };
        let channels: Vec<f64> = fields
            .fields()
            .iter()
            .filter_map(|field| match field {
                Value::F64(v) => Some(*v),
                _ => None,
            })
            .collect();
        match channels[..] {
            [r, g, b] if [r, g, b].iter().all(|c| (0.0..=1.0).contains(c)) => {
                Some(Color::rgb(r as f32, g as f32, b as f32))
            }
            _ => None,
        }
    }

    fn store(namespace: &str, key: &str, value: &Value<'_>) {
        let changed = match (namespace, key, unwrap(value)) {
            (APPEARANCE, "color-scheme", Value::U32(scheme)) => {
                let scheme = if *scheme <= 2 { *scheme as u8 } else { 0 };
                SCHEME.swap(scheme, Ordering::Relaxed) != scheme
            }
            (APPEARANCE, "accent-color", _) => {
                let accent = parse_accent(value);
                let mut slot = ACCENT.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::replace(&mut *slot, accent) != accent
            }
            (GNOME, "enable-animations", Value::Bool(on)) => {
                let state = if *on { 1 } else { 2 };
                ANIMATIONS.swap(state, Ordering::Relaxed) != state
            }
            _ => false,
        };
        if !changed {
            return;
        }
        GENERATION.fetch_add(1, Ordering::Relaxed);
        let windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
        for window in windows.iter().filter_map(Weak::upgrade) {
            window.request_redraw();
        }
    }

    fn start() {
        let (ready, first) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("rndr-appearance-portal".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                runtime.block_on(watch(ready));
            });
        if worker.is_ok() {
            let _ = first.recv_timeout(FIRST_ANSWER);
        }
    }

    async fn read(portal: &Proxy<'_>, namespace: &str, key: &str) {
        let value = match portal
            .call::<_, _, OwnedValue>("ReadOne", &(namespace, key))
            .await
        {
            Ok(value) => Some(value),
            Err(_) => portal
                .call::<_, _, OwnedValue>("Read", &(namespace, key))
                .await
                .ok(),
        };
        if let Some(value) = value {
            store(namespace, key, &value);
        }
    }

    async fn watch(ready: mpsc::SyncSender<()>) {
        let setup = tokio::time::timeout(RPC_TIMEOUT, async {
            let conn = Connection::session().await.ok()?;
            let portal = Proxy::new(&conn, DEST, PATH, INTERFACE).await.ok()?;
            // Subscribe first, so a change between read and subscribe isn't lost.
            let changes = portal.receive_signal("SettingChanged").await.ok()?;
            read(&portal, APPEARANCE, "color-scheme").await;
            read(&portal, APPEARANCE, "accent-color").await;
            read(&portal, GNOME, "enable-animations").await;
            Some((conn, changes))
        })
        .await
        .ok()
        .flatten();
        let _ = ready.send(());
        let Some((_conn, mut changes)) = setup else {
            return;
        };
        while let Some(message) = changes.next().await {
            if let Ok((namespace, key, value)) =
                message.body().deserialize::<(String, String, OwnedValue)>()
            {
                store(&namespace, &key, &value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kde_factor_reads_the_animation_speed() {
        let config = "[General]\nfoo=1\n[KDE]\n AnimationDurationFactor = 0\n";
        assert_eq!(kde_factor(config), Some(0.0));
        assert_eq!(kde_factor("AnimationDurationFactor=0.5"), Some(0.5));
        assert_eq!(kde_factor("[KDE]\nSingleClick=false"), None);
    }

    #[cfg(all(native_appearance, target_os = "linux"))]
    #[test]
    fn portal_values_parse_however_deeply_wrapped() {
        use zbus::zvariant::{StructureBuilder, Value};
        assert_eq!(portal::scheme_dark(1), Some(true));
        assert_eq!(portal::scheme_dark(2), Some(false));
        assert_eq!(portal::scheme_dark(0), None);
        assert_eq!(portal::scheme_dark(9), None);
        let nested = Value::Value(Box::new(Value::Value(Box::new(Value::U32(2)))));
        assert_eq!(portal::unwrap(&nested), &Value::U32(2));
        let rgb = |r: f64, g: f64, b: f64| {
            Value::Structure(
                StructureBuilder::new()
                    .add_field(r)
                    .add_field(g)
                    .add_field(b)
                    .build()
                    .unwrap(),
            )
        };
        let accent = portal::parse_accent(&Value::Value(Box::new(rgb(1.0, 0.5, 0.0)))).unwrap();
        assert_eq!((accent.r, accent.g, accent.b), (1.0, 0.5, 0.0));
        assert_eq!(portal::parse_accent(&rgb(-1.0, -1.0, -1.0)), None);
        assert_eq!(portal::parse_accent(&Value::U32(1)), None);
    }
}
