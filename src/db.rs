use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
    model::{DeviceView, validate_instance_name},
};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::time::{SystemTime, UNIX_EPOCH};
use sunshine_agent_protocol::{Binding, Capabilities, ConfigSnapshot};
use uuid::Uuid;
pub const SCHEMA: &str = "sunshine";
pub use crate::database_schema::{initialize_empty, open_existing, open_or_initialize};

#[derive(Clone, sqlx::FromRow)]
pub struct Device {
    pub device_id: String,
    pub name: String,
    pub installation_id: Option<String>,
    pub credential_hash: Option<Vec<u8>>,
    pub enrollment_hash: Option<Vec<u8>>,
    pub revoked_at_micros: Option<i64>,
    pub session_id: Option<String>,
    pub last_seen_at_micros: Option<i64>,
    pub health_at_micros: Option<i64>,
    pub sunshine_reachable: Option<bool>,
    pub capabilities_json: Option<String>,
    pub snapshot_json: Option<String>,
    pub saved_revision: Option<String>,
    pub configuration_state: String,
    pub created_at_micros: i64,
    pub updated_at_micros: i64,
}
impl Device {
    pub fn view(&self, now: i64) -> DeviceView {
        let online = self.revoked_at_micros.is_none()
            && self.session_id.is_some()
            && self
                .last_seen_at_micros
                .is_some_and(|seen| now.saturating_sub(seen) < 45_000_000);
        let snapshot: Option<ConfigSnapshot> = self
            .snapshot_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok());
        let state = if self.saved_revision.as_ref().is_some_and(|saved| {
            snapshot
                .as_ref()
                .is_some_and(|value| &value.revision != saved)
        }) {
            "drift_detected".to_owned()
        } else {
            self.configuration_state.clone()
        };
        DeviceView {
            id: self.device_id.clone(),
            name: self.name.clone(),
            registered: self.installation_id.is_some(),
            pairing_pending: self.enrollment_hash.is_some() && self.revoked_at_micros.is_none(),
            revoked: self.revoked_at_micros.is_some(),
            agent_online: online,
            sunshine_reachable: if online
                && self
                    .health_at_micros
                    .is_some_and(|seen| now.saturating_sub(seen) < 30_000_000)
            {
                self.sunshine_reachable
            } else {
                None
            },
            configuration_state: state,
            snapshot,
            capabilities: self
                .capabilities_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Capabilities>(value).ok()),
            last_seen_at_micros: self.last_seen_at_micros,
        }
    }
}
#[derive(Serialize)]
pub struct EnrollmentTicket {
    pub device: DeviceView,
    pub manager_id: Uuid,
    pub token: String,
}
pub async fn manager_id(pool: &SqlitePool) -> AppResult<Uuid> {
    let id: String =
        sqlx::query_scalar("SELECT manager_id FROM manager_identity WHERE singleton=1")
            .fetch_one(pool)
            .await?;
    Uuid::parse_str(&id)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid Manager identity")))
}
pub async fn get_device(pool: &SqlitePool, id: &str) -> AppResult<Device> {
    sqlx::query_as("SELECT * FROM devices WHERE device_id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("设备不存在".into()))
}
pub async fn list_devices(pool: &SqlitePool) -> AppResult<Vec<DeviceView>> {
    let now = now_micros()?;
    Ok(
        sqlx::query_as::<_, Device>("SELECT * FROM devices ORDER BY created_at_micros,device_id")
            .fetch_all(pool)
            .await?
            .iter()
            .map(|device| device.view(now))
            .collect(),
    )
}
pub async fn create_device(
    pool: &SqlitePool,
    name: &str,
    actor: &str,
) -> AppResult<EnrollmentTicket> {
    validate_instance_name(name)?;
    let id = Uuid::new_v4().to_string();
    let now = now_micros()?;
    let token = random_token();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&mut *tx)
        .await?;
    if count >= 4096 {
        return Err(AppError::Conflict("设备容量已满".into()));
    }
    sqlx::query("INSERT INTO devices(device_id,name,enrollment_hash,created_at_micros,updated_at_micros) VALUES(?,?,?,?,?)")
        .bind(&id).bind(name).bind(token_hash(&token).as_slice()).bind(now).bind(now).execute(&mut *tx).await?;
    audit(&mut tx, "device.create", &id, actor, None).await?;
    tx.commit().await?;
    Ok(EnrollmentTicket {
        device: get_device(pool, &id).await?.view(now),
        manager_id: manager_id(pool).await?,
        token,
    })
}
pub async fn cancel_pairing(pool: &SqlitePool, id: &str, actor: &str) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let changed = sqlx::query("UPDATE devices SET enrollment_hash=NULL,updated_at_micros=? WHERE device_id=? AND installation_id IS NULL AND revoked_at_micros IS NULL AND enrollment_hash IS NOT NULL")
        .bind(now_micros()?).bind(id).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(AppError::Conflict("配对码已使用或已取消".into()));
    }
    audit(&mut tx, "device.pairing.cancel", id, actor, None).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn rename(pool: &SqlitePool, id: &str, name: &str, actor: &str) -> AppResult<DeviceView> {
    validate_instance_name(name)?;
    let mut tx = pool.begin().await?;
    if sqlx::query("UPDATE devices SET name=?,updated_at_micros=? WHERE device_id=?")
        .bind(name)
        .bind(now_micros()?)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound("设备不存在".into()));
    }
    audit(&mut tx, "device.rename", id, actor, None).await?;
    tx.commit().await?;
    Ok(get_device(pool, id).await?.view(now_micros()?))
}
pub async fn revoke(pool: &SqlitePool, id: &str, actor: &str) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if sqlx::query("UPDATE devices SET revoked_at_micros=?,credential_hash=NULL,enrollment_hash=NULL,session_id=NULL,updated_at_micros=? WHERE device_id=?")
        .bind(now_micros()?).bind(now_micros()?).bind(id).execute(&mut *tx).await?.rows_affected()!=1 { return Err(AppError::NotFound("设备不存在".into())); }
    audit(&mut tx, "device.revoke", id, actor, None).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn enroll(
    pool: &SqlitePool,
    id: &str,
    installation: Uuid,
    token: &str,
    credential: &str,
) -> AppResult<Binding> {
    validate_token(token)?;
    validate_token(credential)?;
    if installation.is_nil() {
        return Err(AppError::BadRequest("安装身份无效".into()));
    }
    let now = now_micros()?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let changed = sqlx::query("UPDATE devices SET installation_id=?,credential_hash=?,enrollment_hash=NULL,updated_at_micros=? WHERE device_id=? AND installation_id IS NULL AND revoked_at_micros IS NULL AND enrollment_hash=?")
        .bind(installation.to_string()).bind(token_hash(credential).as_slice()).bind(now).bind(id).bind(token_hash(token).as_slice()).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(AppError::Unauthorized);
    }
    audit(&mut tx, "device.enroll", id, "agent", None).await?;
    tx.commit().await?;
    Ok(Binding {
        manager_id: manager_id(pool).await?,
        device_id: Uuid::parse_str(id).map_err(|_| AppError::Unauthorized)?,
        installation_id: installation,
    })
}
pub async fn authenticate_device(pool: &SqlitePool, credential: &str) -> AppResult<Device> {
    validate_token(credential).map_err(|_| AppError::Unauthorized)?;
    sqlx::query_as("SELECT * FROM devices WHERE credential_hash=? AND revoked_at_micros IS NULL AND installation_id IS NOT NULL")
        .bind(token_hash(credential).as_slice()).fetch_optional(pool).await?.ok_or(AppError::Unauthorized)
}
pub async fn binding(pool: &SqlitePool, device: &Device) -> AppResult<Binding> {
    Ok(Binding {
        manager_id: manager_id(pool).await?,
        device_id: Uuid::parse_str(&device.device_id).map_err(|_| AppError::Unauthorized)?,
        installation_id: device
            .installation_id
            .as_ref()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| AppError::Conflict("设备尚未注册".into()))?,
    })
}
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}
pub fn validate_token(token: &str) -> AppResult<()> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Err(AppError::BadRequest("设备凭据格式无效".into()))
    } else {
        Ok(())
    }
}
pub async fn audit(
    tx: &mut Transaction<'_, Sqlite>,
    action: &str,
    target: &str,
    actor: &str,
    detail: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_logs(action,target,actor,detail,created_at_micros) VALUES(?,?,?,?,?)",
    )
    .bind(action)
    .bind(target)
    .bind(actor)
    .bind(detail)
    .bind(now_micros()?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
pub fn now_micros() -> anyhow::Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros(),
    )?)
}
pub async fn ready(pool: &SqlitePool) -> bool {
    crate::database_schema::is_current(pool).await
}
#[derive(Clone, Copy, Serialize)]
pub struct DoctorReport {
    pub schema_ready: bool,
    pub integrity_ready: bool,
    pub foreign_keys_ready: bool,
    pub writable: bool,
    pub encrypted_values_ready: bool,
}
impl DoctorReport {
    pub fn healthy(self) -> bool {
        self.schema_ready
            && self.integrity_ready
            && self.foreign_keys_ready
            && self.writable
            && self.encrypted_values_ready
    }
}
pub async fn doctor(pool: &SqlitePool, secrets: &SecretBox) -> DoctorReport {
    let writable = async {
        let mut tx = pool.begin().await?;
        audit(&mut tx, "doctor.write_probe", "doctor", "doctor", None).await?;
        tx.rollback().await?;
        Ok::<_, AppError>(())
    }
    .await
    .is_ok();
    DoctorReport {
        schema_ready: ready(pool).await,
        integrity_ready: sarmg_sqlite::integrity_check(pool).await.is_ok(),
        foreign_keys_ready: sarmg_sqlite::foreign_key_check(pool).await.is_ok(),
        writable,
        encrypted_values_ready: require_current_runtime_state(pool, secrets).await.is_ok(),
    }
}
pub async fn require_current_runtime_state(
    pool: &SqlitePool,
    secrets: &SecretBox,
) -> anyhow::Result<()> {
    sarmg_sqlite::integrity_check(pool).await?;
    sarmg_sqlite::foreign_key_check(pool).await?;
    manager_id(pool).await?;
    let store = sarmg_operations::SqliteOperationStore::new(pool.clone());
    let mut cursor = String::new();
    loop {
        let ids:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM _sarmg_operations WHERE operation_id>? ORDER BY operation_id LIMIT 128").bind(&cursor).fetch_all(pool).await?;
        if ids.is_empty() {
            break;
        }
        for id in &ids {
            let stored = store
                .get(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("operation disappeared"))?;
            crate::operations::decode_task(secrets, &stored)?;
        }
        cursor = ids.last().expect("non-empty batch").clone();
    }
    Ok(())
}
