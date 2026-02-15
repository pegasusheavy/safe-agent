mod agent;
mod approval;
mod config;
mod dashboard;
mod db;
mod error;
mod google;
mod llm;
mod memory;
mod security;
mod telegram;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use crate::agent::Agent;
use crate::config::Config;
use crate::security::SandboxedFs;
use crate::tools::ToolRegistry;

#[tokio::main]
async fn main() {
    // Load .env file (if present) before anything reads env vars
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }

    if args.iter().any(|a| a == "--default-config") {
        print!("{}", Config::default_config_contents());
        return;
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load config
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let config = match Config::load(config_path.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            error!("failed to load config: {e}");
            return;
        }
    };

    info!(
        agent_name = %config.agent_name,
        dashboard = %config.dashboard_bind,
        tick_interval = config.tick_interval_secs,
        "safe-agent starting"
    );

    // Set up sandboxed filesystem
    let data_dir = Config::data_dir();
    let sandbox = match SandboxedFs::new(data_dir.clone()) {
        Ok(s) => s,
        Err(e) => {
            error!("failed to initialize sandbox: {e}");
            return;
        }
    };
    info!(root = %sandbox.root().display(), "sandbox initialized");

    // Open database
    let db_path = sandbox.root().join("safe-agent.db");
    let db = match db::open(&db_path) {
        Ok(d) => d,
        Err(e) => {
            error!("failed to open database: {e}");
            return;
        }
    };
    let db = Arc::new(Mutex::new(db));

    // Handle --download-model
    if args.iter().any(|a| a == "--download-model") {
        info!("downloading model from HuggingFace...");
        let config2 = config.clone();
        let sandbox2 = sandbox.clone();
        match tokio::task::spawn_blocking(move || llm::download_model(&config2, &sandbox2))
            .await
            .expect("download task panicked")
        {
            Ok(path) => {
                info!(path = %path.display(), "model downloaded successfully");
                return;
            }
            Err(e) => {
                error!("failed to download model: {e}");
                return;
            }
        }
    }

    // Handle --check
    if args.iter().any(|a| a == "--check") {
        run_checks(&config, &sandbox).await;
        return;
    }

    // Build the tool registry
    let tool_registry = build_tool_registry(&config);
    info!(tools = tool_registry.len(), "tool registry initialized");

    // Shutdown signal
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Build the agent
    let agent = match Agent::new(config.clone(), db.clone(), sandbox, tool_registry).await {
        Ok(a) => Arc::new(a),
        Err(e) => {
            error!("failed to initialize agent: {e}");
            return;
        }
    };

    // Start Telegram bot (if enabled)
    let _telegram_shutdown = if config.telegram.enabled {
        match telegram::start(db.clone(), config.telegram.clone()).await {
            Ok(tx) => {
                info!("telegram bot started");
                Some(tx)
            }
            Err(e) => {
                error!("failed to start telegram bot: {e}");
                None
            }
        }
    } else {
        None
    };

    // Start the dashboard
    let dashboard_handle = {
        let agent = agent.clone();
        let config = config.clone();
        let db = db.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            if let Err(e) = dashboard::serve(config, agent, db, shutdown_rx).await {
                error!("dashboard error: {e}");
            }
        })
    };

    // Start the agent loop
    let agent_handle = {
        let agent = agent.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            agent.run(shutdown_rx).await;
        })
    };

    info!("safe-agent is running — press Ctrl+C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl+c");

    info!("shutdown signal received, stopping...");
    let _ = shutdown_tx.send(());

    // Wait for tasks to finish
    let _ = tokio::join!(dashboard_handle, agent_handle);
    info!("safe-agent stopped");
}

/// Build the tool registry from config.
fn build_tool_registry(config: &Config) -> ToolRegistry {
    use crate::tools::*;

    let mut registry = ToolRegistry::new();

    // Always register core tools
    if config.tools.exec.enabled {
        registry.register(Box::new(exec::ExecTool::new(config.tools.exec.timeout_secs)));
    }

    registry.register(Box::new(process::ProcessTool::new()));
    registry.register(Box::new(file::ReadFileTool));
    registry.register(Box::new(file::WriteFileTool));
    registry.register(Box::new(file::EditFileTool));
    registry.register(Box::new(file::ApplyPatchTool));

    if config.tools.web.enabled {
        registry.register(Box::new(web::WebSearchTool::new(config.tools.web.max_results)));
        registry.register(Box::new(web::WebFetchTool));
    }

    if config.tools.browser.enabled {
        registry.register(Box::new(browser::BrowserTool::new(config.tools.browser.headless)));
    }

    if config.tools.message.enabled {
        registry.register(Box::new(message::MessageTool::new()));
    }

    if config.sessions.enabled {
        registry.register(Box::new(sessions::SessionsListTool));
        registry.register(Box::new(sessions::SessionsHistoryTool));
        registry.register(Box::new(sessions::SessionsSendTool));
        registry.register(Box::new(sessions::SessionsSpawnTool));
    }

    if config.tools.cron.enabled {
        registry.register(Box::new(cron::CronTool::new()));
    }

    registry.register(Box::new(image::ImageTool::new()));
    registry.register(Box::new(memory::MemorySearchTool));
    registry.register(Box::new(memory::MemoryGetTool));
    registry.register(Box::new(knowledge::KnowledgeGraphTool::new()));

    if config.google.enabled {
        registry.register(Box::new(google::GoogleCalendarTool::new()));
        registry.register(Box::new(google::GoogleDriveTool::new()));
        registry.register(Box::new(google::GoogleDocsTool::new()));
    }

    registry
}

async fn run_checks(config: &Config, sandbox: &SandboxedFs) {
    info!("running pre-flight checks...");

    info!("config: OK");
    info!("  agent_name: {}", config.agent_name);
    info!("  dashboard_bind: {}", config.dashboard_bind);

    info!("sandbox root: {}", sandbox.root().display());

    let model_path = config.resolved_model_path();
    if model_path.exists() {
        info!("model: OK ({})", model_path.display());
    } else {
        error!("model: NOT FOUND ({})", model_path.display());
        error!("  run with --download-model to fetch it");
    }

    if config.telegram.enabled {
        match Config::telegram_bot_token() {
            Ok(_) => info!("TELEGRAM_BOT_TOKEN: set"),
            Err(_) => error!("TELEGRAM_BOT_TOKEN: NOT SET (telegram enabled)"),
        }
    }

    if config.google.enabled {
        match Config::google_client_id() {
            Ok(_) => info!("GOOGLE_CLIENT_ID: set"),
            Err(_) => error!("GOOGLE_CLIENT_ID: NOT SET (google enabled)"),
        }
        match Config::google_client_secret() {
            Ok(_) => info!("GOOGLE_CLIENT_SECRET: set"),
            Err(_) => error!("GOOGLE_CLIENT_SECRET: NOT SET (google enabled)"),
        }
    }
}

fn print_usage() {
    println!(
        "safe-agent — sandboxed autonomous AI agent with tool execution

USAGE:
    safe-agent [OPTIONS]

OPTIONS:
    --config <PATH>     Path to config file (default: ~/.config/safe-agent/config.toml)
    --default-config    Print default config to stdout and exit
    --download-model    Download the configured model from HuggingFace and exit
    --check             Validate config and connectivity, then exit
    -h, --help          Print this help message

ENVIRONMENT:
    TELEGRAM_BOT_TOKEN    Required if Telegram is enabled.
    GOOGLE_CLIENT_ID      Required if Google SSO is enabled.
    GOOGLE_CLIENT_SECRET  Required if Google SSO is enabled.
    RUST_LOG              Optional. Tracing filter (default: info).
"
    );
}
