//! Platform notification calls used only with the `notify` feature.
//!
//! None of these paths focus a window. A failure leaves the token unrecorded
//! by the caller. macOS and Windows return [`NotifyError::Unsupported`]: this
//! crate does not prompt for macOS permission and does not create a Windows
//! package identity. Linux talks to the desktop notification service when a
//! session bus offers it. That path is unverified until a named session.
//! The Linux map stores the service's numeric id so a later dismiss can name
//! it. The token bytes are not written to the bus and are not an application route.

use crate::notify::{NotifyCallback, NotifyError, NotifyPermission, NotifyToken};
use std::collections::BTreeMap;

/// Permission a system service starts with.
///
/// Linux has no separate permission result on `org.freedesktop.Notifications`,
/// so the service starts granted and a missing bus is [`NotifyError::Unsupported`].
/// Other platforms stay at [`NotifyPermission::Prompt`] and [`SystemNotifications::present`]
/// still returns unsupported before that prompt is treated as a denial.
pub(crate) fn system_permission() -> NotifyPermission {
    if cfg!(target_os = "linux") {
        NotifyPermission::Granted
    } else {
        NotifyPermission::Prompt
    }
}

pub(crate) struct SystemNotifications {
    #[cfg(target_os = "linux")]
    linux: LinuxNotifications,
}

impl SystemNotifications {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            linux: LinuxNotifications::new(),
        }
    }

    pub(crate) fn can_present(&self) -> bool {
        cfg!(target_os = "linux")
    }

    pub(crate) fn present(
        &mut self,
        token: &NotifyToken,
        title: &str,
        body: &str,
    ) -> Result<(), NotifyError> {
        #[cfg(target_os = "linux")]
        {
            self.linux.present(token, title, body)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (token, title, body);
            Err(NotifyError::Unsupported)
        }
    }

    pub(crate) fn dismiss(&mut self, token: &NotifyToken) -> Result<(), NotifyError> {
        #[cfg(target_os = "linux")]
        {
            self.linux.dismiss(token)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = token;
            Err(NotifyError::Unsupported)
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<NotifyCallback> {
        #[cfg(target_os = "linux")]
        {
            self.linux.poll()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }
}

#[cfg(target_os = "linux")]
struct LinuxNotifications {
    ids: BTreeMap<NotifyToken, u32>,
}

#[cfg(target_os = "linux")]
impl LinuxNotifications {
    fn new() -> Self {
        Self {
            ids: BTreeMap::new(),
        }
    }

    fn present(&mut self, token: &NotifyToken, title: &str, body: &str) -> Result<(), NotifyError> {
        let id = dbus_notify(title, body)?;
        self.ids.insert(*token, id);
        Ok(())
    }

    fn dismiss(&mut self, token: &NotifyToken) -> Result<(), NotifyError> {
        let Some(id) = self.ids.get(token).copied() else {
            return Ok(());
        };
        dbus_close(id)?;
        self.ids.remove(token);
        Ok(())
    }

    fn poll(&mut self) -> Vec<NotifyCallback> {
        // ActionInvoked would be the click signal. This build does not subscribe
        // to it: a default action is how desktops focus the application, and no
        // session has shown a non-activating click. Unknown callbacks therefore
        // cannot arrive from this bridge.
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn dbus_notify(title: &str, body: &str) -> Result<u32, NotifyError> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let connection = zbus::blocking::Connection::session().map_err(|_| NotifyError::Unsupported)?;
    // No actions. The `default` action is how desktops focus the calling
    // application, and this crate does not activate a window.
    let actions: Vec<&str> = Vec::new();
    let hints = HashMap::<String, Value>::new();
    let message = connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("rndr", 0u32, "", title, body, &actions, &hints, -1i32),
        )
        .map_err(|_| NotifyError::Unsupported)?;
    message
        .body()
        .deserialize()
        .map_err(|_| NotifyError::Unsupported)
}

#[cfg(target_os = "linux")]
fn dbus_close(id: u32) -> Result<(), NotifyError> {
    let connection = zbus::blocking::Connection::session().map_err(|_| NotifyError::Unsupported)?;
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "CloseNotification",
            &(id,),
        )
        .map_err(|_| NotifyError::Unsupported)?;
    Ok(())
}
