use std::fmt;

/// Why a canvas request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasError {
    /// The request or the document it would produce breaks an invariant.
    Invalid(String),
    NotFound,
    /// The caller's expected revision is stale; carries the current one.
    Conflict(u64),
    /// The scene's schema or version is not one this crate reads.
    Unsupported,
    Storage(String),
}

impl fmt::Display for CanvasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanvasError::Invalid(message) => write!(f, "invalid canvas request: {message}"),
            CanvasError::NotFound => f.write_str("canvas document not found"),
            CanvasError::Conflict(revision) => {
                write!(f, "canvas document changed; current revision is {revision}")
            }
            CanvasError::Unsupported => f.write_str("unsupported canvas schema or version"),
            CanvasError::Storage(message) => write!(f, "canvas storage failed: {message}"),
        }
    }
}

impl std::error::Error for CanvasError {}

impl From<std::io::Error> for CanvasError {
    fn from(error: std::io::Error) -> Self {
        CanvasError::Storage(error.to_string())
    }
}

pub type Result<T, E = CanvasError> = std::result::Result<T, E>;

pub(crate) fn invalid(message: impl Into<String>) -> CanvasError {
    CanvasError::Invalid(message.into())
}
