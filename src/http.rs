use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use sarmg_admin_auth::AdministratorOriginMode;
use sarmg_admin_core::AdministratorService;
use sarmg_admin_sqlite::SqliteAdministratorStore;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::{
    client::UpstreamClient,
    cover_policy::CoverUrlPolicy,
    cover_proxy::CoverProxy,
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    model::{
        ClientUpdateRequest, CoverUploadRequest, HealthSnapshot, Host, HostInfo, HostPatchRequest,
        HostSaveRequest, HostStatus, OperationResolutionRequest, PinRequest, ProbeStatus,
        UnpairRequest, web_url,
    },
    operations::{
        HostMutationLocks, OperationManager, OperationView, RemoteOperationRequest,
        validate_idempotency_key,
    },
    release_contract::{API_NAMESPACE, API_VERSION_PREFIX},
};

#[derive(Clone)]
pub struct WorkerState {
    pub pool: SqlitePool,
    pub secrets: SecretBox,
    administrator_service: Arc<AdministratorService<SqliteAdministratorStore>>,
    administrator_origin_mode: AdministratorOriginMode,
    upstream: UpstreamClient,
    health: Arc<RwLock<HashMap<String, HealthSnapshot>>>,
    operations: OperationManager,
    cover_url_policy: CoverUrlPolicy,
    cover_proxy: CoverProxy,
    static_dir: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InternalIdentity {
    subject: String,
}

impl WorkerState {
    pub fn new(
        pool: SqlitePool,
        secrets: SecretBox,
        production: bool,
        static_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let administrator_service = Arc::new(AdministratorService::new(
            SqliteAdministratorStore::new(pool.clone()),
        ));
        let administrator_origin_mode = if production {
            AdministratorOriginMode::ProductionHttps
        } else {
            AdministratorOriginMode::LoopbackDevelopmentHttp
        };
        let upstream = UpstreamClient::new()?;
        let mutation_locks = HostMutationLocks::default();
        let operations = OperationManager::new(
            pool.clone(),
            secrets.clone(),
            mutation_locks,
            upstream.clone(),
        );
        Ok(Self {
            pool,
            secrets,
            administrator_service,
            administrator_origin_mode,
            upstream,
            health: Arc::new(RwLock::new(HashMap::new())),
            operations,
            cover_url_policy: CoverUrlPolicy::default(),
            cover_proxy: CoverProxy::disabled(),
            static_dir,
        })
    }

    pub fn with_cover_url_policy(mut self, policy: CoverUrlPolicy) -> Self {
        self.operations = self.operations.with_cover_url_policy(policy.clone());
        self.cover_url_policy = policy;
        self
    }

    pub fn with_cover_delivery(mut self, policy: CoverUrlPolicy, proxy: CoverProxy) -> Self {
        self.operations = self
            .operations
            .with_cover_delivery(policy.clone(), proxy.clone());
        self.cover_url_policy = policy;
        self.cover_proxy = proxy;
        self
    }

    pub fn operation_manager(&self) -> &OperationManager {
        &self.operations
    }
}

pub fn router(
    state: WorkerState,
    runtime: sarmg_server_runtime::RuntimeHandle,
) -> anyhow::Result<Router> {
    let public_api = Router::new()
        .route(
            "/sunshine/internal/hosts/{host_id}/operations/{operation_id}/covers/{token}",
            get(cover_delivery),
        )
        .layer(DefaultBodyLimit::max(16 * 1024));

    let protected_api = Router::new()
        .route("/sunshine/operations/{operation_id}", get(operation_get))
        .route(
            "/sunshine/operations/{operation_id}/resolve",
            post(operation_resolve),
        )
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
        .nest(API_VERSION_PREFIX, current_api)
        .fallback(|| async { StatusCode::NOT_FOUND });

    let static_dir = state.static_dir.clone();
    let platform = sarmg_server_runtime::platform_router(
        runtime,
        "sunshine-manager",
        state.administrator_origin_mode,
        Arc::clone(&state.administrator_service),
    )?;
    Ok(Router::new()
        .nest(API_NAMESPACE, api_namespace)
        .fallback_service(ServeDir::new(static_dir))
        .with_state(state)
        .merge(platform))
}

async fn authenticate(
    State(state): State<WorkerState>,
    mut request: Request,
    next: Next,
) -> Response {
    let identity = match sarmg_admin_axum::authenticate_request(
        &state.administrator_service,
        request.headers(),
        request.uri(),
        request.method(),
        "sunshine-manager",
        state.administrator_origin_mode,
    )
    .await
    {
        Ok(identity) => identity,
        Err(response) => return *response,
    };
    request.extensions_mut().insert(InternalIdentity {
        subject: identity.administrator_id.to_string(),
    });
    next.run(request).await
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
    let host = db::insert_host(&state.pool, &state.secrets, request, &identity.subject).await?;
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
    let host =
        db::update_host(&state.pool, &state.secrets, &id, request, &identity.subject).await?;
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

async fn cover_delivery(
    State(state): State<WorkerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((host_id, operation_id, token)): Path<(String, String, String)>,
) -> AppResult<Response> {
    let cover = state
        .cover_proxy
        .take(&host_id, &operation_id, &token, peer.ip())?;
    let mut response = cover.bytes.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(cover.content_type),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private, max-age=0"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    Ok(response)
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

async fn operation_resolve(
    State(state): State<WorkerState>,
    Extension(identity): Extension<InternalIdentity>,
    Path(operation_id): Path<String>,
    Json(body): Json<OperationResolutionRequest>,
) -> AppResult<Json<OperationView>> {
    Ok(Json(
        state
            .operations
            .resolve_for_actor(&identity.subject, &operation_id, body.resolution)
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
    db::get_host(&state.pool, &state.secrets, id).await
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
    use axum::http::Method;
    use http_body_util::BodyExt;
    use sarmg_error::ErrorEnvelope;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    fn test_static_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("clients/web")
    }

    fn test_runtime() -> sarmg_server_runtime::RuntimeHandle {
        sarmg_server_runtime::platform_handle(sarmg_server_runtime::ProductDescriptor {
            id: "sunshine-manager".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            foundation_revision: "1e889d08fa69fcf2b5fffe45e8cc42b68218f4f1".to_owned(),
            profile: "server-control-plane".to_owned(),
            capabilities: vec!["server-runtime".to_owned()],
        })
        .unwrap()
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
            false,
            test_static_dir(),
        )
        .unwrap();
        router(state, test_runtime())
            .unwrap()
            .layer(Extension(ConnectInfo(SocketAddr::from((
                [127, 0, 0, 1],
                42_000,
            )))))
    }

    #[tokio::test]
    async fn health_is_public() {
        let live = test_router()
            .await
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(live.status(), StatusCode::NO_CONTENT);
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
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::from(
                        r#"{"username":"admin","password":"bad-password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_uses_one_unambiguous_host_or_uri_authority() {
        let application = test_router().await;
        let body = r#"{"username":"admin","password":"bad-password"}"#;

        let authority_only = Request::post("http://localhost/api/v2/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, "http://localhost")
            .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(
            application
                .clone()
                .oneshot(authority_only)
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let ambiguous = Request::post("http://localhost/api/v2/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, "http://localhost")
            .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(
            application.oneshot(ambiguous).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn login_policy_violations_share_the_credentials_failure() {
        for body in [
            r#"{"username":"admin@example.com","password":"correct-password"}"#,
            r#"{"username":"admin","password":"too-short"}"#,
        ] {
            let response = test_router()
                .await
                .oneshot(
                    Request::post("/api/v2/auth/login")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(header::HOST, "localhost")
                        .header(header::ORIGIN, "http://localhost")
                        .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn email_field_is_rejected_instead_of_becoming_a_login_alias() {
        let response = test_router()
            .await
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::from(
                        r#"{"email":"admin","password":"correct-password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn old_and_unversioned_api_paths_are_not_routes() {
        for (method, path) in [
            (Method::POST, "/api/v1/auth/login"),
            (Method::POST, "/api/v2/sunshine/operations/removed/retry"),
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
                        .body(Body::from(r#"{"username":"admin","password":"password"}"#))
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
            "username": "admin",
            "password": "x".repeat(20 * 1024)
        })
        .to_string();
        let response = test_router()
            .await
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
        AdministratorService::new(SqliteAdministratorStore::new(pool.clone()))
            .bootstrap_administrator(
                "admin",
                "correct horse battery staple",
                u64::try_from(db::now_micros().unwrap()).unwrap(),
            )
            .await
            .unwrap();
        let state = WorkerState::new(
            pool.clone(),
            SecretBox::new("test", [2; 32]).unwrap(),
            false,
            test_static_dir(),
        )
        .unwrap();
        let application = router(state, test_runtime())
            .unwrap()
            .layer(Extension(ConnectInfo(SocketAddr::from((
                [127, 0, 0, 1],
                42_000,
            )))));

        let login = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::from(
                        r#"{"username":" Admin ","password":"correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        assert_eq!(
            login.headers()[header::CACHE_CONTROL],
            "no-store, private, max-age=0"
        );
        let cookie = login
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().split(';').next().unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(cookie.contains("sarmg-sunshine-manager-session="));
        assert!(!cookie.contains("sunshine_csrf="));
        let login_body: Value =
            serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let mut session_keys = login_body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        session_keys.sort_unstable();
        assert_eq!(
            session_keys,
            ["authenticated", "csrf_token", "role", "user_id", "username"]
        );
        assert_eq!(login_body["authenticated"], true);
        assert_eq!(login_body["username"], "admin");
        assert_eq!(login_body["role"], "admin");
        let csrf = login_body["csrf_token"].as_str().unwrap().to_string();
        assert!(!csrf.is_empty());
        let stored_hash: Vec<u8> =
            sqlx::query_scalar("SELECT token_hash FROM _sarmg_admin_sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
        let session_token = cookie
            .split("; ")
            .find_map(|value| value.strip_prefix("sarmg-sunshine-manager-session="))
            .unwrap();
        assert_eq!(stored_hash.len(), 32);
        assert_eq!(
            sarmg_admin_auth::token_hash(session_token).as_slice(),
            stored_hash
        );
        assert_ne!(session_token.as_bytes(), stored_hash);

        for (name, value) in [
            (sarmg_admin_auth::ORIGIN_HEADER, "http://localhost"),
            (sarmg_admin_auth::HOST_HEADER, "localhost"),
            (sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin"),
            (sarmg_admin_auth::CSRF_HEADER, csrf.as_str()),
        ] {
            let mut request = Request::post("/api/v2/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(sarmg_admin_auth::CSRF_HEADER, &csrf)
                .header(header::HOST, "localhost")
                .header(header::ORIGIN, "http://localhost")
                .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                .body(Body::empty())
                .unwrap();
            request.headers_mut().append(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
            let response = application.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{name}");
        }

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
        let current_body: Value =
            serde_json::from_slice(&current.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let refreshed_csrf = current_body["csrf_token"].as_str().unwrap().to_string();
        assert_ne!(refreshed_csrf, csrf);

        let superseded_csrf = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(superseded_csrf.status(), StatusCode::FORBIDDEN);

        let missing_csrf = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
        let request_id = missing_csrf.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let envelope: ErrorEnvelope =
            serde_json::from_slice(&missing_csrf.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(envelope.code.as_str(), "auth.csrf_rejected");
        assert!(!envelope.retryable);
        assert_eq!(envelope.request_id.unwrap().as_str(), request_id);
        assert!(envelope.details.is_empty());

        let missing_origin = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &refreshed_csrf)
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
                    .header("x-csrf-token", &refreshed_csrf)
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
        let envelope: ErrorEnvelope =
            serde_json::from_slice(&revoked.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(envelope.code.as_str(), "auth.session_required");
        assert!(!envelope.retryable);
    }

    #[tokio::test]
    async fn remote_mutation_requires_idempotency_and_returns_queryable_operation() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize_empty(&pool).await.unwrap();
        AdministratorService::new(SqliteAdministratorStore::new(pool.clone()))
            .bootstrap_administrator(
                "admin",
                "correct horse battery staple",
                u64::try_from(db::now_micros().unwrap()).unwrap(),
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
            },
            "bootstrap",
        )
        .await
        .unwrap();
        let state = WorkerState::new(pool.clone(), secrets, false, test_static_dir()).unwrap();
        let actor: String = sqlx::query_scalar(
            "SELECT administrator_id FROM _sarmg_administrators WHERE username='admin'",
        )
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
        let application = router(state, test_runtime())
            .unwrap()
            .layer(Extension(ConnectInfo(SocketAddr::from((
                [127, 0, 0, 1],
                42_000,
            )))));

        let login = application
            .clone()
            .oneshot(
                Request::post("/api/v2/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                    .body(Body::from(
                        r#"{"username":"admin","password":"correct horse battery staple"}"#,
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
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
                    .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
                        .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("idempotency-key", "config-browser-1")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        let config_payload: Vec<u8> = sqlx::query_scalar(
            "SELECT request_payload FROM _sarmg_operations WHERE action='sunshine.config.save'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let config_payload = String::from_utf8(config_payload).unwrap();
        assert!(!config_payload.contains("first-value"));
        assert!(!config_payload.contains("second-value"));

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
                        .header(sarmg_admin_auth::SEC_FETCH_SITE_HEADER, "same-origin")
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
