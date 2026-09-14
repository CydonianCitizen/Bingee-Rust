//! Application errors: a kind, a message that is safe to show the user, and
//! the low-level cause, which only reaches diagnostics.

use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Database,
    Filesystem,
    Configuration,
    /// Reserved for TMDB and other network access (R7).
    #[allow(dead_code)]
    Network,
    /// Stored or received data that violates Bingee's rules.
    InvalidData,
    /// A broken internal assumption: a bug, not a user or environment problem.
    Internal,
}

#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
    /// A complete sentence for the UI. Never contains the low-level cause.
    pub message: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn database(message: impl Into<String>, source: rusqlite::Error) -> Self {
        Self::new(ErrorKind::Database, message).with_source(source)
    }

    pub fn filesystem(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::new(ErrorKind::Filesystem, message).with_source(source)
    }
}

/// The diagnostic form, for logs: kind, message and the whole cause chain.
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} error: {}", self.kind, self.message)?;
        let mut cause = self.source();
        while let Some(error) = cause {
            write!(f, " Cause: {error}")?;
            cause = error.source();
        }
        Ok(())
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|e| e as &(dyn Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_is_user_safe_and_display_keeps_the_cause() {
        let io = std::io::Error::other("os error 5: access denied");
        let error = AppError::filesystem("The data folder could not be created.", io);
        assert_eq!(error.kind, ErrorKind::Filesystem);
        assert!(!error.message.contains("os error"));
        let logged = error.to_string();
        assert!(logged.contains("Filesystem"), "{logged}");
        assert!(logged.contains("The data folder could not be created."));
        assert!(logged.contains("os error 5: access denied"));
        assert!(error.source().is_some());
    }
}
