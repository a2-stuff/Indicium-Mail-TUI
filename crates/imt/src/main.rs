//! `imt` - Indicium Mail TUI binary entry point.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use imt_store::Db;
use imt_sync::SyncEngine;
use imt_tui::InMemoryDataSource;
use tokio::sync::mpsc;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use imt_mcp;

mod cli;
mod config;
mod datasource;
mod snapshot;

use datasource::{command_worker, SyncDataSource};
use snapshot::Snapshot;

#[derive(Parser, Debug)]
#[command(name = "imt", version, about = "Indicium Mail TUI")]
struct Args {
    /// Override the config file path.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Override the SQLite database path.
    #[arg(long)]
    db: Option<PathBuf>,

    /// Override the log file path.
    #[arg(long)]
    log_file: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the TUI (default).
    Run {
        /// Use an in-memory mock data source.
        #[arg(long)]
        mock: bool,
    },
    /// Add a new mail account. Provide flags to skip prompts; password
    /// always comes from `$IMT_PASSWORD` (or stdin if unset).
    AddAccount {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        imap_host: Option<String>,
        #[arg(long, default_value_t = 993)]
        imap_port: u16,
        #[arg(long, default_value = "implicit", value_parser = ["implicit", "starttls", "none"])]
        imap_tls: String,
        #[arg(long)]
        smtp_host: Option<String>,
        #[arg(long, default_value_t = 465)]
        smtp_port: u16,
        #[arg(long, default_value = "implicit", value_parser = ["implicit", "starttls", "none"])]
        smtp_tls: String,
    },
    /// List configured accounts.
    ListAccounts,
    /// Delete an account by UUID.
    DeleteAccount {
        /// Account UUID (see `imt list-accounts`).
        id: String,
    },
    /// Start the MCP (Model Context Protocol) server on stdin/stdout.
    /// AI agents connect by spawning this process and communicating via JSON-RPC 2.0.
    Mcp,
}

fn project_dirs() -> anyhow::Result<ProjectDirs> {
    ProjectDirs::from("dev", "indicium", "indicium-mail-tui")
        .context("could not resolve user directories")
}

fn init_logging(log_file: Option<PathBuf>) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let dirs = project_dirs()?;
    let log_path = log_file.unwrap_or_else(|| dirs.data_local_dir().join("imt.log"));
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening log file {}", log_path.display()))?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();
    Ok(guard)
}

fn resolve_db_path(arg: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(p);
    }
    let dirs = project_dirs()?;
    Ok(dirs.data_local_dir().join("imt.sqlite3"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _log_guard = init_logging(args.log_file.clone())?;

    let db_path = resolve_db_path(args.db.clone())?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    match args.command.unwrap_or(Cmd::Run { mock: false }) {
        Cmd::AddAccount {
            email,
            display_name,
            username,
            imap_host,
            imap_port,
            imap_tls,
            smtp_host,
            smtp_port,
            smtp_tls,
        } => {
            cli::add_account(
                &db_path,
                cli::AddAccountArgs {
                    email,
                    display_name,
                    username,
                    imap_host,
                    imap_port,
                    imap_tls,
                    smtp_host,
                    smtp_port,
                    smtp_tls,
                },
            )
            .await
        }
        Cmd::ListAccounts => cli::list_accounts(&db_path).await,
        Cmd::DeleteAccount { id } => cli::delete_account(&db_path, &id).await,
        Cmd::Run { mock } => run_tui(mock, &db_path, args.config.clone()).await,
        Cmd::Mcp => run_mcp(&db_path).await,
    }
}

async fn run_mcp(db_path: &std::path::Path) -> anyhow::Result<()> {
    tracing::info!("starting MCP server, db={}", db_path.display());
    let db = Arc::new(Db::open(db_path).await.context("opening database")?);

    let (engine, mut event_rx) = SyncEngine::new(db.clone());
    let engine = Arc::new(engine);

    // Start account workers for all configured accounts (in parallel)
    let accounts = imt_store::AccountRepo::new(db.pool()).list().await?;
    let spawn_futures = accounts.into_iter().map(|acc| {
        let engine = engine.clone();
        async move {
            let pwd = imt_store::secrets::load(acc.id, "imap_password").unwrap_or_default();
            let email = acc.address.email.clone();
            if let Err(e) = engine.add_account(acc, pwd, None).await {
                tracing::warn!("spawn account worker for {}: {}", email, e);
            }
        }
    });
    futures::future::join_all(spawn_futures).await;

    // Drain sync events in the background (keeps workers healthy)
    tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let ctx = Arc::new(imt_mcp::McpContext {
        db,
        engine: engine.clone(),
    });
    let result = imt_mcp::run(ctx).await;
    let _ = engine.shutdown().await;
    result
}

async fn run_tui(mock: bool, db_path: &std::path::Path, cfg_path: Option<PathBuf>) -> anyhow::Result<()> {
    let dirs = project_dirs()?;
    let cfg_path = cfg_path.unwrap_or_else(|| dirs.config_dir().join("config.toml"));
    let cfg = config::Config::load_or_default(&cfg_path)?;

    if mock {
        tracing::info!("starting TUI with in-memory mock");
        return imt_tui::run_with(
            InMemoryDataSource::sample(),
            cfg.settings.clone(),
            std::sync::Arc::new(|_| {}),
        )
        .await;
    }

    tracing::info!("starting TUI with real backend, db={}", db_path.display());
    let db = Arc::new(Db::open(db_path).await.context("opening database")?);

    let snapshot = Snapshot::new();
    snapshot.hydrate_from_db(&db).await?;

    let (engine, mut event_rx) = SyncEngine::new(db.clone());
    let engine = Arc::new(engine);

    let accounts = imt_store::AccountRepo::new(db.pool()).list().await?;
    let spawn_futures = accounts.into_iter().map(|acc| {
        let engine = engine.clone();
        async move {
            let pwd = imt_store::secrets::load(acc.id, "imap_password").unwrap_or_default();
            let email = acc.address.email.clone();
            if let Err(e) = engine.add_account(acc, pwd, None).await {
                tracing::warn!("spawn account worker for {}: {}", email, e);
            }
        }
    });
    futures::future::join_all(spawn_futures).await;

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let snap_for_events = snapshot.clone();
    let db_for_events = db.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = snap_for_events.apply_event(&db_for_events, &event).await {
                tracing::warn!("apply event: {}", e);
            }
        }
    });

    let snap_for_cmds = snapshot.clone();
    let engine_for_cmds = engine.clone();
    let db_for_cmds = db.clone();
    let data = SyncDataSource::new(snapshot, cmd_tx);
    let inflight_for_cmds = data.in_flight_bodies.clone();
    tokio::spawn(async move {
        command_worker(engine_for_cmds, db_for_cmds, snap_for_cmds, inflight_for_cmds, cmd_rx).await;
    });

    let initial_settings = cfg.settings.clone();
    let cfg_path_for_save = cfg_path.clone();
    let cfg_shared = std::sync::Arc::new(std::sync::Mutex::new(cfg));
    let cfg_for_cb = cfg_shared.clone();
    let on_settings_changed: std::sync::Arc<dyn Fn(&imt_tui::Settings) + Send + Sync> = std::sync::Arc::new(move |s: &imt_tui::Settings| {
        let mut guard = cfg_for_cb.lock().unwrap();
        guard.settings = s.clone();
        if let Err(e) = guard.save(&cfg_path_for_save) {
            tracing::warn!("save config: {}", e);
        }
    });
    let result = imt_tui::run_with(data, initial_settings, on_settings_changed).await;
    let _ = engine.shutdown().await;
    result
}
