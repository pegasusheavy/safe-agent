use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: i64,
    pub label: String,
    pub node_type: String,
    pub content: String,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub relation: String,
    pub weight: f64,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

pub struct KnowledgeGraph {
    db: Arc<Mutex<Connection>>,
}

impl KnowledgeGraph {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub async fn add_node(
        &self,
        label: &str,
        node_type: &str,
        content: &str,
        confidence: f64,
    ) -> Result<i64> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO knowledge_nodes (label, node_type, content, confidence) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![label, node_type, content, confidence],
        )?;
        Ok(db.last_insert_rowid())
    }

    pub async fn add_edge(
        &self,
        source_id: i64,
        target_id: i64,
        relation: &str,
        weight: f64,
    ) -> Result<i64> {
        let db = self.db.lock().await;
        db.execute(
            "INSERT OR IGNORE INTO knowledge_edges (source_id, target_id, relation, weight) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![source_id, target_id, relation, weight],
        )?;
        Ok(db.last_insert_rowid())
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeNode>> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT n.id, n.label, n.node_type, n.content, n.confidence, n.created_at, n.updated_at
             FROM knowledge_nodes_fts fts
             JOIN knowledge_nodes n ON n.id = fts.rowid
             WHERE knowledge_nodes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let nodes = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok(KnowledgeNode {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    node_type: row.get(2)?,
                    content: row.get(3)?,
                    confidence: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    pub async fn get_node(&self, id: i64) -> Result<KnowledgeNode> {
        let db = self.db.lock().await;
        let node = db.query_row(
            "SELECT id, label, node_type, content, confidence, created_at, updated_at
             FROM knowledge_nodes WHERE id = ?1",
            [id],
            |row| {
                Ok(KnowledgeNode {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    node_type: row.get(2)?,
                    content: row.get(3)?,
                    confidence: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )?;
        Ok(node)
    }

    pub async fn neighbors(
        &self,
        node_id: i64,
        relation_filter: Option<&str>,
    ) -> Result<Vec<(KnowledgeEdge, KnowledgeNode)>> {
        let db = self.db.lock().await;

        let query = if relation_filter.is_some() {
            "SELECT e.id, e.source_id, e.target_id, e.relation, e.weight, e.metadata, e.created_at,
                    n.id, n.label, n.node_type, n.content, n.confidence, n.created_at, n.updated_at
             FROM knowledge_edges e
             JOIN knowledge_nodes n ON n.id = CASE WHEN e.source_id = ?1 THEN e.target_id ELSE e.source_id END
             WHERE (e.source_id = ?1 OR e.target_id = ?1) AND e.relation = ?2"
        } else {
            "SELECT e.id, e.source_id, e.target_id, e.relation, e.weight, e.metadata, e.created_at,
                    n.id, n.label, n.node_type, n.content, n.confidence, n.created_at, n.updated_at
             FROM knowledge_edges e
             JOIN knowledge_nodes n ON n.id = CASE WHEN e.source_id = ?1 THEN e.target_id ELSE e.source_id END
             WHERE (e.source_id = ?1 OR e.target_id = ?1)"
        };

        let mut stmt = db.prepare(query)?;

        let rows = if let Some(rel) = relation_filter {
            stmt.query_map(rusqlite::params![node_id, rel], map_edge_node)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(rusqlite::params![node_id], map_edge_node)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok(rows)
    }

    pub async fn traverse(
        &self,
        node_id: i64,
        relations: &[&str],
        max_depth: usize,
    ) -> Result<Vec<KnowledgeNode>> {
        let db = self.db.lock().await;

        let relation_clause = if relations.is_empty() {
            String::new()
        } else {
            let placeholders: Vec<String> = relations.iter().map(|r| format!("'{}'", r.replace('\'', "''"))).collect();
            format!("AND e.relation IN ({})", placeholders.join(", "))
        };

        let sql = format!(
            "WITH RECURSIVE reachable(nid, depth) AS (
                SELECT ?1, 0
                UNION
                SELECT CASE WHEN e.source_id = r.nid THEN e.target_id ELSE e.source_id END, r.depth + 1
                FROM reachable r
                JOIN knowledge_edges e ON (e.source_id = r.nid OR e.target_id = r.nid)
                WHERE r.depth < ?2 {relation_clause}
            )
            SELECT DISTINCT n.id, n.label, n.node_type, n.content, n.confidence, n.created_at, n.updated_at
            FROM knowledge_nodes n
            JOIN reachable r ON n.id = r.nid
            WHERE n.id != ?1"
        );

        let mut stmt = db.prepare(&sql)?;
        let nodes = stmt
            .query_map(rusqlite::params![node_id, max_depth as i64], |row| {
                Ok(KnowledgeNode {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    node_type: row.get(2)?,
                    content: row.get(3)?,
                    confidence: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    pub async fn update_node(
        &self,
        id: i64,
        content: Option<&str>,
        confidence: Option<f64>,
    ) -> Result<()> {
        let db = self.db.lock().await;
        if let Some(c) = content {
            db.execute(
                "UPDATE knowledge_nodes SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![c, id],
            )?;
        }
        if let Some(conf) = confidence {
            db.execute(
                "UPDATE knowledge_nodes SET confidence = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![conf, id],
            )?;
        }
        Ok(())
    }

    pub async fn remove_node(&self, id: i64) -> Result<()> {
        let db = self.db.lock().await;
        db.execute("DELETE FROM knowledge_nodes WHERE id = ?1", [id])?;
        Ok(())
    }

    pub async fn remove_edge(&self, id: i64) -> Result<()> {
        let db = self.db.lock().await;
        db.execute("DELETE FROM knowledge_edges WHERE id = ?1", [id])?;
        Ok(())
    }

    pub async fn stats(&self) -> Result<(i64, i64)> {
        let db = self.db.lock().await;
        let nodes: i64 = db.query_row("SELECT COUNT(*) FROM knowledge_nodes", [], |r| r.get(0))?;
        let edges: i64 = db.query_row("SELECT COUNT(*) FROM knowledge_edges", [], |r| r.get(0))?;
        Ok((nodes, edges))
    }
}

fn map_edge_node(row: &rusqlite::Row) -> rusqlite::Result<(KnowledgeEdge, KnowledgeNode)> {
    let metadata_str: String = row.get(5)?;
    let metadata = serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Object(Default::default()));
    Ok((
        KnowledgeEdge {
            id: row.get(0)?,
            source_id: row.get(1)?,
            target_id: row.get(2)?,
            relation: row.get(3)?,
            weight: row.get(4)?,
            metadata,
            created_at: row.get(6)?,
        },
        KnowledgeNode {
            id: row.get(7)?,
            label: row.get(8)?,
            node_type: row.get(9)?,
            content: row.get(10)?,
            confidence: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        },
    ))
}
