//! Host-owned notification tokens.
//!
//! The toolkit compares a token only to tell a live notification from a
//! dismissed or unknown one. It does not read the token as a command and it
//! does not keep a table from token to application action. A click returns the
//! token. It does not focus a window.
//!
//! `notify` is off unless the crate is built with that feature. [`Notifications::open`]
//! then returns [`NotifyError::Unsupported`] and does not present.

#[cfg(native_notify)]
use std::collections::BTreeSet;

/// Opaque token created by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotifyToken(pub [u8; 16]);

/// Text the platform may show. The token stays the identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyRequest {
    /// Host-owned identity.
    pub token: NotifyToken,
    /// Non-empty title.
    pub title: String,
    /// Optional body. Empty is allowed.
    pub body: String,
}

/// Why a notification call did not succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyError {
    /// The `notify` feature is off, or this platform cannot present.
    Unsupported,
    /// Permission is not granted. Nothing was presented for this call.
    PermissionDenied,
    /// The token is not a live notification.
    UnknownToken,
    /// The title is empty.
    Invalid,
}

/// Permission observed for this service. `Prompt` is not a grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyPermission {
    /// No grant yet. Present does not show a notification.
    Prompt,
    /// Present may try the platform.
    Granted,
    /// The platform denied notification permission.
    Denied,
}

/// A platform callback. Unknown tokens are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyCallback {
    /// The user activated the notification.
    Clicked(NotifyToken),
    /// The notification went away without a delivered click.
    Closed(NotifyToken),
}

/// A click the service accepted. The token is retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotifyClick {
    /// The host token that was presented and had not been dismissed.
    pub token: NotifyToken,
}

/// Present, dismiss, and accepted clicks for one process.
pub struct Notifications {
    #[cfg(native_notify)]
    live: Live,
}

#[cfg(native_notify)]
struct Live {
    permission: NotifyPermission,
    presented: BTreeSet<NotifyToken>,
    bridge: Bridge,
}

#[cfg(native_notify)]
enum Bridge {
    Headless { callbacks: Vec<NotifyCallback> },
    System(crate::notify_os::SystemNotifications),
}

impl Notifications {
    /// Open the platform service.
    ///
    /// Without the `notify` feature this is [`NotifyError::Unsupported`] and
    /// no notification is presented.
    pub fn open() -> Result<Self, NotifyError> {
        #[cfg(not(native_notify))]
        {
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            Ok(Self {
                live: Live {
                    permission: crate::notify_os::system_permission(),
                    presented: BTreeSet::new(),
                    bridge: Bridge::System(crate::notify_os::SystemNotifications::new()),
                },
            })
        }
    }

    /// In-memory service for admission tests. Not a platform session.
    ///
    /// Without `notify` this returns [`NotifyError::Unsupported`].
    pub fn headless(permission: NotifyPermission) -> Result<Self, NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = permission;
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            Ok(Self {
                live: Live {
                    permission,
                    presented: BTreeSet::new(),
                    bridge: Bridge::Headless {
                        callbacks: Vec::new(),
                    },
                },
            })
        }
    }

    /// Permission the next [`Self::present`] will consult.
    pub fn permission(&self) -> Result<NotifyPermission, NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = self;
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            Ok(self.live.permission)
        }
    }

    /// Replace the headless permission. A system service refuses this.
    pub fn set_permission(&mut self, permission: NotifyPermission) -> Result<(), NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = (self, permission);
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            match &self.live.bridge {
                Bridge::Headless { .. } => {
                    self.live.permission = permission;
                    Ok(())
                }
                Bridge::System(_) => Err(NotifyError::Unsupported),
            }
        }
    }

    /// Show `request` if permission is granted.
    ///
    /// A denied or not-yet-granted permission returns [`NotifyError::PermissionDenied`]
    /// and does not call the platform. A failed platform call does not record the token.
    pub fn present(&mut self, request: NotifyRequest) -> Result<(), NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = request;
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            // A platform that cannot present is unsupported, including when
            // permission was never granted. That refusal is not a denial.
            if !self.live.bridge.can_present() {
                return Err(NotifyError::Unsupported);
            }
            if request.title.trim().is_empty() {
                return Err(NotifyError::Invalid);
            }
            if self.live.permission != NotifyPermission::Granted {
                return Err(NotifyError::PermissionDenied);
            }
            if self.live.presented.contains(&request.token) {
                return Err(NotifyError::Invalid);
            }
            self.live
                .bridge
                .present(&request.token, &request.title, &request.body)?;
            self.live.presented.insert(request.token);
            Ok(())
        }
    }

    /// Remove a live token. An unknown or already dismissed token is refused.
    pub fn dismiss(&mut self, token: NotifyToken) -> Result<(), NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = (self, token);
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            if !self.live.presented.contains(&token) {
                return Err(NotifyError::UnknownToken);
            }
            self.live.bridge.dismiss(&token)?;
            self.live.presented.remove(&token);
            Ok(())
        }
    }

    /// Queue a platform callback on a headless service.
    ///
    /// A system service refuses this. The callback is applied by [`Self::poll`].
    pub fn push_callback(&mut self, callback: NotifyCallback) -> Result<(), NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = (self, callback);
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            match &mut self.live.bridge {
                Bridge::Headless { callbacks } => callbacks.push(callback),
                Bridge::System(_) => return Err(NotifyError::Unsupported),
            }
            Ok(())
        }
    }

    /// Accepted clicks since the last poll. Stale and unknown callbacks are dropped.
    ///
    /// An accepted click retires the token. This does not focus a window.
    pub fn poll(&mut self) -> Result<Vec<NotifyClick>, NotifyError> {
        #[cfg(not(native_notify))]
        {
            let _ = self;
            Err(NotifyError::Unsupported)
        }
        #[cfg(native_notify)]
        {
            let callbacks = self.live.bridge.poll();
            let mut accepted = Vec::new();
            for callback in callbacks {
                match callback {
                    NotifyCallback::Clicked(token) if self.live.presented.remove(&token) => {
                        accepted.push(NotifyClick { token });
                    }
                    NotifyCallback::Closed(token) => {
                        self.live.presented.remove(&token);
                    }
                    NotifyCallback::Clicked(_) => {}
                }
            }
            Ok(accepted)
        }
    }
}

#[cfg(native_notify)]
impl Bridge {
    fn can_present(&self) -> bool {
        match self {
            Bridge::Headless { .. } => true,
            Bridge::System(system) => system.can_present(),
        }
    }

    fn present(&mut self, token: &NotifyToken, title: &str, body: &str) -> Result<(), NotifyError> {
        match self {
            Bridge::Headless { .. } => Ok(()),
            Bridge::System(system) => system.present(token, title, body),
        }
    }

    fn dismiss(&mut self, token: &NotifyToken) -> Result<(), NotifyError> {
        match self {
            Bridge::Headless { .. } => Ok(()),
            Bridge::System(system) => system.dismiss(token),
        }
    }

    fn poll(&mut self) -> Vec<NotifyCallback> {
        match self {
            Bridge::Headless { callbacks } => std::mem::take(callbacks),
            Bridge::System(system) => system.poll(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(native_notify)]
    fn token(byte: u8) -> NotifyToken {
        let mut bytes = [0; 16];
        bytes[0] = byte;
        NotifyToken(bytes)
    }

    #[cfg(native_notify)]
    fn request(byte: u8, title: &str) -> NotifyRequest {
        NotifyRequest {
            token: token(byte),
            title: title.into(),
            body: String::new(),
        }
    }

    #[cfg(not(native_notify))]
    #[test]
    fn feature_off_returns_unsupported() {
        assert!(matches!(
            Notifications::open(),
            Err(NotifyError::Unsupported)
        ));
        assert!(matches!(
            Notifications::headless(NotifyPermission::Granted),
            Err(NotifyError::Unsupported)
        ));
    }

    #[cfg(native_notify)]
    #[test]
    fn denied_or_prompt_does_not_present() {
        let mut notes = Notifications::headless(NotifyPermission::Denied).unwrap();
        assert_eq!(
            notes.present(request(1, "Hello")).unwrap_err(),
            NotifyError::PermissionDenied
        );
        notes
            .push_callback(NotifyCallback::Clicked(token(1)))
            .unwrap();
        assert!(notes.poll().unwrap().is_empty());
        notes.set_permission(NotifyPermission::Prompt).unwrap();
        assert_eq!(
            notes.present(request(1, "Hello")).unwrap_err(),
            NotifyError::PermissionDenied
        );
        assert!(notes.poll().unwrap().is_empty());
    }

    #[cfg(native_notify)]
    #[test]
    fn present_dismiss_and_stale_clicks() {
        let mut notes = Notifications::headless(NotifyPermission::Granted).unwrap();
        assert_eq!(
            notes.present(request(1, "   ")).unwrap_err(),
            NotifyError::Invalid
        );
        notes.present(request(1, "Hello")).unwrap();
        assert_eq!(
            notes.present(request(1, "Hello")).unwrap_err(),
            NotifyError::Invalid
        );
        notes.present(request(2, "Next")).unwrap();
        notes
            .push_callback(NotifyCallback::Clicked(token(9)))
            .unwrap();
        assert!(notes.poll().unwrap().is_empty());
        notes.dismiss(token(2)).unwrap();
        notes
            .push_callback(NotifyCallback::Clicked(token(2)))
            .unwrap();
        notes
            .push_callback(NotifyCallback::Closed(token(1)))
            .unwrap();
        assert!(notes.poll().unwrap().is_empty());
        assert_eq!(
            notes.dismiss(token(1)).unwrap_err(),
            NotifyError::UnknownToken
        );
        assert_eq!(
            notes.dismiss(token(2)).unwrap_err(),
            NotifyError::UnknownToken
        );
        notes.present(request(3, "Again")).unwrap();
        notes
            .push_callback(NotifyCallback::Clicked(token(3)))
            .unwrap();
        assert_eq!(notes.poll().unwrap(), vec![NotifyClick { token: token(3) }]);
        notes
            .push_callback(NotifyCallback::Clicked(token(3)))
            .unwrap();
        assert!(notes.poll().unwrap().is_empty());
        notes.set_permission(NotifyPermission::Denied).unwrap();
        assert_eq!(
            notes.present(request(4, "Later")).unwrap_err(),
            NotifyError::PermissionDenied
        );
        assert_eq!(
            notes.dismiss(token(4)).unwrap_err(),
            NotifyError::UnknownToken
        );
    }

    #[cfg(native_notify)]
    #[test]
    fn close_before_click_drops_the_click() {
        let mut notes = Notifications::headless(NotifyPermission::Granted).unwrap();
        notes.present(request(1, "Hello")).unwrap();
        notes
            .push_callback(NotifyCallback::Closed(token(1)))
            .unwrap();
        notes
            .push_callback(NotifyCallback::Clicked(token(1)))
            .unwrap();
        assert!(notes.poll().unwrap().is_empty());
        assert_eq!(
            notes.dismiss(token(1)).unwrap_err(),
            NotifyError::UnknownToken
        );
        notes.present(request(2, "After")).unwrap();
        notes
            .push_callback(NotifyCallback::Clicked(token(2)))
            .unwrap();
        notes
            .push_callback(NotifyCallback::Closed(token(2)))
            .unwrap();
        assert_eq!(notes.poll().unwrap(), vec![NotifyClick { token: token(2) }]);
        assert_eq!(
            notes.dismiss(token(2)).unwrap_err(),
            NotifyError::UnknownToken
        );
    }

    #[cfg(native_notify)]
    #[test]
    fn system_service_rejects_injected_callbacks() {
        let mut notes = Notifications::open().unwrap();
        assert_eq!(
            notes
                .push_callback(NotifyCallback::Clicked(token(1)))
                .unwrap_err(),
            NotifyError::Unsupported
        );
        assert_eq!(
            notes.set_permission(NotifyPermission::Granted).unwrap_err(),
            NotifyError::Unsupported
        );
        // `present` is not called. A system present would talk to the
        // platform, and this test is not a recorded session.
        assert!(notes.poll().unwrap().is_empty());
    }

    #[cfg(all(native_notify, target_os = "linux"))]
    #[test]
    fn linux_system_starts_granted() {
        let notes = Notifications::open().unwrap();
        assert_eq!(notes.permission().unwrap(), NotifyPermission::Granted);
    }

    #[cfg(all(native_notify, not(target_os = "linux")))]
    #[test]
    fn other_platforms_do_not_present() {
        let mut notes = Notifications::open().unwrap();
        assert_eq!(notes.permission().unwrap(), NotifyPermission::Prompt);
        assert_eq!(
            notes.present(request(1, "Hello")).unwrap_err(),
            NotifyError::Unsupported
        );
        assert_eq!(
            notes.dismiss(token(1)).unwrap_err(),
            NotifyError::UnknownToken
        );
        assert!(notes.poll().unwrap().is_empty());
    }
}
