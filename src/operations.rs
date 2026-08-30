use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard},
    task::{JoinError, JoinSet},
};
use uuid::Uuid;

use crate::{
    client::UpstreamClient,
    cover_policy::CoverUrlPolicy,
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    model::Host,
};

pub use crate::http::{probe_loop, router};

const MAX_ACTIVE_HOSTS: usize = 16;
const DISPATCH_BATCH: i64 = 128;
const OUTBOX_BATCH: i64 = 128;
const IDLE_POLL: Duration = Duration::from_millis(250);

/// Serializes remote mutations for one Sunshine host without blocking
/// unrelated hosts. The registry stores weak references so removed hosts and
/// one-off identifiers do not grow it indefinitely.
#[derive(Clone, Default)]
pub struct HostMutationLocks {
    locks: Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>,
}

impl HostMutationLocks {
    pub async fn lock(&self, host_id: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = recover_lock(&self.locks);
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(host_id).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(AsyncMutex::new(()));
                    locks.insert(host_id.to_string(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        recover_lock(&self.locks).len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl OperationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }

    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            _ => Err(AppError::Internal(anyhow::anyhow!(
                "invalid stored operation state"
            ))),
        }
    }
}

/// Deliberately excludes actor, action, request material and upstream error
/// text. This is the only operation representation returned by HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationView {
    pub operation_id: String,
    pub state: OperationState,
    pub attempt: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
    pub started_at_micros: Option<i64>,
    pub completed_at_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteOperationRequest {
    AppsSave { body: Value },
    AppsClose,
    AppsDelete { index: u32 },
    ClientsUnpair { uuid: String },
    ClientsUnpairAll,
    ClientsUpdate { uuid: String, enabled: bool },
    ConfigSave { body: Value },
    Pin { pin: String, name: String },
    Restart,
    ResetDisplay,
    CoverUpload { key: String, url: String },
}

impl RemoteOperationRequest {
    pub fn action(&self) -> &'static str {
        match self {
            Self::AppsSave { .. } => "sunshine.app.save",
            Self::AppsClose => "sunshine.app.close",
            Self::AppsDelete { .. } => "sunshine.app.delete",
            Self::ClientsUnpair { .. } => "sunshine.client.unpair",
            Self::ClientsUnpairAll => "sunshine.client.unpair_all",
            Self::ClientsUpdate { .. } => "sunshine.client.update",
            Self::ConfigSave { .. } => "sunshine.config.save",
            Self::Pin { .. } => "sunshine.client.pair",
            Self::Restart => "sunshine.system.restart",
            Self::ResetDisplay => "sunshine.display.reset",
            Self::CoverUpload { .. } => "sunshine.cover.upload",
        }
    }
}

#[derive(Clone)]
pub struct OperationManager {
    pool: SqlitePool,
    secrets: SecretBox,
    production: bool,
    locks: HostMutationLocks,
    executor: Arc<dyn RemoteOperationExecutor>,
    notify: Arc<Notify>,
    cover_url_policy: CoverUrlPolicy,
}

impl OperationManager {
    pub fn new(
        pool: SqlitePool,
        secrets: SecretBox,
        production: bool,
        locks: HostMutationLocks,
        upstream: UpstreamClient,
    ) -> Self {
        Self {
            pool,
            secrets,
            production,
            locks,
            executor: Arc::new(SunshineExecutor {
                transport: Arc::new(UpstreamMutationTransport { upstream }),
            }),
            notify: Arc::new(Notify::new()),
            cover_url_policy: CoverUrlPolicy::default(),
        }
    }

    pub fn with_cover_url_policy(mut self, policy: CoverUrlPolicy) -> Self {
        self.cover_url_policy = policy;
        self
    }

    pub async fn lock_host(&self, host_id: &str) -> OwnedMutexGuard<()> {
        self.locks.lock(host_id).await
    }

    #[cfg(test)]
    fn with_executor(mut self, executor: Arc<dyn RemoteOperationExecutor>) -> Self {
        self.executor = executor;
        self
    }

    pub async fn find_idempotent(
        &self,
        actor: &str,
        host_id: &str,
        idempotency_key: &str,
        request: &RemoteOperationRequest,
    ) -> AppResult<Option<OperationView>> {
        validate_idempotency_key(idempotency_key)?;
        validate_actor(actor)?;
        let plaintext =
            serde_json::to_string(request).map_err(|error| AppError::Internal(error.into()))?;
        let request_fingerprint = Sha256::digest(plaintext.as_bytes());
        let key_hash = Sha256::digest(idempotency_key.as_bytes());
        let Some(existing) =
            find_existing(&self.pool, actor, host_id, request.action(), &key_hash).await?
        else {
            return Ok(None);
        };
        compare_existing(existing, &request_fingerprint).map(Some)
    }

    pub async fn enqueue(
        &self,
        actor: &str,
        host_id: &str,
        idempotency_key: &str,
        request: RemoteOperationRequest,
    ) -> AppResult<OperationView> {
        validate_idempotency_key(idempotency_key)?;
        validate_actor(actor)?;

        let plaintext =
            serde_json::to_string(&request).map_err(|error| AppError::Internal(error.into()))?;
        let request_fingerprint = Sha256::digest(plaintext.as_bytes()).to_vec();
        let key_hash = Sha256::digest(idempotency_key.as_bytes()).to_vec();
        let action = request.action();
        if let Some(existing) = find_existing(&self.pool, actor, host_id, action, &key_hash).await?
        {
            return compare_existing(existing, &request_fingerprint);
        }

        let host = db::get_host(&self.pool, &self.secrets, host_id).await?;
        if self.production && !host.verify_tls {
            return Err(AppError::BadRequest(
                "this host must enable TLS verification before use".into(),
            ));
        }
        let ciphertext = self.secrets.encrypt(&plaintext)?;
        let operation_id = format!("op_{}", Uuid::new_v4());
        let outbox_id = format!("out_{}", Uuid::new_v4());
        let now = db::now_micros()?;

        let mut transaction = self.pool.begin().await?;
        let inserted = sqlx::query(
            r#"INSERT INTO operations(
                   operation_id,actor,host_id,action,idempotency_key_hash,
                   request_fingerprint,request_ciphertext,state,attempt,
                   created_at_micros,updated_at_micros
               ) VALUES(?,?,?,?,?,?,?,'pending',0,?,?)
               ON CONFLICT(actor,host_id,action,idempotency_key_hash) DO NOTHING"#,
        )
        .bind(&operation_id)
        .bind(actor)
        .bind(host_id)
        .bind(action)
        .bind(&key_hash)
        .bind(&request_fingerprint)
        .bind(ciphertext)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;

        let view = if inserted.rows_affected() == 1 {
            insert_outbox(
                &mut transaction,
                &outbox_id,
                &operation_id,
                "requested",
                &format!("{action}.requested"),
                host_id,
                actor,
                &format!("operation_id={operation_id} state=pending attempt=0"),
                now,
            )
            .await?;
            OperationView {
                operation_id,
                state: OperationState::Pending,
                attempt: 0,
                created_at_micros: now,
                updated_at_micros: now,
                started_at_micros: None,
                completed_at_micros: None,
            }
        } else {
            let existing =
                find_existing_in_transaction(&mut transaction, actor, host_id, action, &key_hash)
                    .await?;
            match compare_existing(existing, &request_fingerprint) {
                Ok(view) => view,
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            }
        };
        transaction.commit().await?;
        self.notify.notify_one();
        Ok(view)
    }

    pub async fn get_for_actor(&self, actor: &str, operation_id: &str) -> AppResult<OperationView> {
        let stored = sqlx::query_as::<_, StoredOperationView>(
            r#"SELECT operation_id,state,attempt,created_at_micros,updated_at_micros,
                      started_at_micros,completed_at_micros
               FROM operations WHERE operation_id=? AND actor=?"#,
        )
        .bind(operation_id)
        .bind(actor)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("operation not found".into()))?;
        stored.into_view()
    }

    /// Converts interrupted in-flight work to `unknown` before new requests
    /// are accepted. Pending operations are intentionally left recoverable.
    pub async fn recover_startup(&self) -> AppResult<u64> {
        let running = sqlx::query_as::<_, RecoveryOperation>(
            "SELECT operation_id,actor,host_id,action,attempt FROM operations WHERE state='running'",
        )
        .fetch_all(&self.pool)
        .await?;
        if running.is_empty() {
            self.notify.notify_one();
            return Ok(0);
        }

        let now = db::now_micros()?;
        let mut transaction = self.pool.begin().await?;
        for operation in &running {
            let updated = sqlx::query(
                r#"UPDATE operations
                   SET state='unknown',updated_at_micros=?,completed_at_micros=?,
                       error_code='worker_interrupted'
                   WHERE operation_id=? AND state='running'"#,
            )
            .bind(now)
            .bind(now)
            .bind(&operation.operation_id)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() == 1 {
                insert_completion_outbox(&mut transaction, operation, OperationState::Unknown, now)
                    .await?;
            }
        }
        transaction.commit().await?;
        self.notify.notify_one();
        Ok(running.len() as u64)
    }

    /// Publishes a bounded outbox batch. Audit insertion and marking delivery
    /// happen in one transaction; the unique outbox id makes re-delivery safe.
    pub async fn deliver_outbox(&self) -> AppResult<u64> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, StoredOutbox>(
            r#"SELECT outbox_id,action,target,detail,actor,created_at_micros
               FROM audit_outbox
               WHERE delivered_at_micros IS NULL
               ORDER BY created_at_micros,
                        CASE event_kind WHEN 'requested' THEN 0 ELSE 1 END,
                        outbox_id
               LIMIT ?"#,
        )
        .bind(OUTBOX_BATCH)
        .fetch_all(&mut *transaction)
        .await?;
        let delivered_at = db::now_micros()?;
        for row in &rows {
            sqlx::query(
                r#"INSERT INTO audit_logs(
                       action,target,detail,actor,created_at_micros,outbox_id
                   ) VALUES(?,?,?,?,?,?)
                   ON CONFLICT(outbox_id) DO NOTHING"#,
            )
            .bind(&row.action)
            .bind(&row.target)
            .bind(&row.detail)
            .bind(&row.actor)
            .bind(row.created_at_micros)
            .bind(&row.outbox_id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"UPDATE audit_outbox
                   SET delivered_at_micros=?,delivery_attempt=delivery_attempt+1
                   WHERE outbox_id=? AND delivered_at_micros IS NULL"#,
            )
            .bind(delivered_at)
            .bind(&row.outbox_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(rows.len() as u64)
    }

    /// Runs independently of any HTTP request. Dropping this future aborts its
    /// child tasks; claimed rows remain `running` and become `unknown` on the
    /// next startup rather than being retried blindly.
    pub async fn run(self) {
        let mut tasks = JoinSet::new();
        let mut active_hosts = HashSet::new();
        loop {
            while let Some(result) = tasks.try_join_next() {
                finish_task(result, &mut active_hosts);
            }
            if let Err(error) = self.deliver_outbox().await {
                tracing::warn!(%error, "operation audit outbox delivery failed");
            }
            if tasks.len() < MAX_ACTIVE_HOSTS {
                match self.pending().await {
                    Ok(pending) => {
                        for operation in pending {
                            if tasks.len() >= MAX_ACTIVE_HOSTS {
                                break;
                            }
                            if active_hosts.insert(operation.host_id.clone()) {
                                let manager = self.clone();
                                tasks.spawn(async move {
                                    let host_id = operation.host_id;
                                    manager.execute_one(&operation.operation_id, &host_id).await;
                                    host_id
                                });
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, "operation dispatch query failed"),
                }
            }

            tokio::select! {
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(result) = result {
                        finish_task(result, &mut active_hosts);
                    }
                }
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(IDLE_POLL) => {}
            }
        }
    }

    async fn pending(&self) -> AppResult<Vec<PendingOperation>> {
        Ok(sqlx::query_as::<_, PendingOperation>(
            r#"SELECT current.operation_id,current.host_id
               FROM operations current
               WHERE current.state='pending'
                 AND NOT EXISTS (
                     SELECT 1 FROM operations earlier
                     WHERE earlier.state='pending'
                       AND earlier.host_id=current.host_id
                       AND (
                           earlier.created_at_micros<current.created_at_micros
                           OR (
                               earlier.created_at_micros=current.created_at_micros
                               AND earlier.operation_id<current.operation_id
                           )
                       )
                 )
               ORDER BY current.created_at_micros,current.operation_id LIMIT ?"#,
        )
        .bind(DISPATCH_BATCH)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn execute_one(&self, operation_id: &str, host_id: &str) {
        let _guard = self.locks.lock(host_id).await;
        let claimed = match self.claim(operation_id).await {
            Ok(Some(operation)) => operation,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(operation_id, %error, "operation claim failed");
                return;
            }
        };

        let outcome = self.execute_claimed(&claimed).await;
        if let Err(error) = self.finalize(&claimed, outcome).await {
            tracing::error!(operation_id, %error, "operation completion persistence failed");
            self.persist_unknown_without_reexecuting(&claimed).await;
        }
        self.notify.notify_one();
    }

    async fn persist_unknown_without_reexecuting(&self, operation: &ClaimedOperation) {
        let mut retry_delay = Duration::from_millis(100);
        loop {
            match self.mark_unknown_after_persistence_failure(operation).await {
                Ok(()) => return,
                Err(error) => {
                    tracing::error!(
                        operation_id = %operation.operation_id,
                        %error,
                        "operation unknown state persistence will retry without remote execution"
                    );
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(5));
                }
            }
        }
    }

    async fn claim(&self, operation_id: &str) -> AppResult<Option<ClaimedOperation>> {
        let now = db::now_micros()?;
        Ok(sqlx::query_as::<_, ClaimedOperation>(
            r#"UPDATE operations
               SET state='running',attempt=attempt+1,started_at_micros=?,updated_at_micros=?
               WHERE operation_id=? AND state='pending'
               RETURNING operation_id,actor,host_id,action,request_ciphertext,attempt"#,
        )
        .bind(now)
        .bind(now)
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn execute_claimed(&self, operation: &ClaimedOperation) -> ExecutionOutcome {
        let request = self
            .secrets
            .decrypt(&operation.request_ciphertext)
            .ok()
            .and_then(|plaintext| serde_json::from_str::<RemoteOperationRequest>(&plaintext).ok());
        let Some(mut request) = request else {
            return ExecutionOutcome::Failed("request_corrupt");
        };
        if request.action() != operation.action {
            return ExecutionOutcome::Failed("request_corrupt");
        }
        let host = match db::get_host(&self.pool, &self.secrets, &operation.host_id).await {
            Ok(host) => host,
            Err(AppError::NotFound(_)) => return ExecutionOutcome::Failed("host_not_found"),
            Err(_) => return ExecutionOutcome::Failed("local_state_unavailable"),
        };
        if self.production && !host.verify_tls {
            return ExecutionOutcome::Failed("tls_verification_required");
        }
        if let RemoteOperationRequest::CoverUpload { url, .. } = &mut request {
            match self.cover_url_policy.validate(url).await {
                Ok(validated) => *url = validated,
                Err(_) => return ExecutionOutcome::Failed("cover_url_rejected"),
            }
        }
        self.executor.execute(host, request).await
    }

    async fn finalize(
        &self,
        operation: &ClaimedOperation,
        outcome: ExecutionOutcome,
    ) -> AppResult<()> {
        let (state, error_code) = outcome.parts();
        let now = db::now_micros()?;
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"UPDATE operations
               SET state=?,updated_at_micros=?,completed_at_micros=?,error_code=?
               WHERE operation_id=? AND state='running'"#,
        )
        .bind(state.as_str())
        .bind(now)
        .bind(now)
        .bind(error_code)
        .bind(&operation.operation_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("operation is no longer running".into()));
        }
        insert_completion_outbox(&mut transaction, operation, state, now).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn mark_unknown_after_persistence_failure(
        &self,
        operation: &ClaimedOperation,
    ) -> AppResult<()> {
        let now = db::now_micros()?;
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"UPDATE operations
               SET state='unknown',updated_at_micros=?,completed_at_micros=?,
                   error_code='completion_persistence_uncertain'
               WHERE operation_id=? AND state='running'"#,
        )
        .bind(now)
        .bind(now)
        .bind(&operation.operation_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 1 {
            insert_completion_outbox(&mut transaction, operation, OperationState::Unknown, now)
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

pub fn validate_idempotency_key(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(AppError::BadRequest(
            "Idempotency-Key must contain 1-128 ASCII letters, digits, '-', '_', '.' or ':'".into(),
        ));
    }
    Ok(())
}

fn validate_actor(actor: &str) -> AppResult<()> {
    if actor.is_empty() || actor.len() > 128 || actor.chars().any(char::is_control) {
        return Err(AppError::BadRequest("invalid operation actor".into()));
    }
    Ok(())
}

type ExecutionFuture<'a> = Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>>;

trait RemoteOperationExecutor: Send + Sync {
    fn execute<'a>(&'a self, host: Host, request: RemoteOperationRequest) -> ExecutionFuture<'a>;
}

struct SunshineExecutor {
    transport: Arc<dyn MutationTransport>,
}

type TransportFuture<'a> = Pin<Box<dyn Future<Output = AppResult<Value>> + Send + 'a>>;

trait MutationTransport: Send + Sync {
    fn send<'a>(&'a self, host: Host, request: RemoteOperationRequest) -> TransportFuture<'a>;
}

struct UpstreamMutationTransport {
    upstream: UpstreamClient,
}

impl MutationTransport for UpstreamMutationTransport {
    fn send<'a>(&'a self, host: Host, request: RemoteOperationRequest) -> TransportFuture<'a> {
        Box::pin(async move {
            match request {
                RemoteOperationRequest::AppsSave { body } => {
                    self.upstream.apps_save(&host, &body).await
                }
                RemoteOperationRequest::AppsClose => self.upstream.apps_close(&host).await,
                RemoteOperationRequest::AppsDelete { index } => {
                    self.upstream.apps_delete(&host, index).await
                }
                RemoteOperationRequest::ClientsUnpair { uuid } => {
                    self.upstream.clients_unpair(&host, &uuid).await
                }
                RemoteOperationRequest::ClientsUnpairAll => {
                    self.upstream.clients_unpair_all(&host).await
                }
                RemoteOperationRequest::ClientsUpdate { uuid, enabled } => {
                    self.upstream.clients_update(&host, &uuid, enabled).await
                }
                RemoteOperationRequest::ConfigSave { body } => {
                    self.upstream.config_save(&host, &body).await
                }
                RemoteOperationRequest::Pin { pin, name } => {
                    self.upstream.pin(&host, &pin, &name).await
                }
                RemoteOperationRequest::Restart => self.upstream.restart(&host).await,
                RemoteOperationRequest::ResetDisplay => self.upstream.reset_display(&host).await,
                RemoteOperationRequest::CoverUpload { key, url } => {
                    self.upstream.cover_upload(&host, &key, &url).await
                }
            }
        })
    }
}

impl RemoteOperationExecutor for SunshineExecutor {
    fn execute<'a>(&'a self, host: Host, request: RemoteOperationRequest) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let result = self.transport.send(host, request).await;
            match result {
                Ok(_) => ExecutionOutcome::Succeeded,
                Err(AppError::Forbidden(_)) => ExecutionOutcome::Failed("upstream_rejected"),
                Err(_) => ExecutionOutcome::Unknown("upstream_result_unknown"),
            }
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum ExecutionOutcome {
    Succeeded,
    Failed(&'static str),
    Unknown(&'static str),
}

impl ExecutionOutcome {
    fn parts(self) -> (OperationState, Option<&'static str>) {
        match self {
            Self::Succeeded => (OperationState::Succeeded, None),
            Self::Failed(code) => (OperationState::Failed, Some(code)),
            Self::Unknown(code) => (OperationState::Unknown, Some(code)),
        }
    }
}

#[derive(FromRow)]
struct ExistingOperation {
    operation_id: String,
    request_fingerprint: Vec<u8>,
    state: String,
    attempt: i64,
    created_at_micros: i64,
    updated_at_micros: i64,
    started_at_micros: Option<i64>,
    completed_at_micros: Option<i64>,
}

const EXISTING_OPERATION_SQL: &str = r#"SELECT
    operation_id,request_fingerprint,state,attempt,created_at_micros,
    updated_at_micros,started_at_micros,completed_at_micros
FROM operations
WHERE actor=? AND host_id=? AND action=? AND idempotency_key_hash=?"#;

async fn find_existing(
    pool: &SqlitePool,
    actor: &str,
    host_id: &str,
    action: &str,
    key_hash: &[u8],
) -> AppResult<Option<ExistingOperation>> {
    Ok(
        sqlx::query_as::<_, ExistingOperation>(EXISTING_OPERATION_SQL)
            .bind(actor)
            .bind(host_id)
            .bind(action)
            .bind(key_hash)
            .fetch_optional(pool)
            .await?,
    )
}

async fn find_existing_in_transaction(
    transaction: &mut Transaction<'_, Sqlite>,
    actor: &str,
    host_id: &str,
    action: &str,
    key_hash: &[u8],
) -> AppResult<ExistingOperation> {
    Ok(
        sqlx::query_as::<_, ExistingOperation>(EXISTING_OPERATION_SQL)
            .bind(actor)
            .bind(host_id)
            .bind(action)
            .bind(key_hash)
            .fetch_one(&mut **transaction)
            .await?,
    )
}

fn compare_existing(
    existing: ExistingOperation,
    request_fingerprint: &[u8],
) -> AppResult<OperationView> {
    if existing.request_fingerprint != request_fingerprint {
        return Err(AppError::Conflict(
            "Idempotency-Key was already used with a different request".into(),
        ));
    }
    existing.into_view()
}

impl ExistingOperation {
    fn into_view(self) -> AppResult<OperationView> {
        StoredOperationView {
            operation_id: self.operation_id,
            state: self.state,
            attempt: self.attempt,
            created_at_micros: self.created_at_micros,
            updated_at_micros: self.updated_at_micros,
            started_at_micros: self.started_at_micros,
            completed_at_micros: self.completed_at_micros,
        }
        .into_view()
    }
}

#[derive(FromRow)]
struct StoredOperationView {
    operation_id: String,
    state: String,
    attempt: i64,
    created_at_micros: i64,
    updated_at_micros: i64,
    started_at_micros: Option<i64>,
    completed_at_micros: Option<i64>,
}

impl StoredOperationView {
    fn into_view(self) -> AppResult<OperationView> {
        Ok(OperationView {
            operation_id: self.operation_id,
            state: OperationState::parse(&self.state)?,
            attempt: self.attempt,
            created_at_micros: self.created_at_micros,
            updated_at_micros: self.updated_at_micros,
            started_at_micros: self.started_at_micros,
            completed_at_micros: self.completed_at_micros,
        })
    }
}

#[derive(FromRow)]
struct PendingOperation {
    operation_id: String,
    host_id: String,
}

#[derive(FromRow)]
struct ClaimedOperation {
    operation_id: String,
    actor: String,
    host_id: String,
    action: String,
    request_ciphertext: String,
    attempt: i64,
}

#[derive(FromRow)]
struct RecoveryOperation {
    operation_id: String,
    actor: String,
    host_id: String,
    action: String,
    attempt: i64,
}

#[derive(FromRow)]
struct StoredOutbox {
    outbox_id: String,
    action: String,
    target: String,
    detail: String,
    actor: String,
    created_at_micros: i64,
}

trait CompletionOperation {
    fn operation_id(&self) -> &str;
    fn actor(&self) -> &str;
    fn host_id(&self) -> &str;
    fn action(&self) -> &str;
    fn attempt(&self) -> i64;
}

impl CompletionOperation for ClaimedOperation {
    fn operation_id(&self) -> &str {
        &self.operation_id
    }
    fn actor(&self) -> &str {
        &self.actor
    }
    fn host_id(&self) -> &str {
        &self.host_id
    }
    fn action(&self) -> &str {
        &self.action
    }
    fn attempt(&self) -> i64 {
        self.attempt
    }
}

impl CompletionOperation for RecoveryOperation {
    fn operation_id(&self) -> &str {
        &self.operation_id
    }
    fn actor(&self) -> &str {
        &self.actor
    }
    fn host_id(&self) -> &str {
        &self.host_id
    }
    fn action(&self) -> &str {
        &self.action
    }
    fn attempt(&self) -> i64 {
        self.attempt
    }
}

async fn insert_completion_outbox(
    transaction: &mut Transaction<'_, Sqlite>,
    operation: &impl CompletionOperation,
    state: OperationState,
    now: i64,
) -> AppResult<()> {
    insert_outbox(
        transaction,
        &format!("out_{}", Uuid::new_v4()),
        operation.operation_id(),
        "completed",
        &format!("{}.{}", operation.action(), state.as_str()),
        operation.host_id(),
        operation.actor(),
        &format!(
            "operation_id={} state={} attempt={}",
            operation.operation_id(),
            state.as_str(),
            operation.attempt()
        ),
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_outbox(
    transaction: &mut Transaction<'_, Sqlite>,
    outbox_id: &str,
    operation_id: &str,
    event_kind: &str,
    action: &str,
    target: &str,
    actor: &str,
    detail: &str,
    now: i64,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO audit_outbox(
               outbox_id,operation_id,event_kind,action,target,actor,detail,created_at_micros
           ) VALUES(?,?,?,?,?,?,?,?)"#,
    )
    .bind(outbox_id)
    .bind(operation_id)
    .bind(event_kind)
    .bind(action)
    .bind(target)
    .bind(actor)
    .bind(detail)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn finish_task(result: Result<String, JoinError>, active_hosts: &mut HashSet<String>) {
    match result {
        Ok(host_id) => {
            active_hosts.remove(&host_id);
        }
        Err(error) => {
            tracing::error!(%error, "operation worker task failed");
            active_hosts.clear();
        }
    }
}

fn recover_lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use tempfile::TempDir;

    use super::*;
    use crate::model::HostSaveRequest;

    const ACTOR: &str = "actor-a";

    struct FixedExecutor(ExecutionOutcome);

    impl RemoteOperationExecutor for FixedExecutor {
        fn execute<'a>(
            &'a self,
            _host: Host,
            _request: RemoteOperationRequest,
        ) -> ExecutionFuture<'a> {
            let outcome = self.0;
            Box::pin(async move { outcome })
        }
    }

    #[derive(Clone, Copy)]
    enum FakeTransportResult {
        Success,
        Rejected,
        Uncertain,
    }

    struct FakeMutationTransport(FakeTransportResult);

    impl MutationTransport for FakeMutationTransport {
        fn send<'a>(
            &'a self,
            _host: Host,
            _request: RemoteOperationRequest,
        ) -> TransportFuture<'a> {
            let result = self.0;
            Box::pin(async move {
                match result {
                    FakeTransportResult::Success => Ok(serde_json::json!({ "status": true })),
                    FakeTransportResult::Rejected => {
                        Err(AppError::Forbidden("remote body must stay private".into()))
                    }
                    FakeTransportResult::Uncertain => Err(AppError::Upstream(
                        "transport disconnected after request transmission".into(),
                    )),
                }
            })
        }
    }

    #[derive(Default)]
    struct ConcurrencyExecutor {
        active: Mutex<HashMap<String, usize>>,
        same_host_overlap: AtomicBool,
        global_active: AtomicUsize,
        max_global_active: AtomicUsize,
    }

    impl RemoteOperationExecutor for ConcurrencyExecutor {
        fn execute<'a>(
            &'a self,
            host: Host,
            _request: RemoteOperationRequest,
        ) -> ExecutionFuture<'a> {
            Box::pin(async move {
                let global = self.global_active.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_global_active.fetch_max(global, Ordering::SeqCst);
                {
                    let mut active = recover_lock(&self.active);
                    let count = active.entry(host.id.clone()).or_default();
                    *count += 1;
                    if *count > 1 {
                        self.same_host_overlap.store(true, Ordering::SeqCst);
                    }
                }
                tokio::time::sleep(Duration::from_millis(75)).await;
                {
                    let mut active = recover_lock(&self.active);
                    *active.get_mut(&host.id).unwrap() -= 1;
                }
                self.global_active.fetch_sub(1, Ordering::SeqCst);
                ExecutionOutcome::Succeeded
            })
        }
    }

    async fn test_database(
        executor: Arc<dyn RemoteOperationExecutor>,
    ) -> (
        TempDir,
        String,
        SqlitePool,
        SecretBox,
        OperationManager,
        String,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operations.sqlite3");
        let database_url = format!("sqlite://{}", path.display());
        let pool = db::open_or_initialize(&database_url).await.unwrap();
        let secrets = SecretBox::new("test", [41; 32]).unwrap();
        let host_id = insert_test_host(&pool, &secrets, "host-a").await;
        let manager = OperationManager::new(
            pool.clone(),
            secrets.clone(),
            false,
            HostMutationLocks::default(),
            UpstreamClient::new().unwrap(),
        )
        .with_executor(executor);
        (directory, database_url, pool, secrets, manager, host_id)
    }

    async fn insert_test_host(pool: &SqlitePool, secrets: &SecretBox, name: &str) -> String {
        db::insert_host(
            pool,
            secrets,
            HostSaveRequest {
                name: name.into(),
                host: "127.0.0.1".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("upstream-password".into()),
                verify_tls: false,
            },
            false,
            ACTOR,
        )
        .await
        .unwrap()
        .id
    }

    async fn wait_for_state(
        manager: &OperationManager,
        operation_id: &str,
        expected: OperationState,
    ) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let view = manager.get_for_actor(ACTOR, operation_id).await.unwrap();
                if view.state == expected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("operation did not reach its expected state");
    }

    #[tokio::test]
    async fn same_host_waits_but_different_hosts_run_concurrently() {
        let locks = HostMutationLocks::default();
        let first = locks.lock("host-a").await;

        let same_locks = locks.clone();
        let mut same_host = tokio::spawn(async move {
            let _guard = same_locks.lock("host-a").await;
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut same_host)
                .await
                .is_err()
        );

        tokio::time::timeout(Duration::from_secs(1), locks.lock("host-b"))
            .await
            .expect("a different host must not be blocked");

        drop(first);
        tokio::time::timeout(Duration::from_secs(1), same_host)
            .await
            .expect("the waiter must proceed after the host lock is released")
            .expect("the waiter task must not panic");

        let expired = locks.lock("expired-host").await;
        assert_eq!(locks.entry_count(), 1);
        drop(expired);
        let _current = locks.lock("current-host").await;
        assert_eq!(locks.entry_count(), 1);
    }

    #[tokio::test]
    async fn idempotency_is_conflict_safe_and_sensitive_requests_are_encrypted() {
        let (_directory, _url, pool, _secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let request = RemoteOperationRequest::Pin {
            pin: "8642".into(),
            name: "private-laptop".into(),
        };
        let first = manager
            .enqueue(ACTOR, &host_id, "pair-request-1", request.clone())
            .await
            .unwrap();
        let repeated = manager
            .enqueue(ACTOR, &host_id, "pair-request-1", request)
            .await
            .unwrap();
        assert_eq!(first.operation_id, repeated.operation_id);

        let conflict = manager
            .enqueue(
                ACTOR,
                &host_id,
                "pair-request-1",
                RemoteOperationRequest::Pin {
                    pin: "9753".into(),
                    name: "private-laptop".into(),
                },
            )
            .await;
        assert!(matches!(conflict, Err(AppError::Conflict(_))));

        let (ciphertext, fingerprint, key_hash): (String, Vec<u8>, Vec<u8>) = sqlx::query_as(
            "SELECT request_ciphertext,request_fingerprint,idempotency_key_hash \
                 FROM operations WHERE operation_id=?",
        )
        .bind(&first.operation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(ciphertext.starts_with("sunshine:v1:"));
        assert!(!ciphertext.contains("8642"));
        assert!(!ciphertext.contains("private-laptop"));
        assert_eq!(fingerprint.len(), 32);
        assert_eq!(key_hash.len(), 32);
        let stored_operations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operations")
            .fetch_one(&pool)
            .await
            .unwrap();
        let requested_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE event_kind='requested'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let audit_detail: String =
            sqlx::query_scalar("SELECT detail FROM audit_outbox WHERE event_kind='requested'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored_operations, 1);
        assert_eq!(requested_events, 1);
        assert!(!audit_detail.contains("8642"));
        assert!(!audit_detail.contains("private-laptop"));

        sqlx::query(
            r#"CREATE TRIGGER reject_requested_outbox
               BEFORE INSERT ON audit_outbox WHEN NEW.event_kind='requested'
               BEGIN SELECT RAISE(ABORT, 'requested outbox rejected'); END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let transaction_failed = manager
            .enqueue(
                ACTOR,
                &host_id,
                "pair-request-2",
                RemoteOperationRequest::Pin {
                    pin: "1111".into(),
                    name: "must-rollback".into(),
                },
            )
            .await;
        assert!(transaction_failed.is_err());
        let stored_operations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            stored_operations, 1,
            "operation and outbox must roll back together"
        );

        db::delete_host(&pool, &host_id, ACTOR).await.unwrap();
        let after_host_removal = manager
            .enqueue(
                ACTOR,
                &host_id,
                "pair-request-1",
                RemoteOperationRequest::Pin {
                    pin: "8642".into(),
                    name: "private-laptop".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(after_host_removal.operation_id, first.operation_id);

        assert!(validate_idempotency_key("").is_err());
        assert!(validate_idempotency_key("has whitespace").is_err());
        assert!(validate_idempotency_key(&"x".repeat(129)).is_err());
    }

    #[tokio::test]
    async fn terminal_transitions_are_persisted_with_completion_outbox() {
        let (_directory, _url, pool, _secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let succeeded = manager
            .enqueue(
                ACTOR,
                &host_id,
                "terminal-success",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();
        manager.execute_one(&succeeded.operation_id, &host_id).await;
        let succeeded = manager
            .get_for_actor(ACTOR, &succeeded.operation_id)
            .await
            .unwrap();
        assert_eq!(succeeded.state, OperationState::Succeeded);
        assert_eq!(succeeded.attempt, 1);
        assert!(succeeded.started_at_micros.is_some());
        assert!(succeeded.completed_at_micros.is_some());

        let failed_manager =
            manager
                .clone()
                .with_executor(Arc::new(FixedExecutor(ExecutionOutcome::Failed(
                    "definitive_rejection",
                ))));
        let failed = failed_manager
            .enqueue(
                ACTOR,
                &host_id,
                "terminal-failed",
                RemoteOperationRequest::ResetDisplay,
            )
            .await
            .unwrap();
        failed_manager
            .execute_one(&failed.operation_id, &host_id)
            .await;
        assert_eq!(
            failed_manager
                .get_for_actor(ACTOR, &failed.operation_id)
                .await
                .unwrap()
                .state,
            OperationState::Failed
        );

        let unknown_manager =
            manager
                .clone()
                .with_executor(Arc::new(FixedExecutor(ExecutionOutcome::Unknown(
                    "result_uncertain",
                ))));
        let unknown = unknown_manager
            .enqueue(
                ACTOR,
                &host_id,
                "terminal-unknown",
                RemoteOperationRequest::AppsClose,
            )
            .await
            .unwrap();
        unknown_manager
            .execute_one(&unknown.operation_id, &host_id)
            .await;
        let view = unknown_manager
            .get_for_actor(ACTOR, &unknown.operation_id)
            .await
            .unwrap();
        assert_eq!(view.state, OperationState::Unknown);
        assert!(
            unknown_manager
                .get_for_actor("other-actor", &unknown.operation_id)
                .await
                .is_err()
        );
        let public_json = serde_json::to_value(view).unwrap();
        for forbidden in ["actor", "action", "request", "error_code", "error_message"] {
            assert!(public_json.get(forbidden).is_none());
        }

        let completion_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE event_kind='completed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(completion_events, 3);
    }

    #[tokio::test]
    async fn fake_transport_maps_success_rejection_and_uncertainty_conservatively() {
        let (_directory, _url, pool, secrets, _manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let host = db::get_host(&pool, &secrets, &host_id).await.unwrap();
        let executor = SunshineExecutor {
            transport: Arc::new(FakeMutationTransport(FakeTransportResult::Success)),
        };
        assert!(matches!(
            executor
                .execute(host.clone(), RemoteOperationRequest::Restart)
                .await,
            ExecutionOutcome::Succeeded
        ));
        let executor = SunshineExecutor {
            transport: Arc::new(FakeMutationTransport(FakeTransportResult::Rejected)),
        };
        assert!(matches!(
            executor
                .execute(host.clone(), RemoteOperationRequest::ResetDisplay)
                .await,
            ExecutionOutcome::Failed("upstream_rejected")
        ));
        let executor = SunshineExecutor {
            transport: Arc::new(FakeMutationTransport(FakeTransportResult::Uncertain)),
        };
        assert!(matches!(
            executor
                .execute(host, RemoteOperationRequest::AppsClose)
                .await,
            ExecutionOutcome::Unknown("upstream_result_unknown")
        ));
    }

    #[tokio::test]
    async fn cover_request_is_encrypted_and_revalidated_before_remote_execution() {
        let (_directory, _url, pool, _secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let operation = manager
            .enqueue(
                ACTOR,
                &host_id,
                "cover-policy-1",
                RemoteOperationRequest::CoverUpload {
                    key: "cover-1".into(),
                    url: "https://covers.invalid/art.jpg?signature=private-token".into(),
                },
            )
            .await
            .unwrap();
        let ciphertext: String =
            sqlx::query_scalar("SELECT request_ciphertext FROM operations WHERE operation_id=?")
                .bind(&operation.operation_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!ciphertext.contains("private-token"));
        manager.execute_one(&operation.operation_id, &host_id).await;
        assert_eq!(
            manager
                .get_for_actor(ACTOR, &operation.operation_id)
                .await
                .unwrap()
                .state,
            OperationState::Failed
        );
        let error_code: String =
            sqlx::query_scalar("SELECT error_code FROM operations WHERE operation_id=?")
                .bind(&operation.operation_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(error_code, "cover_url_rejected");
    }

    #[tokio::test]
    async fn restart_marks_running_unknown_and_resumes_pending_work() {
        let (directory, database_url, pool, secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let interrupted = manager
            .enqueue(
                ACTOR,
                &host_id,
                "restart-running",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();
        let pending = manager
            .enqueue(
                ACTOR,
                &host_id,
                "restart-pending",
                RemoteOperationRequest::ResetDisplay,
            )
            .await
            .unwrap();
        assert!(
            manager
                .claim(&interrupted.operation_id)
                .await
                .unwrap()
                .is_some()
        );
        drop(manager);
        pool.close().await;

        let reopened = db::open_existing(&database_url).await.unwrap();
        let restarted = OperationManager::new(
            reopened.clone(),
            secrets,
            false,
            HostMutationLocks::default(),
            UpstreamClient::new().unwrap(),
        )
        .with_executor(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded)));
        assert_eq!(restarted.recover_startup().await.unwrap(), 1);
        assert_eq!(
            restarted
                .get_for_actor(ACTOR, &interrupted.operation_id)
                .await
                .unwrap()
                .state,
            OperationState::Unknown
        );
        assert_eq!(
            restarted
                .get_for_actor(ACTOR, &pending.operation_id)
                .await
                .unwrap()
                .state,
            OperationState::Pending
        );

        let runner = tokio::spawn(restarted.clone().run());
        wait_for_state(&restarted, &pending.operation_id, OperationState::Succeeded).await;
        runner.abort();
        let _ = runner.await;
        reopened.close().await;
        drop(directory);
    }

    #[tokio::test]
    async fn outbox_delivery_is_transactional_and_idempotent() {
        let (_directory, _url, pool, _secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        manager
            .enqueue(
                ACTOR,
                &host_id,
                "audit-delivery",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();
        sqlx::query(
            r#"CREATE TRIGGER reject_outbox_audit
               BEFORE INSERT ON audit_logs WHEN NEW.outbox_id IS NOT NULL
               BEGIN SELECT RAISE(ABORT, 'audit delivery rejected'); END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(manager.deliver_outbox().await.is_err());
        let delivered: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_outbox WHERE delivered_at_micros IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let audit_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE outbox_id IS NOT NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((delivered, audit_rows), (0, 0));

        sqlx::query("DROP TRIGGER reject_outbox_audit")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(manager.deliver_outbox().await.unwrap(), 1);
        sqlx::query("UPDATE audit_outbox SET delivered_at_micros=NULL")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(manager.deliver_outbox().await.unwrap(), 1);
        let (audit_rows, attempts): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM audit_logs WHERE outbox_id IS NOT NULL), \
                    (SELECT delivery_attempt FROM audit_outbox)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_rows, 1);
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn completion_and_outbox_roll_back_together_then_retry_unknown_locally() {
        let (_directory, _url, pool, _secrets, manager, host_id) =
            test_database(Arc::new(FixedExecutor(ExecutionOutcome::Succeeded))).await;
        let operation = manager
            .enqueue(
                ACTOR,
                &host_id,
                "completion-transaction",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();
        sqlx::query(
            r#"CREATE TRIGGER reject_completion_outbox
               BEFORE INSERT ON audit_outbox WHEN NEW.event_kind='completed'
               BEGIN SELECT RAISE(ABORT, 'completion outbox rejected'); END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let execution = {
            let manager = manager.clone();
            let operation_id = operation.operation_id.clone();
            let host_id = host_id.clone();
            tokio::spawn(async move { manager.execute_one(&operation_id, &host_id).await })
        };
        let mut execution = execution;
        assert!(
            tokio::time::timeout(Duration::from_millis(250), &mut execution)
                .await
                .is_err(),
            "unknown persistence must keep retrying locally"
        );
        let state: String = sqlx::query_scalar("SELECT state FROM operations WHERE operation_id=?")
            .bind(&operation.operation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            state, "running",
            "completion state cannot commit without its outbox"
        );

        sqlx::query("DROP TRIGGER reject_completion_outbox")
            .execute(&pool)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), execution)
            .await
            .expect("unknown persistence retry did not finish")
            .unwrap();
        assert_eq!(
            manager
                .get_for_actor(ACTOR, &operation.operation_id)
                .await
                .unwrap()
                .state,
            OperationState::Unknown
        );
        let completion_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_outbox WHERE operation_id=? AND event_kind='completed'",
        )
        .bind(&operation.operation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(completion_events, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn operation_execution_serializes_each_host_and_parallelizes_hosts() {
        let executor = Arc::new(ConcurrencyExecutor::default());
        let (_directory, _url, pool, secrets, manager, host_a) =
            test_database(executor.clone()).await;
        let host_b = insert_test_host(&pool, &secrets, "host-b").await;
        let first = manager
            .enqueue(
                ACTOR,
                &host_a,
                "parallel-a-1",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();
        let second = manager
            .enqueue(
                ACTOR,
                &host_a,
                "parallel-a-2",
                RemoteOperationRequest::ResetDisplay,
            )
            .await
            .unwrap();
        let other = manager
            .enqueue(
                ACTOR,
                &host_b,
                "parallel-b-1",
                RemoteOperationRequest::Restart,
            )
            .await
            .unwrap();

        let first_task = {
            let manager = manager.clone();
            let host = host_a.clone();
            tokio::spawn(async move { manager.execute_one(&first.operation_id, &host).await })
        };
        let second_task = {
            let manager = manager.clone();
            let host = host_a.clone();
            tokio::spawn(async move { manager.execute_one(&second.operation_id, &host).await })
        };
        let other_task = {
            let manager = manager.clone();
            let host = host_b.clone();
            tokio::spawn(async move { manager.execute_one(&other.operation_id, &host).await })
        };
        first_task.await.unwrap();
        second_task.await.unwrap();
        other_task.await.unwrap();

        assert!(!executor.same_host_overlap.load(Ordering::SeqCst));
        assert!(executor.max_global_active.load(Ordering::SeqCst) >= 2);
        let succeeded: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE state='succeeded'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(succeeded, 3);
    }
}
