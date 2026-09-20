use crate::DynError;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum AcquireResult {
    Acquired(SingletonGuard),
    AlreadyRunning,
}

#[derive(Debug)]
pub struct SingletonGuard {
    file: File,
    path: PathBuf,
}

impl SingletonGuard {
    pub fn acquire() -> Result<AcquireResult, DynError> {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or("XDG_RUNTIME_DIR is not set; refusing an unsafe singleton location")?;
        Self::acquire_in(Path::new(&runtime_dir))
    }

    fn acquire_in(runtime_dir: &Path) -> Result<AcquireResult, DynError> {
        validate_runtime_dir(runtime_dir)?;
        let lock_dir = runtime_dir.join("maclife");
        match fs::create_dir(&lock_dir) {
            Ok(()) => fs::set_permissions(&lock_dir, fs::Permissions::from_mode(0o700))?,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        validate_private_dir(&lock_dir)?;

        let path = lock_dir.join("daemon.lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        validate_private_file(&file, &path)?;

        // SAFETY: flock only reads the valid file descriptor and does not retain it.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::WouldBlock {
                return Ok(AcquireResult::AlreadyRunning);
            }
            return Err(format!("could not lock {}: {error}", path.display()).into());
        }

        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "{}", std::process::id())?;
        file.sync_data()?;
        Ok(AcquireResult::Acquired(Self { file, path }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SingletonGuard {
    fn drop(&mut self) {
        // Keep the file in place: unlinking a locked file creates a race in which
        // another process can lock a new inode while this guard still owns the old one.
        // SAFETY: the guard owns this valid descriptor until Drop completes.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn validate_runtime_dir(path: &Path) -> Result<(), DynError> {
    if !path.is_absolute() {
        return Err("XDG_RUNTIME_DIR must be an absolute path".into());
    }
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect XDG_RUNTIME_DIR {}: {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!("XDG_RUNTIME_DIR {} is not a directory", path.display()).into());
    }
    if metadata.uid() != current_uid() {
        return Err(format!("XDG_RUNTIME_DIR {} is not owned by this user", path.display()).into());
    }
    Ok(())
}

fn validate_private_dir(path: &Path) -> Result<(), DynError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.uid() != current_uid() {
        return Err(format!("singleton directory {} is unsafe", path.display()).into());
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!("singleton directory {} is not private", path.display()).into());
    }
    Ok(())
}

fn validate_private_file(file: &File, path: &Path) -> Result<(), DynError> {
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file()
        || metadata.uid() != current_uid()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(format!("singleton lock {} is unsafe", path.display()).into());
    }
    Ok(())
}

fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::{AcquireResult, SingletonGuard};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "maclife-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("test runtime dir");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("permissions");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn second_instance_is_refused_until_guard_drops() {
        let runtime = TestDir::new("singleton");
        let first = match SingletonGuard::acquire_in(&runtime.0).expect("first lock") {
            AcquireResult::Acquired(guard) => guard,
            AcquireResult::AlreadyRunning => panic!("first lock refused"),
        };
        assert!(matches!(
            SingletonGuard::acquire_in(&runtime.0).expect("second attempt"),
            AcquireResult::AlreadyRunning
        ));
        drop(first);
        assert!(matches!(
            SingletonGuard::acquire_in(&runtime.0).expect("lock after drop"),
            AcquireResult::Acquired(_)
        ));
    }

    #[test]
    fn stale_unlocked_file_is_reused_safely() {
        let runtime = TestDir::new("stale");
        let lock_dir = runtime.0.join("maclife");
        fs::create_dir(&lock_dir).expect("lock dir");
        fs::set_permissions(&lock_dir, fs::Permissions::from_mode(0o700)).expect("permissions");
        fs::write(lock_dir.join("daemon.lock"), "999999\n").expect("stale file");
        fs::set_permissions(
            lock_dir.join("daemon.lock"),
            fs::Permissions::from_mode(0o600),
        )
        .expect("lock permissions");

        let guard = match SingletonGuard::acquire_in(&runtime.0).expect("stale acquisition") {
            AcquireResult::Acquired(guard) => guard,
            AcquireResult::AlreadyRunning => panic!("stale file treated as live lock"),
        };
        assert_eq!(guard.path(), lock_dir.join("daemon.lock"));
        let contents = fs::read_to_string(guard.path()).expect("pid contents");
        assert_eq!(contents.trim(), std::process::id().to_string());
    }
}
