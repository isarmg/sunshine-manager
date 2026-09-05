use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sarmg_admin_core::AdministratorStore;
use sunshine_manager::{
    ServeConfig, db,
    http::{WorkerState, probe_loop, router},
    release_bundle, release_contract,
    runtime_lock::{ApplicationLock, MaintenanceLock},
};

#[derive(Parser)]
#[command(
    name = "sunshine-manager",
    version,
    about = "Independent Sunshine manager"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve a development build (the default command; release binaries reject it).
    Serve,
    /// Verify and serve the exact immutable release tree.
    ServeRelease(VerifyReleaseArgs),
    /// Create the initial local administrator.
    AdminCreate(DatabaseArgs),
    /// Reset an existing local administrator password.
    AdminResetPassword(AdminResetPasswordArgs),
    /// Run a deployment health check against the configured instance.
    Doctor,
    /// Print the exact machine-readable product, API, schema and target identity.
    Identity,
    /// Verify an immutable release tree with the binary contained in that tree.
    VerifyRelease(VerifyReleaseArgs),
}

#[derive(clap::Args)]
struct DatabaseArgs {
    #[arg(long, hide_env_values = true)]
    database_url: String,
}

#[derive(clap::Args)]
struct AdminResetPasswordArgs {
    #[arg(long, hide_env_values = true)]
    database_url: String,
    #[arg(long)]
    username: String,
    #[arg(long, hide_env_values = true)]
    password: String,
}

#[derive(clap::Args)]
struct VerifyReleaseArgs {
    #[arg(long)]
    root: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sunshine_manager=info,tower_http=info".into()),
        )
        .init();
    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Serve => {
            anyhow::ensure!(
                !release_contract::BinaryIdentity::current()?.is_release_bound(),
                "source-bound release binaries must use serve-release"
            );
            serve(None).await
        }
        Command::ServeRelease(args) => {
            release_bundle::verify_release(&args.root)?;
            serve(Some(&args.root)).await
        }
        Command::AdminCreate(args) => {
            let maintenance = MaintenanceLock::exclusive(&args.database_url)?;
            let pool = db::open_or_initialize(&maintenance.database_url()).await?;
            let username = sarmg_admin_auth::normalize_administrator_username(
                &std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_USERNAME")
                    .unwrap_or_else(|_| "admin".to_string()),
            )?;
            let password = std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD").ok();
            let service = sarmg_admin_core::AdministratorService::new(
                sarmg_admin_sqlite::SqliteAdministratorStore::new(pool),
            );
            anyhow::ensure!(
                service
                    .bootstrap_administrator(
                        &username,
                        password.as_deref().ok_or_else(|| anyhow::anyhow!(
                            "SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD is required"
                        ))?,
                        current_time_micros()?,
                    )
                    .await
                    .map_err(|error| anyhow::anyhow!(error))?,
                "admin-create only initializes a database with no administrator"
            );
            println!("{{\"status\":\"admin-ready\",\"username\":{username:?}}}");
            Ok(())
        }
        Command::AdminResetPassword(args) => {
            let maintenance = MaintenanceLock::exclusive(&args.database_url)?;
            let pool = db::open_existing(&maintenance.database_url()).await?;
            let username = sarmg_admin_auth::normalize_administrator_username(&args.username)?;
            let service = sarmg_admin_core::AdministratorService::new(
                sarmg_admin_sqlite::SqliteAdministratorStore::new(pool),
            );
            service
                .change_administrator_password(&username, &args.password, current_time_micros()?)
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
            println!(
                "{{\"status\":\"password-reset\",\"username\":{:?}}}",
                username
            );
            Ok(())
        }
        Command::Doctor => {
            let config = ServeConfig::from_runtime()?;
            let maintenance = MaintenanceLock::shared(&config.database_url)?;
            let pool = db::open_existing(&maintenance.database_url()).await?;
            let report = db::doctor(&pool, &config.secrets).await;
            println!(
                "{{\"status\":\"{}\",\"bind\":\"{}\",\"schema_ready\":{},\
                 \"integrity_ready\":{},\"foreign_keys_ready\":{},\"writable\":{},\
                 \"encrypted_values_ready\":{}}}",
                if report.healthy() { "ok" } else { "degraded" },
                config.bind,
                report.schema_ready,
                report.integrity_ready,
                report.foreign_keys_ready,
                report.writable,
                report.encrypted_values_ready,
            );
            if !report.healthy() {
                anyhow::bail!("Sunshine Manager doctor found an unhealthy local boundary");
            }
            Ok(())
        }
        Command::Identity => {
            println!("{}", release_contract::current_json()?);
            Ok(())
        }
        Command::VerifyRelease(args) => {
            let report = release_bundle::verify_release(&args.root)?;
            println!("{}", serde_json::to_string(&report)?);
            Ok(())
        }
    }
}

async fn serve(release_root: Option<&std::path::Path>) -> anyhow::Result<()> {
    let config = ServeConfig::from_runtime()?;
    if let Some(root) = release_root {
        anyhow::ensure!(
            config.static_dir == root.join("web"),
            "SUNSHINE_MANAGER_STATIC_DIR must belong to the verified release"
        );
    }
    // Hold both locks for the complete process lifetime. The instance lock
    // rejects a second worker, while the shared maintenance lock excludes
    // external restore, upgrade, and administrator maintenance.
    let application_lock = ApplicationLock::acquire(&config.database_url)?;
    let pool = db::open_or_initialize(&application_lock.database_url()).await?;
    db::require_current_runtime_state(&pool, &config.secrets).await?;
    let administrator_service = sarmg_admin_core::AdministratorService::new(
        sarmg_admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    );
    if administrator_service.store().administrator_count().await? == 0 {
        administrator_service
            .bootstrap_administrator(
                &config.bootstrap_admin_username,
                config.bootstrap_admin_password.as_deref().ok_or_else(|| anyhow::anyhow!(
                    "SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD is required while no administrators exist"
                ))?,
                current_time_micros()?,
            )
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
    }
    let state = WorkerState::new(pool, config.secrets, config.production, config.static_dir)?
        .with_cover_delivery(config.cover_url_policy, config.cover_proxy);
    let recovered = state.operation_manager().recover_startup().await?;
    if let Err(error) = state.operation_manager().deliver_outbox().await {
        tracing::warn!(%error, "initial audit outbox delivery failed; background retry will continue");
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let health_pool = state.pool.clone();
    let probe_state = state.clone();
    let operation_manager = state.operation_manager().clone();
    let audit_pool = state.pool.clone();
    let operations_pool = state.pool.clone();
    let runtime =
        sarmg_server_runtime::ServerRuntime::builder(sarmg_server_runtime::ProductDescriptor {
            id: "sunshine-manager".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            foundation_revision: "1e889d08fa69fcf2b5fffe45e8cc42b68218f4f1".to_owned(),
            profile: "server-control-plane".to_owned(),
            capabilities: vec![
                "admin-persistent".to_owned(),
                "server-runtime".to_owned(),
                "server-health".to_owned(),
                "durable-operations".to_owned(),
                "secret-envelope".to_owned(),
            ],
        })
        .with_schema_identity(sunshine_manager::database_schema::current_schema_identity())
        .register_metric(
            sarmg_server_runtime::DiagnosticMetric::AuditBacklog,
            move || {
                let store = sarmg_operations::SqliteOperationStore::new(audit_pool.clone());
                async move { store.pending_audit_count().await.ok() }
            },
        )
        .register_metric(
            sarmg_server_runtime::DiagnosticMetric::OperationBacklog,
            move || {
                let store = sarmg_operations::SqliteOperationStore::new(operations_pool.clone());
                async move { store.active_operation_count().await.ok() }
            },
        )
        .register_health_check(
            "database",
            sarmg_server_runtime::health_check(move || {
                let pool = health_pool.clone();
                async move { db::ready(&pool).await }
            }),
        )
        .register_background_task(
            "host-probe",
            sarmg_server_runtime::TaskCriticality::Degrading,
            move |mut shutdown| async move {
                tokio::select! {
                    _ = probe_loop(probe_state) => Ok(()),
                    _ = shutdown.changed() => Ok(()),
                }
            },
        )
        .register_background_task(
            "durable-operations",
            sarmg_server_runtime::TaskCriticality::Critical,
            move |shutdown| operation_manager.run_until(shutdown),
        )
        .build()
        .await?;
    let runtime_handle = runtime.handle();
    tracing::info!(
        bind = %config.bind,
        schema = db::SCHEMA,
        recovered_running_operations = recovered,
        "Sunshine manager ready"
    );
    runtime
        .serve(listener, router(state, runtime_handle)?)
        .await?;
    Ok(())
}

fn current_time_micros() -> anyhow::Result<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros())
        .map_err(|_| anyhow::anyhow!("current time exceeds administrator timestamp range"))
}
