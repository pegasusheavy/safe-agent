pub mod actions;
pub mod reasoning;
pub mod tick;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use crate::approval::ApprovalQueue;
use crate::config::Config;
use crate::error::Result;
use crate::llm::LlmEngine;
use crate::memory::MemoryManager;
use crate::tools::{ToolContext, ToolRegistry};
use crate::security::SandboxedFs;

pub struct Agent {
    pub config: Config,
    pub memory: MemoryManager,
    pub approval_queue: ApprovalQueue,
    pub tools: ToolRegistry,
    pub llm: Arc<Mutex<LlmEngine>>,
    pub ctx: ToolContext,
    paused: AtomicBool,
    sse_tx: broadcast::Sender<String>,
}

impl Agent {
    pub async fn new(
        config: Config,
        db: Arc<Mutex<Connection>>,
        sandbox: SandboxedFs,
        tools: ToolRegistry,
    ) -> Result<Self> {
        // Initialize memory
        let memory = MemoryManager::new(db.clone(), config.conversation_window);
        memory.init(&config.core_personality).await?;

        // Initialize approval queue
        let approval_queue = ApprovalQueue::new(db.clone(), config.approval_expiry_secs);

        // Load LLM
        let llm = LlmEngine::load(&config)?;

        // Build tool context
        let http_client = reqwest::Client::builder()
            .user_agent("safe-agent/0.1.0")
            .build()
            .unwrap_or_default();

        let ctx = ToolContext {
            sandbox,
            db: db.clone(),
            http_client,
        };

        // SSE broadcast channel
        let (sse_tx, _) = broadcast::channel(64);

        Ok(Self {
            config,
            memory,
            approval_queue,
            tools,
            llm: Arc::new(Mutex::new(llm)),
            ctx,
            paused: AtomicBool::new(false),
            sse_tx,
        })
    }

    /// Run the agent loop until shutdown.
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        let tick_interval = tokio::time::Duration::from_secs(self.config.tick_interval_secs);

        info!(interval_secs = self.config.tick_interval_secs, "agent loop starting");

        loop {
            // Execute any approved actions first
            if let Err(e) = self.execute_approved().await {
                error!("error executing approved actions: {e}");
            }

            // Run a tick if not paused
            if !self.is_paused() {
                if let Err(e) = self.tick().await {
                    error!("tick error: {e}");
                    self.memory
                        .log_activity("tick", "tick failed", Some(&e.to_string()), "error")
                        .await
                        .ok();
                }
            }

            // Wait for tick interval or shutdown
            tokio::select! {
                _ = tokio::time::sleep(tick_interval) => {}
                _ = shutdown.recv() => {
                    info!("agent loop shutting down");
                    break;
                }
            }
        }
    }

    /// Force an immediate tick (from dashboard or Telegram).
    pub async fn force_tick(&self) -> Result<()> {
        self.tick().await
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
        info!("agent paused");
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
        info!("agent resumed");
    }

    /// Subscribe to SSE updates.
    pub fn subscribe_sse(&self) -> broadcast::Receiver<String> {
        self.sse_tx.subscribe()
    }

    /// Notify SSE subscribers of an update.
    pub fn notify_update(&self) {
        let _ = self.sse_tx.send("update".to_string());
    }
}
