//! Allowlisted Lucide-style icon ids for `{{icon:name}}` and `icon:` fields.

/// Accepted ids, including aliases that normalize to a canonical id.
pub const ICON_ALLOWLIST: &[&str] = &[
    "sparkles",
    "lock",
    "globe",
    "message-circle",
    "messagecircle",
    "brain",
    "shield",
    "code",
    "cpu",
    "zap",
    "clock",
    "hourglass",
    "coins",
    "tag",
    "mic",
    "pencil",
    "file-code",
    "filecode",
    "table",
    "layers",
    "rocket",
    "star",
    "check",
    "x",
    "info",
    "alert-triangle",
    "alerttriangle",
    "search",
    "book",
    "map",
    "compass",
    "plane",
    "map-pin",
    "mappin",
    "hotel",
    "camera",
    "heart",
    "home",
    "calendar",
    "sun",
    "moon",
    "coffee",
    "train",
    "train-front",
    "trainfront",
    "car",
    "building",
    "building-2",
    "building2",
    "landmark",
    "mountain",
    "utensils",
    "shopping-bag",
    "shoppingbag",
    "music",
    "users",
    "flag",
    "navigation",
    "house",
    "bed",
];

/// Canonical kebab-case id for `raw`, or `None` when it is not allowlisted.
///
/// ```
/// assert_eq!(rndr_liquid::normalize_icon_id(" Map_Pin "), Some("map-pin".into()));
/// assert_eq!(rndr_liquid::normalize_icon_id("home"), Some("house".into()));
/// assert_eq!(rndr_liquid::normalize_icon_id("skull"), None);
/// ```
pub fn normalize_icon_id(raw: &str) -> Option<String> {
    let id = raw.trim().to_lowercase().replace('_', "-");
    if !ICON_ALLOWLIST.contains(&id.as_str()) {
        return None;
    }
    let canonical = match id.as_str() {
        "messagecircle" => "message-circle",
        "filecode" => "file-code",
        "alerttriangle" => "alert-triangle",
        "mappin" => "map-pin",
        "building2" | "building" => "building-2",
        "shoppingbag" => "shopping-bag",
        "trainfront" => "train-front",
        "home" => "house",
        other => other,
    };
    Some(canonical.to_string())
}

/// A run of inline text or an icon from `{{icon:id}}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineIcon<'a> {
    /// Text between shortcodes.
    Text(&'a str),
    /// Canonical icon id.
    Icon(&'a str),
}

/// Splits preprocessed text on `{{icon:id}}` shortcodes. Ids are emitted as
/// written; [`LiquidDocument`](crate::LiquidDocument) has already
/// canonicalized allowlisted ids and removed the rest.
pub fn split_icons(text: &str) -> Vec<InlineIcon<'_>> {
    const OPEN: &str = "{{icon:";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find("}}") else { break };
        let id = &after[..end];
        let valid = !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !valid {
            out.push(InlineIcon::Text(&rest[..start + OPEN.len()]));
            rest = after;
            continue;
        }
        if start > 0 {
            out.push(InlineIcon::Text(&rest[..start]));
        }
        out.push(InlineIcon::Icon(id));
        rest = &after[end + 2..];
    }
    if !rest.is_empty() {
        out.push(InlineIcon::Text(rest));
    }
    out
}
