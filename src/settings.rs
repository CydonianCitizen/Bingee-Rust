//! Bingee's own settings. Platform facts (`LOCALAPPDATA`, `HOME`, `XDG_*`)
//! stay in `paths.rs`.
//!
//! R6 has one setting, the `BINGEE_HOME` environment override. User
//! preferences (theme, language, notifications, TMDB state) join this struct
//! as typed fields with defaults when the feature that needs them arrives,
//! persisted in the library database by the migration that introduces the
//! first one. No generic key-value framework, and nothing is stored before a
//! feature needs it.

use std::path::PathBuf;

/// Environment variable behind `Settings::home_override`.
pub const HOME_OVERRIDE: &str = "BINGEE_HOME";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Keep data, cache and logs under this folder instead of the per-user
    /// locations. For development builds and tests that must not touch a real
    /// library. Must be absolute (checked by `AppPaths::resolve`).
    pub home_override: Option<PathBuf>,
}

impl Settings {
    pub fn from_env() -> Self {
        Self {
            home_override: std::env::var_os(HOME_OVERRIDE).map(PathBuf::from),
        }
    }
}
