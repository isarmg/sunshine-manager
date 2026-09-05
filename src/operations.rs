use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard, watch};
use uuid::Uuid;

pub use sarmg_operations::OperationState;
use sarmg_operations::{
    EnqueueOutcome, NewOperation, OperationContext, OperationExecutor, OperationOutcome,
    SqliteOperationStore, StoredOperation, Transition,
};

use crate::{
    client::UpstreamClient,
    cover_policy::CoverUrlPolicy,
    cover_proxy::CoverProxy,
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    model::Host,
};

pub use crate::http::{probe_loop, router};

const NAMESPACE: &str = "sunshine.remote";
const MAX_ACTIVE_HOSTS: usize = 16;
const OUTBOX_BATCH: u32 = 128;
const IDLE_POLL: Duration = Duration::from_millis(250);
const EXECUTION_TIMEOUT: Duration = Duration::from_secs(90);
const OPERATION_LEASE_MICROS: i64 = 120_000_000;

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
                    locks.insert(host_id.to_owned(), Arc::downgrade(&lock));
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationView {
    pub operation_id: String,
    pub state: OperationState,
    pub attempt: i64,
    pub max_attempts: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
    pub started_at_micros: Option<i64>,
    pub completed_at_micros: Option<i64>,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurablePayload {
    actor: String,
    request_ciphertext: String,
}

#[derive(Clone)]
pub struct OperationManager {
    pool: sqlx::SqlitePool,
    store: SqliteOperationStore,
    secrets: SecretBox,
    locks: HostMutationLocks,
    executor: Arc<dyn OperationExecutor<Request = (Host, RemoteOperationRequest), Result = Value>>,
    notify: Arc<Notify>,
    cover_url_policy: CoverUrlPolicy,
    cover_proxy: CoverProxy,
}

impl OperationManager {
    pub fn new(
        pool: sqlx::SqlitePool,
        secrets: SecretBox,
        locks: HostMutationLocks,
        upstream: UpstreamClient,
    ) -> Self {
        Self {
            store: SqliteOperationStore::new(pool.clone()),
            pool,
            secrets,
            locks,
            executor: Arc::new(SunshineExecutor {
                transport: Arc::new(UpstreamMutationTransport { upstream }),
            }),
            notify: Arc::new(Notify::new()),
            cover_url_policy: CoverUrlPolicy::default(),
            cover_proxy: CoverProxy::disabled(),
        }
    }

    pub fn with_cover_url_policy(mut self, policy: CoverUrlPolicy) -> Self {
        self.cover_url_policy = policy;
        self
    }

    pub fn with_cover_delivery(mut self, policy: CoverUrlPolicy, proxy: CoverProxy) -> Self {
        self.cover_url_policy = policy;
        self.cover_proxy = proxy;
        self
    }

    pub async fn lock_host(&self, host_id: &str) -> OwnedMutexGuard<()> {
        self.locks.lock(host_id).await
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
        let plaintext = serde_json::to_string(request).map_err(internal)?;
        let fingerprint = self.secrets.operation_request_fingerprint(&plaintext);
        let digest = scoped_idempotency_digest(
            actor,
            host_id,
            request.action(),
            &self.secrets.operation_idempotency_key_hash(idempotency_key),
        );
        let Some(existing) = self
            .store
            .get_by_idempotency(NAMESPACE, &digest)
            .await
            .map_err(internal)?
        else {
            return Ok(None);
        };
        sarmg_operations::validate_idempotency(&existing.operation, &digest, &fingerprint)
            .map_err(|error| match error {
                sarmg_operations::Error::IdempotencyConflict => AppError::Conflict(
                    "Idempotency-Key was already used with a different request".into(),
                ),
                other => AppError::Internal(other.into()),
            })?;
        ensure_actor(&existing, actor)?;
        Ok(Some(operation_view(existing)))
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
        db::get_host(&self.pool, &self.secrets, host_id).await?;
        let plaintext = serde_json::to_string(&request).map_err(internal)?;
        let request_fingerprint = self.secrets.operation_request_fingerprint(&plaintext);
        let action = request.action();
        let idempotency_digest = scoped_idempotency_digest(
            actor,
            host_id,
            action,
            &self.secrets.operation_idempotency_key_hash(idempotency_key),
        );
        let operation_id = format!("op_{}", Uuid::new_v4());
        let payload = DurablePayload {
            actor: actor.to_owned(),
            request_ciphertext: self.secrets.encrypt_operation_request(
                &operation_id,
                action,
                &plaintext,
            )?,
        };
        let now = db::now_micros()?;
        let outcome = self
            .store
            .enqueue(NewOperation {
                operation_id,
                namespace: NAMESPACE.into(),
                target_key: host_id.to_owned(),
                action: action.into(),
                idempotency_digest,
                request_fingerprint,
                request_payload: serde_json::to_vec(&payload).map_err(internal)?,
                max_attempts: 3,
                not_before_micros: now,
                created_at_micros: now,
            })
            .await
            .map_err(|error| match error {
                sarmg_operations::Error::IdempotencyConflict => AppError::Conflict(
                    "Idempotency-Key was already used with a different request".into(),
                ),
                other => AppError::Internal(other.into()),
            })?;
        let stored = match outcome {
            EnqueueOutcome::Created(value) | EnqueueOutcome::Existing(value) => value,
        };
        ensure_actor(&stored, actor)?;
        self.notify.notify_one();
        Ok(operation_view(stored))
    }

    pub async fn get_for_actor(&self, actor: &str, operation_id: &str) -> AppResult<OperationView> {
        let stored = self
            .store
            .get(operation_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| AppError::NotFound("operation not found".into()))?;
        ensure_actor(&stored, actor)?;
        Ok(operation_view(stored))
    }

    pub async fn resolve_for_actor(
        &self,
        actor: &str,
        operation_id: &str,
        resolution: sarmg_operations::Resolution,
    ) -> AppResult<OperationView> {
        validate_actor(actor)?;
        let current = self
            .store
            .get(operation_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| AppError::NotFound("operation not found".into()))?;
        ensure_actor(&current, actor)?;
        let now = db::now_micros()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated =
            SqliteOperationStore::resolve_in(&mut transaction, operation_id, resolution, now)
                .await
                .map_err(|error| match error {
                    sarmg_operations::Error::InvalidTransition { .. } => {
                        AppError::Conflict("operation does not require resolution".into())
                    }
                    other => AppError::Internal(other.into()),
                })?;
        sqlx::query("INSERT INTO audit_logs(action,target,detail,actor,created_at_micros) VALUES('operation.resolve',?,?,?,?)")
            .bind(&current.operation.target_key)
            .bind(format!("operation_id={} resolution={}", operation_id, resolution.as_str()))
            .bind(actor).bind(now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.notify.notify_one();
        Ok(operation_view(updated))
    }

    pub async fn recover_startup(&self) -> AppResult<u64> {
        self.store
            .recover_running(NAMESPACE, "worker_interrupted", db::now_micros()?)
            .await
            .map_err(internal)
    }

    pub async fn deliver_outbox(&self) -> AppResult<u64> {
        let events = self
            .store
            .pending_audit_events(OUTBOX_BATCH)
            .await
            .map_err(internal)?;
        let delivered_at = db::now_micros()?;
        let mut delivered = 0_u64;
        for event in events {
            let Some(operation) = self
                .store
                .get(&event.operation_id)
                .await
                .map_err(internal)?
            else {
                continue;
            };
            let payload = decode_payload(&operation)?;
            let detail = format!(
                "operation_id={} from={} state={}",
                event.operation_id, event.from_state, event.to_state
            );
            let mut transaction = self.pool.begin().await?;
            sqlx::query(
                "INSERT INTO audit_logs(action,target,detail,actor,created_at_micros,outbox_id) \
                 VALUES(?,?,?,?,?,?) ON CONFLICT(outbox_id) DO NOTHING",
            )
            .bind(format!("{}.{}", operation.action, event.to_state))
            .bind(&operation.operation.target_key)
            .bind(detail)
            .bind(payload.actor)
            .bind(event.created_at_micros)
            .bind(&event.event_id)
            .execute(&mut *transaction)
            .await?;
            if SqliteOperationStore::mark_audit_delivered_in(
                &mut transaction,
                &event.event_id,
                delivered_at,
            )
            .await
            .map_err(internal)?
            {
                delivered += 1;
            }
            transaction.commit().await?;
        }
        Ok(delivered)
    }

    pub async fn run_until(self, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            if *shutdown.borrow() || shutdown.has_changed().is_err() {
                break;
            }
            while let Some(result) = tasks.try_join_next() {
                result.map_err(|_| "operation worker task failed".to_owned())?;
            }
            if let Err(error) = self.deliver_outbox().await {
                tracing::warn!(%error, "operation audit outbox delivery failed");
            }
            while tasks.len() < MAX_ACTIVE_HOSTS {
                if *shutdown.borrow() || shutdown.has_changed().is_err() {
                    break;
                }
                let now = match db::now_micros() {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, "operation clock unavailable");
                        break;
                    }
                };
                match self
                    .store
                    .claim_next(
                        NAMESPACE,
                        &Uuid::new_v4().to_string(),
                        now,
                        now.saturating_add(OPERATION_LEASE_MICROS),
                    )
                    .await
                {
                    Ok(Some(operation)) => {
                        let manager = self.clone();
                        tasks.spawn(async move { manager.execute_claimed(operation).await });
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(%error, "operation claim failed");
                        break;
                    }
                }
            }
            tokio::select! {
                _ = sarmg_server_runtime::wait_for_shutdown(&mut shutdown) => break,
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(result) = result {
                        result.map_err(|_| "operation worker task failed".to_owned())?;
                    }
                }
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(IDLE_POLL) => {}
            }
        }
        let drain = async {
            while let Some(result) = tasks.join_next().await {
                result.map_err(|_| "operation worker task failed".to_owned())?;
            }
            Ok::<(), String>(())
        };
        if let Ok(result) = tokio::time::timeout(Duration::from_secs(25), drain).await {
            result?;
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        // Aborted claims remain durable Running until exclusive startup recovery
        // marks them Unknown; they are never requeued by shutdown.
        self.deliver_outbox()
            .await
            .map_err(|_| "operation audit delivery failed".to_owned())?;
        Ok(())
    }

    async fn execute_claimed(&self, operation: StoredOperation) {
        let outcome = tokio::time::timeout(EXECUTION_TIMEOUT, self.execute_request(&operation))
            .await
            .unwrap_or_else(|_| OperationOutcome::Indeterminate {
                code: "execution_deadline_exceeded".into(),
            });
        let completion = match outcome {
            OperationOutcome::Succeeded(value) => {
                let result = serde_json::to_vec(&value).ok();
                self.finish(&operation, Transition::Succeed, result.as_deref())
                    .await
            }
            OperationOutcome::DefinitiveFailure { code, retryable } => {
                let now = db::now_micros().unwrap_or_default();
                self.finish(
                    &operation,
                    Transition::Fail {
                        code,
                        retryable,
                        retry_not_before_micros: now.saturating_add(1_000_000),
                    },
                    None,
                )
                .await
            }
            OperationOutcome::Indeterminate { code } => {
                self.finish(&operation, Transition::MarkIndeterminate { code }, None)
                    .await
            }
        };
        if let Err(error) = completion {
            tracing::error!(operation_id=%operation.operation.operation_id, %error, "operation completion persistence failed");
            self.persist_unknown_without_reexecuting(&operation).await;
        }
        self.notify.notify_one();
    }

    async fn finish(
        &self,
        operation: &StoredOperation,
        transition: Transition,
        result: Option<&[u8]>,
    ) -> AppResult<()> {
        self.store
            .apply_transition_owned(
                &operation.operation.operation_id,
                operation
                    .operation
                    .lease_owner
                    .as_deref()
                    .ok_or_else(|| internal(anyhow::anyhow!("operation claim has no owner")))?,
                transition,
                result,
                db::now_micros()?,
            )
            .await
            .map(|_| ())
            .map_err(internal)
    }

    async fn persist_unknown_without_reexecuting(&self, operation: &StoredOperation) {
        let mut delay = Duration::from_millis(100);
        loop {
            let now = db::now_micros().unwrap_or_default();
            match self
                .store
                .abandon_claim(
                    &operation.operation,
                    "completion_persistence_uncertain",
                    now,
                )
                .await
            {
                Ok(_)
                | Err(
                    sarmg_operations::Error::ConcurrentModification
                    | sarmg_operations::Error::NotFound,
                ) => return,
                Err(error) => {
                    tracing::error!(operation_id=%operation.operation.operation_id, %error, "persisting unknown state failed");
                    tokio::time::sleep(delay).await;
                    delay = delay.saturating_mul(2).min(Duration::from_secs(5));
                }
            }
        }
    }

    async fn execute_request(&self, operation: &StoredOperation) -> OperationOutcome<Value> {
        let payload = match decode_payload(operation) {
            Ok(value) => value,
            Err(_) => {
                return OperationOutcome::DefinitiveFailure {
                    code: "request_corrupt".into(),
                    retryable: false,
                };
            }
        };
        let request = self
            .secrets
            .decrypt_operation_request(
                &operation.operation.operation_id,
                &operation.action,
                &payload.request_ciphertext,
            )
            .ok()
            .and_then(|value| serde_json::from_str::<RemoteOperationRequest>(&value).ok());
        let Some(mut request) = request else {
            return OperationOutcome::DefinitiveFailure {
                code: "request_corrupt".into(),
                retryable: false,
            };
        };
        if request.action() != operation.action {
            return OperationOutcome::DefinitiveFailure {
                code: "request_corrupt".into(),
                retryable: false,
            };
        }
        let host =
            match db::get_host(&self.pool, &self.secrets, &operation.operation.target_key).await {
                Ok(host) => host,
                Err(AppError::NotFound(_)) => {
                    return OperationOutcome::DefinitiveFailure {
                        code: "host_not_found".into(),
                        retryable: false,
                    };
                }
                Err(_) => {
                    return OperationOutcome::DefinitiveFailure {
                        code: "local_state_unavailable".into(),
                        retryable: true,
                    };
                }
            };
        if let RemoteOperationRequest::CoverUpload { url, .. } = &mut request {
            let cover = match self.cover_url_policy.download(url).await {
                Ok(value) => value,
                Err(_) => {
                    return OperationOutcome::DefinitiveFailure {
                        code: "cover_download_rejected".into(),
                        retryable: false,
                    };
                }
            };
            match self
                .cover_proxy
                .publish(&host, &operation.operation.operation_id, cover)
                .await
            {
                Ok(delivery) => *url = delivery,
                Err(_) => {
                    return OperationOutcome::DefinitiveFailure {
                        code: "cover_delivery_unavailable".into(),
                        retryable: true,
                    };
                }
            }
        }
        self.executor
            .execute(
                OperationContext {
                    operation_id: operation.operation.operation_id.clone(),
                    namespace: NAMESPACE.into(),
                    target_key: operation.operation.target_key.clone(),
                    attempt: operation.operation.attempt,
                },
                (host, request),
            )
            .await
    }
}

struct SunshineExecutor {
    transport: Arc<dyn MutationTransport>,
}

#[async_trait]
impl OperationExecutor for SunshineExecutor {
    type Request = (Host, RemoteOperationRequest);
    type Result = Value;

    async fn execute(
        &self,
        _context: OperationContext,
        (host, request): Self::Request,
    ) -> OperationOutcome<Self::Result> {
        match self.transport.send(host, request).await {
            Ok(value) => OperationOutcome::Succeeded(value),
            Err(AppError::Forbidden(_)) => OperationOutcome::DefinitiveFailure {
                code: "upstream_rejected".into(),
                retryable: false,
            },
            Err(_) => OperationOutcome::Indeterminate {
                code: "upstream_result_unknown".into(),
            },
        }
    }
}

#[async_trait]
trait MutationTransport: Send + Sync {
    async fn send(&self, host: Host, request: RemoteOperationRequest) -> AppResult<Value>;
}

struct UpstreamMutationTransport {
    upstream: UpstreamClient,
}

#[async_trait]
impl MutationTransport for UpstreamMutationTransport {
    async fn send(&self, host: Host, request: RemoteOperationRequest) -> AppResult<Value> {
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
    }
}

fn scoped_idempotency_digest(actor: &str, host_id: &str, action: &str, key: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for value in [actor.as_bytes(), host_id.as_bytes(), action.as_bytes(), key] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    hasher.finalize().into()
}

fn decode_payload(operation: &StoredOperation) -> AppResult<DurablePayload> {
    serde_json::from_slice(&operation.request_payload).map_err(internal)
}

fn ensure_actor(operation: &StoredOperation, actor: &str) -> AppResult<()> {
    if decode_payload(operation)?.actor == actor {
        Ok(())
    } else {
        Err(AppError::NotFound("operation not found".into()))
    }
}

fn operation_view(value: StoredOperation) -> OperationView {
    let state = value.operation.state;
    let started = (value.operation.attempt > 0).then_some(value.updated_at_micros);
    let completed = matches!(
        state,
        OperationState::Succeeded
            | OperationState::Failed
            | OperationState::Unknown
            | OperationState::DeadLetter
            | OperationState::Resolved
    )
    .then_some(value.updated_at_micros);
    OperationView {
        operation_id: value.operation.operation_id,
        state,
        attempt: i64::from(value.operation.attempt),
        max_attempts: i64::from(value.operation.max_attempts),
        created_at_micros: value.created_at_micros,
        updated_at_micros: value.updated_at_micros,
        started_at_micros: started,
        completed_at_micros: completed,
        resolution: value.resolution_code,
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

fn internal(error: impl Into<anyhow::Error>) -> AppError {
    AppError::Internal(error.into())
}

fn recover_lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_manager() -> OperationManager {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize_empty(&pool).await.unwrap();
        OperationManager::new(
            pool,
            SecretBox::new("test", [2; 32]).unwrap(),
            HostMutationLocks::default(),
            UpstreamClient::new().unwrap(),
        )
    }

    async fn seed_claim(manager: &OperationManager, expires: i64) -> StoredOperation {
        manager
            .store
            .enqueue(NewOperation {
                operation_id: "operation-test".into(),
                namespace: NAMESPACE.into(),
                target_key: "host-test".into(),
                action: "sunshine.system.restart".into(),
                idempotency_digest: [1; 32],
                request_fingerprint: [2; 32],
                request_payload: serde_json::to_vec(&DurablePayload {
                    actor: "test-admin".into(),
                    request_ciphertext: "opaque".into(),
                })
                .unwrap(),
                max_attempts: 2,
                created_at_micros: 1,
                not_before_micros: 1,
            })
            .await
            .unwrap();
        manager
            .store
            .claim_next(NAMESPACE, "test-owner", 2, expires)
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn expired_completion_becomes_unknown_without_publishing_success() {
        let manager = test_manager().await;
        let claim = seed_claim(&manager, 10).await;
        assert!(
            manager
                .finish(&claim, Transition::Succeed, Some(b"success"))
                .await
                .is_err()
        );
        manager.persist_unknown_without_reexecuting(&claim).await;
        let current = manager.store.get("operation-test").await.unwrap().unwrap();
        assert_eq!(current.operation.state, OperationState::Unknown);
        assert!(current.result_payload.is_none());
        assert_eq!(current.operation.attempt, 1);
    }

    #[tokio::test]
    async fn completion_audit_failure_records_unknown_without_reexecution() {
        let manager = test_manager().await;
        let claim = seed_claim(&manager, db::now_micros().unwrap() + OPERATION_LEASE_MICROS).await;
        sqlx::raw_sql("CREATE TRIGGER reject_success BEFORE INSERT ON _sarmg_operation_audit_outbox WHEN NEW.to_state='succeeded' BEGIN SELECT RAISE(FAIL, 'injected'); END;")
            .execute(&manager.pool).await.unwrap();
        assert!(
            manager
                .finish(&claim, Transition::Succeed, Some(b"success"))
                .await
                .is_err()
        );
        manager.persist_unknown_without_reexecuting(&claim).await;
        let current = manager.store.get("operation-test").await.unwrap().unwrap();
        assert_eq!(current.operation.state, OperationState::Unknown);
        assert!(current.result_payload.is_none());
        assert_eq!(manager.store.pending_audit_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn audit_delivery_and_human_resolution_are_atomic() {
        let manager = test_manager().await;
        let claim = seed_claim(&manager, 10).await;
        manager.persist_unknown_without_reexecuting(&claim).await;
        sqlx::raw_sql("CREATE TRIGGER reject_ack BEFORE UPDATE OF delivered_at_micros ON _sarmg_operation_audit_outbox BEGIN SELECT RAISE(FAIL, 'injected'); END;")
            .execute(&manager.pool).await.unwrap();
        assert!(manager.deliver_outbox().await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_logs")
                .fetch_one(&manager.pool)
                .await
                .unwrap(),
            0
        );
        sqlx::query("DROP TRIGGER reject_ack")
            .execute(&manager.pool)
            .await
            .unwrap();
        assert_eq!(manager.deliver_outbox().await.unwrap(), 3);
        assert_eq!(manager.deliver_outbox().await.unwrap(), 0);
        sqlx::raw_sql("CREATE TRIGGER reject_resolution BEFORE INSERT ON audit_logs WHEN NEW.action='operation.resolve' BEGIN SELECT RAISE(FAIL, 'injected'); END;")
            .execute(&manager.pool).await.unwrap();
        assert!(
            manager
                .resolve_for_actor(
                    "test-admin",
                    "operation-test",
                    sarmg_operations::Resolution::ConfirmedSucceeded
                )
                .await
                .is_err()
        );
        assert_eq!(
            manager
                .store
                .get("operation-test")
                .await
                .unwrap()
                .unwrap()
                .operation
                .state,
            OperationState::Unknown
        );
        sqlx::query("DROP TRIGGER reject_resolution")
            .execute(&manager.pool)
            .await
            .unwrap();
        assert!(
            manager
                .resolve_for_actor(
                    "other-admin",
                    "operation-test",
                    sarmg_operations::Resolution::ConfirmedSucceeded
                )
                .await
                .is_err()
        );
        let resolved = manager
            .resolve_for_actor(
                "test-admin",
                "operation-test",
                sarmg_operations::Resolution::UnableToConfirm,
            )
            .await
            .unwrap();
        assert_eq!(resolved.state, OperationState::DeadLetter);
    }

    #[tokio::test]
    async fn preexisting_shutdown_does_not_claim_work() {
        let manager = test_manager().await;
        let claim = seed_claim(&manager, db::now_micros().unwrap() + OPERATION_LEASE_MICROS).await;
        manager
            .finish(
                &claim,
                Transition::Fail {
                    code: "rejected".into(),
                    retryable: true,
                    retry_not_before_micros: 1,
                },
                None,
            )
            .await
            .unwrap();
        let store = manager.store.clone();
        let (shutdown, receiver) = watch::channel(true);
        manager.run_until(receiver).await.unwrap();
        assert_eq!(store.active_operation_count().await.unwrap(), 1);
        let current = store.get("operation-test").await.unwrap().unwrap();
        assert_eq!(current.operation.state, OperationState::Pending);
        assert_eq!(current.operation.attempt, 1);
        drop(shutdown);
    }

    #[test]
    fn idempotency_scope_binds_actor_target_action_and_key() {
        let key = [7; 32];
        let baseline = scoped_idempotency_digest("actor", "host", "action", &key);
        assert_ne!(
            baseline,
            scoped_idempotency_digest("other", "host", "action", &key)
        );
        assert_ne!(
            baseline,
            scoped_idempotency_digest("actor", "other", "action", &key)
        );
        assert_ne!(
            baseline,
            scoped_idempotency_digest("actor", "host", "other", &key)
        );
    }

    #[tokio::test]
    async fn host_lock_registry_does_not_retain_unused_targets() {
        let locks = HostMutationLocks::default();
        {
            let _guard = locks.lock("host").await;
            assert_eq!(locks.entry_count(), 1);
        }
        let _guard = locks.lock("other").await;
        assert_eq!(locks.entry_count(), 1);
    }

    #[test]
    fn idempotency_keys_are_required_and_bounded() {
        assert!(validate_idempotency_key("request:1").is_ok());
        assert!(validate_idempotency_key("").is_err());
    }
}
