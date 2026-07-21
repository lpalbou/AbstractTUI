//! One error type for the whole engine. Degraded paths (capability
//! fallbacks) are not errors — they are recorded as labeled warnings by
//! the layer that degrades; `Error` is for genuinely failed operations.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The current terminal cannot do what was asked and no fallback exists.
    Unsupported(String),
    /// Malformed input data (escape sequences, GLB, PNG, theme files...).
    Parse(String),
    /// Terminal/platform layer failure that is not a plain I/O error.
    Term(String),
    /// Anything raised by user components.
    App(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::Parse(m) => write!(f, "parse: {m}"),
            Error::Term(m) => write!(f, "terminal: {m}"),
            Error::App(m) => write!(f, "app: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
