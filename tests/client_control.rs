use sunshine_client_protocol::{Command, Rejection, Report, Uncertainty};
use sunshine_manager::{
    crypto::SecretBox,
    db,
    operations::{OperationManager, OperationState},
};
use uuid::Uuid;

async fn database() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = db::open_or_initialize(&format!(
        "sqlite://{}",
        dir.path().join("state.db").display()
    ))
    .await
    .unwrap();
    (dir, pool)
}
async fn registered(pool: &sqlx::SqlitePool) -> (String, String) {
    let ticket = db::create_device(pool, "测试设备", "admin").await.unwrap();
    let credential = db::random_token();
    db::enroll(
        pool,
        &ticket.device.id,
        Uuid::new_v4(),
        &ticket.token,
        &credential,
    )
    .await
    .unwrap();
    (ticket.device.id, credential)
}
async fn session(pool: &sqlx::SqlitePool, id: &str, session: &str) {
    sqlx::query("UPDATE devices SET session_id=?,last_seen_at_micros=? WHERE device_id=?")
        .bind(session)
        .bind(db::now_micros().unwrap())
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}
fn manager(pool: &sqlx::SqlitePool) -> OperationManager {
    OperationManager::new(pool.clone(), SecretBox::new("test", [7; 32]).unwrap())
}

#[tokio::test]
async fn pairing_code_resolves_only_active_unconsumed_devices() {
    let (_dir, pool) = database().await;
    let ticket = db::create_device(&pool, "pair", "admin").await.unwrap();
    let target = db::resolve_pairing(&pool, &ticket.token).await.unwrap();
    assert_eq!(target["device_id"], ticket.device.id);
    assert_eq!(target["manager_id"], ticket.manager_id.to_string());
    assert!(db::resolve_pairing(&pool, "bad").await.is_err());
    assert!(
        db::resolve_pairing(&pool, &db::random_token())
            .await
            .is_err()
    );
    db::enroll(
        &pool,
        &ticket.device.id,
        Uuid::new_v4(),
        &ticket.token,
        &db::random_token(),
    )
    .await
    .unwrap();
    assert!(db::resolve_pairing(&pool, &ticket.token).await.is_err());
    let cancelled = db::create_device(&pool, "cancel", "admin").await.unwrap();
    db::cancel_pairing(&pool, &cancelled.device.id, "admin")
        .await
        .unwrap();
    assert!(db::resolve_pairing(&pool, &cancelled.token).await.is_err());
    let revoked = db::create_device(&pool, "revoke", "admin").await.unwrap();
    db::revoke(&pool, &revoked.device.id, "admin")
        .await
        .unwrap();
    assert!(db::resolve_pairing(&pool, &revoked.token).await.is_err());
}
#[tokio::test]
async fn enrollment_is_single_use_hashed_bound_and_revocable() {
    let (_dir, pool) = database().await;
    let ticket = db::create_device(&pool, "设备", "admin").await.unwrap();
    let installation = Uuid::new_v4();
    let credential = db::random_token();
    assert!(
        db::enroll(
            &pool,
            &ticket.device.id,
            installation,
            &db::random_token(),
            &credential
        )
        .await
        .is_err()
    );
    let binding = db::enroll(
        &pool,
        &ticket.device.id,
        installation,
        &ticket.token,
        &credential,
    )
    .await
    .unwrap();
    assert_eq!(binding.manager_id, ticket.manager_id);
    assert_eq!(binding.installation_id, installation);
    assert!(
        db::enroll(
            &pool,
            &ticket.device.id,
            installation,
            &ticket.token,
            &credential
        )
        .await
        .is_err()
    );
    let stored = db::authenticate_device(&pool, &credential).await.unwrap();
    assert_eq!(
        stored.credential_hash,
        Some(db::token_hash(&credential).to_vec())
    );
    assert!(stored.enrollment_hash.is_none());
    db::revoke(&pool, &ticket.device.id, "admin").await.unwrap();
    assert!(db::authenticate_device(&pool, &credential).await.is_err());
    assert!(
        db::cancel_pairing(&pool, &ticket.device.id, "admin")
            .await
            .is_err()
    );
    let audit: String =
        sqlx::query_scalar("SELECT COALESCE(group_concat(detail),'') FROM audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!audit.contains(&credential) && !audit.contains(&ticket.token));
}

#[tokio::test]
async fn pairing_has_no_expiry_and_duplicate_installation_is_rejected() {
    let (_dir, pool) = database().await;
    let first = db::create_device(&pool, "first", "admin").await.unwrap();
    sqlx::query("UPDATE devices SET created_at_micros=0,updated_at_micros=0")
        .execute(&pool)
        .await
        .unwrap();
    let installation = Uuid::new_v4();
    db::enroll(
        &pool,
        &first.device.id,
        installation,
        &first.token,
        &db::random_token(),
    )
    .await
    .unwrap();
    let other = db::create_device(&pool, "second", "admin").await.unwrap();
    assert!(
        db::enroll(
            &pool,
            &other.device.id,
            installation,
            &other.token,
            &db::random_token()
        )
        .await
        .is_err()
    );
    assert!(
        db::create_device(&pool, &"长".repeat(33), "admin")
            .await
            .is_err()
    );
    db::cancel_pairing(&pool, &other.device.id, "admin")
        .await
        .unwrap();
    assert!(
        !db::get_device(&pool, &other.device.id)
            .await
            .unwrap()
            .view(db::now_micros().unwrap())
            .pairing_pending
    );
    assert!(
        db::enroll(
            &pool,
            &other.device.id,
            Uuid::new_v4(),
            &other.token,
            &db::random_token()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn claims_are_serial_session_fenced_and_revocation_stops_new_work() {
    let (_dir, pool) = database().await;
    let (id, _) = registered(&pool).await;
    let ops = manager(&pool);
    let a = ops
        .enqueue("admin", &id, "one", Command::ReadConfig {})
        .await
        .unwrap();
    assert_eq!(a.state, OperationState::Pending);
    assert!(ops.next(&id, "offline").await.is_err());
    let again = ops
        .enqueue("admin", &id, "one", Command::ReadConfig {})
        .await
        .unwrap();
    assert_eq!(a.operation_id, again.operation_id);
    // Idempotency is scoped by actor/device/action; compare different bodies of the same action.
    ops.enqueue(
        "admin",
        &id,
        "restart",
        Command::Restart {
            expected_revision: "a".repeat(64),
            administrator_confirmed: true,
        },
    )
    .await
    .unwrap();
    assert!(
        ops.enqueue(
            "admin",
            &id,
            "restart",
            Command::Restart {
                expected_revision: "b".repeat(64),
                administrator_confirmed: true
            }
        )
        .await
        .is_err()
    );
    ops.enqueue("admin", &id, "two", Command::ReadConfig {})
        .await
        .unwrap();
    session(&pool, &id, "s1").await;
    let claimed = ops.next(&id, "s1").await.unwrap().unwrap();
    assert!(ops.next(&id, "s1").await.unwrap().is_none());
    session(&pool, &id, "s2").await;
    assert!(
        ops.complete(
            &claimed,
            "s1",
            Report::Rejected {
                reason: Rejection::SunshineUnavailable
            }
        )
        .await
        .is_err()
    );
    ops.disconnected(&claimed).await.unwrap();
    assert_eq!(
        ops.get_for_actor("admin", &a.operation_id)
            .await
            .unwrap()
            .state,
        OperationState::Unknown
    );
    assert!(ops.next(&id, "s2").await.unwrap().is_none());
    db::revoke(&pool, &id, "admin").await.unwrap();
    assert!(ops.next(&id, "s2").await.is_err());
    assert!(
        ops.enqueue("admin", &id, "three", Command::ReadConfig {})
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unknown_reconciliation_is_evidence_not_a_second_state_machine() {
    let (_dir, pool) = database().await;
    let (id, _) = registered(&pool).await;
    let ops = manager(&pool);
    let queued = ops
        .enqueue("admin", &id, "one", Command::ReadConfig {})
        .await
        .unwrap();
    session(&pool, &id, "s1").await;
    let claimed = ops.next(&id, "s1").await.unwrap().unwrap();
    assert!(
        ops.get_for_actor("other", &queued.operation_id)
            .await
            .is_err()
    );
    ops.disconnected(&claimed).await.unwrap();
    let unknown = ops.uncertain(&id).await.unwrap().unwrap();
    ops.complete(
        &unknown,
        "s1",
        Report::Unknown {
            reason: Uncertainty::NoExecutionRecord,
        },
    )
    .await
    .unwrap();
    let view = ops
        .get_for_actor("admin", &queued.operation_id)
        .await
        .unwrap();
    assert_eq!(view.state, OperationState::Unknown);
    assert!(view.reconciliation.is_some());
    assert_eq!(
        ops.task(&claimed).unwrap().binding.device_id.to_string(),
        id
    );
    ops.recover_startup().await.unwrap();
    assert!(
        !db::get_device(&pool, &id)
            .await
            .unwrap()
            .view(db::now_micros().unwrap())
            .client_online
    );
}

#[tokio::test]
async fn connection_health_and_effectiveness_are_not_conflated() {
    let (_dir, pool) = database().await;
    let (id, _) = registered(&pool).await;
    session(&pool, &id, "s").await;
    let device = db::get_device(&pool, &id).await.unwrap();
    let view = device.view(db::now_micros().unwrap());
    assert!(view.client_online);
    assert_eq!(view.sunshine_reachable, None);
    assert_eq!(view.configuration_state, "unknown");
    let stale = device.view(db::now_micros().unwrap() + 46_000_000);
    assert!(!stale.client_online);
    assert_eq!(stale.sunshine_reachable, None);
}
