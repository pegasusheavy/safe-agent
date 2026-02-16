use tracing::{error, info};

use crate::error::Result;

use super::Agent;

impl Agent {
    /// Maintenance tick: expire stale actions.
    ///
    /// Message handling is now event-driven (via `handle_message`), so the
    /// tick loop only performs background housekeeping.
    pub async fn tick(&self) -> Result<()> {
        // Expire stale pending actions
        let expired = self.approval_queue.expire_stale().await?;
        if expired > 0 {
            info!(count = expired, "expired stale actions");
        }

        // Record tick
        self.memory.record_tick().await?;

        Ok(())
    }

    /// Drain and execute all approved tool calls from the approval queue.
    pub async fn execute_approved(&self) -> Result<()> {
        while let Some(action) = self.approval_queue.next_approved().await? {
            let call = super::actions::parse_tool_call(&action.action)?;

            match super::actions::execute_tool_call(&self.tools, &self.ctx, &call).await {
                Ok(output) => {
                    self.approval_queue
                        .mark_executed(&action.id, true)
                        .await?;
                    info!(
                        tool = %call.tool,
                        id = %action.id,
                        output_len = output.output.len(),
                        "executed approved tool call"
                    );
                }
                Err(e) => {
                    self.approval_queue
                        .mark_executed(&action.id, false)
                        .await?;
                    error!(
                        tool = %call.tool,
                        id = %action.id,
                        err = %e,
                        "tool call failed"
                    );
                }
            }
        }
        Ok(())
    }
}
