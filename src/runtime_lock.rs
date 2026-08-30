use std::{
    ffi::OsString,
    fs::File,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, ensure};
use rustix::{
    fs::{FileType, FlockOperation, Mode, OFlags, flock, fstat, open},
    io::Errno,
};

/// Locks held for the entire HTTP worker lifetime.
///
/// The process-local per-host mutex is only safe when one application instance
/// owns a SQLite database. The second, shared lock lets online backups coexist
/// with that instance while making restore and other offline maintenance fail
/// closed.
pub struct ApplicationLock {
    _instance: File,
    _maintenance: File,
}

pub struct MaintenanceLock {
    _file: File,
}

impl ApplicationLock {
    pub fn acquire(database_url: &str) -> anyhow::Result<Self> {
        let paths = lock_paths(database_url)?;
        let instance = acquire(&paths.instance, LockKind::Exclusive)
            .context("another Sunshine Manager process already owns this SQLite database")?;
        let maintenance = acquire(&paths.maintenance, LockKind::Shared)
            .context("Sunshine Manager database maintenance is active; refusing to start")?;
        Ok(Self {
            _instance: instance,
            _maintenance: maintenance,
        })
    }
}

impl MaintenanceLock {
    pub fn shared(database_url: &str) -> anyhow::Result<Self> {
        let path = lock_paths(database_url)?.maintenance;
        acquire(&path, LockKind::Shared)
            .map(|file| Self { _file: file })
            .context("exclusive Sunshine Manager database maintenance is active")
    }

    pub fn exclusive(database_url: &str) -> anyhow::Result<Self> {
        let path = lock_paths(database_url)?.maintenance;
        acquire(&path, LockKind::Exclusive)
            .map(|file| Self { _file: file })
            .context("Sunshine Manager is running or another maintenance command is active")
    }
}

#[derive(Clone, Copy)]
enum LockKind {
    Shared,
    Exclusive,
}

struct LockPaths {
    instance: PathBuf,
    maintenance: PathBuf,
}

fn lock_paths(database_url: &str) -> anyhow::Result<LockPaths> {
    let database = crate::database_schema::database_path(database_url)?;
    let parent = database.parent().unwrap_or_else(|| Path::new("."));
    require_real_directory(parent)?;
    let name = database
        .file_name()
        .context("SQLite database path must name a file")?;
    Ok(LockPaths {
        instance: parent.join(lock_name(name, ".sunshine-manager.instance.lock")),
        maintenance: parent.join(lock_name(name, ".sunshine-manager.maintenance.lock")),
    })
}

fn lock_name(database_name: &std::ffi::OsStr, suffix: &str) -> OsString {
    let mut name = OsString::from(".");
    name.push(database_name);
    name.push(suffix);
    name
}

fn acquire(path: &Path, kind: LockKind) -> anyhow::Result<File> {
    let fd = open(
        path,
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .with_context(|| format!("open runtime lock {}", path.display()))?;
    let metadata = fstat(&fd)?;
    ensure!(
        FileType::from_raw_mode(metadata.st_mode) == FileType::RegularFile,
        "runtime lock must be a regular file"
    );
    let operation = match kind {
        LockKind::Shared => FlockOperation::NonBlockingLockShared,
        LockKind::Exclusive => FlockOperation::NonBlockingLockExclusive,
    };
    match flock(&fd, operation) {
        Ok(()) => Ok(File::from(fd)),
        Err(Errno::WOULDBLOCK) => {
            anyhow::bail!("runtime lock is already held")
        }
        Err(error) => Err(std::io::Error::from(error)).context("acquire runtime lock"),
    }
}

fn require_real_directory(path: &Path) -> anyhow::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => current.push("."),
            Component::Normal(value) => current.push(value),
            Component::ParentDir => {
                anyhow::bail!("SQLite database path must not contain parent traversal")
            }
        }
        let metadata = std::fs::symlink_metadata(&current)
            .with_context(|| format!("database directory does not exist: {}", current.display()))?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "SQLite database path must not traverse symbolic links"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_url(directory: &Path, name: &str) -> String {
        format!("sqlite://{}", directory.join(name).display())
    }

    #[test]
    fn one_instance_is_enforced_but_online_backup_lock_can_coexist() {
        let directory = tempfile::tempdir().unwrap();
        let url = database_url(directory.path(), "app.sqlite3");
        let application = ApplicationLock::acquire(&url).unwrap();
        assert!(ApplicationLock::acquire(&url).is_err());
        let backup = MaintenanceLock::shared(&url).unwrap();
        assert!(MaintenanceLock::exclusive(&url).is_err());
        drop(backup);
        drop(application);
        MaintenanceLock::exclusive(&url).unwrap();
    }

    #[test]
    fn exclusive_maintenance_blocks_start_and_other_maintenance() {
        let directory = tempfile::tempdir().unwrap();
        let url = database_url(directory.path(), "app.sqlite3");
        let maintenance = MaintenanceLock::exclusive(&url).unwrap();
        assert!(ApplicationLock::acquire(&url).is_err());
        assert!(MaintenanceLock::shared(&url).is_err());
        assert!(MaintenanceLock::exclusive(&url).is_err());
        drop(maintenance);
        ApplicationLock::acquire(&url).unwrap();
    }

    #[test]
    fn databases_have_independent_locks() {
        let directory = tempfile::tempdir().unwrap();
        let first = database_url(directory.path(), "first.sqlite3");
        let second = database_url(directory.path(), "second.sqlite3");
        let _first = ApplicationLock::acquire(&first).unwrap();
        ApplicationLock::acquire(&second).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_database_directories_and_lock_files_are_rejected() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let linked = directory.path().join("linked");
        symlink(&real, &linked).unwrap();
        assert!(ApplicationLock::acquire(&database_url(&linked, "app.sqlite3")).is_err());

        let url = database_url(&real, "app.sqlite3");
        let paths = lock_paths(&url).unwrap();
        symlink(directory.path().join("outside"), paths.instance).unwrap();
        assert!(ApplicationLock::acquire(&url).is_err());
    }
}
