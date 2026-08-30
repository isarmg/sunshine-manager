use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub const APPLICATION: &str = "sunshine-manager";
pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_REVISION: i64 = 1;
pub const SCHEMA_SHA256: &str = "1e55653f9b9b4805873164e52b79d399aec4fe327a8648218d4cbcb16b561b98";

const CURRENT_SCHEMA_SQL: &str = include_str!("../schema.sql");
const PRODUCT_METADATA_DDL: &str = "CREATE TABLE product_metadata (\n\
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),\n\
    application TEXT NOT NULL,\n\
    application_version TEXT NOT NULL,\n\
    schema_revision INTEGER NOT NULL,\n\
    schema_sha256 TEXT NOT NULL\n\
)";

#[derive(Debug, PartialEq, Eq)]
struct ProductMetadata {
    singleton: i64,
    application: String,
    application_version: String,
    schema_revision: i64,
    schema_sha256: String,
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
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) =
        sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(pool)
            .await
            .context("checkpoint initialized Sunshine Manager schema")?;
    ensure!(
        busy == 0 && checkpointed_frames == log_frames,
        "initialized schema WAL checkpoint was incomplete"
    );
    sync_file_and_parent(path)
}

async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .synchronous(SqliteSynchronous::Full);
    Ok(SqlitePoolOptions::new()
        .max_connections(12)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?)
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
    sqlx::raw_sql(PRODUCT_METADATA_DDL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(CURRENT_SCHEMA_SQL)
        .execute(&mut *transaction)
        .await?;
    let actual = fingerprint_transaction(&mut transaction).await?;
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
    let metadata_sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_schema WHERE type='table' AND name='product_metadata'",
    )
    .fetch_optional(pool)
    .await?;
    ensure!(
        metadata_sql.as_deref() == Some(PRODUCT_METADATA_DDL),
        "database product_metadata schema is not the exact current contract"
    );
    let rows = sqlx::query(
        "SELECT singleton,application,application_version,schema_revision,schema_sha256 \
         FROM product_metadata ORDER BY singleton",
    )
    .fetch_all(pool)
    .await?;
    let metadata = rows
        .iter()
        .map(|row| ProductMetadata {
            singleton: row.get(0),
            application: row.get(1),
            application_version: row.get(2),
            schema_revision: row.get(3),
            schema_sha256: row.get(4),
        })
        .collect::<Vec<_>>();
    validate_metadata(&metadata)?;
    let actual = fingerprint_pool(pool).await?;
    validate_fingerprint(&metadata[0], &actual)
}

pub async fn is_current(pool: &SqlitePool) -> bool {
    validate_pool(pool).await.is_ok()
}

pub async fn actual_schema_sha256(pool: &SqlitePool) -> anyhow::Result<String> {
    fingerprint_pool(pool).await
}

fn validate_read_only(path: &Path) -> anyhow::Result<()> {
    validate_existing_file(path)?;
    let path = path
        .to_str()
        .context("SQLite database path must be valid UTF-8")?;
    // A regular read-only SQLite connection may create or rewrite WAL shared
    // memory. Immutable preflight guarantees rejection leaves old state byte
    // for byte unchanged. The live pool validates the same contract again.
    let read_only_uri = format!("file:{path}?mode=ro&immutable=1");
    let connection = Connection::open_with_flags(
        read_only_uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .context("open Sunshine Manager database immutable read-only")?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")?;
    validate_connection_contract(&connection)
}

fn validate_connection_contract(connection: &Connection) -> anyhow::Result<()> {
    let metadata_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name='product_metadata'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        metadata_sql.as_deref() == Some(PRODUCT_METADATA_DDL),
        "database product_metadata schema is not the exact current contract"
    );
    let metadata = {
        let mut statement = connection.prepare(
            "SELECT singleton,application,application_version,schema_revision,schema_sha256 \
             FROM product_metadata ORDER BY singleton",
        )?;
        statement
            .query_map([], |row| {
                Ok(ProductMetadata {
                    singleton: row.get(0)?,
                    application: row.get(1)?,
                    application_version: row.get(2)?,
                    schema_revision: row.get(3)?,
                    schema_sha256: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    validate_metadata(&metadata)?;
    let actual = fingerprint_connection(connection)?;
    validate_fingerprint(&metadata[0], &actual)
}

fn validate_metadata(metadata: &[ProductMetadata]) -> anyhow::Result<()> {
    ensure!(
        metadata.len() == 1,
        "product_metadata must contain exactly one row"
    );
    let metadata = &metadata[0];
    ensure!(
        metadata.singleton == 1
            && metadata.application == APPLICATION
            && metadata.application_version == APPLICATION_VERSION
            && metadata.schema_revision == SCHEMA_REVISION
            && metadata.schema_sha256 == SCHEMA_SHA256,
        "database metadata is not the exact current Sunshine Manager version; use isarmg-upgrade"
    );
    Ok(())
}

fn validate_fingerprint(metadata: &ProductMetadata, actual: &str) -> anyhow::Result<()> {
    ensure!(
        metadata.schema_sha256 == SCHEMA_SHA256 && actual == SCHEMA_SHA256,
        "database schema is not the exact current Sunshine Manager schema; use isarmg-upgrade"
    );
    Ok(())
}

async fn fingerprint_pool(pool: &SqlitePool) -> anyhow::Result<String> {
    Ok(fingerprint(schema_rows(pool).await?))
}

async fn fingerprint_transaction(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
) -> anyhow::Result<String> {
    let rows = sqlx::query(
        "SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_schema \
         WHERE name NOT GLOB 'sqlite_*' AND name <> 'product_metadata' \
         ORDER BY type,name,tbl_name",
    )
    .fetch_all(&mut **transaction)
    .await?
    .into_iter()
    .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
    .collect();
    Ok(fingerprint(rows))
}

async fn schema_rows(pool: &SqlitePool) -> anyhow::Result<Vec<(String, String, String, String)>> {
    Ok(sqlx::query(
        "SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_schema \
         WHERE name NOT GLOB 'sqlite_*' AND name <> 'product_metadata' \
         ORDER BY type,name,tbl_name",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
    .collect())
}

fn fingerprint_connection(connection: &Connection) -> anyhow::Result<String> {
    let mut statement = connection.prepare(
        "SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_schema \
         WHERE name NOT GLOB 'sqlite_*' AND name <> 'product_metadata' \
         ORDER BY type,name,tbl_name",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(fingerprint(rows))
}

fn fingerprint(rows: Vec<(String, String, String, String)>) -> String {
    let mut digest = Sha256::new();
    for row in rows {
        for field in [row.0, row.1, row.2, row.3] {
            let bytes = field.as_bytes();
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        }
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
