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
use crate::skills::SkillManager;
use crate::tools::{ToolContext, ToolRegistry};
use crate::security::SandboxedFs;

pub struct Agent {
    pub config: Config,
    pub memory: MemoryManager,
    pub approval_queue: ApprovalQueue,
    pub tools: ToolRegistry,
    pub llm: LlmEngine,
    pub ctx: ToolContext,
    pub skill_manager: Mutex<SkillManager>,
    paused: AtomicBool,
    sse_tx: broadcast::Sender<String>,
}

impl Agent {
    pub async fn new(
        config: Config,
        db: Arc<Mutex<Connection>>,
        sandbox: SandboxedFs,
        tools: ToolRegistry,
        telegram_bot: Option<teloxide::Bot>,
        telegram_chat_id: Option<i64>,
    ) -> Result<Self> {
        // Initialize memory
        let memory = MemoryManager::new(db.clone(), config.conversation_window);
        memory.init(&config.core_personality).await?;

        // Initialize approval queue
        let approval_queue = ApprovalQueue::new(db.clone(), config.approval_expiry_secs);

        // Initialize Claude CLI engine
        let llm = LlmEngine::new(&config)?;

        // Build tool context
        let http_client = reqwest::Client::builder()
            .user_agent("safe-agent/0.1.0")
            .build()
            .unwrap_or_default();

        let ctx = ToolContext {
            sandbox: sandbox.clone(),
            db: db.clone(),
            http_client,
            telegram_bot,
            telegram_chat_id,
        };

        // Initialize skill manager
        let skills_dir = sandbox.root().join("skills");
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
        let skill_manager = SkillManager::new(skills_dir, bot_token, telegram_chat_id);

        // SSE broadcast channel
        let (sse_tx, _) = broadcast::channel(64);

        Ok(Self {
            config,
            memory,
            approval_queue,
            tools,
            llm,
            ctx,
            skill_manager: Mutex::new(skill_manager),
            paused: AtomicBool::new(false),
            sse_tx,
        })
    }

    /// Run the agent loop until shutdown.
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        let tick_interval = tokio::time::Duration::from_secs(self.config.tick_interval_secs);

        info!(interval_secs = self.config.tick_interval_secs, "agent loop starting");

        // Initial skill reconciliation on startup
        {
            let mut sm = self.skill_manager.lock().await;
            if let Err(e) = sm.reconcile().await {
                error!("initial skill reconciliation failed: {e}");
            }
        }

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

            // Reconcile skills every tick
            {
                let mut sm = self.skill_manager.lock().await;
                if let Err(e) = sm.reconcile().await {
                    error!("skill reconciliation failed: {e}");
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

        // Shut down all skills
        {
            let mut sm = self.skill_manager.lock().await;
            sm.shutdown().await;
        }
    }

    /// Force an immediate tick (from dashboard or Telegram).
    pub async fn force_tick(&self) -> Result<()> {
        self.tick().await
    }

    /// Handle an incoming user message: call Claude CLI and return the reply.
    ///
    /// This is the event-driven path — called directly from the Telegram
    /// handler so the user gets a response in seconds, not on the next tick.
    pub async fn handle_message(&self, user_message: &str) -> Result<String> {
        // Store the user message in conversation history
        self.memory
            .conversation
            .append("user", user_message)
            .await?;

        // Call Claude
        let reply = self.llm.generate(user_message).await?;

        // Store the assistant reply
        self.memory
            .conversation
            .append("assistant", &reply)
            .await?;

        // Reconcile skills after every message so newly created or deleted
        // skills are picked up immediately instead of waiting for the next tick.
        {
            let mut sm = self.skill_manager.lock().await;
            if let Err(e) = sm.reconcile().await {
                error!("skill reconciliation after message failed: {e}");
            }
        }

        // Record the action
        self.memory.record_action().await?;
        self.memory
            .log_activity("message", "claude reply", Some(&reply), "ok")
            .await?;
        self.notify_update();

        Ok(reply)
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
