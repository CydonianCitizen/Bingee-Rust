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

use crate::database::Database;
use crate::error::AppError;

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

/// Automatic remote maintenance is opt-in. The preference lives in the
/// durable database and is included in backup V1.
pub fn automatic_refresh(db: &Database) -> Result<bool, AppError> {
    db.conn()
        .query_row(
            "SELECT automatic_refresh_enabled FROM app_settings WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AppError::database("Automatic refresh setting could not be read.", e))
}

pub fn set_automatic_refresh(db: &Database, enabled: bool) -> Result<(), AppError> {
    db.conn()
        .execute(
            "UPDATE app_settings SET automatic_refresh_enabled = ?1 WHERE id = 1",
            [enabled],
        )
        .map(|_| ())
        .map_err(|e| AppError::database("Automatic refresh setting could not be saved.", e))
}
