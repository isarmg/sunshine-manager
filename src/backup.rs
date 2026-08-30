use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail, ensure};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use uuid::Uuid;

use crate::runtime_lock::MaintenanceLock;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

const SQLITE_SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

const REQUIRED_TABLES: [&str; 6] = [
    "hosts",
    "audit_logs",
    "auth_users",
    "auth_sessions",
    "operations",
    "audit_outbox",
];

pub fn create(database_url: &str, output: &Path) -> anyhow::Result<()> {
    // Online backup may run beside the service, but an exclusive restore or
    // schema maintenance command must not begin while the snapshot is open.
    let _maintenance = MaintenanceLock::shared(database_url)?;
    let source = database_path(database_url)?;
    ensure!(source.is_file(), "SQLite database file does not exist");
    ensure_distinct_files(&source, output)?;

    let mut pending = PendingFile::create(output)
        .with_context(|| format!("create backup output {}", output.display()))?;
    copy_database(&source, output)?;
    verify(output)?;
    sync_file_and_parent(output)?;
    pending.commit();
    Ok(())
}

pub fn verify(path: &Path) -> anyhow::Result<()> {
    require_secure_regular_file(path, "SQLite backup")?;
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .context("open SQLite backup read-only")?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .context("configure SQLite backup verification timeout")?;

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .context("run SQLite integrity check")?;
    ensure!(
        integrity.eq_ignore_ascii_case("ok"),
        "SQLite integrity check failed"
    );

    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .context("prepare SQLite foreign-key check")?;
    ensure!(
        foreign_keys.query([])?.next()?.is_none(),
        "SQLite foreign-key check failed"
    );

    let table_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='table' AND name IN (\
             'hosts','audit_logs','auth_users','auth_sessions','operations','audit_outbox'\
         )",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        table_count == REQUIRED_TABLES.len() as i64,
        "backup is not a complete Sunshine Manager database"
    );
    let operation_columns: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('operations') \
         WHERE name IN (\
             'operation_id','actor','host_id','action','idempotency_key_hash',\
             'request_fingerprint','request_ciphertext','state','attempt',\
             'created_at_micros','updated_at_micros','started_at_micros',\
             'completed_at_micros','error_code'\
         )",
        [],
        |row| row.get(0),
    )?;
    let outbox_columns: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('audit_outbox') \
         WHERE name IN (\
             'outbox_id','operation_id','event_kind','action','target','actor',\
             'detail','created_at_micros','delivered_at_micros','delivery_attempt'\
         )",
        [],
        |row| row.get(0),
    )?;
    let audit_outbox_column: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('audit_logs') WHERE name='outbox_id'",
        [],
        |row| row.get(0),
    )?;
    let operation_indexes: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (\
             'operations_idempotency_idx','audit_logs_outbox_id_idx',\
             'audit_outbox_operation_event_idx'\
         )",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        operation_columns == 14
            && outbox_columns == 10
            && audit_outbox_column == 1
            && operation_indexes == 3,
        "backup is missing the durable operation or audit outbox schema"
    );
    Ok(())
}

pub fn restore(database_url: &str, input: &Path) -> anyhow::Result<()> {
    restore_with_hook(database_url, input, |_| Ok(()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestorePoint {
    DestinationQuiesced,
    SidecarPreserved,
    BeforeInstalledVerification,
    AfterInstalledVerification,
    CommittedBeforeCleanup,
}

fn restore_with_hook(
    database_url: &str,
    input: &Path,
    mut hook: impl FnMut(RestorePoint) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // This is the authoritative stop-the-world check. SQLite locking alone is
    // insufficient because a live process could keep using the replaced inode.
    let _maintenance = MaintenanceLock::exclusive(database_url)?;

    // No destination path is changed until both the supplied database and the
    // exact staged image that will be installed have passed full verification.
    ensure_sqlite_sidecars_absent(input)?;
    verify(input)?;
    ensure_sqlite_sidecars_absent(input)?;
    let destination = database_path(database_url)?;
    require_real_parent(&destination, "restore destination")?;
    ensure_distinct_files(input, &destination)?;
    inspect_original_generation(&destination, input)?;

    let (mut pending, temporary) = PendingFile::create_adjacent(&destination)?;
    copy_database(input, &temporary)?;
    ensure_sqlite_sidecars_absent(input)?;
    verify(&temporary)?;
    remove_sqlite_sidecars_checked(&temporary)?;
    sync_file_and_parent(&temporary)?;

    let destination_lock = lock_destination(&destination)?;
    drop(destination_lock);
    sync_parent(&destination)?;
    hook(RestorePoint::DestinationQuiesced)?;

    // SQLite may remove stale sidecars while the checkpointing connection is
    // closed, so inspect again immediately before the reversible exchange.
    let original = inspect_original_generation(&destination, input)?;
    let mut recovery = RecoverySet::create(&destination)?;
    let install_result = (|| {
        for original_file in original {
            let is_sidecar = original_file.is_sidecar;
            recovery.preserve(original_file)?;
            if is_sidecar {
                hook(RestorePoint::SidecarPreserved)?;
            }
        }

        ensure_generation_paths_absent(&destination)?;
        rename_without_overwrite(&temporary, &destination).with_context(|| {
            format!(
                "atomically install verified restore at {}",
                destination.display()
            )
        })?;
        pending.commit();
        recovery.installed = true;
        sync_file_and_parent(&destination)?;
        ensure_sqlite_sidecars_absent(&destination)?;

        hook(RestorePoint::BeforeInstalledVerification)?;
        verify(&destination).context("verify installed SQLite restore")?;
        ensure_sqlite_sidecars_absent(&destination)?;
        hook(RestorePoint::AfterInstalledVerification)?;
        sync_file_and_parent(&destination)?;
        Ok::<(), anyhow::Error>(())
    })();

    if let Err(install_error) = install_result {
        let evidence = recovery.directory.clone();
        return match recovery.rollback() {
            Ok(()) => Err(install_error
                .context("restore failed; the original SQLite generation was restored")),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "restore failed and automatic rollback was incomplete; recoverable evidence is preserved at {}: restore error: {install_error:#}; rollback error: {rollback_error:#}",
                evidence.display()
            )),
        };
    }

    let evidence = recovery.directory.clone();
    hook(RestorePoint::CommittedBeforeCleanup)
        .and_then(|()| recovery.cleanup_committed())
        .with_context(|| {
            format!(
                "the restored database is installed and verified, but old-generation cleanup is incomplete at {}",
                evidence.display()
            )
        })?;
    Ok(())
}

pub(crate) fn database_path(database_url: &str) -> anyhow::Result<PathBuf> {
    let value = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .context("database URL must use the sqlite scheme")?;
    ensure!(!value.is_empty(), "SQLite database path must not be empty");
    ensure!(
        value != ":memory:",
        "in-memory databases cannot be backed up"
    );
    ensure!(
        !value.contains('?')
            && !value.contains('#')
            && !value.contains('%')
            && !value.contains('\0'),
        "backup commands require a plain, unescaped SQLite file URL without query or fragment"
    );
    Ok(PathBuf::from(value))
}

fn copy_database(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let source = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .context("open SQLite backup source")?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut destination = Connection::open_with_flags(
        destination,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .context("open SQLite backup destination")?;
    {
        let backup =
            Backup::new(&source, &mut destination).context("start SQLite online backup")?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .context("copy SQLite database pages")?;
    }
    destination
        .execute_batch("PRAGMA journal_mode=DELETE;")
        .context("finalize standalone SQLite backup")?;
    Ok(())
}

fn lock_destination(path: &Path) -> anyhow::Result<Option<Connection>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => require_secure_regular_file(path, "restore destination")?,
    };
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .context("open restore destination")?;
    connection.busy_timeout(Duration::from_secs(2))?;
    let (busy, _, _): (i64, i64, i64) =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    ensure!(busy == 0, "restore destination is busy");
    connection.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
    connection
        .execute_batch("BEGIN EXCLUSIVE")
        .context("restore destination is in use; stop the service before restoring")?;
    Ok(Some(connection))
}

fn ensure_distinct_files(left: &Path, right: &Path) -> anyhow::Result<()> {
    if left.exists() && right.exists() {
        let left_metadata = fs::metadata(left)?;
        let right_metadata = fs::metadata(right)?;
        #[cfg(unix)]
        let same_file = {
            use std::os::unix::fs::MetadataExt;
            left_metadata.dev() == right_metadata.dev()
                && left_metadata.ino() == right_metadata.ino()
        };
        #[cfg(not(unix))]
        let same_file = fs::canonicalize(left)? == fs::canonicalize(right)?;
        ensure!(!same_file, "source and destination must be different files");
    }
    Ok(())
}

struct OriginalFile {
    path: PathBuf,
    recovery_name: OsString,
    is_sidecar: bool,
}

fn inspect_original_generation(
    destination: &Path,
    input: &Path,
) -> anyhow::Result<Vec<OriginalFile>> {
    require_real_parent(destination, "restore destination")?;
    let database_exists = secure_regular_file_exists(destination, "restore destination")?;
    if database_exists {
        ensure_distinct_files(input, destination)?;
    }

    let mut files = Vec::new();
    for suffix in SQLITE_SIDECAR_SUFFIXES {
        let sidecar = sqlite_sidecar(destination, suffix);
        if secure_regular_file_exists(&sidecar, "SQLite sidecar")? {
            ensure!(
                database_exists,
                "orphan SQLite sidecars are not accepted for a first restore"
            );
            ensure_distinct_files(input, &sidecar)?;
            files.push(OriginalFile {
                path: sidecar,
                recovery_name: OsString::from(format!("database{suffix}")),
                is_sidecar: true,
            });
        }
    }
    if database_exists {
        files.push(OriginalFile {
            path: destination.to_path_buf(),
            recovery_name: OsString::from("database"),
            is_sidecar: false,
        });
    }
    Ok(files)
}

struct RecoveryEntry {
    original: PathBuf,
    recovery: PathBuf,
}

struct RecoverySet {
    destination: PathBuf,
    directory: PathBuf,
    entries: Vec<RecoveryEntry>,
    installed: bool,
}

impl RecoverySet {
    fn create(destination: &Path) -> anyhow::Result<Self> {
        let directory = create_recovery_directory(destination)?;
        Ok(Self {
            destination: destination.to_path_buf(),
            directory,
            entries: Vec::new(),
            installed: false,
        })
    }

    fn preserve(&mut self, original: OriginalFile) -> anyhow::Result<()> {
        require_secure_regular_file(&original.path, "original SQLite generation file")?;
        File::open(&original.path)?.sync_all()?;
        let recovery = self.directory.join(original.recovery_name);
        rename_without_overwrite(&original.path, &recovery).with_context(|| {
            format!(
                "preserve original SQLite generation file {}",
                original.path.display()
            )
        })?;
        self.entries.push(RecoveryEntry {
            original: original.path.clone(),
            recovery: recovery.clone(),
        });
        sync_relocated_file(&original.path, &recovery)
    }

    fn rollback(&mut self) -> anyhow::Result<()> {
        if self.installed {
            remove_installed_generation(&self.destination)?;
            self.installed = false;
        }

        for entry in &self.entries {
            match fs::symlink_metadata(&entry.recovery) {
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
                Ok(_) => {}
            }
            require_secure_regular_file(&entry.recovery, "restore recovery evidence")?;
            ensure_path_absent(
                &entry.original,
                "original SQLite path changed during rollback",
            )?;
            rename_without_overwrite(&entry.recovery, &entry.original).with_context(|| {
                format!(
                    "restore original SQLite generation file {}",
                    entry.original.display()
                )
            })?;
            sync_relocated_file(&entry.recovery, &entry.original)?;
        }

        fs::remove_dir(&self.directory)
            .with_context(|| format!("remove recovery directory {}", self.directory.display()))?;
        sync_parent(&self.directory)?;
        Ok(())
    }

    fn cleanup_committed(&mut self) -> anyhow::Result<()> {
        // The checkpointed database is self-contained. Delete sidecar evidence
        // first and the old main file last, so a cleanup failure still leaves
        // a complete old database or a complete verified new database.
        for entry in &self.entries {
            require_secure_regular_file(&entry.recovery, "old SQLite recovery evidence")?;
        }
        for entry in &self.entries {
            fs::remove_file(&entry.recovery)
                .with_context(|| format!("remove old evidence {}", entry.recovery.display()))?;
            sync_parent(&entry.recovery)?;
        }
        fs::remove_dir(&self.directory)
            .with_context(|| format!("remove recovery directory {}", self.directory.display()))?;
        sync_parent(&self.directory)?;
        Ok(())
    }
}

fn remove_installed_generation(destination: &Path) -> anyhow::Result<()> {
    let mut paths = SQLITE_SIDECAR_SUFFIXES
        .iter()
        .map(|suffix| sqlite_sidecar(destination, suffix))
        .collect::<Vec<_>>();
    paths.push(destination.to_path_buf());

    for path in &paths {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => {
                require_secure_regular_file(path, "failed restored SQLite generation file")?;
            }
        }
    }
    for path in paths {
        match fs::remove_file(&path) {
            Ok(()) => sync_parent(&path)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("remove {}", path.display()));
            }
        }
    }
    Ok(())
}

fn remove_sqlite_sidecars_checked(path: &Path) -> anyhow::Result<()> {
    let sidecars = SQLITE_SIDECAR_SUFFIXES
        .iter()
        .map(|suffix| sqlite_sidecar(path, suffix))
        .collect::<Vec<_>>();
    for sidecar in &sidecars {
        match fs::symlink_metadata(sidecar) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => require_secure_regular_file(sidecar, "SQLite sidecar")?,
        }
    }
    for sidecar in sidecars {
        match fs::remove_file(&sidecar) {
            Ok(()) => sync_parent(&sidecar)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("remove {}", sidecar.display()));
            }
        }
    }
    Ok(())
}

fn create_recovery_directory(destination: &Path) -> anyhow::Result<PathBuf> {
    let parent = real_parent(destination);
    let name = destination
        .file_name()
        .context("restore destination must name a file")?;
    for _ in 0..16 {
        let mut recovery_name = OsString::from(".");
        recovery_name.push(name);
        recovery_name.push(format!(".restore-{}.recovery", Uuid::new_v4()));
        let directory = parent.join(recovery_name);
        match fs::create_dir(&directory) {
            Ok(()) => {
                #[cfg(unix)]
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                File::open(&directory)?.sync_all()?;
                sync_parent(&directory)?;
                return Ok(directory);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("could not allocate a unique restore recovery directory")
}

fn rename_without_overwrite(source: &Path, destination: &Path) -> anyhow::Result<()> {
    ensure_path_absent(destination, "restore target already exists")?;
    #[cfg(target_os = "linux")]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            source,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
    }
    #[cfg(not(target_os = "linux"))]
    fs::rename(source, destination)?;
    Ok(())
}

fn sync_relocated_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    require_secure_regular_file(destination, "relocated SQLite generation file")?;
    File::open(destination)?.sync_all()?;
    sync_parent(source)?;
    if source.parent() != destination.parent() {
        sync_parent(destination)?;
    }
    Ok(())
}

fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn ensure_path_absent(path: &Path, message: &str) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => bail!("{message}: {}", path.display()),
    }
}

fn ensure_generation_paths_absent(destination: &Path) -> anyhow::Result<()> {
    ensure_sqlite_sidecars_absent(destination)?;
    ensure_path_absent(destination, "restore destination changed during restore")
}

fn ensure_sqlite_sidecars_absent(destination: &Path) -> anyhow::Result<()> {
    for suffix in SQLITE_SIDECAR_SUFFIXES {
        ensure_path_absent(
            &sqlite_sidecar(destination, suffix),
            "unexpected SQLite sidecar appeared during restore",
        )?;
    }
    Ok(())
}

fn secure_regular_file_exists(path: &Path, description: &str) -> anyhow::Result<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => {
            require_secure_regular_file(path, description)?;
            Ok(true)
        }
    }
}

fn require_secure_regular_file(path: &Path, description: &str) -> anyhow::Result<()> {
    require_real_parent(path, description)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{description} does not exist: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "{description} must be a regular file without symbolic links"
    );
    #[cfg(unix)]
    ensure!(
        metadata.nlink() == 1,
        "{description} must not have multiple hard links"
    );
    Ok(())
}

fn require_real_parent(path: &Path, description: &str) -> anyhow::Result<()> {
    ensure!(path.file_name().is_some(), "{description} must name a file");
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "{description} path must not contain parent traversal"
    );
    let parent = real_parent(path);
    let mut current = PathBuf::new();
    for component in parent.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => current.push("."),
            Component::Normal(value) => current.push(value),
            Component::ParentDir => bail!("{description} path must not contain parent traversal"),
        }
        let metadata = fs::symlink_metadata(&current).with_context(|| {
            format!("{description} parent does not exist: {}", current.display())
        })?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "{description} path must not traverse symbolic links or special files"
        );
    }
    Ok(())
}

fn real_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn sync_file_and_parent(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all()?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()?;
    }
    Ok(())
}

struct PendingFile {
    path: PathBuf,
    committed: bool,
}

impl PendingFile {
    fn create(path: &Path) -> anyhow::Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        options.open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            committed: false,
        })
    }

    fn create_adjacent(destination: &Path) -> anyhow::Result<(Self, PathBuf)> {
        require_real_parent(destination, "restore destination")?;
        let parent = real_parent(destination);
        let name = destination
            .file_name()
            .context("restore destination must name a file")?;
        for _ in 0..16 {
            let mut temporary_name = OsString::from(".");
            temporary_name.push(name);
            temporary_name.push(format!(".restore-{}.tmp", Uuid::new_v4()));
            let temporary = parent.join(temporary_name);
            match Self::create(&temporary) {
                Ok(pending) => return Ok((pending, temporary)),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        anyhow::bail!("could not allocate a unique restore temporary file")
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
            let _ = remove_sqlite_sidecars_checked(&self.path);
            let _ = sync_parent(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_product_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        connection
            .execute_batch(include_str!("../migrations/202608270001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/202608290001_auth_users.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/202608290002_auth_sessions.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!(
                "../migrations/202608290003_persistent_operations.sql"
            ))
            .unwrap();
        connection
            .execute_batch(
                "CREATE TABLE parents(id INTEGER PRIMARY KEY);\
                 CREATE TABLE children(\
                     id INTEGER PRIMARY KEY,\
                     parent_id INTEGER NOT NULL REFERENCES parents(id)\
                 );\
                 INSERT INTO parents(id) VALUES(1);\
                 INSERT INTO children(id,parent_id) VALUES(1,1);",
            )
            .unwrap();
    }

    fn parent_count(path: &Path) -> i64 {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        connection
            .query_row("SELECT COUNT(*) FROM parents", [], |row| row.get(0))
            .unwrap()
    }

    fn add_old_generation_marker(path: &Path) {
        Connection::open(path)
            .unwrap()
            .execute("INSERT INTO parents(id) VALUES(2)", [])
            .unwrap();
    }

    fn assert_no_restore_artifacts(database: &Path) {
        let prefix = format!(
            ".{}.restore-",
            database.file_name().unwrap().to_string_lossy()
        );
        let artifacts = fs::read_dir(real_parent(database))
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&prefix))
            .collect::<Vec<_>>();
        assert!(
            artifacts.is_empty(),
            "restore artifacts remain: {artifacts:?}"
        );
    }

    #[test]
    fn backup_is_verified_non_overwriting_and_restores_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("app.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        create_product_database(&database);
        let database_url = format!("sqlite://{}", database.display());

        let application = crate::runtime_lock::ApplicationLock::acquire(&database_url).unwrap();
        create(&database_url, &backup).unwrap();
        verify(&backup).unwrap();
        assert!(create(&database_url, &backup).is_err());

        let connection = Connection::open(&database).unwrap();
        connection
            .execute("INSERT INTO parents(id) VALUES(2)", [])
            .unwrap();
        drop(connection);
        assert!(
            restore(&database_url, &backup).is_err(),
            "restore must fail closed while the service lock is held"
        );
        drop(application);
        restore(&database_url, &backup).unwrap();

        let restored = Connection::open(&database).unwrap();
        let count: i64 = restored
            .query_row("SELECT COUNT(*) FROM parents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        verify(&database).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn installed_verification_and_post_verify_failures_restore_the_old_database() {
        for fail_after_verification in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let database = directory.path().join("app.sqlite3");
            let backup = directory.path().join("backup.sqlite3");
            create_product_database(&database);
            let database_url = format!("sqlite://{}", database.display());
            create(&database_url, &backup).unwrap();
            add_old_generation_marker(&database);

            let mut installed_was_seen = false;
            let result = restore_with_hook(&database_url, &backup, |point| {
                if !fail_after_verification && point == RestorePoint::BeforeInstalledVerification {
                    installed_was_seen = true;
                    fs::write(&database, b"injected invalid installed database")?;
                    File::open(&database)?.sync_all()?;
                }
                if fail_after_verification && point == RestorePoint::AfterInstalledVerification {
                    installed_was_seen = true;
                    bail!("injected post-verification sync failure");
                }
                Ok(())
            });
            assert!(result.is_err());
            assert!(installed_was_seen, "fault must run after installation");

            // A new connection models the next process start. It must observe
            // the original generation, never the briefly installed backup.
            assert_eq!(parent_count(&database), 2);
            verify(&database).unwrap();
            assert_no_restore_artifacts(&database);
        }
    }

    #[test]
    fn committed_cleanup_failure_keeps_the_verified_new_database_and_old_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("app.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        create_product_database(&database);
        let database_url = format!("sqlite://{}", database.display());
        create(&database_url, &backup).unwrap();
        add_old_generation_marker(&database);

        let error = restore_with_hook(&database_url, &backup, |point| {
            if point == RestorePoint::CommittedBeforeCleanup {
                bail!("injected committed cleanup failure");
            }
            Ok(())
        })
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("restored database is installed and verified"),
            "cleanup error must describe the committed generation: {error:#}"
        );
        assert_eq!(parent_count(&database), 1);
        verify(&database).unwrap();

        let prefix = format!(
            ".{}.restore-",
            database.file_name().unwrap().to_string_lossy()
        );
        assert!(
            fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix)),
            "old-generation evidence must remain available"
        );
    }

    #[test]
    fn sidecar_exchange_failure_restores_sidecar_and_restart_reads_old_database() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("app.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        let old_sidecar = sqlite_sidecar(&database, "-wal");
        let old_sidecar_bytes = b"old-generation-sidecar-evidence";
        create_product_database(&database);
        let database_url = format!("sqlite://{}", database.display());
        create(&database_url, &backup).unwrap();
        add_old_generation_marker(&database);

        let mut injected = false;
        let result = restore_with_hook(&database_url, &backup, |point| {
            if point == RestorePoint::DestinationQuiesced {
                fs::write(&old_sidecar, old_sidecar_bytes)?;
            } else if point == RestorePoint::SidecarPreserved && !injected {
                injected = true;
                bail!("injected sidecar exchange failure");
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(injected);
        assert_eq!(fs::read(&old_sidecar).unwrap(), old_sidecar_bytes);
        assert_eq!(parent_count(&database), 2);
        verify(&database).unwrap();
        assert_no_restore_artifacts(&database);
    }

    #[test]
    fn first_restore_is_safe_with_and_without_an_injected_install_failure() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("backup.sqlite3");
        create_product_database(&input);

        let installed = directory.path().join("installed.sqlite3");
        let installed_url = format!("sqlite://{}", installed.display());
        restore(&installed_url, &input).unwrap();
        assert_eq!(parent_count(&installed), 1);
        assert_no_restore_artifacts(&installed);

        let absent = directory.path().join("absent.sqlite3");
        let absent_url = format!("sqlite://{}", absent.display());
        let result = restore_with_hook(&absent_url, &input, |point| {
            if point == RestorePoint::BeforeInstalledVerification {
                bail!("injected first-install verification failure");
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!absent.exists());
        for suffix in SQLITE_SIDECAR_SUFFIXES {
            assert!(!sqlite_sidecar(&absent, suffix).exists());
        }
        assert_no_restore_artifacts(&absent);
    }

    #[cfg(unix)]
    #[test]
    fn restore_rejects_symlinks_special_files_hardlinks_and_path_traversal() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.sqlite3");
        create_product_database(&input);

        let linked_input = directory.path().join("linked-input.sqlite3");
        symlink(&input, &linked_input).unwrap();
        let missing = directory.path().join("missing.sqlite3");
        assert!(restore(&format!("sqlite://{}", missing.display()), &linked_input).is_err());
        assert!(!missing.exists());

        let hardlinked_input = directory.path().join("hardlinked-input.sqlite3");
        fs::hard_link(&input, &hardlinked_input).unwrap();
        let other_missing = directory.path().join("other-missing.sqlite3");
        assert!(
            restore(
                &format!("sqlite://{}", other_missing.display()),
                &hardlinked_input,
            )
            .is_err()
        );

        let clean_input = directory.path().join("clean-input.sqlite3");
        create_product_database(&clean_input);
        let destination = directory.path().join("destination.sqlite3");
        create_product_database(&destination);
        let destination_alias = directory.path().join("destination-alias.sqlite3");
        fs::hard_link(&destination, destination_alias).unwrap();
        assert!(restore(&format!("sqlite://{}", destination.display()), &clean_input,).is_err());
        assert_eq!(parent_count(&destination), 1);

        let special_destination = directory.path().join("special.sqlite3");
        create_product_database(&special_destination);
        fs::create_dir(sqlite_sidecar(&special_destination, "-wal")).unwrap();
        assert!(
            restore(
                &format!("sqlite://{}", special_destination.display()),
                &clean_input,
            )
            .is_err()
        );

        let real = directory.path().join("real");
        fs::create_dir(&real).unwrap();
        let nested_input = real.join("nested.sqlite3");
        create_product_database(&nested_input);
        let linked_parent = directory.path().join("linked-parent");
        symlink(&real, &linked_parent).unwrap();
        assert!(
            restore(
                &format!(
                    "sqlite://{}",
                    directory.path().join("linked-dest.sqlite3").display()
                ),
                &linked_parent.join("nested.sqlite3"),
            )
            .is_err()
        );

        let subdirectory = directory.path().join("sub");
        fs::create_dir(&subdirectory).unwrap();
        assert!(
            restore(
                &format!(
                    "sqlite://{}",
                    directory.path().join("traversal.sqlite3").display()
                ),
                &subdirectory.join("..").join("clean-input.sqlite3"),
            )
            .is_err()
        );
    }

    #[test]
    fn verification_rejects_corruption_foreign_keys_and_wrong_schema() {
        let directory = tempfile::tempdir().unwrap();
        let corrupt = directory.path().join("corrupt.sqlite3");
        fs::write(&corrupt, b"not a sqlite database").unwrap();
        assert!(verify(&corrupt).is_err());

        let invalid = directory.path().join("invalid.sqlite3");
        create_product_database(&invalid);
        let connection = Connection::open(&invalid).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        connection
            .execute("INSERT INTO children(id,parent_id) VALUES(2,999)", [])
            .unwrap();
        drop(connection);
        assert!(verify(&invalid).is_err());

        let wrong = directory.path().join("wrong.sqlite3");
        Connection::open(&wrong)
            .unwrap()
            .execute("CREATE TABLE unrelated(id INTEGER)", [])
            .unwrap();
        assert!(verify(&wrong).is_err());
    }

    #[test]
    fn database_url_parser_rejects_memory_and_ambiguous_file_urls() {
        assert!(database_path("sqlite::memory:").is_err());
        assert!(database_path("sqlite:///tmp/app.db?mode=ro").is_err());
        assert!(database_path("postgresql:///app").is_err());
        assert_eq!(
            database_path("sqlite:///var/lib/app.db").unwrap(),
            PathBuf::from("/var/lib/app.db")
        );
    }
}
