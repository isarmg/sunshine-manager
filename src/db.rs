use std::{
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{
    Sqlite, SqlitePool, Transaction,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::{
    auth::{InternalAuth, IssuedSession},
    crypto::SecretBox,
    error::{AppError, AppResult},
    model::{Host, HostPatchRequest, HostSaveRequest, normalize_host, validate_host_request},
};

pub const SCHEMA: &str = "sunshine";
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub(crate) struct StoredHost {
    pub host_id: String,
    pub name: String,
    pub address: String,
    pub web_port: i32,
    pub username: String,
    pub secret: Option<String>,
    pub verify_tls: bool,
    pub position: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
}

pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .synchronous(SqliteSynchronous::Full);
    Ok(SqlitePoolOptions::new()
        .max_connections(12)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?)
}

/// Run this product's embedded SQLite migrations.
///
/// Deployment must create the database parent directory for the Sunshine
/// Manager service account; the application creates the database file itself.
pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

pub async fn ready(pool: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>(
        r#"SELECT
             (SELECT COUNT(*) FROM sqlite_master
              WHERE type='table' AND name IN (
                  'hosts','audit_logs','auth_users','auth_sessions','operations','audit_outbox'
              )) = 6
             AND
             (SELECT COUNT(*) FROM pragma_table_info('operations')
              WHERE name IN (
                  'operation_id','actor','host_id','action','idempotency_key_hash',
                  'request_fingerprint','request_ciphertext','state','attempt',
                  'created_at_micros','updated_at_micros','started_at_micros',
                  'completed_at_micros','error_code'
              )) = 14
             AND
             (SELECT COUNT(*) FROM pragma_table_info('audit_outbox')
              WHERE name IN (
                  'outbox_id','operation_id','event_kind','action','target','actor',
                  'detail','created_at_micros','delivered_at_micros','delivery_attempt'
              )) = 10
             AND
             (SELECT COUNT(*) FROM pragma_table_info('audit_logs')
              WHERE name='outbox_id') = 1
             AND
             (SELECT COUNT(*) FROM sqlite_master
              WHERE type='index' AND name IN (
                  'operations_idempotency_idx','audit_logs_outbox_id_idx',
                  'audit_outbox_operation_event_idx'
              )) = 3"#,
    )
    .fetch_one(pool)
    .await
    .map(|ready| ready != 0)
    .unwrap_or(false)
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct StoredUser {
    pub user_id: String,
    pub email: String,
    pub password_hash: String,
    pub active: bool,
    pub session_version: i64,
}

pub async fn find_active_user_by_email(
    pool: &SqlitePool,
    email: &str,
) -> AppResult<Option<StoredUser>> {
    let normalized = email.trim().to_lowercase();
    let row = sqlx::query_as::<_, StoredUser>(
        "SELECT user_id,email,password_hash,active,session_version FROM auth_users \
         WHERE email=? AND active=true",
    )
    .bind(normalized)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn ensure_admin_user(
    pool: &SqlitePool,
    email: &str,
    password: Option<&str>,
) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }
    let password = password.ok_or_else(|| {
        AppError::BadRequest(
            "SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD is required while no users exist".into(),
        )
    })?;
    let normalized = email.trim().to_lowercase();
    let password_hash = crate::auth::hash_password(password)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    sqlx::query(
        "INSERT INTO auth_users(user_id,email,password_hash,active,created_at_micros) \
         VALUES(?,?,?,true,?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(normalized)
    .bind(password_hash)
    .bind(now_micros()?)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_admin_password(pool: &SqlitePool, email: &str, password: &str) -> AppResult<()> {
    let normalized = email.trim().to_lowercase();
    let password_hash = crate::auth::hash_password(password)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    let now = now_micros()?;
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE auth_users \
         SET password_hash=?, session_version=session_version+1 \
         WHERE email=?",
    )
    .bind(password_hash)
    .bind(normalized)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound(format!(
            "no active or existing user matched {email}"
        )));
    }
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at_micros=? \
         WHERE user_id=(SELECT user_id FROM auth_users WHERE email=?) \
           AND revoked_at_micros IS NULL",
    )
    .bind(now)
    .bind(email.trim().to_lowercase())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredSession {
    pub session_id: String,
    pub user_id: String,
    pub email: String,
    pub csrf_hash: Vec<u8>,
    pub absolute_expires_at_micros: i64,
}

pub async fn create_session(
    pool: &SqlitePool,
    user: &StoredUser,
    session: &IssuedSession,
    now_micros: i64,
) -> AppResult<()> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "DELETE FROM auth_sessions \
         WHERE revoked_at_micros IS NOT NULL \
            OR idle_expires_at_micros<=? \
            OR absolute_expires_at_micros<=?",
    )
    .bind(now_micros)
    .bind(now_micros)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM auth_sessions \
         WHERE user_id=? AND session_id IN ( \
             SELECT session_id FROM auth_sessions WHERE user_id=? \
             ORDER BY created_at_micros DESC,session_id DESC LIMIT -1 OFFSET 31 \
         )",
    )
    .bind(&user.user_id)
    .bind(&user.user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO auth_sessions(
               session_id,user_id,token_hash,csrf_hash,user_session_version,
               created_at_micros,last_seen_at_micros,idle_expires_at_micros,
               absolute_expires_at_micros
           ) VALUES(?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&session.session_id)
    .bind(&user.user_id)
    .bind(&session.token_hash)
    .bind(&session.csrf_hash)
    .bind(user.session_version)
    .bind(now_micros)
    .bind(now_micros)
    .bind(session.idle_expires_at_micros)
    .bind(session.absolute_expires_at_micros)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn authenticate_session(
    pool: &SqlitePool,
    auth: &InternalAuth,
    token_hash: &[u8],
    now_micros: i64,
) -> AppResult<Option<StoredSession>> {
    let session = sqlx::query_as::<_, StoredSession>(
        r#"SELECT s.session_id,s.user_id,u.email,s.csrf_hash,s.absolute_expires_at_micros
           FROM auth_sessions s
           JOIN auth_users u ON u.user_id=s.user_id
           WHERE s.token_hash=?
             AND s.revoked_at_micros IS NULL
             AND s.idle_expires_at_micros>?
             AND s.absolute_expires_at_micros>?
             AND u.active=true
             AND u.session_version=s.user_session_version"#,
    )
    .bind(token_hash)
    .bind(now_micros)
    .bind(now_micros)
    .fetch_optional(pool)
    .await?;
    let Some(session) = session else {
        return Ok(None);
    };
    let idle_expires_at_micros =
        auth.refreshed_idle_expiry(now_micros, session.absolute_expires_at_micros)?;
    let updated = sqlx::query(
        r#"UPDATE auth_sessions
           SET last_seen_at_micros=?,idle_expires_at_micros=?
           WHERE session_id=?
             AND revoked_at_micros IS NULL
             AND idle_expires_at_micros>?
             AND absolute_expires_at_micros>?"#,
    )
    .bind(now_micros)
    .bind(idle_expires_at_micros)
    .bind(&session.session_id)
    .bind(now_micros)
    .bind(now_micros)
    .execute(pool)
    .await?;
    Ok((updated.rows_affected() == 1).then_some(session))
}

pub async fn revoke_session(pool: &SqlitePool, session_id: &str, now_micros: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at_micros=? \
         WHERE session_id=? AND revoked_at_micros IS NULL",
    )
    .bind(now_micros)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_hosts(pool: &SqlitePool, secrets: &SecretBox) -> AppResult<Vec<Host>> {
    let rows = sqlx::query_as::<_, StoredHost>(
        r#"SELECT host_id,name,address,web_port,username,secret,verify_tls,position,
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
    production: bool,
    actor: &str,
) -> AppResult<Host> {
    validate_host_request(&request, production)?;
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
        verify_tls: request.verify_tls,
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
            "name={} host={} port={} verify_tls={}",
            host.name, host.host, host.web_port, host.verify_tls
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
    production: bool,
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
    if let Some(value) = patch.verify_tls {
        host.verify_tls = value;
    }
    validate_host_request(
        &HostSaveRequest {
            name: host.name.clone(),
            host: host.host.clone(),
            web_port: host.web_port,
            username: host.username.clone(),
            password: update_password.then(|| host.password.clone()),
            verify_tls: host.verify_tls,
        },
        production,
    )?;
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
            "name={} host={} port={} verify_tls={}",
            host.name, host.host, host.web_port, host.verify_tls
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
        r#"SELECT host_id,name,address,web_port,username,secret,verify_tls,position,
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
        r#"SELECT host_id,name,address,web_port,username,secret,verify_tls,position,
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
               host_id,name,address,web_port,username,secret,verify_tls,position,
               created_at_micros,updated_at_micros)
           VALUES(?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&row.host_id)
    .bind(&row.name)
    .bind(&row.address)
    .bind(row.web_port)
    .bind(&row.username)
    .bind(&row.secret)
    .bind(row.verify_tls)
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
             secret=?,verify_tls=?,position=?,updated_at_micros=?
           WHERE host_id=?"#,
    )
    .bind(&row.name)
    .bind(&row.address)
    .bind(row.web_port)
    .bind(&row.username)
    .bind(&row.secret)
    .bind(row.verify_tls)
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
            .then(|| secrets.encrypt(&host.password))
            .transpose()?,
        verify_tls: host.verify_tls,
        position: host.position,
        created_at_micros: host.created_at_micros,
        updated_at_micros: host.updated_at_micros,
    })
}

pub(crate) fn decode_host(row: StoredHost, secrets: &SecretBox) -> AppResult<Host> {
    Ok(Host {
        id: row.host_id,
        name: row.name,
        host: row.address,
        web_port: u16::try_from(row.web_port)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid stored web_port")))?,
        username: row.username,
        password: row
            .secret
            .map(|value| secrets.decrypt(&value))
            .transpose()?
            .unwrap_or_default(),
        verify_tls: row.verify_tls,
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

    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn host_crud_matches_the_sqlite_schema() {
        let pool = migrated_pool().await;
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
                verify_tls: false,
            },
            false,
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
                username: Some("operator".into()),
                password: Some("rotated-secret".into()),
                verify_tls: Some(false),
            },
            false,
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
        let pool = migrated_pool().await;
        assert!(ready(&pool).await);
        sqlx::query("DROP TABLE audit_outbox")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!ready(&pool).await);
    }

    #[tokio::test]
    async fn production_pool_is_durable_and_enforces_sqlite_safety_pragmas() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("sunshine.sqlite3");
        let database_url = format!("sqlite://{}", database_path.display());
        let secrets = SecretBox::new("test", [9; 32]).unwrap();

        let pool = connect(&database_url).await.unwrap();
        migrate(&pool).await.unwrap();

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
                verify_tls: false,
            },
            false,
            "test-user",
        )
        .await
        .unwrap();
        pool.close().await;

        let reopened = connect(&database_url).await.unwrap();
        migrate(&reopened).await.unwrap();
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

    #[tokio::test]
    async fn password_reset_invalidates_existing_database_sessions() {
        let pool = migrated_pool().await;
        ensure_admin_user(
            &pool,
            "admin@example.com",
            Some("correct horse battery staple"),
        )
        .await
        .unwrap();
        let user = find_active_user_by_email(&pool, "admin@example.com")
            .await
            .unwrap()
            .unwrap();
        let auth =
            InternalAuth::new(Duration::from_secs(3_600), Duration::from_secs(600), false).unwrap();
        let now = now_micros().unwrap();
        let issued = auth.issue(now).unwrap();
        create_session(&pool, &user, &issued, now).await.unwrap();
        assert!(
            authenticate_session(&pool, &auth, &issued.token_hash, now)
                .await
                .unwrap()
                .is_some()
        );

        reset_admin_password(
            &pool,
            "admin@example.com",
            "another correct horse battery staple",
        )
        .await
        .unwrap();
        assert!(
            authenticate_session(&pool, &auth, &issued.token_hash, now)
                .await
                .unwrap()
                .is_none()
        );
        let revoked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_sessions WHERE revoked_at_micros IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(revoked, 1);
    }
}
