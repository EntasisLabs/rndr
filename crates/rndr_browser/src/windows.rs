//! Windows WebView2. The view's owner is this module.
//!
//! The WebView2 loader looks up an already installed runtime. A missing
//! runtime is [`BrowserError::RuntimeMissing`]. This module does not download
//! or bundle a runtime, and it does not pass a browser-executable folder.
//!
//! File input has no COM event. The host hook is the DevTools
//! `Page.fileChooserOpened` event, after `Page.setInterceptFileChooserDialog`.
//! That is not a page message channel and it is not script evaluation. The
//! page cannot call into the host through it.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    BasicAuthenticationRequestedEventHandler, CallDevToolsProtocolMethodCompletedHandler,
    ClientCertificateRequestedEventHandler, CoTaskMemPWSTR,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DevToolsProtocolEventReceivedEventHandler, DownloadStartingEventHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::HiDpi::GetDpiForWindow;

use crate::contract::{Contract, LiveDocument};
use crate::policy::{
    self, client_pixels, file_chooser_command, file_chooser_multiple, suggested_download_name,
};
use crate::{
    BrowserError, DownloadIntent, FileInputIntent, IntentToken, LoadOutcome, NavigationState,
};

struct Host {
    notes: RefCell<Vec<Note>>,
    downloads: RefCell<HashMap<IntentToken, DownloadSlot>>,
    file: RefCell<Option<IntentToken>>,
    next: Cell<u64>,
    pending_url: RefCell<String>,
    loading: Cell<bool>,
    stop_requested: Cell<bool>,
    alive: Cell<bool>,
}

enum Note {
    Navigation(NavigationState),
    Download(DownloadIntent),
    File(FileInputIntent),
}

struct DownloadSlot {
    args: ICoreWebView2DownloadStartingEventArgs,
    deferral: ICoreWebView2Deferral,
}

pub(crate) struct WebView2 {
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2_10,
    hwnd: HWND,
    host: Rc<Host>,
    user_data: PathBuf,
    closed: bool,
}

impl WebView2 {
    pub(crate) fn attach(parent: &impl HasWindowHandle) -> Result<Self, BrowserError> {
        let hwnd = parent_hwnd(parent)?;
        ensure_sta()?;
        ensure_runtime()?;
        let user_data = user_data_dir()?;
        let (controller, webview) = create_controller(hwnd, &user_data)?;
        let host = Rc::new(Host {
            notes: RefCell::new(Vec::new()),
            downloads: RefCell::new(HashMap::new()),
            file: RefCell::new(None),
            next: Cell::new(0),
            pending_url: RefCell::new(String::new()),
            loading: Cell::new(false),
            stop_requested: Cell::new(false),
            alive: Cell::new(true),
        });
        wire(&webview, &host)?;
        unsafe {
            controller.SetIsVisible(false).map_err(hresult_error)?;
            controller
                .SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                })
                .map_err(hresult_error)?;
        }
        Ok(Self {
            controller,
            webview,
            hwnd,
            host,
            user_data,
            closed: false,
        })
    }

    pub(crate) fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let wide = CoTaskMemPWSTR::from(url);
        unsafe { self.webview.Navigate(*wide.as_ref().as_pcwstr()) }.map_err(|_| BrowserError::Url)
    }

    pub(crate) fn back(&self) -> Result<(), BrowserError> {
        unsafe {
            let _ = self.webview.GoBack();
        }
        Ok(())
    }

    pub(crate) fn forward(&self) -> Result<(), BrowserError> {
        unsafe {
            let _ = self.webview.GoForward();
        }
        Ok(())
    }

    pub(crate) fn reload(&self) -> Result<(), BrowserError> {
        unsafe {
            let _ = self.webview.Reload();
        }
        Ok(())
    }

    pub(crate) fn stop(&self) {
        self.host.stop_requested.set(true);
        unsafe {
            let _ = self.webview.Stop();
        }
    }

    pub(crate) fn apply(&self, rect: crate::ClientRect, visible: bool) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let left = client_pixels(rect.x, dpi);
        let top = client_pixels(rect.y, dpi);
        let bounds = RECT {
            left,
            top,
            right: left.saturating_add(client_pixels(rect.width, dpi)),
            bottom: top.saturating_add(client_pixels(rect.height, dpi)),
        };
        unsafe {
            let _ = self.controller.SetBounds(bounds);
            let _ = self.controller.SetIsVisible(visible);
        }
    }

    pub(crate) fn show(&self) {
        unsafe {
            let _ = self.controller.SetIsVisible(true);
        }
    }

    pub(crate) fn hide(&self) {
        unsafe {
            let _ = self.controller.SetIsVisible(false);
        }
    }

    pub(crate) fn document(&self) -> LiveDocument {
        LiveDocument {
            title: pwstr(|out| unsafe { self.webview.DocumentTitle(out) }),
            url: {
                let source = pwstr(|out| unsafe { self.webview.Source(out) });
                if source.is_empty() {
                    self.host.pending_url.borrow().clone()
                } else {
                    source
                }
            },
            loading: self.host.loading.get(),
            can_go_back: flag(|out| unsafe { self.webview.CanGoBack(out) }),
            can_go_forward: flag(|out| unsafe { self.webview.CanGoForward(out) }),
        }
    }

    pub(crate) fn pull(&self, contract: &mut Contract) {
        let notes: Vec<Note> = self.host.notes.borrow_mut().drain(..).collect();
        for note in notes {
            match note {
                Note::Navigation(state) => contract.observe(state),
                Note::Download(intent) => contract.open_download(intent),
                Note::File(intent) => contract.open_file(intent),
            }
        }
    }

    pub(crate) fn commit_download(
        &self,
        token: IntentToken,
        destination: &Path,
    ) -> Result<(), BrowserError> {
        let Some(slot) = self.host.downloads.borrow_mut().remove(&token) else {
            return Err(BrowserError::UnknownIntent);
        };
        let wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let path = PCWSTR(wide.as_ptr());
        unsafe {
            slot.args.SetResultFilePath(path).map_err(hresult_error)?;
            slot.deferral.Complete().map_err(hresult_error)?;
        }
        Ok(())
    }

    pub(crate) fn cancel_download(&self, token: IntentToken) {
        if let Some(slot) = self.host.downloads.borrow_mut().remove(&token) {
            cancel_download(slot);
        }
    }

    pub(crate) fn finish_file(
        &self,
        token: IntentToken,
        paths: Option<&[PathBuf]>,
    ) -> Result<(), BrowserError> {
        let current = self.host.file.borrow().as_ref().copied();
        if current != Some(token) {
            return Err(BrowserError::UnknownIntent);
        }
        self.host.file.borrow_mut().take();
        let owned: Option<Vec<String>> = paths.map(|paths| {
            paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        });
        let command = file_chooser_command(owned.as_deref());
        let _ = call_cdp(&self.webview, "Page.handleFileChooser", &command);
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        self.host.alive.set(false);
        let downloads: Vec<_> = self
            .host
            .downloads
            .borrow_mut()
            .drain()
            .map(|(_, slot)| slot)
            .collect();
        for slot in downloads {
            cancel_download(slot);
        }
        if self.host.file.borrow_mut().take().is_some() {
            let _ = call_cdp(
                &self.webview,
                "Page.handleFileChooser",
                "{\"action\":\"cancel\"}",
            );
        }
        self.close();
        let _ = std::fs::remove_dir_all(&self.user_data);
    }

    fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        unsafe {
            let _ = self.controller.Close();
        }
    }
}

impl Drop for WebView2 {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn parent_hwnd(parent: &impl HasWindowHandle) -> Result<HWND, BrowserError> {
    let handle = parent.window_handle().map_err(|_| BrowserError::Parent)?;
    match handle.as_raw() {
        RawWindowHandle::Win32(window) => Ok(HWND(window.hwnd.get() as *mut _)),
        _ => Err(BrowserError::Parent),
    }
}

fn ensure_sta() -> Result<(), BrowserError> {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr == windows::Win32::Foundation::RPC_E_CHANGED_MODE {
        return Err(BrowserError::Thread);
    }
    hr.ok().map_err(hresult_error)
}

fn ensure_runtime() -> Result<(), BrowserError> {
    let mut version = PWSTR::null();
    match unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) } {
        Ok(()) => {
            let text = webview2_com::take_pwstr(version);
            if text.is_empty() {
                Err(BrowserError::RuntimeMissing {
                    code: 0x8007_0002_u32 as i32,
                })
            } else {
                Ok(())
            }
        }
        Err(error) => Err(hresult_error(error)),
    }
}

fn user_data_dir() -> Result<PathBuf, BrowserError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("rndr-browser-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&path).map_err(|_| BrowserError::Create { code: 0 })?;
    Ok(path)
}

fn create_controller(
    hwnd: HWND,
    user_data: &Path,
) -> Result<(ICoreWebView2Controller, ICoreWebView2_10), BrowserError> {
    let folder = path_wide(user_data);
    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(folder.as_ptr()),
                None::<&ICoreWebView2EnvironmentOptions>,
                &handler,
            )
            .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, environment| {
            let _ = tx.send(error.and_then(|_| {
                environment.ok_or_else(|| {
                    windows::core::Error::from(windows::Win32::Foundation::E_POINTER)
                })
            }));
            Ok(())
        }),
    )
    .map_err(com_error)?;
    let environment = rx
        .recv()
        .map_err(|_| BrowserError::Create { code: 0 })?
        .map_err(hresult_error)?;

    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            environment
                .CreateCoreWebView2Controller(hwnd, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, controller| {
            let _ = tx.send(error.and_then(|_| {
                controller.ok_or_else(|| {
                    windows::core::Error::from(windows::Win32::Foundation::E_POINTER)
                })
            }));
            Ok(())
        }),
    )
    .map_err(com_error)?;
    let controller = rx
        .recv()
        .map_err(|_| BrowserError::Create { code: 0 })?
        .map_err(hresult_error)?;
    let webview = unsafe { controller.CoreWebView2() }.map_err(hresult_error)?;
    let webview = webview.cast::<ICoreWebView2_10>().map_err(hresult_error)?;
    Ok((controller, webview))
}

fn wire(webview: &ICoreWebView2_10, host: &Rc<Host>) -> Result<(), BrowserError> {
    let mut token = 0_i64;
    let started = host.clone();
    unsafe {
        webview
            .add_NavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        started.pending_url.replace(pwstr(|out| args.Uri(out)));
                        started.loading.set(true);
                        started.push_navigation(LoadOutcome::Loading);
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    let completed = host.clone();
    unsafe {
        webview
            .add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |_sender, args| {
                    completed.loading.set(false);
                    let mut status = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
                    let success = args.as_ref().is_some_and(|args| {
                        let _ = args.WebErrorStatus(&mut status);
                        flag(|out| args.IsSuccess(out))
                    });
                    let stopped = completed.stop_requested.replace(false)
                        || status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED;
                    let load = if stopped {
                        LoadOutcome::Stopped
                    } else if success {
                        LoadOutcome::Finished
                    } else {
                        LoadOutcome::Failed
                    };
                    completed.push_navigation(load);
                    Ok(())
                })),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    unsafe {
        webview
            .add_NewWindowRequested(
                &NewWindowRequestedEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let _ = args.SetHandled(true);
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    let basic = host.clone();
    unsafe {
        webview
            .add_BasicAuthenticationRequested(
                &BasicAuthenticationRequestedEventHandler::create(Box::new(
                    move |_sender, args| {
                        if let Some(args) = args {
                            let _ = args.SetCancel(true);
                        }
                        let _ = basic;
                        Ok(())
                    },
                )),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    unsafe {
        webview
            .add_ClientCertificateRequested(
                &ClientCertificateRequestedEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let _ = args.SetCancel(true);
                        let _ = args.SetHandled(true);
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    let downloads = host.clone();
    unsafe {
        webview
            .add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_sender, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    if !downloads.alive.get() {
                        let _ = args.SetCancel(true);
                        return Ok(());
                    }
                    let _ = args.SetHandled(true);
                    let deferral = match args.GetDeferral() {
                        Ok(deferral) => deferral,
                        Err(_) => {
                            let _ = args.SetCancel(true);
                            return Ok(());
                        }
                    };
                    let operation = args.DownloadOperation().ok();
                    let url = operation
                        .as_ref()
                        .map(|operation| pwstr(|out| operation.Uri(out)))
                        .unwrap_or_default();
                    let disposition = operation
                        .as_ref()
                        .map(|operation| pwstr(|out| operation.ContentDisposition(out)))
                        .unwrap_or_default();
                    let mime = operation
                        .as_ref()
                        .map(|operation| pwstr(|out| operation.MimeType(out)))
                        .unwrap_or_default();
                    let token = downloads.mint();
                    downloads
                        .downloads
                        .borrow_mut()
                        .insert(token, DownloadSlot { args, deferral });
                    downloads
                        .notes
                        .borrow_mut()
                        .push(Note::Download(DownloadIntent {
                            token,
                            suggested_name: suggested_download_name(&disposition, &url),
                            url,
                            mime,
                        }));
                    Ok(())
                })),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    call_cdp(webview, "Page.enable", "{}")?;
    call_cdp(
        webview,
        "Page.setInterceptFileChooserDialog",
        "{\"enabled\":true}",
    )?;
    let files = host.clone();
    let event_name = CoTaskMemPWSTR::from("Page.fileChooserOpened");
    let receiver =
        unsafe { webview.GetDevToolsProtocolEventReceiver(*event_name.as_ref().as_pcwstr()) }
            .map_err(hresult_error)?;
    unsafe {
        receiver
            .add_DevToolsProtocolEventReceived(
                &DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                    move |_sender, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let json = pwstr(|out| args.ParameterObjectAsJson(out));
                        if !files.alive.get() {
                            return Ok(());
                        }
                        if files.file.borrow().is_some() {
                            return Ok(());
                        }
                        let token = files.mint();
                        *files.file.borrow_mut() = Some(token);
                        files.notes.borrow_mut().push(Note::File(FileInputIntent {
                            token,
                            multiple: file_chooser_multiple(&json),
                        }));
                        Ok(())
                    },
                )),
                &mut token,
            )
            .map_err(hresult_error)?;
    }
    Ok(())
}

impl Host {
    fn mint(&self) -> IntentToken {
        let next = self.next.get().saturating_add(1);
        self.next.set(next);
        IntentToken::minted(next)
    }

    fn push_navigation(&self, load: LoadOutcome) {
        if !self.alive.get() {
            return;
        }
        let state = NavigationState {
            title: String::new(),
            url: self.pending_url.borrow().clone(),
            load,
            can_go_back: false,
            can_go_forward: false,
        };
        self.notes.borrow_mut().push(Note::Navigation(state));
    }
}

fn cancel_download(slot: DownloadSlot) {
    unsafe {
        let _ = slot.args.SetCancel(true);
        let _ = slot.deferral.Complete();
    }
}

fn call_cdp(webview: &ICoreWebView2_10, method: &str, params: &str) -> Result<(), BrowserError> {
    let method = method.to_string();
    let params = params.to_string();
    let webview = webview.clone();
    CallDevToolsProtocolMethodCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            let method = CoTaskMemPWSTR::from(method.as_str());
            let params = CoTaskMemPWSTR::from(params.as_str());
            webview
                .CallDevToolsProtocolMethod(
                    *method.as_ref().as_pcwstr(),
                    *params.as_ref().as_pcwstr(),
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(|error, _result| error),
    )
    .map_err(com_error)
}

fn path_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn pwstr(fill: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut value = PWSTR::null();
    if fill(&mut value).is_err() {
        return String::new();
    }
    webview2_com::take_pwstr(value)
}

fn flag(fill: impl FnOnce(*mut windows::core::BOOL) -> windows::core::Result<()>) -> bool {
    let mut value = windows::core::BOOL(0);
    fill(&mut value).is_ok() && value.as_bool()
}

fn hresult_error(error: windows::core::Error) -> BrowserError {
    let code = error.code().0;
    if policy::is_missing_runtime(code) {
        BrowserError::RuntimeMissing { code }
    } else {
        BrowserError::Create { code }
    }
}

fn com_error(error: webview2_com::Error) -> BrowserError {
    match error {
        webview2_com::Error::WindowsError(error) => hresult_error(error),
        _ => BrowserError::Create { code: 0 },
    }
}
