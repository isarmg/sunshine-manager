use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, ensure};
use rusqlite::{Connection, OpenFlags};
use sarmg_schema_identity::{
    ProductMetadataRow, SQLITE_SCHEMA_ROWS_QUERY, SchemaIdentity, SchemaRow, verify_current_schema,
};
use sarmg_sqlite::PoolOptions;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub const APPLICATION: &str = "sunshine-manager";
pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_REVISION: i64 = 2;
pub const SCHEMA_SHA256: &str = "c9dedb33dd7a5ad613e762eb135a7aa5184ce1df52166459bee7b3485b4b3be3";

const CURRENT_SCHEMA_SQL: &str = include_str!("../schema/generated/current_schema.sql");

pub fn current_schema_identity() -> SchemaIdentity {
    SchemaIdentity::new(
        APPLICATION,
        APPLICATION_VERSION,
        u64::try_from(SCHEMA_REVISION).expect("current schema revision is non-negative"),
        SCHEMA_SHA256,
    )
    .expect("compiled Sunshine Manager schema identity is valid")
}

pub async fn open_or_initialize(database_url: &str) -> anyhow::Result<SqlitePool> {
    let path = database_path(database_url)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    match options.open(&path) {
        Ok(file) => initialize_created(&path, file).await,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            open_existing_path(&path).await
        }
        Err(error) => Err(error).context("create current Sunshine Manager database"),
    }
}

pub async fn open_existing(database_url: &str) -> anyhow::Result<SqlitePool> {
    let path = database_path(database_url)?;
    open_existing_path(&path).await
}

async fn open_existing_path(path: &Path) -> anyhow::Result<SqlitePool> {
    validate_existing_file(path)?;
    let validation_path = path.to_path_buf();
    tokio::task::spawn_blocking(move || validate_read_only(&validation_path))
        .await
        .context("join read-only database schema validation")??;
    let pool = open_pool(path).await?;
    if let Err(error) = validate_pool(&pool).await {
        pool.close().await;
        return Err(error);
    }
    Ok(pool)
}

async fn initialize_created(path: &Path, file: File) -> anyhow::Result<SqlitePool> {
    if let Err(error) = file
        .sync_all()
        .context("synchronize new Sunshine Manager database file")
    {
        drop(file);
        return fail_initialization(path, error);
    }
    if let Err(error) = sync_parent(path) {
        drop(file);
        return fail_initialization(path, error);
    }
    drop(file);
    let pool = match open_pool(path).await {
        Ok(pool) => pool,
        Err(error) => return fail_initialization(path, error),
    };
    if let Err(error) = initialize_empty(&pool).await {
        pool.close().await;
        return fail_initialization(path, error);
    }
    if let Err(error) = checkpoint_and_sync(&pool, path).await {
        pool.close().await;
        return fail_initialization(path, error);
    }
    Ok(pool)
}

fn fail_initialization<T>(path: &Path, error: anyhow::Error) -> anyhow::Result<T> {
    if let Err(cleanup_error) = cleanup_failed_initialization(path) {
        return Err(cleanup_error.context(format!(
            "current schema initialization failed and cleanup was incomplete; original error: {error:#}"
        )));
    }
    Err(error.context("initialize current Sunshine Manager schema"))
}

async fn checkpoint_and_sync(pool: &SqlitePool, path: &Path) -> anyhow::Result<()> {
    sarmg_sqlite::checkpoint(pool)
        .await
        .context("checkpoint initialized Sunshine Manager schema")?;
    sync_file_and_parent(path)
}

async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let options = PoolOptions::new(12)
        .with_min_connections(1)
        .with_acquire_timeout(Duration::from_secs(10));
    Ok(sarmg_sqlite::open_existing(path, options).await?)
}

/// Initialize one completely empty SQLite database with the exact current
/// schema. Existing objects are never altered.
pub async fn initialize_empty(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut transaction = pool.begin().await?;
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'")
            .fetch_one(&mut *transaction)
            .await?;
    ensure!(
        existing == 0,
        "database is not empty; upgrades require the external upgrade tool"
    );
    sqlx::raw_sql(CURRENT_SCHEMA_SQL)
        .execute(&mut *transaction)
        .await?;
    let created_at_micros = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_micros(),
    )?;
    sqlx::query(
        "INSERT INTO _sarmg_platform_metadata(\
           singleton,platform_generation,platform_schema_revision,profile,created_at_micros\
         ) VALUES(1,1,1,'server-control-plane',?)",
    )
    .bind(created_at_micros)
    .execute(&mut *transaction)
    .await?;
    let actual = sarmg_sqlite::schema_fingerprint(&mut *transaction).await?;
    ensure!(
        actual == SCHEMA_SHA256,
        "compiled current schema fingerprint mismatch: expected {SCHEMA_SHA256}, computed {actual}"
    );
    sqlx::query(
        "INSERT INTO product_metadata(\
           singleton,application,application_version,schema_revision,schema_sha256\
         ) VALUES(1,?,?,?,?)",
    )
    .bind(APPLICATION)
    .bind(APPLICATION_VERSION)
    .bind(SCHEMA_REVISION)
    .bind(SCHEMA_SHA256)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    validate_pool(pool).await
}

pub async fn validate_pool(pool: &SqlitePool) -> anyhow::Result<()> {
    sarmg_sqlite::require_pool_current_schema(pool, &current_schema_identity())
        .await
        .context("database is not the exact current Sunshine Manager schema; use sarmg-upgrade")?;
    let platform = sarmg_platform_db::read_platform_metadata(pool).await?;
    ensure!(
        platform.platform_generation == sarmg_platform_db::PLATFORM_GENERATION
            && platform.platform_schema_revision == sarmg_platform_db::PLATFORM_SCHEMA_REVISION
            && platform.profile == "server-control-plane",
        "database platform metadata is not the exact current contract"
    );
    Ok(())
}

pub async fn is_current(pool: &SqlitePool) -> bool {
    validate_pool(pool).await.is_ok()
}

pub async fn actual_schema_sha256(pool: &SqlitePool) -> anyhow::Result<String> {
    Ok(sarmg_sqlite::schema_fingerprint(pool).await?)
}

fn validate_read_only(path: &Path) -> anyhow::Result<()> {
    validate_existing_file(path)?;
    let snapshot = snapshot_generation(path)?;
    let connection = Connection::open_with_flags(
        &snapshot.database,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .context("open private Sunshine Manager schema-validation snapshot")?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")?;
    validate_connection_contract(&connection)
}

struct ValidationSnapshot {
    _directory: tempfile::TempDir,
    database: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceSnapshot {
    hash: [u8; 32],
    length: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn snapshot_generation(path: &Path) -> anyhow::Result<ValidationSnapshot> {
    let mut last_change = None;
    for _ in 0..4 {
        match snapshot_generation_once(path) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if error.is::<GenerationChanged>() => {
                last_change = Some(error);
                std::thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_change.expect("snapshot retry loop records every generation change"))
}

#[derive(Debug, thiserror::Error)]
#[error("SQLite generation changed during current-schema validation")]
struct GenerationChanged;

fn snapshot_generation_once(path: &Path) -> anyhow::Result<ValidationSnapshot> {
    let directory = tempfile::Builder::new()
        .prefix("sunshine-schema-check-")
        .tempdir()
        .context("create private current-schema validation directory")?;
    let database = directory.path().join("database.sqlite3");
    let sources = [
        path.to_path_buf(),
        sqlite_sidecar(path, "-wal"),
        sqlite_sidecar(path, "-journal"),
    ];
    let destinations = [
        database.clone(),
        sqlite_sidecar(&database, "-wal"),
        sqlite_sidecar(&database, "-journal"),
    ];

    // Read-only WAL connections still update shared-memory lock bytes. Copy a
    // stable generation and validate the private copy so rejected databases
    // leave the source main/WAL/journal/SHM files byte-for-byte untouched.
    let mut expected = Vec::with_capacity(sources.len());
    for (source, destination) in sources.iter().zip(&destinations) {
        expected.push(copy_generation_file(source, destination)?);
    }
    let _ = source_snapshot(&sqlite_sidecar(path, "-shm"))?;
    for (source, expected) in sources.iter().zip(expected) {
        if source_snapshot(source)? != expected {
            return Err(GenerationChanged.into());
        }
    }

    Ok(ValidationSnapshot {
        _directory: directory,
        database,
    })
}

fn copy_generation_file(
    source_path: &Path,
    destination_path: &Path,
) -> anyhow::Result<Option<SourceSnapshot>> {
    let Some((mut source, before)) = open_source_snapshot(source_path)? else {
        return Ok(None);
    };
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut destination = options.open(destination_path)?;
    std::io::copy(&mut source, &mut destination)?;
    destination.flush()?;
    destination.sync_all()?;
    destination.seek(SeekFrom::Start(0))?;
    let copied_hash = hash_reader(&mut destination)?;
    let Some(after) = source_snapshot(source_path)? else {
        return Err(GenerationChanged.into());
    };
    if before != after || copied_hash != after.hash {
        return Err(GenerationChanged.into());
    }
    Ok(Some(after))
}

fn source_snapshot(path: &Path) -> anyhow::Result<Option<SourceSnapshot>> {
    let Some((_, snapshot)) = open_source_snapshot(path)? else {
        return Ok(None);
    };
    Ok(Some(snapshot))
}

fn open_source_snapshot(path: &Path) -> anyhow::Result<Option<(File, SourceSnapshot)>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    ensure!(
        opened.is_file() && named.is_file() && !named.file_type().is_symlink(),
        "SQLite generation must contain only regular files without symbolic links"
    );
    #[cfg(unix)]
    ensure!(
        opened.nlink() == 1
            && named.nlink() == 1
            && opened.dev() == named.dev()
            && opened.ino() == named.ino(),
        "SQLite generation files must not have hard-link aliases or change while opened"
    );
    let hash = hash_reader(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(Some((
        file,
        SourceSnapshot {
            hash,
            length: opened.len(),
            #[cfg(unix)]
            device: opened.dev(),
            #[cfg(unix)]
            inode: opened.ino(),
        },
    )))
}

fn hash_reader(reader: &mut impl Read) -> anyhow::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn validate_connection_contract(connection: &Connection) -> anyhow::Result<()> {
    let metadata = {
        let mut statement = connection.prepare(
            "SELECT singleton,application,application_version,schema_revision,schema_sha256 \
             FROM product_metadata ORDER BY singleton",
        )?;
        statement
            .query_map([], |row| {
                Ok(ProductMetadataRow {
                    singleton: row.get(0)?,
                    application: row.get(1)?,
                    application_version: row.get(2)?,
                    schema_revision: row.get(3)?,
                    schema_sha256: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut statement = connection.prepare(SQLITE_SCHEMA_ROWS_QUERY)?;
    let schema_rows = statement
        .query_map([], |row| {
            Ok(SchemaRow::new(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    verify_current_schema(&metadata, &schema_rows, &current_schema_identity())
        .context("database is not the exact current Sunshine Manager schema; use sarmg-upgrade")?;
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
        "in-memory database files are unsupported"
    );
    ensure!(
        !value.contains('?')
            && !value.contains('#')
            && !value.contains('%')
            && !value.contains('\0'),
        "database requires a plain, unescaped SQLite file URL without query or fragment"
    );
    let path = PathBuf::from(value);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .context("resolve database working directory")?
            .join(path)
    };
    let mut normalized = PathBuf::from("/");
    for component in absolute.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir => {
                anyhow::bail!("database path must not contain parent traversal")
            }
            Component::Prefix(_) => anyhow::bail!("database path has an unsupported prefix"),
        }
    }
    ensure!(
        normalized.file_name().is_some(),
        "database path must name a file"
    );
    Ok(normalized)
}

fn validate_existing_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("database does not exist: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "database must be a regular file"
    );
    #[cfg(unix)]
    ensure!(
        metadata.nlink() == 1,
        "database must have exactly one hard link"
    );
    Ok(())
}

fn cleanup_failed_initialization(path: &Path) -> anyhow::Result<()> {
    for suffix in ["-wal", "-shm", "-journal", ""] {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        match fs::remove_file(PathBuf::from(value)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("remove failed schema initialization file"),
        }
    }
    sync_parent(path)
}

fn sync_file_and_parent(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all()?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> anyhow::Result<()> {
    File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()?;
    Ok(())
}
