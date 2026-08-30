use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, ensure};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const REQUIRED_TABLES: [&str; 6] = [
    "hosts",
    "audit_logs",
    "auth_users",
    "auth_sessions",
    "operations",
    "audit_outbox",
];

pub fn create(database_url: &str, output: &Path) -> anyhow::Result<()> {
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
    ensure!(path.is_file(), "backup file does not exist");
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
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
    verify(input)?;
    let destination = database_path(database_url)?;
    ensure_distinct_files(input, &destination)?;
    reject_destination_symlink(&destination)?;

    let (mut pending, temporary) = PendingFile::create_adjacent(&destination)?;
    copy_database(input, &temporary)?;
    verify(&temporary)?;
    sync_file_and_parent(&temporary)?;

    let destination_lock = lock_destination(&destination)?;
    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "atomically replace {} with the verified restore",
            destination.display()
        )
    })?;
    pending.commit();
    sync_parent(&destination)?;
    drop(destination_lock);
    remove_sqlite_sidecars(&destination)?;
    sync_parent(&destination)?;
    verify(&destination)?;
    Ok(())
}

fn database_path(database_url: &str) -> anyhow::Result<PathBuf> {
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
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open SQLite backup source")?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut destination = Connection::open_with_flags(
        destination,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
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
    if !path.exists() {
        return Ok(None);
    }
    ensure!(path.is_file(), "restore destination is not a regular file");
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
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

fn reject_destination_symlink(path: &Path) -> anyhow::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        ensure!(
            !metadata.file_type().is_symlink(),
            "restore destination must not be a symbolic link"
        );
    }
    Ok(())
}

fn remove_sqlite_sidecars(path: &Path) -> anyhow::Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("remove {}", sidecar.display()));
            }
        }
    }
    Ok(())
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
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        ensure!(
            parent.is_dir(),
            "restore destination directory does not exist"
        );
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .context("restore destination must have a UTF-8 file name")?;
        for _ in 0..16 {
            let temporary = parent.join(format!(".{name}.restore-{}.tmp", Uuid::new_v4()));
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
            let _ = remove_sqlite_sidecars(&self.path);
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

    #[test]
    fn backup_is_verified_non_overwriting_and_restores_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("app.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        create_product_database(&database);
        let database_url = format!("sqlite://{}", database.display());

        create(&database_url, &backup).unwrap();
        verify(&backup).unwrap();
        assert!(create(&database_url, &backup).is_err());

        let connection = Connection::open(&database).unwrap();
        connection
            .execute("INSERT INTO parents(id) VALUES(2)", [])
            .unwrap();
        drop(connection);
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
