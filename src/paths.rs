//! Where Bingee Desktop keeps its files: per-user data, cache and log
//! folders, resolved from the platform's standard environment variables.
//! Never beside the executable, never relative to the working directory.
//! See `docs/adr/0006-production-storage-and-schema-v1.md`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::{AppError, ErrorKind};
use crate::settings::{HOME_OVERRIDE, Settings};
use crate::{APP_ID, APP_NAME};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

/// The folder conventions to follow. Only `Os::CURRENT` is used at run time;
/// the others exist so every policy is tested on any host.
#[derive(Debug, Clone, Copy)]
pub enum Os {
    Windows,
    Mac,
    /// Linux and other Unix desktops: the XDG base directories.
    Xdg,
}

impl Os {
    pub const CURRENT: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Mac
    } else {
        Os::Xdg
    };
}

impl AppPaths {
    pub fn from_env(settings: &Settings) -> Result<Self, AppError> {
        Self::resolve(Os::CURRENT, settings, |name| std::env::var_os(name))
    }

    /// Pure: depends only on `os`, `settings` and the variables `env` returns.
    /// `settings.home_override`, if set, replaces every location with
    /// `<dir>/data`, `<dir>/cache` and `<dir>/logs`.
    pub fn resolve(
        os: Os,
        settings: &Settings,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, AppError> {
        let not_absolute = |name: &str| {
            AppError::new(
                ErrorKind::Configuration,
                format!(
                    "The environment variable {name} is not set to an absolute path, so \
                     {APP_NAME} cannot find its data folder."
                ),
            )
        };
        if let Some(root) = &settings.home_override {
            return match root.is_absolute() {
                true => Ok(Self::under(root)),
                false => Err(not_absolute(HOME_OVERRIDE)),
            };
        }
        let absolute = |name: &str| env(name).map(PathBuf::from).filter(|p| p.is_absolute());
        let required = |name: &str| absolute(name).ok_or_else(|| not_absolute(name));
        Ok(match os {
            Os::Windows => Self::under(&required("LOCALAPPDATA")?.join(APP_NAME)),
            Os::Mac => {
                let library = required("HOME")?.join("Library");
                Self {
                    data: library.join("Application Support").join(APP_NAME),
                    cache: library.join("Caches").join(APP_NAME),
                    logs: library.join("Logs").join(APP_NAME),
                }
            }
            Os::Xdg => {
                // The XDG spec says to ignore unset, empty or relative values.
                let base = |var: &str, default: &[&str]| -> Result<PathBuf, AppError> {
                    let dir = match absolute(var) {
                        Some(dir) => dir,
                        None => default.iter().fold(required("HOME")?, |dir, c| dir.join(c)),
                    };
                    Ok(dir.join(APP_ID))
                };
                Self {
                    data: base("XDG_DATA_HOME", &[".local", "share"])?,
                    cache: base("XDG_CACHE_HOME", &[".cache"])?,
                    logs: base("XDG_STATE_HOME", &[".local", "state"])?,
                }
            }
        })
    }

    fn under(root: &Path) -> Self {
        Self {
            data: root.join("data"),
            cache: root.join("cache"),
            logs: root.join("logs"),
        }
    }

    pub fn database(&self) -> PathBuf {
        self.data.join("bingee.db")
    }

    pub fn log_file(&self) -> PathBuf {
        self.logs.join(format!("{APP_ID}.log"))
    }

    pub fn create_dirs(&self) -> Result<(), AppError> {
        for dir in [&self.data, &self.cache, &self.logs] {
            std::fs::create_dir_all(dir).map_err(|err| {
                AppError::filesystem(
                    format!("The folder {} could not be created.", dir.display()),
                    err,
                )
            })?;
        }
        Ok(())
    }
}

/// A fresh directory under the OS temp dir, removed on drop. Tests never use
/// the real per-user folders.
#[cfg(test)]
pub struct TestDir(pub PathBuf);

#[cfg(test)]
impl TestDir {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bingee-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolves with a fake environment and no override. Values are built
    /// from a host-absolute base, so the Windows policy is tested on Unix
    /// and vice versa.
    fn resolve(os: Os, vars: &[(&str, &Path)]) -> Result<AppPaths, AppError> {
        AppPaths::resolve(os, &Settings::default(), |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.as_os_str().to_owned())
        })
    }

    fn overridden(root: &Path) -> Settings {
        Settings {
            home_override: Some(root.to_owned()),
        }
    }

    fn base() -> PathBuf {
        std::env::temp_dir().join("fake-profile")
    }

    #[test]
    fn windows_uses_local_app_data() {
        let local = base().join("AppData").join("Local");
        let paths = resolve(Os::Windows, &[("LOCALAPPDATA", &local)]).unwrap();
        let root = local.join("Bingee Desktop");
        assert_eq!(paths.data, root.join("data"));
        assert_eq!(paths.cache, root.join("cache"));
        assert_eq!(paths.logs, root.join("logs"));
        assert_eq!(paths.database(), root.join("data").join("bingee.db"));
        assert_eq!(
            paths.log_file(),
            root.join("logs").join("bingee-desktop.log")
        );
    }

    #[test]
    fn macos_uses_the_library_folders() {
        let home = base();
        let paths = resolve(Os::Mac, &[("HOME", &home)]).unwrap();
        let library = home.join("Library");
        assert_eq!(
            paths.data,
            library.join("Application Support").join("Bingee Desktop")
        );
        assert_eq!(paths.cache, library.join("Caches").join("Bingee Desktop"));
        assert_eq!(paths.logs, library.join("Logs").join("Bingee Desktop"));
    }

    #[test]
    fn xdg_variables_win_and_relative_ones_are_ignored() {
        let home = base();
        let defaults = resolve(Os::Xdg, &[("HOME", &home)]).unwrap();
        assert_eq!(
            defaults.data,
            home.join(".local").join("share").join("bingee-desktop")
        );
        assert_eq!(defaults.cache, home.join(".cache").join("bingee-desktop"));
        assert_eq!(
            defaults.logs,
            home.join(".local").join("state").join("bingee-desktop")
        );

        let data = base().join("xdg-data");
        let relative = Path::new("relative/cache");
        let vars = [
            ("HOME", home.as_path()),
            ("XDG_DATA_HOME", &data),
            ("XDG_CACHE_HOME", relative),
        ];
        let paths = resolve(Os::Xdg, &vars).unwrap();
        assert_eq!(paths.data, data.join("bingee-desktop"));
        assert_eq!(paths.cache, defaults.cache, "relative XDG value ignored");
    }

    #[test]
    fn override_replaces_every_location_and_must_be_absolute() {
        let root = base().join("bingee-home");
        for os in [Os::Windows, Os::Mac, Os::Xdg] {
            let paths = AppPaths::resolve(os, &overridden(&root), |_| None).unwrap();
            assert_eq!(paths, AppPaths::under(&root));
        }
        for relative in ["data", ""] {
            let error = AppPaths::resolve(Os::CURRENT, &overridden(Path::new(relative)), |_| None);
            assert_eq!(
                error.unwrap_err().kind,
                ErrorKind::Configuration,
                "{relative:?}"
            );
        }
    }

    #[test]
    fn missing_or_relative_base_is_a_configuration_error_not_the_working_directory() {
        let relative = Path::new("AppData");
        for (os, vars) in [
            (Os::Windows, vec![]),
            (Os::Windows, vec![("LOCALAPPDATA", relative)]),
            (Os::Mac, vec![("HOME", relative)]),
            (Os::Xdg, vec![]),
        ] {
            let error = resolve(os, &vars).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Configuration, "{os:?}");
        }
    }

    #[test]
    fn resolution_is_deterministic_and_absolute_on_this_host() {
        let settings = Settings::default();
        let first = AppPaths::from_env(&settings).unwrap();
        assert_eq!(first, AppPaths::from_env(&settings).unwrap());
        for dir in [&first.data, &first.cache, &first.logs] {
            assert!(dir.is_absolute(), "{}", dir.display());
        }
    }

    #[test]
    fn create_dirs_creates_all_three_and_is_idempotent() {
        let dir = TestDir::new("create-dirs");
        let paths = AppPaths::from_env(&overridden(&dir.0)).unwrap();
        paths.create_dirs().unwrap();
        paths.create_dirs().unwrap();
        assert!(paths.data.is_dir() && paths.cache.is_dir() && paths.logs.is_dir());

        // A file where a folder should be: a filesystem error, not a panic.
        let blocked = AppPaths::under(&dir.0.join("file"));
        std::fs::write(dir.0.join("file"), b"").unwrap();
        assert_eq!(
            blocked.create_dirs().unwrap_err().kind,
            ErrorKind::Filesystem
        );
    }
}
