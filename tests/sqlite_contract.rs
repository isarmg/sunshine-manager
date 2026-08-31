use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use sha2::Digest;
use sunshine_manager::{database_schema, db};

fn database_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

fn try_directory_snapshot(directory: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = if entry.file_type()?.is_file() {
                fs::read(entry.path())?
            } else {
                Vec::new()
            };
            Ok((name, bytes))
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn stable_directory_snapshot(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut previous = None;
    loop {
        match try_directory_snapshot(directory) {
            Ok(current) if previous.as_ref() == Some(&current) => return current,
            Ok(current) => previous = Some(current),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => previous = None,
            Err(error) => panic!("snapshot {}: {error}", directory.display()),
        }
        assert!(
            Instant::now() < deadline,
            "SQLite directory did not reach a stable state before snapshot"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_snapshot_unchanged(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) {
    let summarize = |snapshot: &[(String, Vec<u8>)]| {
        snapshot
            .iter()
            .map(|(name, bytes)| {
                let digest = sha2::Sha256::digest(bytes);
                format!("{name}:{}:{digest:x}", bytes.len())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(summarize(after), summarize(before));
}

fn mutate(path: &Path, sql: &str) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch(sql).unwrap();
}

#[tokio::test]
async fn exact_current_schema_is_durable_and_self_identifying() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sunshine.sqlite3");
    let url = database_url(&path);

    let pool = db::open_or_initialize(&url).await.unwrap();
    assert!(db::ready(&pool).await);
    assert_eq!(
        database_schema::actual_schema_sha256(&pool).await.unwrap(),
        database_schema::SCHEMA_SHA256
    );
    let metadata: (i64, String, String, i64, String) = sqlx::query_as(
        "SELECT singleton,application,application_version,schema_revision,schema_sha256 \
         FROM product_metadata",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        metadata,
        (
            1,
            database_schema::APPLICATION.to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            database_schema::SCHEMA_REVISION,
            database_schema::SCHEMA_SHA256.to_string(),
        )
    );

    sqlx::query(
        "INSERT INTO hosts(\
           host_id,name,address,web_port,username,secret,position,\
           created_at_micros,updated_at_micros\
         ) VALUES('persistent','Persistent','192.0.2.1',47990,'sunshine',NULL,0,1,1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
    pool.close().await;

    let reopened = db::open_existing(&url).await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM hosts WHERE host_id='persistent'")
        .fetch_one(&reopened)
        .await
        .unwrap();
    assert_eq!(name, "Persistent");
    assert!(db::ready(&reopened).await);
    reopened.close().await;
}

#[tokio::test]
async fn existing_empty_legacy_wrong_version_and_drift_are_read_only_rejections() {
    let directory = tempfile::tempdir().unwrap();

    let empty = directory.path().join("empty.sqlite3");
    fs::File::create(&empty).unwrap();
    let before = stable_directory_snapshot(directory.path());
    assert!(db::open_or_initialize(&database_url(&empty)).await.is_err());
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));

    let legacy = directory.path().join("legacy.sqlite3");
    mutate(&legacy, "CREATE TABLE legacy_data(id INTEGER PRIMARY KEY)");
    let before = stable_directory_snapshot(directory.path());
    assert!(
        db::open_or_initialize(&database_url(&legacy))
            .await
            .is_err()
    );
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));

    let wrong_version = directory.path().join("wrong-version.sqlite3");
    let pool = db::open_or_initialize(&database_url(&wrong_version))
        .await
        .unwrap();
    pool.close().await;
    mutate(
        &wrong_version,
        "UPDATE product_metadata SET application_version='0.6.0'",
    );
    let before = stable_directory_snapshot(directory.path());
    assert!(
        db::open_or_initialize(&database_url(&wrong_version))
            .await
            .is_err()
    );
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));

    let drifted = directory.path().join("drifted.sqlite3");
    let pool = db::open_or_initialize(&database_url(&drifted))
        .await
        .unwrap();
    pool.close().await;
    mutate(&drifted, "CREATE TABLE unexpected(id INTEGER)");
    let before = stable_directory_snapshot(directory.path());
    assert!(
        db::open_or_initialize(&database_url(&drifted))
            .await
            .is_err()
    );
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));
}

#[cfg(unix)]
#[tokio::test]
async fn database_symlinks_and_hardlinks_are_rejected_without_mutation() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.sqlite3");
    let pool = db::open_or_initialize(&database_url(&original))
        .await
        .unwrap();
    pool.close().await;

    let symbolic = directory.path().join("symbolic.sqlite3");
    symlink(&original, &symbolic).unwrap();
    let before = stable_directory_snapshot(directory.path());
    assert!(
        db::open_or_initialize(&database_url(&symbolic))
            .await
            .is_err()
    );
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));

    fs::remove_file(&symbolic).unwrap();
    let hard = directory.path().join("hard.sqlite3");
    fs::hard_link(&original, &hard).unwrap();
    let before = stable_directory_snapshot(directory.path());
    assert!(db::open_or_initialize(&database_url(&hard)).await.is_err());
    assert_snapshot_unchanged(&before, &stable_directory_snapshot(directory.path()));
}
