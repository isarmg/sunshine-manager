use std::path::PathBuf;

use clap::{Parser, Subcommand};
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
            let username = sunshine_manager::auth::normalize_administrator_username(
                &std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_USERNAME")
                    .unwrap_or_else(|_| "admin".to_string()),
            )?;
            let password = std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD").ok();
            anyhow::ensure!(
                db::ensure_admin_user(&pool, &username, password.as_deref()).await?,
                "admin-create only initializes a database with no administrator"
            );
            println!("{{\"status\":\"admin-ready\",\"username\":{username:?}}}");
            Ok(())
        }
        Command::AdminResetPassword(args) => {
            let maintenance = MaintenanceLock::exclusive(&args.database_url)?;
            let pool = db::open_existing(&maintenance.database_url()).await?;
            let username =
                sunshine_manager::auth::normalize_administrator_username(&args.username)?;
            db::reset_admin_password(&pool, &username, &args.password).await?;
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
    db::ensure_admin_user(
        &pool,
        &config.bootstrap_admin_username,
        config.bootstrap_admin_password.as_deref(),
    )
    .await?;
    let state = WorkerState::new(
        pool,
        config.secrets,
        config.internal_auth,
        config.static_dir,
    )?
    .with_cover_delivery(config.cover_url_policy, config.cover_proxy);
    let recovered = state.operation_manager().recover_startup().await?;
    if let Err(error) = state.operation_manager().deliver_outbox().await {
        tracing::warn!(%error, "initial audit outbox delivery failed; background retry will continue");
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(
        bind = %config.bind,
        schema = db::SCHEMA,
        recovered_running_operations = recovered,
        "Sunshine manager ready"
    );
    let probe = tokio::spawn(probe_loop(state.clone()));
    let operations = tokio::spawn(state.operation_manager().clone().run());
    let result = axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await;
    probe.abort();
    operations.abort();
    result?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
