//! Multi-node federation — run multiple agent instances that share
//! memory and coordinate tasks.
//!
//! Each node registers with peers via HTTP.  Memory mutations are
//! replicated asynchronously.  Goal tasks use distributed locking so
//! only one node executes a given task.
//!
//! Architecture:
//!   - Each node has a unique `node_id` (UUID generated at startup).
//!   - Peers are configured in `[federation]` config section.
//!   - A background task periodically syncs with peers.
//!   - Goal tasks are claimed with a `claimed_by` column; only the
//!     claiming node executes them.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, info};

/// Unique identity of this agent node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub name: String,
    pub address: String,
    pub version: String,
    pub started_at: String,
    pub last_heartbeat: String,
    pub status: NodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Online,
    Offline,
    Syncing,
}

/// A memory mutation to replicate to peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDelta {
    pub id: String,
    pub origin_node: String,
    pub table: String,
    pub operation: String, // insert, update, delete
    pub key: String,
    pub data: serde_json::Value,
    pub timestamp: String,
}

/// A distributed lock claim for a goal task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaim {
    pub task_id: String,
    pub claimed_by: String,
    pub claimed_at: String,
}

/// Federation manager handles peer communication and state sync.
pub struct FederationManager {
    node_id: String,
    node_name: String,
    address: String,
    peers: Mutex<HashMap<String, NodeInfo>>,
    enabled: bool,
}

impl FederationManager {
    pub fn new(node_name: &str, address: &str, enabled: bool) -> Self {
        let node_id = uuid::Uuid::new_v4().to_string();
        info!(
            node_id = %node_id,
            node_name = %node_name,
            enabled,
            "federation manager initialized"
        );

        Self {
            node_id,
            node_name: node_name.to_string(),
            address: address.to_string(),
            peers: Mutex::new(HashMap::new()),
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get this node's info.
    pub fn local_info(&self) -> NodeInfo {
        NodeInfo {
            node_id: self.node_id.clone(),
            name: self.node_name.clone(),
            address: self.address.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_heartbeat: chrono::Utc::now().to_rfc3339(),
            status: NodeStatus::Online,
        }
    }

    /// Register a peer node.
    pub async fn register_peer(&self, info: NodeInfo) {
        let mut peers = self.peers.lock().await;
        info!(
            node_id = %info.node_id,
            name = %info.name,
            address = %info.address,
            "peer registered"
        );
        peers.insert(info.node_id.clone(), info);
    }

    /// Remove a peer node.
    pub async fn remove_peer(&self, node_id: &str) {
        let mut peers = self.peers.lock().await;
        if peers.remove(node_id).is_some() {
            info!(node_id, "peer removed");
        }
    }

    /// List all known peers.
    pub async fn list_peers(&self) -> Vec<NodeInfo> {
        let peers = self.peers.lock().await;
        peers.values().cloned().collect()
    }

    /// Apply incoming deltas from a peer (called when receiving sync).
    pub async fn apply_deltas(
        &self,
        db: &Arc<Mutex<rusqlite::Connection>>,
        deltas: Vec<MemoryDelta>,
    ) {
        let db = db.lock().await;
        for delta in &deltas {
            if delta.origin_node == self.node_id {
                continue; // Skip our own deltas
            }

            debug!(
                table = %delta.table,
                operation = %delta.operation,
                key = %delta.key,
                origin = %delta.origin_node,
                "applying remote delta"
            );

            match delta.table.as_str() {
                "archival_memory" => {
                    match delta.operation.as_str() {
                        "insert" => {
                            let _ = db.execute(
                                "INSERT OR IGNORE INTO archival_memory (id, content, tags, created_at) VALUES (?1, ?2, ?3, ?4)",
                                rusqlite::params![
                                    delta.key,
                                    delta.data.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                    delta.data.get("tags").and_then(|v| v.as_str()).unwrap_or(""),
                                    delta.timestamp,
                                ],
                            );
                        }
                        "delete" => {
                            let _ = db.execute(
                                "DELETE FROM archival_memory WHERE id = ?1",
                                [&delta.key],
                            );
                        }
                        _ => {}
                    }
                }
                "activity_log" => {
                    if delta.operation == "insert" {
                        let _ = db.execute(
                            "INSERT OR IGNORE INTO activity_log (id, action_type, summary, detail, status, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            rusqlite::params![
                                delta.key,
                                delta.data.get("action_type").and_then(|v| v.as_str()).unwrap_or(""),
                                delta.data.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                                delta.data.get("detail").and_then(|v| v.as_str()),
                                delta.data.get("status").and_then(|v| v.as_str()).unwrap_or("ok"),
                                delta.timestamp,
                            ],
                        );
                    }
                }
                _ => {
                    debug!(table = %delta.table, "unknown table in delta, skipping");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_info() {
        let mgr = FederationManager::new("test-node", "http://localhost:3030", false);
        let info = mgr.local_info();
        assert_eq!(info.name, "test-node");
        assert!(!mgr.is_enabled());
    }

    #[tokio::test]
    async fn test_register_and_list_peers() {
        let mgr = FederationManager::new("node-a", "http://a:3030", true);
        assert!(mgr.list_peers().await.is_empty());

        mgr.register_peer(NodeInfo {
            node_id: "node-b".to_string(),
            name: "node-b".to_string(),
            address: "http://b:3030".to_string(),
            version: "0.1.0".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            last_heartbeat: "2026-01-01T00:00:00Z".to_string(),
            status: NodeStatus::Online,
        }).await;

        let peers = mgr.list_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].name, "node-b");
    }

    #[tokio::test]
    async fn test_remove_peer() {
        let mgr = FederationManager::new("node-a", "http://a:3030", true);
        mgr.register_peer(NodeInfo {
            node_id: "node-b".to_string(),
            name: "b".to_string(),
            address: "http://b:3030".to_string(),
            version: "0.1.0".to_string(),
            started_at: String::new(),
            last_heartbeat: String::new(),
            status: NodeStatus::Online,
        }).await;

        mgr.remove_peer("node-b").await;
        assert!(mgr.list_peers().await.is_empty());
    }

    #[test]
    fn test_version_gt() {
        assert!(crate::dashboard::handlers::version_gt("0.2.0", "0.1.0"));
        assert!(!crate::dashboard::handlers::version_gt("0.1.0", "0.2.0"));
        assert!(!crate::dashboard::handlers::version_gt("0.1.0", "0.1.0"));
        assert!(crate::dashboard::handlers::version_gt("1.0.0", "0.9.9"));
    }
}
