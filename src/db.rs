use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    crypto::{SecretBox, constant_time_equal_32},
    error::{AppError, AppResult},
    model::{Host, HostPatchRequest, HostSaveRequest, normalize_host, validate_host_request},
};

pub const SCHEMA: &str = "sunshine";
pub use crate::database_schema::{initialize_empty, open_existing, open_or_initialize};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub(crate) struct StoredHost {
    pub host_id: String,
    pub name: String,
    pub address: String,
    pub web_port: i32,
    pub username: String,
    pub secret: Option<String>,
    pub position: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
}

pub async fn ready(pool: &SqlitePool) -> bool {
    crate::database_schema::is_current(pool).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DoctorReport {
    pub schema_ready: bool,
    pub integrity_ready: bool,
    pub foreign_keys_ready: bool,
    pub writable: bool,
    pub encrypted_values_ready: bool,
}

impl DoctorReport {
    pub const fn healthy(self) -> bool {
        self.schema_ready
            && self.integrity_ready
            && self.foreign_keys_ready
            && self.writable
            && self.encrypted_values_ready
    }
}

/// Exercise the local durability and encryption boundary without retaining a
/// probe row or contacting any configured Sunshine host.
pub async fn doctor(pool: &SqlitePool, secrets: &SecretBox) -> DoctorReport {
    let schema_ready = ready(pool).await;
    let integrity_ready = sarmg_sqlite::integrity_check(pool).await.is_ok();
    let foreign_keys_ready = sarmg_sqlite::foreign_key_check(pool).await.is_ok();
    let writable = schema_ready && doctor_write_probe(pool).await.is_ok();
    let encrypted_values_ready = schema_ready
        && validate_encrypted_values(pool, secrets)
            .await
            .unwrap_or(false);

    DoctorReport {
        schema_ready,
        integrity_ready,
        foreign_keys_ready,
        writable,
        encrypted_values_ready,
    }
}

async fn doctor_write_probe(pool: &SqlitePool) -> anyhow::Result<()> {
    let target = format!("doctor:rollback:{}", uuid::Uuid::new_v4());
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO audit_logs(action,target,detail,actor,created_at_micros) \
         VALUES('doctor.write_probe',?,NULL,'doctor',?)",
    )
    .bind(&target)
    .bind(now_micros()?)
    .execute(&mut *transaction)
    .await?;
    transaction.rollback().await?;

    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE target = ?")
        .bind(target)
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(retained == 0, "doctor write probe was not rolled back");
    Ok(())
}

async fn validate_encrypted_values(pool: &SqlitePool, secrets: &SecretBox) -> anyhow::Result<bool> {
    let mut host_cursor = String::new();
    loop {
        let rows = sqlx::query_as::<_, StoredHost>(
            r#"SELECT host_id,name,address,web_port,username,secret,position,
                      created_at_micros,updated_at_micros
               FROM hosts WHERE host_id > ? ORDER BY host_id LIMIT 128"#,
        )
        .bind(&host_cursor)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            let Ok(host) = decode_host(row.clone(), secrets) else {
                return Ok(false);
            };
            let request = HostSaveRequest {
                name: host.name.clone(),
                host: host.host.clone(),
                web_port: host.web_port,
                username: host.username.clone(),
                password: Some(host.password),
            };
            if host.name != host.name.trim()
                || host.host != normalize_host(&host.host)
                || host.username != host.username.trim()
                || validate_host_request(&request).is_err()
            {
                return Ok(false);
            }
        }
        host_cursor = rows.last().expect("non-empty batch").host_id.clone();
    }

    let mut operation_cursor = String::new();
    loop {
        let rows = sqlx::query_as::<_, (String, String, Vec<u8>, Vec<u8>)>(
            "SELECT operation_id,action,request_fingerprint,request_payload FROM _sarmg_operations \
             WHERE operation_id > ? ORDER BY operation_id LIMIT 128",
        )
        .bind(&operation_cursor)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            break;
        }
        for (operation_id, action, fingerprint, payload) in &rows {
            let Ok(payload) = serde_json::from_slice::<serde_json::Value>(payload) else {
                return Ok(false);
            };
            let Some(ciphertext) = payload
                .get("request_ciphertext")
                .and_then(|value| value.as_str())
            else {
                return Ok(false);
            };
            let Ok(plaintext) = secrets.decrypt_operation_request(operation_id, action, ciphertext)
            else {
                return Ok(false);
            };
            let expected_fingerprint = secrets.operation_request_fingerprint(&plaintext);
            if !constant_time_equal_32(fingerprint, &expected_fingerprint) {
                return Ok(false);
            }
            let Ok(request) =
                serde_json::from_str::<crate::operations::RemoteOperationRequest>(&plaintext)
            else {
                return Ok(false);
            };
            if request.action() != action.as_str() {
                return Ok(false);
            }
        }
        operation_cursor = rows.last().expect("non-empty batch").0.clone();
    }
    Ok(true)
}

/// Require the complete current runtime state before the network listener is
/// opened. Schema identity alone cannot prove that SQLite pages, foreign-key
/// relationships, or every persisted authenticated ciphertext are usable with
/// the one configured credential key.
pub async fn require_current_runtime_state(
    pool: &SqlitePool,
    secrets: &SecretBox,
) -> anyhow::Result<()> {
    sarmg_sqlite::integrity_check(pool).await?;
    sarmg_sqlite::foreign_key_check(pool).await?;
    anyhow::ensure!(
        validate_encrypted_values(pool, secrets).await?,
        "persisted ciphertext or its record context is not valid under the current credential key"
    );
    Ok(())
}

pub async fn list_hosts(pool: &SqlitePool, secrets: &SecretBox) -> AppResult<Vec<Host>> {
    let rows = sqlx::query_as::<_, StoredHost>(
        r#"SELECT host_id,name,address,web_port,username,secret,position,
                  created_at_micros,updated_at_micros
           FROM hosts
           ORDER BY position,created_at_micros,host_id"#,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| decode_host(row, secrets))
        .collect()
}

pub async fn get_host(pool: &SqlitePool, secrets: &SecretBox, id: &str) -> AppResult<Host> {
    let row = get_stored_host(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Sunshine host '{id}' does not exist")))?;
    decode_host(row, secrets)
}

pub async fn insert_host(
    pool: &SqlitePool,
    secrets: &SecretBox,
    request: HostSaveRequest,
    actor: &str,
) -> AppResult<Host> {
    validate_host_request(&request)?;
    let now = now_micros()?;
    let mut transaction = pool.begin().await?;
    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM hosts")
        .fetch_one(&mut *transaction)
        .await?;
    let host = Host {
        id: uuid::Uuid::new_v4().to_string(),
        name: request.name.trim().to_string(),
        host: normalize_host(&request.host),
        web_port: request.web_port,
        username: request.username.trim().to_string(),
        password: request.password.unwrap_or_default(),
        position,
        created_at_micros: now,
        updated_at_micros: now,
    };
    let stored = encode_host(&host, secrets)?;
    insert_stored(&mut transaction, &stored).await?;
    insert_audit(
        &mut transaction,
        "host.create",
        &host.id,
        actor,
        Some(&format!(
            "name={} host={} port={}",
            host.name, host.host, host.web_port
        )),
    )
    .await?;
    transaction.commit().await?;
    Ok(host)
}

pub async fn update_host(
    pool: &SqlitePool,
    secrets: &SecretBox,
    id: &str,
    patch: HostPatchRequest,
    actor: &str,
) -> AppResult<Host> {
    if patch.is_empty() {
        return Err(AppError::BadRequest(
            "at least one host field must be provided".to_string(),
        ));
    }
    let update_password = patch.password.is_some();
    let mut transaction = pool.begin().await?;
    let row = get_stored_host_for_update(&mut transaction, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Sunshine host '{id}' does not exist")))?;
    let mut host = decode_host(row.clone(), secrets)?;
    if let Some(value) = patch.name {
        host.name = value.trim().to_string();
    }
    if let Some(value) = patch.host {
        host.host = normalize_host(&value);
    }
    if let Some(value) = patch.web_port {
        host.web_port = value;
    }
    if let Some(value) = patch.username {
        host.username = value.trim().to_string();
    }
    if let Some(value) = patch.password {
        host.password = value;
    }
    validate_host_request(&HostSaveRequest {
        name: host.name.clone(),
        host: host.host.clone(),
        web_port: host.web_port,
        username: host.username.clone(),
        password: update_password.then(|| host.password.clone()),
    })?;
    host.updated_at_micros = now_micros()?;
    let mut stored = encode_host(&host, secrets)?;
    if !update_password {
        stored.secret = row.secret;
    }
    update_stored(&mut transaction, &stored).await?;
    insert_audit(
        &mut transaction,
        "host.update",
        &host.id,
        actor,
        Some(&format!(
            "name={} host={} port={}",
            host.name, host.host, host.web_port
        )),
    )
    .await?;
    transaction.commit().await?;
    Ok(host)
}

pub async fn delete_host(pool: &SqlitePool, id: &str, actor: &str) -> AppResult<()> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query("DELETE FROM hosts WHERE host_id=?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Sunshine host '{id}' does not exist"
        )));
    }
    insert_audit(
        &mut transaction,
        "host.delete",
        id,
        actor,
        Some("host removed"),
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub(crate) async fn get_stored_host(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<StoredHost>, sqlx::Error> {
    sqlx::query_as::<_, StoredHost>(
        r#"SELECT host_id,name,address,web_port,username,secret,position,
                  created_at_micros,updated_at_micros
           FROM hosts WHERE host_id=?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn get_stored_host_for_update(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<Option<StoredHost>, sqlx::Error> {
    sqlx::query_as::<_, StoredHost>(
        r#"SELECT host_id,name,address,web_port,username,secret,position,
                  created_at_micros,updated_at_micros
           FROM hosts WHERE host_id=?"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await
}

pub(crate) async fn insert_stored(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &StoredHost,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO hosts(
               host_id,name,address,web_port,username,secret,position,
               created_at_micros,updated_at_micros)
           VALUES(?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&row.host_id)
    .bind(&row.name)
    .bind(&row.address)
    .bind(row.web_port)
    .bind(&row.username)
    .bind(&row.secret)
    .bind(row.position)
    .bind(row.created_at_micros)
    .bind(row.updated_at_micros)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(crate) async fn update_stored(
    transaction: &mut Transaction<'_, Sqlite>,
    row: &StoredHost,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE hosts SET name=?,address=?,web_port=?,username=?,
             secret=?,position=?,updated_at_micros=?
           WHERE host_id=?"#,
    )
    .bind(&row.name)
    .bind(&row.address)
    .bind(row.web_port)
    .bind(&row.username)
    .bind(&row.secret)
    .bind(row.position)
    .bind(row.updated_at_micros)
    .bind(&row.host_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(crate) async fn insert_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    action: &str,
    target: &str,
    actor: &str,
    detail: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs(action,target,detail,actor,created_at_micros) VALUES(?,?,?,?,?)",
    )
    .bind(action)
    .bind(target)
    .bind(detail)
    .bind(actor)
    .bind(now_micros().unwrap_or(0))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(crate) fn encode_host(host: &Host, secrets: &SecretBox) -> AppResult<StoredHost> {
    Ok(StoredHost {
        host_id: host.id.clone(),
        name: host.name.clone(),
        address: host.host.clone(),
        web_port: i32::from(host.web_port),
        username: host.username.clone(),
        secret: (!host.password.is_empty())
            .then(|| secrets.encrypt_host_credential(&host.id, &host.password))
            .transpose()?,
        position: host.position,
        created_at_micros: host.created_at_micros,
        updated_at_micros: host.updated_at_micros,
    })
}

pub(crate) fn decode_host(row: StoredHost, secrets: &SecretBox) -> AppResult<Host> {
    let password = row
        .secret
        .as_deref()
        .map(|value| secrets.decrypt_host_credential(&row.host_id, value))
        .transpose()?
        .unwrap_or_default();
    Ok(Host {
        id: row.host_id,
        name: row.name,
        host: row.address,
        web_port: u16::try_from(row.web_port)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid stored web_port")))?,
        username: row.username,
        password,
        position: row.position,
        created_at_micros: row.created_at_micros,
        updated_at_micros: row.updated_at_micros,
    })
}

pub(crate) fn now_micros() -> anyhow::Result<i64> {
    let micros = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
    i64::try_from(micros).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    use sqlx::sqlite::SqlitePoolOptions;

    async fn current_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        initialize_empty(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn host_crud_matches_the_sqlite_schema() {
        let pool = current_pool().await;
        let secrets = SecretBox::new("test", [7; 32]).unwrap();

        let created = insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "Desktop".into(),
                host: "192.0.2.10".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("initial-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();

        assert_eq!(created.name, "Desktop");
        assert_eq!(created.host, "192.0.2.10");
        assert_eq!(created.password, "initial-secret");
        assert_eq!(
            list_hosts(&pool, &secrets).await.unwrap(),
            vec![created.clone()]
        );

        let updated = update_host(
            &pool,
            &secrets,
            &created.id,
            HostPatchRequest {
                name: Some("Living Room".into()),
                host: Some("192.0.2.20".into()),
                web_port: Some(48_000),
                // This is the upstream Sunshine Basic Auth username, not a
                // Sunshine Manager role or administrator identity.
                username: Some("operator".into()),
                password: Some("rotated-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "Living Room");
        assert_eq!(updated.host, "192.0.2.20");
        assert_eq!(updated.web_port, 48_000);
        assert_eq!(updated.username, "operator");
        assert_eq!(updated.password, "rotated-secret");
        assert_eq!(updated.created_at_micros, created.created_at_micros);
        assert_eq!(
            get_host(&pool, &secrets, &created.id).await.unwrap(),
            updated
        );

        let stored_secret: String = sqlx::query_scalar("SELECT secret FROM hosts WHERE host_id=?")
            .bind(&created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(stored_secret, "rotated-secret");

        let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(audit_count, 2);

        delete_host(&pool, &created.id, "test-user").await.unwrap();
        assert!(matches!(
            get_host(&pool, &secrets, &created.id).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn readiness_requires_the_operation_and_outbox_schema() {
        let pool = current_pool().await;
        assert!(ready(&pool).await);
        sqlx::query("DROP TABLE _sarmg_operation_audit_outbox")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!ready(&pool).await);
    }

    #[tokio::test]
    async fn doctor_checks_writes_and_encryption_without_retaining_probe_rows() {
        let pool = current_pool().await;
        let secrets = SecretBox::new("doctor", [17; 32]).unwrap();
        insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "Doctor Host".into(),
                host: "192.0.2.90".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("doctor-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();
        let audit_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();

        let report = doctor(&pool, &secrets).await;
        assert!(report.healthy(), "{report:?}");
        require_current_runtime_state(&pool, &secrets)
            .await
            .unwrap();
        let audit_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(audit_after, audit_before);

        let wrong_key = SecretBox::new("doctor", [18; 32]).unwrap();
        let report = doctor(&pool, &wrong_key).await;
        assert!(!report.healthy());
        assert!(!report.encrypted_values_ready);
        assert!(report.schema_ready && report.integrity_ready && report.foreign_keys_ready);
        assert!(report.writable);
        assert!(
            require_current_runtime_state(&pool, &wrong_key)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn startup_rejects_host_ciphertext_moved_to_another_record() {
        let pool = current_pool().await;
        let secrets = SecretBox::new("doctor", [19; 32]).unwrap();
        let first = insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "First Host".into(),
                host: "192.0.2.91".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("first-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();
        let second = insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "Second Host".into(),
                host: "192.0.2.92".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("second-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();
        let first_ciphertext: String =
            sqlx::query_scalar("SELECT secret FROM hosts WHERE host_id=?")
                .bind(&first.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("UPDATE hosts SET secret=? WHERE host_id=?")
            .bind(first_ciphertext)
            .bind(&second.id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(matches!(
            get_host(&pool, &secrets, &second.id).await,
            Err(AppError::Crypto)
        ));
        assert!(!doctor(&pool, &secrets).await.encrypted_values_ready);
        assert!(
            require_current_runtime_state(&pool, &secrets)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn production_pool_is_durable_and_enforces_sqlite_safety_pragmas() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("sunshine.sqlite3");
        let database_url = format!("sqlite://{}", database_path.display());
        let secrets = SecretBox::new("test", [9; 32]).unwrap();

        let pool = open_or_initialize(&database_url).await.unwrap();

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&pool)
            .await
            .unwrap();
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode, "wal");
        assert_eq!(busy_timeout, 5_000);
        assert_eq!(synchronous, 2);

        let created = insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "Persistent".into(),
                host: "192.0.2.30".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("persistent-secret".into()),
            },
            "test-user",
        )
        .await
        .unwrap();
        pool.close().await;

        let reopened = open_existing(&database_url).await.unwrap();
        assert_eq!(
            get_host(&reopened, &secrets, &created.id).await.unwrap(),
            created
        );
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&reopened)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&reopened)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
