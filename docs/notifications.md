# Notifications

`notify` is off by default on `rndr_render` and on the `rndr` facade. Turning it
on does not turn on `dialog` or `browser`. `hotkey` and `tray` stay default-on.

[`Notifications::open`](https://docs.rs/rndr_render/latest/rndr_render/struct.Notifications.html)
is the entry point. Without the feature it returns `NotifyError::Unsupported`
and does not present a notification. It is not a successful no-op.

The host creates a `NotifyToken`. Present, dismiss, and click all use that
token. The toolkit compares the bytes to decide whether the notification is
still live. It does not treat them as a command and it does not store a route
from the token to application code. `poll` returns `NotifyClick` for a live
token and then retires it. A click for a dismissed or unknown token is
dropped. A close callback retires a live token and does not report a click.
`poll` does not focus a window. The application raises a window itself, if it
wants to, through the existing window APIs.

`Prompt` and `Denied` both refuse `present` with `PermissionDenied` when the
platform can present. The platform is not called, and the token is not
recorded. An empty title is `Invalid` and is not presented. Presenting a token
that is already live is `Invalid`. A platform that cannot present returns
`Unsupported` before the permission check, so a missing platform is not
reported as a denial.

`Notifications::headless` is an in-memory service for these rules. It is not a
platform session. `open` is the system service. On a headless service,
`push_callback` feeds a platform callback. The system service rejects that
injection.

## Platforms

No cell is `Supported`. A headless test and a successful compile do not change
that.

| Platform | This crate | Why |
| --- | --- | --- |
| Linux | Unverified | `present` calls `org.freedesktop.Notifications` when a session bus has that name. The service has no separate permission result, so a system service starts `Granted` and a missing bus is `Unsupported`. The numeric id returned by `Notify` is kept only so `CloseNotification` can name it. The token is not written to the bus. No actions are sent, because the `default` action is how a desktop focuses the application. Clicks are not subscribed. No named session has shown a present, a dismiss, or a click. |
| macOS | Unsupported | `present` returns `Unsupported`. `UNUserNotificationCenter` is not linked and permission is not prompted. `permission` stays `Prompt`, which is not a grant. |
| Windows | Unsupported | `present` returns `Unsupported`. Toast notifications need an application-owned AppUserModelID or package. This repository does not add a bundle, a shortcut, or an entitlement. |

A denied permission is not support.
