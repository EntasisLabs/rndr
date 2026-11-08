//! A request to open a link in the system's default handler.
//!
//! Only `http:`, `https:` and `mailto:` links are accepted. Everything else
//! (`file:`, `javascript:`, custom app schemes) is refused before it reaches
//! the platform, as are empty links and links with whitespace, control
//! characters or malformed percent escapes.
//!
//! Core only queues the checked link. The desktop runner opens it after the
//! frame: `NSWorkspace` through `open` on macOS, `ShellExecuteW` on Windows,
//! `xdg-open` on Linux and a new tab in the browser. Headless runs and tests
//! open nothing; the links stay queued for [`take_url_requests`].

use crate::world::World;

const MAX_LEN: usize = 8192;

/// Why a link was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenUrlError {
    /// Nothing to open.
    Empty,
    /// Too long, has whitespace, control characters or bad escapes, or lacks
    /// a scheme or host.
    Invalid,
    /// A scheme other than `http`, `https` or `mailto`, lowercased.
    UnsupportedScheme(String),
}

impl std::fmt::Display for OpenUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty link"),
            Self::Invalid => f.write_str("not a valid link"),
            Self::UnsupportedScheme(scheme) => {
                write!(f, "links with the {scheme}: scheme can't be opened")
            }
        }
    }
}

impl std::error::Error for OpenUrlError {}

#[derive(Clone, Debug, Default)]
struct UrlRequests(Vec<String>);

/// Check `url` and queue it for the runner to open.
///
/// The queued link is trimmed and its scheme lowercased.
pub fn request_open_url(world: &mut World, url: &str) -> Result<(), OpenUrlError> {
    let url = check_url(url)?;
    if world.resource::<UrlRequests>().is_none() {
        world.insert_resource(UrlRequests::default());
    }
    if let Some(requests) = world.resource_mut::<UrlRequests>() {
        requests.0.push(url);
    }
    Ok(())
}

/// Take the queued links, oldest first. Each was already checked.
pub fn take_url_requests(world: &mut World) -> Vec<String> {
    world
        .resource_mut::<UrlRequests>()
        .map(|requests| std::mem::take(&mut requests.0))
        .unwrap_or_default()
}

/// `url` trimmed with a lowercase scheme, or why it can't be opened.
pub fn check_url(url: &str) -> Result<String, OpenUrlError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(OpenUrlError::Empty);
    }
    if url.len() > MAX_LEN
        || url.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '\\' | '"' | '<' | '>' | '`' | '^' | '{' | '|' | '}')
        })
        || bad_percent(url.as_bytes())
    {
        return Err(OpenUrlError::Invalid);
    }
    let (scheme, rest) = url.split_once(':').ok_or(OpenUrlError::Invalid)?;
    let scheme = scheme.to_ascii_lowercase();
    if !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err(OpenUrlError::Invalid);
    }
    match scheme.as_str() {
        "http" | "https" => {
            let after = rest.strip_prefix("//").ok_or(OpenUrlError::Invalid)?;
            let split = after.find(['/', '?', '#']).unwrap_or(after.len());
            let (authority, tail) = after.split_at(split);
            if tail.contains(['[', ']']) || !valid_authority(authority) {
                return Err(OpenUrlError::Invalid);
            }
        }
        "mailto" => {
            if rest.is_empty() || rest.starts_with("//") || rest.contains(['[', ']']) {
                return Err(OpenUrlError::Invalid);
            }
        }
        _ => return Err(OpenUrlError::UnsupportedScheme(scheme)),
    }
    Ok(format!("{scheme}:{rest}"))
}

/// A `%` not followed by two hex digits, or an escaped NUL.
fn bad_percent(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        match (bytes.get(i + 1), bytes.get(i + 2)) {
            (Some(a), Some(b)) if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => {
                if *a == b'0' && *b == b'0' {
                    return true;
                }
            }
            _ => return true,
        }
        i += 3;
    }
    false
}

fn valid_authority(authority: &str) -> bool {
    let (userinfo, host_port) = match authority.rsplit_once('@') {
        Some((user, rest)) => (Some(user), rest),
        None => (None, authority),
    };
    if userinfo.is_some_and(|user| user.is_empty() || user.contains(['[', ']', '@'])) {
        return false;
    }
    let port = if let Some(rest) = host_port.strip_prefix('[') {
        let Some((ip, port)) = rest.split_once(']') else {
            return false;
        };
        if ip.len() < 2
            || !ip.contains(':')
            || ip.contains("..")
            || !ip
                .chars()
                .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'))
        {
            return false;
        }
        if !port.is_empty() && !port.starts_with(':') {
            return false;
        }
        port.strip_prefix(':')
    } else {
        let (host, port) = match host_port.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        };
        if host.is_empty()
            || host.starts_with('.')
            || host.contains("..")
            || host.contains(['%', '[', ']'])
        {
            return false;
        }
        port
    };
    port.is_none_or(|port| port.parse::<u16>().is_ok())
}
