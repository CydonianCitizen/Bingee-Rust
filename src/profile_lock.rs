//! OS-owned profile lock. The file may survive a crash; the lock cannot.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use crate::error::{AppError, ErrorKind};

pub struct ProfileLock(File);

impl ProfileLock {
    pub fn acquire(data_dir: &Path) -> Result<Self, AppError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(data_dir.join("bingee.lock"))
            .map_err(|error| {
                AppError::filesystem("The profile lock could not be opened.", error)
            })?;
        match file.try_lock().map_err(io::Error::from) {
            Ok(()) => Ok(Self(file)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(AppError::new(
                ErrorKind::Database,
                "Bingee Desktop is already open for this profile. Close the other window and try again.",
            )),
            Err(error) => Err(AppError::filesystem(
                "The profile lock could not be acquired.",
                error,
            )),
        }
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::TestDir;

    #[test]
    fn only_one_owner_and_stale_file_is_safe() {
        let dir = TestDir::new("profile-lock");
        let first = ProfileLock::acquire(&dir.0).unwrap();
        assert!(ProfileLock::acquire(&dir.0).is_err());
        drop(first);
        assert!(dir.0.join("bingee.lock").exists());
        let second = ProfileLock::acquire(&dir.0).unwrap();
        assert!(ProfileLock::acquire(&dir.0).is_err());
        drop(second);
        ProfileLock::acquire(&dir.0).unwrap();
    }
}
