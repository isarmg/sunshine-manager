use clap::{Parser, Subcommand};
use sunshine_manager::{
    ServeConfig, backup, db,
    http::{WorkerState, probe_loop, router},
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
    /// Serve the private loopback API (the default command).
    Serve,
    /// Apply the product-owned SQLite migrations.
    Migrate(DatabaseArgs),
    /// Create the initial local administrator.
    AdminCreate(DatabaseArgs),
    /// Reset an existing local administrator password.
    AdminResetPassword(AdminResetPasswordArgs),
    /// Run a deployment health check against the configured instance.
    Doctor,
    /// Create a verified online SQLite backup without overwriting a file.
    BackupCreate(BackupArgs),
    /// Verify SQLite integrity, foreign keys and the product schema.
    BackupVerify(BackupArgs),
    /// Atomically restore a verified SQLite backup while the service is stopped.
    Restore(RestoreArgs),
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
    email: String,
    #[arg(long, hide_env_values = true)]
    password: String,
}

#[derive(clap::Args)]
struct BackupArgs {
    #[arg(long, hide_env_values = true)]
    database_url: String,
    #[arg(long)]
    output: std::path::PathBuf,
}

#[derive(clap::Args)]
struct RestoreArgs {
    #[arg(long, hide_env_values = true)]
    database_url: String,
    #[arg(long)]
    input: std::path::PathBuf,
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
        Command::Serve => serve().await,
        Command::Migrate(args) => {
            let pool = db::connect(&args.database_url).await?;
            db::migrate(&pool).await?;
            println!("{{\"status\":\"migrated\",\"schema\":\"sunshine\"}}");
            Ok(())
        }
        Command::AdminCreate(args) => {
            let pool = db::connect(&args.database_url).await?;
            db::migrate(&pool).await?;
            let email = std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_EMAIL")
                .unwrap_or_else(|_| "admin@example.com".to_string());
            let password = std::env::var("SUNSHINE_MANAGER_BOOTSTRAP_ADMIN_PASSWORD").ok();
            db::ensure_admin_user(&pool, &email, password.as_deref()).await?;
            println!("{{\"status\":\"admin-ready\",\"email\":{email:?}}}");
            Ok(())
        }
        Command::AdminResetPassword(args) => {
            let pool = db::connect(&args.database_url).await?;
            db::reset_admin_password(&pool, &args.email, &args.password).await?;
            println!(
                "{{\"status\":\"password-reset\",\"email\":{:?}}}",
                args.email
            );
            Ok(())
        }
        Command::BackupCreate(args) => {
            backup::create(&args.database_url, &args.output)?;
            println!(
                "{{\"status\":\"backup-created\",\"output\":{:?}}}",
                args.output
            );
            Ok(())
        }
        Command::BackupVerify(args) => {
            backup::verify(&args.output)?;
            println!(
                "{{\"status\":\"backup-verified\",\"output\":{:?}}}",
                args.output
            );
            Ok(())
        }
        Command::Restore(args) => {
            backup::restore(&args.database_url, &args.input)?;
            println!("{{\"status\":\"restored\",\"input\":{:?}}}", args.input);
            Ok(())
        }
        Command::Doctor => {
            let config = ServeConfig::from_runtime()?;
            let pool = db::connect(&config.database_url).await?;
            let database_ready = db::ready(&pool).await;
            println!(
                "{{\"status\":\"{}\",\"bind\":\"{}\",\"database_ready\":{database_ready}}}",
                if database_ready { "ok" } else { "degraded" },
                config.bind
            );
            if !database_ready {
                anyhow::bail!("database is not ready");
            }
            Ok(())
        }
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = ServeConfig::from_runtime()?;
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    db::ensure_admin_user(
        &pool,
        &config.bootstrap_admin_email,
        config.bootstrap_admin_password.as_deref(),
    )
    .await?;
    let state = WorkerState::new(
        pool,
        config.secrets,
        config.internal_auth,
        config.production,
    )?
    .with_cover_url_policy(config.cover_url_policy);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(bind = %config.bind, schema = db::SCHEMA, "Sunshine manager ready");
    let probe = tokio::spawn(probe_loop(state.clone()));
    let result = axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await;
    probe.abort();
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
