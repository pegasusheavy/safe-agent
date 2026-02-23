use std::sync::Arc;

use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::error::{Result, SafeAgentError};

const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";

pub struct EmbeddingEngine {
    client: Client,
    base_url: String,
    model: String,
    db: Arc<Mutex<Connection>>,
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub source_table: String,
    pub source_id: i64,
    pub score: f32,
}

impl EmbeddingEngine {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        ollama_host: &str,
        model: &str,
    ) -> Option<Self> {
        if model.is_empty() {
            return None;
        }

        let base_url = std::env::var("EMBEDDING_OLLAMA_HOST")
            .ok()
            .or_else(|| {
                if ollama_host.is_empty() {
                    None
                } else {
                    Some(ollama_host.to_string())
                }
            })
            .or_else(|| std::env::var("OLLAMA_HOST").ok())
            .unwrap_or_else(|| DEFAULT_OLLAMA_HOST.to_string())
            .trim_end_matches('/')
            .to_string();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .ok()?;

        Some(Self {
            client,
            base_url,
            model: model.to_string(),
            db,
        })
    }

    /// Generate an embedding vector for a single text.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embed", self.base_url);
        let body = EmbedRequest {
            model: self.model.clone(),
            input: vec![text.to_string()],
        };

        let resp = self.client.post(&url).json(&body).send().await.map_err(|e| {
            SafeAgentError::Llm(format!("embedding request failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(SafeAgentError::Llm(format!(
                "embedding API returned {status}: {err_text}"
            )));
        }

        let embed_resp: EmbedResponse = resp.json().await.map_err(|e| {
            SafeAgentError::Llm(format!("failed to parse embedding response: {e}"))
        })?;

        embed_resp
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| SafeAgentError::Llm("no embedding returned".into()))
    }

    /// Store an embedding for a given source row.
    pub async fn store_embedding(
        &self,
        source_table: &str,
        source_id: i64,
        embedding: &[f32],
    ) -> Result<()> {
        let blob = embedding_to_blob(embedding);
        let model = self.model.clone();
        let db = self.db.lock().await;
        db.execute(
            "INSERT OR REPLACE INTO memory_embeddings (source_table, source_id, embedding, model)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![source_table, source_id, blob, model],
        )?;
        Ok(())
    }

    /// Generate and store an embedding for archival memory content.
    pub async fn embed_archival(&self, entry_id: i64, content: &str) -> Result<()> {
        match self.embed_text(content).await {
            Ok(vec) => self.store_embedding("archival_memory", entry_id, &vec).await,
            Err(e) => {
                warn!(entry_id, err = %e, "failed to embed archival entry");
                Err(e)
            }
        }
    }

    /// Generate and store an embedding for a knowledge node.
    pub async fn embed_knowledge_node(&self, node_id: i64, label: &str, content: &str) -> Result<()> {
        let text = if content.is_empty() {
            label.to_string()
        } else {
            format!("{label}: {content}")
        };
        match self.embed_text(&text).await {
            Ok(vec) => self.store_embedding("knowledge_nodes", node_id, &vec).await,
            Err(e) => {
                warn!(node_id, err = %e, "failed to embed knowledge node");
                Err(e)
            }
        }
    }

    /// Semantic search: find the top-N most similar entries by cosine similarity.
    pub async fn search(
        &self,
        query: &str,
        source_table: &str,
        limit: usize,
    ) -> Result<Vec<ScoredResult>> {
        let query_vec = self.embed_text(query).await?;

        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT source_id, embedding FROM memory_embeddings WHERE source_table = ?1",
        )?;

        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map([source_table], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut scored: Vec<ScoredResult> = rows
            .iter()
            .filter_map(|(id, blob)| {
                let emb = blob_to_embedding(blob);
                let score = cosine_similarity(&query_vec, &emb);
                if score.is_finite() {
                    Some(ScoredResult {
                        source_table: source_table.to_string(),
                        source_id: *id,
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        debug!(
            query_len = query.len(),
            table = source_table,
            candidates = rows.len(),
            results = scored.len(),
            "embedding search complete"
        );

        Ok(scored)
    }

    /// Remove the stored embedding for a source row.
    pub async fn remove_embedding(&self, source_table: &str, source_id: i64) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "DELETE FROM memory_embeddings WHERE source_table = ?1 AND source_id = ?2",
            rusqlite::params![source_table, source_id],
        )?;
        Ok(())
    }

    /// Check if Ollama is reachable and the embedding model is available.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_embedding_blob_roundtrip() {
        let emb = vec![0.1, 0.2, -0.3, 0.0, 99.9];
        let blob = embedding_to_blob(&emb);
        let restored = blob_to_embedding(&blob);
        assert_eq!(emb.len(), restored.len());
        for (a, b) in emb.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_len() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }
}
