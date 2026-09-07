#[path = "../foundation/platform_router.rs"]
mod foundation_platform;

use crate::{
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    model::{DeviceName, DeviceView, OperationResolutionRequest},
    operations::{OperationManager, OperationView},
    release_contract::{API_NAMESPACE, API_VERSION_PREFIX},
};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, Extension, Path, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use sarmg_admin_auth::AdministratorOriginMode;
use sarmg_admin_core::AdministratorService;
use sarmg_admin_sqlite::SqliteAdministratorStore;
use serde::Deserialize;
use std::{collections::HashSet, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use sunshine_client_protocol::{
    Binding, ClientMessage, Command, DeliveryMode, MAX_MESSAGE_BYTES, ManagerMessage, PROTOCOL,
    SUNSHINE_VERSION, WEBSOCKET_SUBPROTOCOL,
};
use tokio::{
    sync::Semaphore,
    time::{Instant, timeout},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct WorkerState {
    pub pool: sqlx::SqlitePool,
    pub secrets: SecretBox,
    administrator_service: Arc<AdministratorService<SqliteAdministratorStore>>,
    administrator_origin_mode: AdministratorOriginMode,
    operations: OperationManager,
    static_dir: PathBuf,
    client_slots: Arc<Semaphore>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct InternalIdentity {
    subject: String,
}
impl WorkerState {
    pub fn new(
        pool: sqlx::SqlitePool,
        secrets: SecretBox,
        production: bool,
        static_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            administrator_service: Arc::new(AdministratorService::new(
                SqliteAdministratorStore::new(pool.clone()),
            )),
            administrator_origin_mode: if production {
                AdministratorOriginMode::ProductionHttps
            } else {
                AdministratorOriginMode::LoopbackDevelopmentHttp
            },
            operations: OperationManager::new(pool.clone(), secrets.clone()),
            pool,
            secrets,
            static_dir,
            client_slots: Arc::new(Semaphore::new(256)),
        })
    }
    pub fn operation_manager(&self) -> &OperationManager {
        &self.operations
    }
}
pub fn router(
    state: WorkerState,
    runtime: sarmg_server_runtime::RuntimeHandle,
) -> anyhow::Result<Router> {
    let protected = Router::new()
        .route("/sunshine/devices", get(devices).post(create_device))
        .route("/sunshine/devices/{id}", patch(rename_device))
        .route(
            "/sunshine/devices/{id}/pairing",
            axum::routing::delete(cancel_pairing),
        )
        .route("/sunshine/devices/{id}/revoke", post(revoke_device))
        .route(
            "/sunshine/devices/{id}/tasks",
            get(device_tasks).post(submit_task),
        )
        .route("/sunshine/operations/{id}", get(operation_get))
        .route("/sunshine/operations/{id}/resolve", post(operation_resolve))
        .layer(DefaultBodyLimit::max(MAX_MESSAGE_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate));
    let api = Router::new()
        .nest(API_VERSION_PREFIX, protected)
        .fallback(|| async { StatusCode::NOT_FOUND });
    let platform = foundation_platform::platform_router(
        runtime,
        "sunshine-manager",
        state.administrator_origin_mode,
        Arc::clone(&state.administrator_service),
    )?;
    Ok(Router::new()
        .nest(API_NAMESPACE, api)
        .route("/sunshine-client/v1/enroll", post(enroll))
        .route("/sunshine-client/v1/identity", get(client_identity))
        .route("/sunshine-client/v1/connect", get(client_connect))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .fallback_service(ServeDir::new(state.static_dir.clone()))
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
async fn devices(State(state): State<WorkerState>) -> AppResult<Json<Vec<DeviceView>>> {
    Ok(Json(db::list_devices(&state.pool).await?))
}
async fn create_device(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Json(value): Json<DeviceName>,
) -> AppResult<Response> {
    let ticket = db::create_device(&state.pool, &value.name, &actor.subject).await?;
    Ok((
        StatusCode::CREATED,
        [("cache-control", "no-store")],
        Json(ticket),
    )
        .into_response())
}
async fn cancel_pairing(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    db::cancel_pairing(&state.pool, &id, &actor.subject).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn rename_device(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
    Json(value): Json<DeviceName>,
) -> AppResult<Json<DeviceView>> {
    Ok(Json(
        db::rename(&state.pool, &id, &value.name, &actor.subject).await?,
    ))
}
async fn revoke_device(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    db::revoke(&state.pool, &id, &actor.subject).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn device_tasks(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<OperationView>>> {
    Ok(Json(
        state.operations.list_for_actor(&actor.subject, &id).await?,
    ))
}
async fn submit_task(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> AppResult<(StatusCode, Json<OperationView>)> {
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("缺少 Idempotency-Key".into()))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            state
                .operations
                .enqueue(&actor.subject, &id, key, command)
                .await?,
        ),
    ))
}
async fn operation_get(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
) -> AppResult<Json<OperationView>> {
    Ok(Json(
        state.operations.get_for_actor(&actor.subject, &id).await?,
    ))
}
async fn operation_resolve(
    State(state): State<WorkerState>,
    Extension(actor): Extension<InternalIdentity>,
    Path(id): Path<String>,
    Json(value): Json<OperationResolutionRequest>,
) -> AppResult<Json<OperationView>> {
    Ok(Json(
        state
            .operations
            .resolve_for_actor(&actor.subject, &id, value.resolution)
            .await?,
    ))
}

// Only a local trusted TLS ingress may supply this assertion. The server bind is loopback-only.
// Reject browser cookies/Origin on the independent device channel.
fn require_client_ingress(peer: SocketAddr, headers: &HeaderMap) -> AppResult<()> {
    if !peer.ip().is_loopback()
        || headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            != Some("https")
        || headers.contains_key("origin")
        || headers.contains_key("cookie")
    {
        return Err(AppError::Forbidden(
            "Client requires trusted HTTPS/WSS ingress".into(),
        ));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentRequest {
    device_id: uuid::Uuid,
    installation_id: uuid::Uuid,
    token: String,
    credential: String,
}
async fn enroll(
    State(state): State<WorkerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(value): Json<EnrollmentRequest>,
) -> AppResult<Response> {
    require_client_ingress(peer, &headers)?;
    let binding = db::enroll(
        &state.pool,
        &value.device_id.to_string(),
        value.installation_id,
        &value.token,
        &value.credential,
    )
    .await?;
    Ok(([("cache-control", "no-store")], Json(binding)).into_response())
}
async fn client_identity(
    State(state): State<WorkerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Response> {
    require_client_ingress(peer, &headers)?;
    let credential = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    let device = db::authenticate_device(&state.pool, credential).await?;
    Ok((
        [("cache-control", "no-store")],
        Json(db::binding(&state.pool, &device).await?),
    )
        .into_response())
}
async fn client_connect(
    State(state): State<WorkerState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> AppResult<Response> {
    require_client_ingress(peer, &headers)?;
    let credential = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    let device = db::authenticate_device(&state.pool, credential).await?;
    let binding = db::binding(&state.pool, &device).await?;
    if headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        != Some(WEBSOCKET_SUBPROTOCOL)
    {
        return Err(AppError::BadRequest("Client 子协议不支持".into()));
    }
    let permit = state
        .client_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::TooManyRequests { retry_after: 30 })?;
    Ok(upgrade
        .protocols([WEBSOCKET_SUBPROTOCOL])
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            let mut pending = None;
            let session = uuid::Uuid::new_v4().to_string();
            let _ = serve_client(&state, socket, &binding, &session, &mut pending).await;
            if let Some(operation) = pending {
                let _ = state.operations.disconnected(&operation).await;
            }
            let _ = sqlx::query(
                "UPDATE devices SET session_id=NULL WHERE device_id=? AND session_id=?",
            )
            .bind(binding.device_id.to_string())
            .bind(&session)
            .execute(&state.pool)
            .await;
        }))
}
async fn receive(socket: &mut WebSocket) -> AppResult<ClientMessage> {
    let frame = timeout(Duration::from_secs(10), socket.recv())
        .await
        .map_err(|_| AppError::Unauthorized)?
        .ok_or(AppError::Unauthorized)?
        .map_err(|_| AppError::Unauthorized)?;
    let Message::Text(text) = frame else {
        return Err(AppError::Unauthorized);
    };
    serde_json::from_str(&text).map_err(|_| AppError::BadRequest("Client 消息无效".into()))
}
async fn send(socket: &mut WebSocket, message: ManagerMessage) -> AppResult<()> {
    let text = serde_json::to_string(&message)
        .map_err(|_| AppError::BadRequest("Client 指令无效".into()))?;
    timeout(
        Duration::from_secs(10),
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| AppError::Unauthorized)?
    .map_err(|_| AppError::Unauthorized)
}
async fn serve_client(
    state: &WorkerState,
    mut socket: WebSocket,
    binding: &Binding,
    session: &str,
    pending: &mut Option<sarmg_operations::StoredOperation>,
) -> AppResult<()> {
    let ClientMessage::Hello {
        binding: hello,
        capabilities,
    } = receive(&mut socket).await?
    else {
        return Err(AppError::Unauthorized);
    };
    if &hello != binding
        || capabilities.protocol != PROTOCOL
        || capabilities.sunshine_version != SUNSHINE_VERSION
        || capabilities.client_version.len() > 64
        || capabilities.managed_fields.len() != sunshine_client_protocol::config::FIELDS.len()
        || sunshine_client_protocol::config::FIELDS
            .iter()
            .any(|field| {
                !capabilities
                    .managed_fields
                    .iter()
                    .any(|value| value == field)
            })
    {
        return Err(AppError::Unauthorized);
    }
    let id = binding.device_id.to_string();
    let changed=sqlx::query("UPDATE devices SET session_id=?,last_seen_at_micros=?,capabilities_json=? WHERE device_id=? AND installation_id=? AND revoked_at_micros IS NULL AND credential_hash IS NOT NULL")
        .bind(session).bind(db::now_micros()?).bind(serde_json::to_string(&capabilities).map_err(|_|AppError::Unauthorized)?).bind(&id).bind(binding.installation_id.to_string()).execute(&state.pool).await?.rows_affected();
    if changed != 1 {
        return Err(AppError::Unauthorized);
    }
    let mut poll = tokio::time::interval(Duration::from_secs(1));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_seen = Instant::now();
    let mut dispatched = Instant::now();
    let mut inspected = HashSet::new();
    loop {
        tokio::select! {
            message=socket.recv()=>{
                let frame=message.ok_or(AppError::Unauthorized)?.map_err(|_|AppError::Unauthorized)?;
                let current=db::get_device(&state.pool,&id).await?;
                if current.revoked_at_micros.is_some()||current.session_id.as_deref()!=Some(session){let _=send(&mut socket,ManagerMessage::Revoked{}).await;return Err(AppError::Unauthorized);}
                last_seen=Instant::now();
                sqlx::query("UPDATE devices SET last_seen_at_micros=? WHERE device_id=? AND session_id=?").bind(db::now_micros()?).bind(&id).bind(session).execute(&state.pool).await?;
                match frame{
                    Message::Ping(bytes)=>{timeout(Duration::from_secs(10),socket.send(Message::Pong(bytes))).await.map_err(|_|AppError::Unauthorized)?.map_err(|_|AppError::Unauthorized)?;}
                    Message::Pong(_)=>{}
                    Message::Text(text)=>{
                        let message:ClientMessage=serde_json::from_str(&text).map_err(|_|AppError::BadRequest("Client 消息无效".into()))?;
                        match message{
                            ClientMessage::Heartbeat{sunshine_reachable,configuration}=>{
                                if let Some(snapshot)=&configuration{crate::operations::validate_snapshot(snapshot)?;}
                                sqlx::query("UPDATE devices SET sunshine_reachable=?,health_at_micros=?,snapshot_json=COALESCE(?,snapshot_json) WHERE device_id=? AND session_id=? AND revoked_at_micros IS NULL")
                                    .bind(sunshine_reachable).bind(db::now_micros()?).bind(configuration.as_ref().map(serde_json::to_string).transpose().map_err(|_|AppError::Unauthorized)?).bind(&id).bind(session).execute(&state.pool).await?;
                            }
                            ClientMessage::Result{operation_id,report}=>{
                                let operation=pending.as_ref().filter(|op|op.operation.operation_id==operation_id).ok_or_else(||AppError::BadRequest("Client 结果未匹配当前任务".into()))?;
                                state.operations.complete(operation,session,report).await?;
                                *pending=None;
                            }
                            _=>return Err(AppError::BadRequest("重复 Client Hello".into())),
                        }
                    }
                    _=>return Err(AppError::Unauthorized),
                }
            }
            _=poll.tick()=>{
                if last_seen.elapsed()>Duration::from_secs(45)||(pending.is_some()&&dispatched.elapsed()>Duration::from_secs(110)){return Err(AppError::Unauthorized);}
                let current=db::get_device(&state.pool,&id).await?;
                if current.revoked_at_micros.is_some()||current.session_id.as_deref()!=Some(session){let _=send(&mut socket,ManagerMessage::Revoked{}).await;return Err(AppError::Unauthorized);}
                if pending.is_none(){
                    let (operation,mode)=if let Some(unknown)=state.operations.uncertain(&id).await?{
                        if inspected.contains(&unknown.operation.operation_id){continue;}
                        inspected.insert(unknown.operation.operation_id.clone());(Some(unknown),DeliveryMode::InspectOnly)
                    }else{(state.operations.next(&id,session).await?,DeliveryMode::Execute)};
                    if let Some(operation)=operation{
                        let task=state.operations.task(&operation)?;
                        *pending=Some(operation);dispatched=Instant::now();
                        send(&mut socket,ManagerMessage::Task{mode,task}).await?;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Method;
    use axum::http::{HeaderValue, header};
    use http_body_util::BodyExt;
    use sarmg_error::ErrorEnvelope;
    use serde_json::Value;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    fn test_static_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web")
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
                Request::get("/api/v2/sunshine/devices")
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
}
