//! Durable lifecycle belongs to Foundation; this module only dispatches product commands to devices.
use crate::{
    crypto::{SecretBox, constant_time_equal_32},
    db,
    error::{AppError, AppResult},
};
pub use sarmg_operations::OperationState;
use sarmg_operations::{
    EnqueueOutcome, NewOperation, SqliteOperationStore, StoredOperation, Transition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use sunshine_agent_protocol::{
    Binding, Command, ConfigSnapshot, Effectiveness, PROTOCOL, Permission, Report,
    SUNSHINE_VERSION, Task,
};
use tokio::sync::{Notify, watch};
use uuid::Uuid;

#[derive(Clone)]
pub struct OperationManager {
    pub pool: sqlx::SqlitePool,
    pub store: SqliteOperationStore,
    secrets: SecretBox,
    notify: Arc<Notify>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    actor: String,
    request_ciphertext: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    binding: Binding,
    command: Command,
}
#[derive(Serialize)]
pub struct OperationView {
    pub operation_id: String,
    pub device_id: String,
    pub action: String,
    pub state: OperationState,
    pub attempt: i64,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
    pub result: Option<Report>,
    pub reconciliation: Option<Report>,
    pub resolution: Option<String>,
}
pub fn namespace(device_id: &str) -> String {
    format!("sunshine.agent.v1.{device_id}")
}
pub fn action(command: &Command) -> &'static str {
    match command {
        Command::ReadConfig {} => "sunshine.config.read",
        Command::PatchConfig { .. } => "sunshine.config.patch",
        Command::Restart { .. } => "sunshine.restart",
    }
}
fn permission(command: &Command) -> Permission {
    match command {
        Command::ReadConfig {} => Permission::ReadConfig,
        Command::PatchConfig { .. } => Permission::WriteConfig,
        Command::Restart { .. } => Permission::Restart,
    }
}
fn internal(error: impl Into<anyhow::Error>) -> AppError {
    AppError::Internal(error.into())
}
fn payload(stored: &StoredOperation) -> AppResult<Payload> {
    serde_json::from_slice(&stored.request_payload).map_err(internal)
}
pub fn decode_task(secrets: &SecretBox, stored: &StoredOperation) -> AppResult<Task> {
    let payload = payload(stored)?;
    let plaintext = secrets.decrypt_operation_request(
        &stored.operation.operation_id,
        &stored.action,
        &payload.request_ciphertext,
    )?;
    if !constant_time_equal_32(
        &stored.operation.request_fingerprint,
        &secrets.operation_request_fingerprint(&plaintext),
    ) {
        return Err(AppError::Crypto);
    }
    let request: Request = serde_json::from_str(&plaintext).map_err(internal)?;
    let task = Task {
        protocol: PROTOCOL.into(),
        operation_id: stored.operation.operation_id.clone(),
        permission: permission(&request.command),
        binding: request.binding,
        command: request.command,
    };
    if action(&task.command) != stored.action
        || task.binding.device_id.to_string() != stored.operation.target_key
        || stored.operation.namespace != namespace(&stored.operation.target_key)
        || task.validate(&task.binding, true).is_err()
    {
        return Err(AppError::Crypto);
    }
    Ok(task)
}
pub fn validate_idempotency_key(key: &str) -> AppResult<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        Err(AppError::BadRequest(
            "必须提供 1–128 字符的 Idempotency-Key".into(),
        ))
    } else {
        Ok(())
    }
}
impl OperationManager {
    pub fn new(pool: sqlx::SqlitePool, secrets: SecretBox) -> Self {
        Self {
            store: SqliteOperationStore::new(pool.clone()),
            pool,
            secrets,
            notify: Arc::new(Notify::new()),
        }
    }
    pub async fn enqueue(
        &self,
        actor: &str,
        id: &str,
        key: &str,
        command: Command,
    ) -> AppResult<OperationView> {
        validate_idempotency_key(key)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let device: db::Device = sqlx::query_as("SELECT * FROM devices WHERE device_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound("设备不存在".into()))?;
        if device.revoked_at_micros.is_some() {
            return Err(AppError::Conflict("设备凭据已撤销".into()));
        }
        // Resolve the identity in the same transaction to fence enrollment/revocation.
        let manager: String =
            sqlx::query_scalar("SELECT manager_id FROM manager_identity WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await?;
        let binding = Binding {
            manager_id: Uuid::parse_str(&manager).map_err(internal)?,
            device_id: Uuid::parse_str(id).map_err(internal)?,
            installation_id: device
                .installation_id
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok())
                .ok_or_else(|| AppError::Conflict("请先注册 Agent".into()))?,
        };
        let operation_id = format!("op_{}", Uuid::new_v4());
        let task = Task {
            protocol: PROTOCOL.into(),
            operation_id: operation_id.clone(),
            binding: binding.clone(),
            permission: permission(&command),
            command: command.clone(),
        };
        task.validate(&binding, true)
            .map_err(|_| AppError::BadRequest("业务指令不符合白名单、权限或修订要求".into()))?;
        let plaintext = serde_json::to_string(&Request { binding, command }).map_err(internal)?;
        let mut digest = Sha256::new();
        for part in [
            actor.as_bytes(),
            id.as_bytes(),
            action(&task.command).as_bytes(),
            self.secrets.operation_idempotency_key_hash(key).as_slice(),
        ] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part);
        }
        let payload = Payload {
            actor: actor.into(),
            request_ciphertext: self.secrets.encrypt_operation_request(
                &operation_id,
                action(&task.command),
                &plaintext,
            )?,
        };
        let now = db::now_micros()?;
        let value = NewOperation {
            operation_id,
            namespace: namespace(id),
            target_key: id.into(),
            action: action(&task.command).into(),
            idempotency_digest: digest.finalize().into(),
            request_fingerprint: self.secrets.operation_request_fingerprint(&plaintext),
            request_payload: serde_json::to_vec(&payload).map_err(internal)?,
            max_attempts: 1,
            not_before_micros: now,
            created_at_micros: now,
        };
        let stored = SqliteOperationStore::enqueue_in(&mut tx, value)
            .await
            .map_err(|error| match error {
                sarmg_operations::Error::IdempotencyConflict => {
                    AppError::Conflict("幂等键已用于不同指令".into())
                }
                other => internal(other),
            })?;
        tx.commit().await?;
        let stored = match stored {
            EnqueueOutcome::Created(stored) | EnqueueOutcome::Existing(stored) => stored,
        };
        self.notify.notify_waiters();
        self.view(stored).await
    }
    async fn view(&self, stored: StoredOperation) -> AppResult<OperationView> {
        let reconciliation: Option<String> =
            sqlx::query_scalar("SELECT report_json FROM agent_observations WHERE operation_id=?")
                .bind(&stored.operation.operation_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(OperationView {
            operation_id: stored.operation.operation_id,
            device_id: stored.operation.target_key,
            action: stored.action,
            state: stored.operation.state,
            attempt: i64::from(stored.operation.attempt),
            created_at_micros: stored.created_at_micros,
            updated_at_micros: stored.updated_at_micros,
            result: stored
                .result_payload
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()
                .map_err(internal)?,
            reconciliation: reconciliation
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(internal)?,
            resolution: stored.resolution_code,
        })
    }
    pub async fn get_for_actor(&self, actor: &str, id: &str) -> AppResult<OperationView> {
        let stored = self
            .store
            .get(id)
            .await
            .map_err(internal)?
            .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
        if payload(&stored)?.actor != actor {
            return Err(AppError::Forbidden("不能访问其他管理员的任务".into()));
        }
        self.view(stored).await
    }
    pub async fn list_for_actor(&self, actor: &str, device: &str) -> AppResult<Vec<OperationView>> {
        let ids:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM _sarmg_operations WHERE target_key=? AND json_extract(CAST(request_payload AS TEXT),'$.actor')=? ORDER BY created_at_micros DESC LIMIT 50").bind(device).bind(actor).fetch_all(&self.pool).await?;
        let mut views = Vec::new();
        for id in ids {
            views.push(self.get_for_actor(actor, &id).await?);
        }
        Ok(views)
    }
    pub async fn resolve_for_actor(
        &self,
        actor: &str,
        id: &str,
        resolution: sarmg_operations::Resolution,
    ) -> AppResult<OperationView> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let bytes: Vec<u8> = sqlx::query_scalar(
            "SELECT request_payload FROM _sarmg_operations WHERE operation_id=?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
        if serde_json::from_slice::<Payload>(&bytes)
            .map_err(internal)?
            .actor
            != actor
        {
            return Err(AppError::Forbidden("不能处理其他管理员的任务".into()));
        }
        let updated = SqliteOperationStore::resolve_in(&mut tx, id, resolution, db::now_micros()?)
            .await
            .map_err(|_| AppError::Conflict("任务不需要人工核对".into()))?;
        db::audit(
            &mut tx,
            "operation.resolve",
            &updated.operation.target_key,
            actor,
            Some(&format!(
                "operation_id={id} resolution={}",
                resolution.as_str()
            )),
        )
        .await?;
        tx.commit().await?;
        self.view(updated).await
    }
    pub async fn recover_startup(&self) -> AppResult<u64> {
        sqlx::query("UPDATE devices SET session_id=NULL,last_seen_at_micros=NULL,health_at_micros=NULL,sunshine_reachable=NULL").execute(&self.pool).await?;
        let spaces: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT namespace FROM _sarmg_operations")
                .fetch_all(&self.pool)
                .await?;
        let mut count = 0;
        for space in spaces {
            count += self
                .store
                .recover_running(&space, "manager_interrupted", db::now_micros()?)
                .await
                .map_err(internal)?;
        }
        Ok(count)
    }
    pub async fn next(&self, device: &str, owner: &str) -> AppResult<Option<StoredOperation>> {
        let now = db::now_micros()?;
        // Linearize authorization and claim with revocation/session replacement. A task
        // already claimed before revocation can be in flight; revocation cannot undo it.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE device_id=? AND session_id=? AND revoked_at_micros IS NULL AND credential_hash IS NOT NULL)")
            .bind(device).bind(owner).fetch_one(&mut *tx).await?;
        if !valid {
            return Err(AppError::Unauthorized);
        }
        let claimed = SqliteOperationStore::claim_next_in(
            &mut tx,
            &namespace(device),
            owner,
            now,
            now + 120_000_000,
        )
        .await
        .map_err(internal)?;
        tx.commit().await?;
        Ok(claimed)
    }
    pub async fn uncertain(&self, device: &str) -> AppResult<Option<StoredOperation>> {
        let id:Option<String>=sqlx::query_scalar("SELECT operation_id FROM _sarmg_operations WHERE namespace=? AND state='unknown' ORDER BY created_at_micros LIMIT 1").bind(namespace(device)).fetch_optional(&self.pool).await?;
        match id {
            Some(id) => self.store.get(&id).await.map_err(internal),
            None => Ok(None),
        }
    }
    pub fn task(&self, stored: &StoredOperation) -> AppResult<Task> {
        decode_task(&self.secrets, stored)
    }
    pub async fn disconnected(&self, stored: &StoredOperation) -> AppResult<()> {
        if stored.operation.state == OperationState::Running {
            self.store
                .abandon_claim(&stored.operation, "agent_disconnected", db::now_micros()?)
                .await
                .map_err(internal)?;
        }
        Ok(())
    }
    pub async fn complete(
        &self,
        stored: &StoredOperation,
        session: &str,
        report: Report,
    ) -> AppResult<()> {
        let task = self.task(stored)?;
        validate_report(&task.command, &report)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE device_id=? AND session_id=? AND revoked_at_micros IS NULL)").bind(&stored.operation.target_key).bind(session).fetch_one(&mut *tx).await?;
        if !valid {
            return Err(AppError::Unauthorized);
        }
        let bytes = serde_json::to_vec(&report).map_err(internal)?;
        if stored.operation.state == OperationState::Unknown {
            // Evidence only: never forge a human Foundation resolution.
            let state: String =
                sqlx::query_scalar("SELECT state FROM _sarmg_operations WHERE operation_id=?")
                    .bind(&stored.operation.operation_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if state != "unknown" {
                return Err(AppError::Conflict("任务已由管理员处理".into()));
            }
            sqlx::query("INSERT INTO agent_observations(operation_id,report_json,observed_at_micros) VALUES(?,?,?) ON CONFLICT(operation_id) DO UPDATE SET report_json=excluded.report_json,observed_at_micros=excluded.observed_at_micros")
                .bind(&stored.operation.operation_id).bind(std::str::from_utf8(&bytes).map_err(internal)?).bind(db::now_micros()?).execute(&mut *tx).await?;
            db::audit(
                &mut tx,
                "operation.reconciled",
                &stored.operation.target_key,
                "agent",
                Some(&stored.operation.operation_id),
            )
            .await?;
        } else {
            let transition = match &report {
                Report::ConfigRead { .. }
                | Report::ConfigSaved { .. }
                | Report::RestartAcknowledged { .. } => Transition::Succeed,
                Report::Unknown { .. } => Transition::MarkIndeterminate {
                    code: "agent_uncertain".into(),
                },
                Report::Conflict { .. } | Report::Rejected { .. } => Transition::Fail {
                    code: if matches!(report, Report::Conflict { .. }) {
                        "configuration_conflict"
                    } else {
                        "agent_rejected"
                    }
                    .into(),
                    retryable: false,
                    retry_not_before_micros: db::now_micros()?,
                },
            };
            SqliteOperationStore::apply_transition_owned_in(
                &mut tx,
                &stored.operation.operation_id,
                stored
                    .operation
                    .lease_owner
                    .as_deref()
                    .ok_or_else(|| internal(anyhow::anyhow!("missing lease owner")))?,
                transition,
                Some(&bytes),
                db::now_micros()?,
            )
            .await
            .map_err(internal)?;
        }
        match &report {
            Report::ConfigRead { snapshot } => {
                sqlx::query("UPDATE devices SET snapshot_json=? WHERE device_id=?")
                    .bind(serde_json::to_string(snapshot).map_err(internal)?)
                    .bind(&stored.operation.target_key)
                    .execute(&mut *tx)
                    .await?;
            }
            Report::ConfigSaved { snapshot } | Report::RestartAcknowledged { snapshot } => {
                let state = if matches!(report, Report::ConfigSaved { .. }) {
                    "awaiting_restart"
                } else {
                    "pending_verification"
                };
                sqlx::query("UPDATE devices SET snapshot_json=?,saved_revision=?,configuration_state=? WHERE device_id=?").bind(serde_json::to_string(snapshot).map_err(internal)?).bind(&snapshot.revision).bind(state).bind(&stored.operation.target_key).execute(&mut *tx).await?;
            }
            _ => {}
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn deliver_outbox(&self) -> AppResult<u64> {
        let events = self
            .store
            .pending_audit_events(128)
            .await
            .map_err(internal)?;
        let mut count = 0;
        for event in events {
            let stored = self
                .store
                .get(&event.operation_id)
                .await
                .map_err(internal)?
                .ok_or_else(|| internal(anyhow::anyhow!("missing audited operation")))?;
            let mut tx = self.pool.begin().await?;
            sqlx::query("INSERT INTO audit_logs(action,target,detail,actor,created_at_micros,outbox_id) VALUES(?,?,?,?,?,?) ON CONFLICT(outbox_id) DO NOTHING")
                .bind(format!("{}.{}",stored.action,event.to_state)).bind(&stored.operation.target_key).bind(format!("operation_id={} from={} state={}",event.operation_id,event.from_state,event.to_state)).bind(payload(&stored)?.actor).bind(event.created_at_micros).bind(&event.event_id).execute(&mut *tx).await?;
            if SqliteOperationStore::mark_audit_delivered_in(
                &mut tx,
                &event.event_id,
                db::now_micros()?,
            )
            .await
            .map_err(internal)?
            {
                count += 1;
            }
            tx.commit().await?;
        }
        Ok(count)
    }
    pub async fn run_until(self, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            self.deliver_outbox()
                .await
                .map_err(|_| "operation audit unavailable".to_owned())?;
            let spaces: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT namespace FROM _sarmg_operations WHERE state='running'",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|_| "operation database unavailable".to_owned())?;
            for space in spaces {
                self.store
                    .recover_expired(
                        &space,
                        db::now_micros().map_err(|_| "clock unavailable".to_owned())?,
                    )
                    .await
                    .map_err(|_| "operation recovery unavailable".to_owned())?;
            }
            tokio::select! {_ = sarmg_server_runtime::wait_for_shutdown(&mut shutdown)=>break,_=tokio::time::sleep(Duration::from_secs(1))=>{}}
        }
        Ok(())
    }
}
pub fn validate_snapshot(snapshot: &ConfigSnapshot) -> AppResult<()> {
    sunshine_agent_protocol::validate_revision(&snapshot.revision)
        .map_err(|_| AppError::BadRequest("Agent 配置修订无效".into()))?;
    if snapshot.sunshine_version != SUNSHINE_VERSION
        || snapshot.fields.iter().any(|(key, value)| {
            !sunshine_agent_protocol::config::FIELDS.contains(&key.as_str()) || value.len() > 512
        })
    {
        return Err(AppError::BadRequest("Agent 配置响应无效".into()));
    }
    Ok(())
}
fn validate_report(command: &Command, report: &Report) -> AppResult<()> {
    let valid = match (command, report) {
        (Command::ReadConfig {}, Report::ConfigRead { snapshot }) => {
            validate_snapshot(snapshot)?;
            snapshot.effectiveness == Effectiveness::PendingVerification
        }
        (Command::PatchConfig { .. }, Report::ConfigSaved { snapshot }) => {
            validate_snapshot(snapshot)?;
            snapshot.effectiveness == Effectiveness::AwaitingRestart
        }
        (Command::Restart { .. }, Report::RestartAcknowledged { snapshot }) => {
            validate_snapshot(snapshot)?;
            snapshot.effectiveness == Effectiveness::PendingVerification
        }
        (_, Report::Conflict { actual_revision }) => {
            sunshine_agent_protocol::validate_revision(actual_revision).is_ok()
        }
        (_, Report::Rejected { .. } | Report::Unknown { .. }) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::BadRequest("Agent 结果与业务指令不符".into()))
    }
}
