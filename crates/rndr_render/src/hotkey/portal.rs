//! XDG desktop portal `org.freedesktop.portal.GlobalShortcuts`.
//!
//! A session bus without that interface is [`HotkeyFailure::Unsupported`].
//! There is no X11 fallback: an X grab does not see keys delivered to Wayland
//! clients. `BindShortcuts` may show a system dialog, and the user may change
//! the trigger. The shortcut id we asked for is what `Activated` reports.
//! `Deactivated` is the matching release. A second `Activated` while that
//! shortcut is still active is a repeat, as is a `Deactivated` and
//! `Activated` pair that share a timestamp.
//!
//! Each registration is its own session. The portal only accepts one
//! `BindShortcuts` call per session.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use futures_util::StreamExt;
use rndr_core::hotkey::{HotkeyBackend, HotkeyFailure, HotkeyId, PlatformAccel};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use super::press::PressQueue;
use super::repeat::RepeatFilter;

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const SESSION_WAIT: Duration = Duration::from_secs(8);
const BIND_WAIT: Duration = Duration::from_secs(300);

enum Cmd {
    Bind {
        id: u32,
        trigger: String,
        reply: mpsc::Sender<Result<(), HotkeyFailure>>,
    },
    Unbind {
        id: u32,
        reply: mpsc::Sender<()>,
    },
    Stop,
}

pub(super) struct PortalHotkeys {
    tx: UnboundedSender<Cmd>,
    thread: Option<JoinHandle<()>>,
    presses: Arc<PressQueue>,
}

impl PortalHotkeys {
    pub(super) fn open(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, ()> {
        let presses = Arc::new(PressQueue::new(wake));
        let (tx, rx) = unbounded_channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let presses_thread = Arc::clone(&presses);
        let thread = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => {
                    let _ = ready_tx.send(false);
                    return;
                }
            };
            runtime.block_on(async move {
                match Connection::session().await {
                    Ok(conn) if portal_available(&conn).await => {
                        let _ = ready_tx.send(true);
                        run(conn, rx, presses_thread).await;
                    }
                    _ => {
                        let _ = ready_tx.send(false);
                    }
                }
            });
        });
        match ready_rx.recv() {
            Ok(true) => Ok(Self {
                tx,
                thread: Some(thread),
                presses,
            }),
            _ => {
                let _ = thread.join();
                Err(())
            }
        }
    }
}

impl HotkeyBackend for PortalHotkeys {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        let Some(trigger) = accel.wayland_trigger.clone() else {
            return Err(HotkeyFailure::Invalid);
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Cmd::Bind {
                id: id.get(),
                trigger,
                reply: reply_tx,
            })
            .map_err(|_| HotkeyFailure::Unsupported)?;
        reply_rx.recv().unwrap_or(Err(HotkeyFailure::Unsupported))
    }

    fn unregister(&mut self, id: HotkeyId) {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self
            .tx
            .send(Cmd::Unbind {
                id: id.get(),
                reply: reply_tx,
            })
            .is_ok()
        {
            let _ = reply_rx.recv();
        }
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        self.presses.drain()
    }
}

impl Drop for PortalHotkeys {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn portal_available(conn: &Connection) -> bool {
    let Ok(proxy) = shortcuts_proxy(conn).await else {
        return false;
    };
    proxy
        .get_property::<u32>("version")
        .await
        .is_ok_and(|version| version >= 1)
}

async fn run(conn: Connection, mut rx: UnboundedReceiver<Cmd>, presses: Arc<PressQueue>) {
    let Ok(mut activated) = shortcut_stream(&conn, "Activated").await else {
        return;
    };
    let mut deactivated = shortcut_stream(&conn, "Deactivated").await.ok();
    let mut track_repeat = deactivated.is_some();
    let mut repeats = RepeatFilter::default();
    let mut sessions: HashMap<u32, OwnedObjectPath> = HashMap::new();
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    Some(Cmd::Bind { id, trigger, reply }) => {
                        match bind_one(&conn, id, &trigger).await {
                            Ok(path) => {
                                sessions.insert(id, path);
                                let _ = reply.send(Ok(()));
                            }
                            Err(err) => {
                                let _ = reply.send(Err(err));
                            }
                        }
                    }
                    Some(Cmd::Unbind { id, reply }) => {
                        if let Some(path) = sessions.remove(&id) {
                            close_session(&conn, &path).await;
                        }
                        let _ = reply.send(());
                    }
                    Some(Cmd::Stop) | None => {
                        for path in sessions.values() {
                            close_session(&conn, path).await;
                        }
                        break;
                    }
                }
            }
            msg = activated.next() => {
                match msg {
                    Some(Ok(message)) => {
                        if let Some((id, time)) = shortcut_signal(&message) {
                            if !track_repeat || repeats.press(id, time) {
                                presses.push(HotkeyId::from_raw(id));
                            }
                        }
                    }
                    Some(Err(_)) => break,
                    None => break,
                }
            }
            msg = next_signal(deactivated.as_mut()) => {
                match msg {
                    Some(Ok(message)) => {
                        if let Some((id, time)) = shortcut_signal(&message) {
                            repeats.release(id, time);
                        }
                    }
                    _ => {
                        track_repeat = false;
                        deactivated = None;
                    }
                }
            }
        }
    }
}

async fn next_signal(stream: Option<&mut MessageStream>) -> Option<zbus::Result<zbus::Message>> {
    match stream {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

async fn bind_one(
    conn: &Connection,
    id: u32,
    trigger: &str,
) -> Result<OwnedObjectPath, HotkeyFailure> {
    let unique = conn
        .unique_name()
        .ok_or(HotkeyFailure::Unsupported)?
        .to_string();
    let handle_token = format!("rndr{id}");
    let session_token = format!("rndrsess{id}");
    let session = create_session(conn, &unique, &handle_token, &session_token).await?;
    if let Err(err) = bind_shortcuts(conn, &unique, &session, id, trigger).await {
        close_session(conn, &session).await;
        return Err(err);
    }
    Ok(session)
}

async fn create_session(
    conn: &Connection,
    unique: &str,
    handle_token: &str,
    session_token: &str,
) -> Result<OwnedObjectPath, HotkeyFailure> {
    let request = request_path(unique, handle_token);
    let mut responses = response_stream(conn, &request).await?;
    let proxy = shortcuts_proxy(conn)
        .await
        .map_err(|_| HotkeyFailure::Unsupported)?;
    let options = dict(&[
        ("handle_token", handle_token),
        ("session_handle_token", session_token),
    ]);
    proxy
        .call::<_, _, OwnedObjectPath>("CreateSession", &(options,))
        .await
        .map_err(map_dbus)?;
    let (code, results) = wait_response(&mut responses, SESSION_WAIT).await?;
    if code != 0 {
        return Err(HotkeyFailure::Unsupported);
    }
    object_path(&results, "session_handle").or_else(|_| {
        OwnedObjectPath::try_from(session_path(unique, session_token))
            .map_err(|_| HotkeyFailure::Unsupported)
    })
}

async fn bind_shortcuts(
    conn: &Connection,
    unique: &str,
    session: &OwnedObjectPath,
    id: u32,
    trigger: &str,
) -> Result<(), HotkeyFailure> {
    let handle_token = format!("rndrbind{id}");
    let request = request_path(unique, &handle_token);
    let mut responses = response_stream(conn, &request).await?;
    let proxy = shortcuts_proxy(conn)
        .await
        .map_err(|_| HotkeyFailure::Unsupported)?;
    let shortcut_id = format!("hk{id}");
    let info = dict(&[
        ("description", "Show or hide the window"),
        ("preferred_trigger", trigger),
    ]);
    let shortcuts = vec![(shortcut_id.clone(), info)];
    let options = dict(&[("handle_token", handle_token.as_str())]);
    proxy
        .call::<_, _, OwnedObjectPath>("BindShortcuts", &(session, shortcuts, "", options))
        .await
        .map_err(map_dbus)?;
    let (code, results) = wait_response(&mut responses, BIND_WAIT).await?;
    if code != 0 {
        return Err(HotkeyFailure::Taken);
    }
    if granted(&results, &shortcut_id) {
        Ok(())
    } else {
        Err(HotkeyFailure::Taken)
    }
}

fn granted(results: &HashMap<String, OwnedValue>, shortcut_id: &str) -> bool {
    let Some(value) = results.get("shortcuts") else {
        return false;
    };
    let Ok(list) = Vec::<(String, HashMap<String, OwnedValue>)>::try_from(value.clone()) else {
        return false;
    };
    list.iter().any(|(id, _)| id == shortcut_id)
}

async fn wait_response(
    stream: &mut MessageStream,
    wait: Duration,
) -> Result<(u32, HashMap<String, OwnedValue>), HotkeyFailure> {
    let message = tokio::time::timeout(wait, stream.next())
        .await
        .map_err(|_| HotkeyFailure::Unsupported)?
        .ok_or(HotkeyFailure::Unsupported)?
        .map_err(|_| HotkeyFailure::Unsupported)?;
    message
        .body()
        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
        .map_err(|_| HotkeyFailure::Unsupported)
}

async fn response_stream(conn: &Connection, path: &str) -> Result<MessageStream, HotkeyFailure> {
    let path = ObjectPath::try_from(path).map_err(|_| HotkeyFailure::Unsupported)?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .map_err(|_| HotkeyFailure::Unsupported)?
        .member("Response")
        .map_err(|_| HotkeyFailure::Unsupported)?
        .path(path)
        .map_err(|_| HotkeyFailure::Unsupported)?
        .build();
    MessageStream::for_match_rule(rule, conn, Some(4))
        .await
        .map_err(|_| HotkeyFailure::Unsupported)
}

async fn shortcut_stream(conn: &Connection, member: &str) -> Result<MessageStream, HotkeyFailure> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(IFACE)
        .map_err(|_| HotkeyFailure::Unsupported)?
        .member(member)
        .map_err(|_| HotkeyFailure::Unsupported)?
        .path(PATH)
        .map_err(|_| HotkeyFailure::Unsupported)?
        .build();
    MessageStream::for_match_rule(rule, conn, Some(16))
        .await
        .map_err(|_| HotkeyFailure::Unsupported)
}

fn shortcut_signal(message: &zbus::Message) -> Option<(u32, u64)> {
    let (_session, shortcut, timestamp, _options): (
        OwnedObjectPath,
        String,
        u64,
        HashMap<String, OwnedValue>,
    ) = message.body().deserialize().ok()?;
    let id = shortcut.strip_prefix("hk")?.parse().ok()?;
    Some((id, timestamp))
}

async fn close_session(conn: &Connection, path: &OwnedObjectPath) {
    let Ok(proxy) = Proxy::new(conn, DEST, path.as_ref(), "org.freedesktop.portal.Session").await
    else {
        return;
    };
    let _ = proxy.call_method("Close", &()).await;
}

async fn shortcuts_proxy(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Proxy::new(conn, DEST, PATH, IFACE).await
}

fn dict(pairs: &[(&str, &str)]) -> HashMap<String, OwnedValue> {
    pairs
        .iter()
        .map(|(key, value)| {
            let owned = Value::from(*value)
                .try_to_owned()
                .expect("a string value owns");
            ((*key).to_string(), owned)
        })
        .collect()
}

fn object_path(
    results: &HashMap<String, OwnedValue>,
    key: &str,
) -> Result<OwnedObjectPath, HotkeyFailure> {
    let value = results.get(key).ok_or(HotkeyFailure::Unsupported)?;
    OwnedObjectPath::try_from(value.clone()).map_err(|_| HotkeyFailure::Unsupported)
}

fn request_path(unique: &str, token: &str) -> String {
    let name = unique.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{name}/{token}")
}

fn session_path(unique: &str, token: &str) -> String {
    let name = unique.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/session/{name}/{token}")
}

fn map_dbus(err: zbus::Error) -> HotkeyFailure {
    let text = err.to_string();
    if text.contains("UnknownInterface")
        || text.contains("UnknownMethod")
        || text.contains("ServiceUnknown")
        || text.contains("NameHasNoOwner")
    {
        HotkeyFailure::Unsupported
    } else {
        HotkeyFailure::Taken
    }
}
