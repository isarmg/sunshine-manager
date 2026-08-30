use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::{
    InternalAuth, InternalIdentity,
    client::UpstreamClient,
    cover_policy::CoverUrlPolicy,
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    model::{
        ClientUpdateRequest, CoverUploadRequest, HealthSnapshot, Host, HostInfo, HostPatchRequest,
        HostSaveRequest, HostStatus, PinRequest, ProbeStatus, UnpairRequest, web_url,
    },
    operations::{
        HostMutationLocks, OperationManager, OperationView, RemoteOperationRequest,
        validate_idempotency_key,
    },
};

const API_NAMESPACE: &str = "/api";
const CURRENT_API_VERSION_PREFIX: &str = "/v2";

#[derive(Clone)]
pub struct WorkerState {
    pub pool: SqlitePool,
    pub secrets: SecretBox,
    pub auth: InternalAuth,
    pub production: bool,
    upstream: UpstreamClient,
    health: Arc<RwLock<HashMap<String, HealthSnapshot>>>,
    operations: OperationManager,
    cover_url_policy: CoverUrlPolicy,
    login_admission: crate::login_admission::LoginAdmission,
    dummy_password_hash: Arc<String>,
    static_dir: PathBuf,
}

impl WorkerState {
    pub fn new(
        pool: SqlitePool,
        secrets: SecretBox,
        auth: InternalAuth,
        production: bool,
        static_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let dummy_password_hash =
            crate::auth::hash_password("sunshine-dummy-password-value-that-is-never-accepted")
                .map_err(|error| anyhow::anyhow!(error))?;
        let upstream = UpstreamClient::new()?;
        let mutation_locks = HostMutationLocks::default();
        let operations = OperationManager::new(
            pool.clone(),
            secrets.clone(),
            production,
            mutation_locks,
            upstream.clone(),
        );
        Ok(Self {
            pool,
            secrets,
            auth,
            production,
            upstream,
            health: Arc::new(RwLock::new(HashMap::new())),
            operations,
            cover_url_policy: CoverUrlPolicy::default(),
            login_admission: crate::login_admission::LoginAdmission::default(),
            dummy_password_hash: Arc::new(dummy_password_hash),
            static_dir,
        })
    }

    pub fn with_cover_url_policy(mut self, policy: CoverUrlPolicy) -> Self {
        self.operations = self.operations.with_cover_url_policy(policy.clone());
        self.cover_url_policy = policy;
        self
    }

    pub fn operation_manager(&self) -> &OperationManager {
        &self.operations
    }
}

pub fn router(state: WorkerState) -> Router {
    let health = Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready));

    let public_api = Router::new()
        .route("/auth/login", post(login))
        .layer(DefaultBodyLimit::max(16 * 1024));

    let protected_api = Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/session", get(session))
        .route("/sunshine/operations/{operation_id}", get(operation_get))
        .route("/sunshine/hosts", get(list_hosts).post(create_host))
        .route(
            "/sunshine/hosts/{id}",
            patch(update_host).delete(delete_host),
        )
        .route("/sunshine/hosts/{id}/status", get(status))
        .route("/sunshine/hosts/{id}/apps", get(apps_list).post(apps_save))
        .route("/sunshine/hosts/{id}/apps/close", post(apps_close))
        .route("/sunshine/hosts/{id}/apps/{index}", delete(apps_delete))
        .route("/sunshine/hosts/{id}/clients", get(clients_list))
        .route("/sunshine/hosts/{id}/clients/unpair", post(clients_unpair))
        .route(
            "/sunshine/hosts/{id}/clients/unpair-all",
            post(clients_unpair_all),
        )
        .route("/sunshine/hosts/{id}/clients/update", post(clients_update))
        .route(
            "/sunshine/hosts/{id}/config",
            get(config_get).post(config_save),
        )
        .route("/sunshine/hosts/{id}/config/locale", get(config_locale))
        .route("/sunshine/hosts/{id}/api-logs", get(logs))
        .route("/sunshine/hosts/{id}/pin", post(pin))
        .route("/sunshine/hosts/{id}/restart", post(restart))
        .route("/sunshine/hosts/{id}/reset-display", post(reset_display))
        .route("/sunshine/hosts/{id}/covers/{index}", get(cover))
        .route("/sunshine/hosts/{id}/covers/upload", post(cover_upload))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate));

    let current_api = Router::new().merge(public_api).merge(protected_api);
    let api_namespace = Router::new()
        .nest(CURRENT_API_VERSION_PREFIX, current_api)
        .fallback(|| async { StatusCode::NOT_FOUND });

    let static_dir = state.static_dir.clone();
    Router::new()
        .merge(health)
        .nest(API_NAMESPACE, api_namespace)
        .fallback_service(ServeDir::new(static_dir))
        .with_state(state)
}

async fn authenticate(
    State(state): State<WorkerState>,
    mut request: Request,
    next: Next,
) -> AppResult<Response> {
    let token = state
        .auth
        .parse_session_token(request.headers())
        .ok_or(AppError::Unauthorized)?;
    let now = db::now_micros()?;
    let session = db::authenticate_session(
        &state.pool,
        &state.auth,
        &crate::auth::token_hash(&token),
        now,
    )
    .await?
    .ok_or(AppError::Unauthorized)?;
    if is_mutation(request.method()) {
        verify_mutation_request(request.headers(), &session.csrf_hash)?;
    }
    request.extensions_mut().insert(InternalIdentity {
        subject: session.user_id,
        email: session.email,
        session_id: session.session_id,
        csrf_hash: session.csrf_hash,
    });
    Ok(next.run(request).await)
}

async fn live(State(state): State<WorkerState>) -> Response {
    let _ = state;
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

async fn ready(State(state): State<WorkerState>) -> Response {
    let ready = db::ready(&state.pool).await;
    let response = if ready {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "ready" })),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not-ready" })),
        )
            .into_response()
    };
    let _ = state;
    response
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn login(
    State(state): State<WorkerState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> AppResult<Response> {
    let normalized_email = request.email.trim().to_lowercase();
    state
        .login_admission
        .admit(Some(source.ip()), &normalized_email)
        .await?;
    let user = db::find_active_user_by_email(&state.pool, &normalized_email).await?;
    let password_hash = user
        .as_ref()
        .map(|user| user.password_hash.clone())
        .unwrap_or_else(|| (*state.dummy_password_hash).clone());
    let password = request.password;
    let permit = state.login_admission.argon2_permit().await?;
    let verified = tokio::task::spawn_blocking(move || {
        crate::auth::verify_password(&password, &password_hash)
    })
    .await
    .map_err(|error| {
        AppError::Internal(anyhow::anyhow!("password verification failed: {error}"))
    })?;
    drop(permit);
    let Some(user) = user.filter(|_| verified) else {
        return Err(AppError::Unauthorized);
    };
    state.login_admission.clear_account(&normalized_email).await;
    let now = db::now_micros()?;
    let session = state.auth.issue(now)?;
    db::create_session(&state.pool, &user, &session, now).await?;
    let session_cookie = HeaderValue::from_str(&state.auth.session_cookie(&session.token))
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    let csrf_cookie = HeaderValue::from_str(&state.auth.csrf_cookie(&session.csrf_token))
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    let mut response = Json(serde_json::json!({
        "authenticated": true,
        "user_id": user.user_id,
        "email": user.email,
        "csrf_token": session.csrf_token
    }))
    .into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookie);
    response
        .headers_mut()
        .append(header::SET_COOKIE, csrf_cookie);
    Ok(response)
}

async fn logout(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
) -> AppResult<Response> {
    db::revoke_session(&state.pool, &identity.session_id, db::now_micros()?).await?;
    let session_cookie = HeaderValue::from_str(&state.auth.expired_session_cookie())
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    let csrf_cookie = HeaderValue::from_str(&state.auth.expired_csrf_cookie())
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookie);
    response
        .headers_mut()
        .append(header::SET_COOKIE, csrf_cookie);
    Ok(response)
}

async fn session(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let csrf_token = state
        .auth
        .parse_csrf_token(&headers)
        .filter(|token| crate::auth::token_matches_hash(token, &identity.csrf_hash))
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(serde_json::json!({
        "authenticated": true,
        "user_id": identity.subject,
        "email": identity.email,
        "csrf_token": csrf_token
    })))
}

fn is_mutation(method: &Method) -> bool {
    !matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

fn verify_mutation_request(headers: &HeaderMap, expected_csrf_hash: &[u8]) -> AppResult<()> {
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| crate::auth::token_matches_hash(value, expected_csrf_hash))
        .ok_or_else(|| AppError::Forbidden("invalid CSRF token".into()))?;
    let _ = csrf;
    verify_same_origin(headers)
}

fn verify_same_origin(headers: &HeaderMap) -> AppResult<()> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| reqwest::Url::parse(value).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .ok_or_else(|| AppError::Forbidden("missing or invalid Origin header".into()))?;
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Forbidden("missing Host header".into()))?;
    let expected = reqwest::Url::parse(&format!("{}://{host}", origin.scheme()))
        .map_err(|_| AppError::Forbidden("invalid Host header".into()))?;
    if origin.origin() != expected.origin() {
        return Err(AppError::Forbidden("cross-origin mutation rejected".into()));
    }
    Ok(())
}

async fn list_hosts(State(state): State<WorkerState>) -> AppResult<Json<Vec<HostInfo>>> {
    let hosts = db::list_hosts(&state.pool, &state.secrets).await?;
    let health = state.health.read().await;
    Ok(Json(
        hosts
            .iter()
            .map(|host| host_info(host, health.get(&host.id)))
            .collect(),
    ))
}

async fn create_host(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Json(request): Json<HostSaveRequest>,
) -> AppResult<(StatusCode, Json<HostInfo>)> {
    let host = db::insert_host(
        &state.pool,
        &state.secrets,
        request,
        state.production,
        &identity.subject,
    )
    .await?;
    state
        .health
        .write()
        .await
        .insert(host.id.clone(), HealthSnapshot::default());
    Ok((StatusCode::CREATED, Json(host_info(&host, None))))
}

async fn update_host(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    Json(request): Json<HostPatchRequest>,
) -> AppResult<Json<HostInfo>> {
    let _guard = state.operations.lock_host(&id).await;
    let host = db::update_host(
        &state.pool,
        &state.secrets,
        &id,
        request,
        state.production,
        &identity.subject,
    )
    .await?;
    state
        .health
        .write()
        .await
        .insert(host.id.clone(), HealthSnapshot::default());
    Ok(Json(host_info(&host, None)))
}

async fn delete_host(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let _guard = state.operations.lock_host(&id).await;
    let deleted_id = id.clone();
    db::delete_host(&state.pool, &id, &identity.subject).await?;
    state.health.write().await.remove(&deleted_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn status(
    State(state): State<WorkerState>,
    Path(id): Path<String>,
) -> AppResult<Json<HostStatus>> {
    let host = load_host(&state, &id).await?;
    let health = state.health.read().await.get(&id).cloned();
    let reachable = health
        .as_ref()
        .and_then(|item| item.reachable)
        .unwrap_or(false);
    Ok(Json(HostStatus {
        host: host.host.clone(),
        web_port: host.web_port,
        web_url: web_url(&host),
        reachable,
        message: match health.and_then(|item| item.reachable) {
            Some(true) => "Sunshine Web UI port is reachable".into(),
            Some(false) => "Sunshine Web UI port is not reachable".into(),
            None => "Sunshine Web UI reachability check is pending".into(),
        },
    }))
}

async fn apps_list(
    State(state): State<WorkerState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        state
            .upstream
            .apps_list(&load_host(&state, &id).await?)
            .await?,
    ))
}

async fn apps_save(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    validate_object(&body, 256 * 1024)?;
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::AppsSave { body },
    )
    .await
}

async fn apps_close(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::AppsClose,
    )
    .await
}

async fn apps_delete(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path((id, index)): Path<(String, u32)>,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    validate_index(index)?;
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::AppsDelete { index },
    )
    .await
}

async fn clients_list(
    State(state): State<WorkerState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        state
            .upstream
            .clients_list(&load_host(&state, &id).await?)
            .await?,
    ))
}

async fn clients_unpair(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UnpairRequest>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let uuid = validate_opaque("client uuid", &body.uuid, 128)?.to_string();
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::ClientsUnpair { uuid },
    )
    .await
}

async fn clients_unpair_all(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::ClientsUnpairAll,
    )
    .await
}

async fn clients_update(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ClientUpdateRequest>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let uuid = validate_opaque("client uuid", &body.uuid, 128)?.to_string();
    let enabled = body.enabled;
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::ClientsUpdate { uuid, enabled },
    )
    .await
}

async fn config_get(
    State(state): State<WorkerState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        state
            .upstream
            .config_get(&load_host(&state, &id).await?)
            .await?,
    ))
}

async fn config_save(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    validate_object(&body, 1024 * 1024)?;
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::ConfigSave { body },
    )
    .await
}

async fn config_locale(
    State(state): State<WorkerState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        state
            .upstream
            .config_locale(&load_host(&state, &id).await?)
            .await?,
    ))
}

async fn logs(State(state): State<WorkerState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    Ok(Json(
        state.upstream.logs(&load_host(&state, &id).await?).await?,
    ))
}

async fn pin(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PinRequest>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let pin = body.pin.trim().to_string();
    if !(4..=8).contains(&pin.len()) || !pin.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::BadRequest("PIN must contain 4-8 digits".into()));
    }
    let name = validate_opaque("client name", &body.name, 80)?.to_string();
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::Pin { pin, name },
    )
    .await
}

async fn restart(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::Restart,
    )
    .await
}

async fn reset_display(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    enqueue_remote(
        &state,
        &identity,
        &id,
        &headers,
        RemoteOperationRequest::ResetDisplay,
    )
    .await
}

async fn cover(
    State(state): State<WorkerState>,
    Path((id, index)): Path<(String, u32)>,
) -> AppResult<Response> {
    validate_index(index)?;
    let (upstream_type, bytes) = state
        .upstream
        .cover(&load_host(&state, &id).await?, index)
        .await?;
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, safe_cover_type(&upstream_type));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    Ok(response)
}

async fn cover_upload(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CoverUploadRequest>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let key = validate_opaque("cover key", &body.key, 512)?.to_string();
    let request = RemoteOperationRequest::CoverUpload { key, url: body.url };
    let idempotency_key = idempotency_key(&headers)?;
    if let Some(operation) = state
        .operations
        .find_idempotent(&identity.subject, &id, idempotency_key, &request)
        .await?
    {
        return Ok((StatusCode::ACCEPTED, Json(operation)));
    }
    let RemoteOperationRequest::CoverUpload { url, .. } = &request else {
        unreachable!();
    };
    state.cover_url_policy.validate(url).await?;
    enqueue_remote(&state, &identity, &id, &headers, request).await
}

async fn operation_get(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(operation_id): Path<String>,
) -> AppResult<Json<OperationView>> {
    if operation_id.is_empty() || operation_id.len() > 64 {
        return Err(AppError::NotFound("operation not found".into()));
    }
    Ok(Json(
        state
            .operations
            .get_for_actor(&identity.subject, &operation_id)
            .await?,
    ))
}

async fn enqueue_remote(
    state: &WorkerState,
    identity: &InternalIdentity,
    host_id: &str,
    headers: &HeaderMap,
    request: RemoteOperationRequest,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let key = idempotency_key(headers)?;
    let operation = state
        .operations
        .enqueue(&identity.subject, host_id, key, request)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(operation)))
}

fn idempotency_key(headers: &HeaderMap) -> AppResult<&str> {
    let mut values = headers.get_all("idempotency-key").iter();
    let value = values.next().ok_or_else(|| {
        AppError::BadRequest("Idempotency-Key is required for remote mutations".into())
    })?;
    if values.next().is_some() {
        return Err(AppError::BadRequest(
            "exactly one Idempotency-Key is required".into(),
        ));
    }
    let value = value
        .to_str()
        .map_err(|_| AppError::BadRequest("invalid Idempotency-Key".into()))?;
    validate_idempotency_key(value)?;
    Ok(value)
}

async fn load_host(state: &WorkerState, id: &str) -> AppResult<Host> {
    let host = db::get_host(&state.pool, &state.secrets, id).await?;
    if state.production && !host.verify_tls {
        return Err(AppError::BadRequest(
            "this host must enable TLS verification before use".into(),
        ));
    }
    Ok(host)
}

fn host_info(host: &Host, health: Option<&HealthSnapshot>) -> HostInfo {
    let health = health.cloned().unwrap_or_default();
    let complete = health.reachable.is_some() && health.connected.is_some();
    HostInfo {
        id: host.id.clone(),
        name: host.name.clone(),
        host: host.host.clone(),
        web_port: host.web_port,
        username: host.username.clone(),
        password_set: !host.password.is_empty(),
        verify_tls: host.verify_tls,
        web_url: web_url(host),
        probe_status: if complete {
            ProbeStatus::Complete
        } else {
            ProbeStatus::Pending
        },
        reachable: health.reachable,
        connected: health.connected,
        connection_error: health.connection_error,
    }
}

fn validate_object(value: &Value, limit: usize) -> AppResult<()> {
    if !value.is_object()
        || serde_json::to_vec(value)
            .map_err(|error| AppError::BadRequest(error.to_string()))?
            .len()
            > limit
    {
        return Err(AppError::BadRequest(
            "payload must be a JSON object within its size limit".into(),
        ));
    }
    Ok(())
}

fn validate_opaque<'a>(label: &str, value: &'a str, limit: usize) -> AppResult<&'a str> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > limit || value.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!("invalid {label}")));
    }
    Ok(value)
}

fn validate_index(index: u32) -> AppResult<()> {
    if index > 10_000 {
        Err(AppError::BadRequest(
            "Sunshine app index is out of range".into(),
        ))
    } else {
        Ok(())
    }
}

fn safe_cover_type(value: &str) -> HeaderValue {
    match value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/jpeg" => HeaderValue::from_static("image/jpeg"),
        "image/png" => HeaderValue::from_static("image/png"),
        "image/webp" => HeaderValue::from_static("image/webp"),
        "image/gif" => HeaderValue::from_static("image/gif"),
        "image/avif" => HeaderValue::from_static("image/avif"),
        _ => HeaderValue::from_static("application/octet-stream"),
    }
}

/// Run the process-owned health probe. A stale result is published only when
/// the complete host row is still current after the network round trip.
pub async fn probe_once(state: &WorkerState) -> AppResult<()> {
    let hosts = db::list_hosts(&state.pool, &state.secrets).await?;
    for host in hosts {
        let reachable = state.upstream.check_reachable(&host).await;
        let connection = if reachable {
            state
                .upstream
                .apps_list(&host)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        } else {
            Err("Sunshine Web port is not reachable".to_string())
        };
        let current = db::get_host(&state.pool, &state.secrets, &host.id).await;
        if current.as_ref().is_ok_and(|current| current == &host) {
            state.health.write().await.insert(
                host.id.clone(),
                HealthSnapshot {
                    reachable: Some(reachable),
                    connected: Some(reachable && connection.is_ok()),
                    connection_error: connection.err(),
                },
            );
        }
    }
    Ok(())
}

pub async fn probe_loop(state: WorkerState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = probe_once(&state).await {
            tracing::warn!(%error, "Sunshine health probe failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    fn test_static_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web")
    }

    async fn test_router() -> Router {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize_empty(&pool).await.unwrap();
        let state = WorkerState::new(
            pool,
            SecretBox::new("test", [2; 32]).unwrap(),
            InternalAuth::new(
                std::time::Duration::from_secs(60),
                std::time::Duration::from_secs(30),
                false,
            )
            .unwrap(),
            true,
            test_static_dir(),
        )
        .unwrap();
        router(state).layer(Extension(ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            42_000,
        )))))
    }

    #[tokio::test]
    async fn health_is_public() {
        let live = test_router()
            .await
            .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(live.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn private_route_requires_a_session_cookie() {
        let private = test_router()
            .await
            .oneshot(
                Request::get("/api/v2/sunshine/hosts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(private.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_route_is_public() {
        let response = test_router()
            .await
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"email":"admin@example.com","password":"bad-password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn old_and_unversioned_api_paths_are_not_routes() {
        for (method, path) in [
            (Method::POST, "/api/v1/auth/login"),
            (Method::GET, "/api/services/sunshine/hosts"),
            (Method::GET, "/api/sunshine/hosts"),
        ] {
            let response = test_router()
                .await
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            r#"{"email":"admin@example.com","password":"password"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn login_body_is_bounded_before_password_work() {
        let oversized = serde_json::json!({
            "email": "admin@example.com",
            "password": "x".repeat(20 * 1024)
        })
        .to_string();
        let response = test_router()
            .await
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn database_session_requires_csrf_and_logout_revokes_it() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize_empty(&pool).await.unwrap();
        db::ensure_admin_user(
            &pool,
            "admin@example.com",
            Some("correct horse battery staple"),
        )
        .await
        .unwrap();
        let state = WorkerState::new(
            pool.clone(),
            SecretBox::new("test", [2; 32]).unwrap(),
            InternalAuth::new(
                std::time::Duration::from_secs(60),
                std::time::Duration::from_secs(30),
                false,
            )
            .unwrap(),
            false,
            test_static_dir(),
        )
        .unwrap();
        let application = router(state).layer(Extension(ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            42_000,
        )))));

        let login = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"email":"admin@example.com","password":"correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().split(';').next().unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(cookie.contains("sunshine_session="));
        assert!(cookie.contains("sunshine_csrf="));
        let login_body: Value =
            serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let csrf = login_body["csrf_token"].as_str().unwrap().to_string();
        assert!(!csrf.is_empty());
        let stored_hash: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM auth_sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        let session_token = cookie
            .split("; ")
            .find_map(|value| value.strip_prefix("sunshine_session="))
            .unwrap();
        assert_eq!(stored_hash.len(), 32);
        assert_eq!(crate::auth::token_hash(session_token), stored_hash);
        assert_ne!(session_token.as_bytes(), stored_hash);

        let current = application
            .clone()
            .oneshot(
                Request::get("/api/v2/auth/session")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(current.status(), StatusCode::OK);

        let missing_csrf = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let missing_origin = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_origin.status(), StatusCode::FORBIDDEN);

        let logout = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);

        let revoked = application
            .oneshot(
                Request::get("/api/v2/auth/session")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn remote_mutation_requires_idempotency_and_returns_queryable_operation() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize_empty(&pool).await.unwrap();
        db::ensure_admin_user(
            &pool,
            "admin@example.com",
            Some("correct horse battery staple"),
        )
        .await
        .unwrap();
        let secrets = SecretBox::new("test", [12; 32]).unwrap();
        let host = db::insert_host(
            &pool,
            &secrets,
            HostSaveRequest {
                name: "Desktop".into(),
                host: "127.0.0.1".into(),
                web_port: 47_990,
                username: "sunshine".into(),
                password: Some("upstream-secret".into()),
                verify_tls: false,
            },
            false,
            "bootstrap",
        )
        .await
        .unwrap();
        let state = WorkerState::new(
            pool.clone(),
            secrets,
            InternalAuth::new(
                std::time::Duration::from_secs(60),
                std::time::Duration::from_secs(30),
                false,
            )
            .unwrap(),
            false,
            test_static_dir(),
        )
        .unwrap();
        let actor: String =
            sqlx::query_scalar("SELECT user_id FROM auth_users WHERE email='admin@example.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let existing_cover = state
            .operations
            .enqueue(
                &actor,
                &host.id,
                "cover-browser-1",
                RemoteOperationRequest::CoverUpload {
                    key: "cover-key".into(),
                    url: "https://covers.invalid/art.jpg?signature=stable-secret".into(),
                },
            )
            .await
            .unwrap();
        let application = router(state).layer(Extension(ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            42_000,
        )))));

        let login = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"email":"admin@example.com","password":"correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().split(';').next().unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        let login_body: Value =
            serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let csrf = login_body["csrf_token"].as_str().unwrap();
        let restart_url = format!("/api/v2/sunshine/hosts/{}/restart", host.id);

        let missing_key = application
            .clone()
            .oneshot(
                Request::post(&restart_url)
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_key.status(), StatusCode::BAD_REQUEST);

        let accepted = application
            .clone()
            .oneshot(
                Request::post(&restart_url)
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header("idempotency-key", "restart-browser-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::ACCEPTED);
        let accepted_body: Value =
            serde_json::from_slice(&accepted.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(accepted_body["state"], "pending");
        let operation_id = accepted_body["operation_id"].as_str().unwrap().to_string();

        let repeated = application
            .clone()
            .oneshot(
                Request::post(&restart_url)
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header("idempotency-key", "restart-browser-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(repeated.status(), StatusCode::ACCEPTED);
        let repeated_body: Value =
            serde_json::from_slice(&repeated.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(repeated_body["operation_id"], operation_id);

        let query = application
            .clone()
            .oneshot(
                Request::get(format!("/api/v2/sunshine/operations/{operation_id}"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(query.status(), StatusCode::OK);
        let query_body: Value =
            serde_json::from_slice(&query.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(query_body["operation_id"], operation_id);
        for forbidden in ["actor", "action", "request", "error_code", "error_message"] {
            assert!(query_body.get(forbidden).is_none());
        }

        let config_url = format!("/api/v2/sunshine/hosts/{}/config", host.id);
        for (body, expected) in [
            (r#"{"private":"first-value"}"#, StatusCode::ACCEPTED),
            (r#"{"private":"second-value"}"#, StatusCode::CONFLICT),
        ] {
            let response = application
                .clone()
                .oneshot(
                    Request::post(&config_url)
                        .header(header::COOKIE, &cookie)
                        .header("x-csrf-token", csrf)
                        .header(header::HOST, "localhost")
                        .header(header::ORIGIN, "http://localhost")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("idempotency-key", "config-browser-1")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        let config_ciphertext: String = sqlx::query_scalar(
            "SELECT request_ciphertext FROM operations WHERE action='sunshine.config.save'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!config_ciphertext.contains("first-value"));
        assert!(!config_ciphertext.contains("second-value"));

        let cover_url = format!("/api/v2/sunshine/hosts/{}/covers/upload", host.id);
        for (signature, expected) in [
            ("stable-secret", StatusCode::ACCEPTED),
            ("different-secret", StatusCode::CONFLICT),
        ] {
            let response = application
                .clone()
                .oneshot(
                    Request::post(&cover_url)
                        .header(header::COOKIE, &cookie)
                        .header("x-csrf-token", csrf)
                        .header(header::HOST, "localhost")
                        .header(header::ORIGIN, "http://localhost")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("idempotency-key", "cover-browser-1")
                        .body(Body::from(
                            serde_json::json!({
                                "key": "cover-key",
                                "url": format!(
                                    "https://covers.invalid/art.jpg?signature={signature}"
                                )
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::ACCEPTED {
                let body: Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                assert_eq!(body["operation_id"], existing_cover.operation_id);
            }
        }
    }
}
