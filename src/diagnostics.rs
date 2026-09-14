//! Local diagnostics: a small append-only text log, also echoed to stderr.
//!
//! No telemetry: nothing leaves the machine. Only startup, database
//! migrations, failures and panics are logged, never per-interaction events,
//! and never secrets.

use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A log above this size is moved to `<name>.log.1` (replacing the previous
/// one) when the app starts, so the two files stay below about 2 MiB.
const MAX_BYTES: u64 = 1024 * 1024;

pub struct Log {
    file: Option<File>,
    path: Option<PathBuf>,
}

impl Log {
    pub fn stderr_only() -> Self {
        Self {
            file: None,
            path: None,
        }
    }

    /// Appends to `path`. If it cannot be opened, logs to stderr only: a
    /// missing log must not stop the app.
    pub fn open(path: &Path) -> Self {
        if std::fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
            let _ = std::fs::rename(path, path.with_extension("log.1"));
        }
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => Self {
                file: Some(file),
                path: Some(path.to_owned()),
            },
            Err(err) => {
                eprintln!("The log file {} cannot be written: {err}", path.display());
                Self::stderr_only()
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn info(&self, message: impl Display) {
        self.write("INFO", message, true);
    }

    pub fn error(&self, message: impl Display) {
        self.write("ERROR", message, true);
    }

    fn write(&self, level: &str, message: impl Display, echo: bool) {
        let line = format!("{} {level} {message}\n", utc_timestamp(SystemTime::now()));
        if echo {
            eprint!("{line}");
        }
        if let Some(mut file) = self.file.as_ref() {
            // Nowhere left to report a failed log write.
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// Also records panics in the log file: a Windows release build has no
    /// console, so a panic would otherwise leave no trace.
    pub fn record_panics(&self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            Log::open(&path).write("PANIC", info, false);
            default(info);
        }));
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ`, without a date/time dependency.
fn utc_timestamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::TestDir;
    use std::time::Duration;

    #[test]
    fn timestamps_are_utc_iso_8601() {
        let at = |secs| utc_timestamp(UNIX_EPOCH + Duration::from_secs(secs));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(at(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn appends_lines_and_rotates_a_large_log() {
        let dir = TestDir::new("log");
        let path = dir.0.join("bingee-desktop.log");
        Log::open(&path).info("first");
        Log::open(&path).error("second");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with("Z INFO first"), "{text}");
        assert!(lines[1].ends_with("Z ERROR second"), "{text}");

        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        let log = Log::open(&path);
        log.info("after rotation");
        assert_eq!(log.path(), Some(path.as_path()));
        let rotated = dir.0.join("bingee-desktop.log.1");
        assert_eq!(std::fs::metadata(&rotated).unwrap().len(), MAX_BYTES + 1);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .ends_with("INFO after rotation\n")
        );
    }

    #[test]
    fn unwritable_log_falls_back_to_stderr() {
        let dir = TestDir::new("log-missing");
        let log = Log::open(&dir.0.join("no-such-folder").join("x.log"));
        assert_eq!(log.path(), None);
        log.info("still fine");
    }
}
