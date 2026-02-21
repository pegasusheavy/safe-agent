pub mod chunker;
pub mod embed;

// ---------------------------------------------------------------------------
// VectorStore — LanceDB-backed semantic memory and document store
// ---------------------------------------------------------------------------

use std::path::Path;
use std::sync::Arc;

use arrow_array::{
    types::Float32Type, FixedSizeListArray, Int32Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::{Result, SafeAgentError};
use chunker::Chunk;
use embed::Embedder;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single search result from the vector store.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub table: String,
    pub score: f32,
}

/// LanceDB-backed vector store with lazy table creation.
///
/// The embedding dimension may not be known at startup when using the API
/// backend.  Tables are created lazily on the first insert, once the
/// dimension is discovered from the first embedding call.
pub struct VectorStore {
    db: lancedb::Connection,
    embedder: Embedder,
    /// Embedding dimension, discovered on first embed call.
    dim: RwLock<Option<usize>>,
}

// ---------------------------------------------------------------------------
// Table and column names
// ---------------------------------------------------------------------------

const MEMORIES_TABLE: &str = "memories";
const DOCUMENTS_TABLE: &str = "documents";

// ---------------------------------------------------------------------------
// Schema helpers
// ---------------------------------------------------------------------------

/// Build the Arrow schema for the `memories` table.
fn memories_schema(dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim,
            ),
            false,
        ),
        Field::new("category", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
        Field::new("created_at", DataType::Utf8, false),
    ]))
}

/// Build the Arrow schema for the `documents` table.
fn documents_schema(dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim,
            ),
            false,
        ),
        Field::new("file_path", DataType::Utf8, false),
        Field::new("file_type", DataType::Utf8, false),
        Field::new("chunk_index", DataType::Int32, false),
        Field::new("created_at", DataType::Utf8, false),
    ]))
}

// ---------------------------------------------------------------------------
// VectorStore implementation
// ---------------------------------------------------------------------------

impl VectorStore {
    /// Open or create the vector store at the given directory.
    ///
    /// No tables are created at this point — they are created lazily on the
    /// first insert, once the embedding dimension is known.
    pub async fn new(store_dir: &Path, embedder: Embedder) -> Result<Self> {
        let db = lancedb::connect(store_dir.to_str().ok_or_else(|| {
            SafeAgentError::VectorStore("store directory path is not valid UTF-8".into())
        })?)
        .execute()
        .await
        .map_err(|e| SafeAgentError::VectorStore(format!("failed to open LanceDB: {e}")))?;

        // If the embedder already knows its dimension (e.g. local model),
        // seed the cached value.
        let dim = embedder.dim();

        debug!(
            store_dir = %store_dir.display(),
            dim = ?dim,
            "vector store opened"
        );

        Ok(Self {
            db,
            embedder,
            dim: RwLock::new(dim),
        })
    }

    /// Store text as a semantic memory.
    ///
    /// Returns the generated UUID for the stored memory.
    pub async fn remember(
        &self,
        content: &str,
        category: &str,
        source: &str,
    ) -> Result<String> {
        let vector = self.embedder.embed_one(content).await?;
        let dim = self.ensure_dim(vector.len()).await?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let schema = memories_schema(dim as i32);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![content])),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        vec![Some(vector.into_iter().map(Some).collect::<Vec<_>>())],
                        dim as i32,
                    ),
                ),
                Arc::new(StringArray::from(vec![category])),
                Arc::new(StringArray::from(vec![source])),
                Arc::new(StringArray::from(vec![now.as_str()])),
            ],
        )
        .map_err(|e| {
            SafeAgentError::VectorStore(format!("failed to build memories RecordBatch: {e}"))
        })?;

        self.upsert_into_table(MEMORIES_TABLE, schema, batch).await?;

        debug!(id = %id, category = %category, "memory stored");
        Ok(id)
    }

    /// Ingest file chunks into the documents table.
    ///
    /// Returns the number of chunks ingested.
    pub async fn ingest_chunks(
        &self,
        chunks: &[Chunk],
        file_path: &str,
        file_type: &str,
    ) -> Result<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }

        // Embed all chunk texts in a single batch call.
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        let vectors = self.embedder.embed_batch(&texts).await?;

        if vectors.is_empty() {
            return Ok(0);
        }

        let dim = self.ensure_dim(vectors[0].len()).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let count = chunks.len();

        let ids: Vec<String> = (0..count).map(|_| uuid::Uuid::new_v4().to_string()).collect();
        let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let content_refs: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        let file_paths: Vec<&str> = vec![file_path; count];
        let file_types: Vec<&str> = vec![file_type; count];
        let chunk_indices: Vec<i32> = chunks.iter().map(|c| c.index as i32).collect();
        let created_ats: Vec<&str> = vec![now.as_str(); count];

        let vector_data: Vec<Option<Vec<Option<f32>>>> = vectors
            .into_iter()
            .map(|v| Some(v.into_iter().map(Some).collect()))
            .collect();

        let schema = documents_schema(dim as i32);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(id_refs)),
                Arc::new(StringArray::from(content_refs)),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        vector_data,
                        dim as i32,
                    ),
                ),
                Arc::new(StringArray::from(file_paths)),
                Arc::new(StringArray::from(file_types)),
                Arc::new(Int32Array::from(chunk_indices)),
                Arc::new(StringArray::from(created_ats)),
            ],
        )
        .map_err(|e| {
            SafeAgentError::VectorStore(format!("failed to build documents RecordBatch: {e}"))
        })?;

        self.upsert_into_table(DOCUMENTS_TABLE, schema, batch)
            .await?;

        debug!(count = count, file_path = %file_path, "chunks ingested");
        Ok(count)
    }

    /// Search for similar content.
    ///
    /// Pass `"memories"`, `"documents"`, or `"all"` as `table_name`.
    /// When `"all"`, both tables are searched and results are merged by score.
    pub async fn search(
        &self,
        query: &str,
        table_name: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let vector = self.embedder.embed_one(query).await?;

        let tables_to_search: Vec<&str> = match table_name {
            "all" => vec![MEMORIES_TABLE, DOCUMENTS_TABLE],
            other => vec![other],
        };

        let mut all_results = Vec::new();

        for tbl in tables_to_search {
            match self.search_single_table(tbl, &vector, limit).await {
                Ok(results) => all_results.extend(results),
                Err(e) => {
                    // Table may not exist yet (lazy creation). That is not an
                    // error; it just means there are no results from that table.
                    let msg = e.to_string();
                    if msg.contains("was not found") || msg.contains("TableNotFound") {
                        debug!(table = tbl, "table not found during search, skipping");
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Sort by score descending (higher = more similar).
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(limit);

        Ok(all_results)
    }

    /// Return the list of table names currently in the store.
    pub async fn table_names(&self) -> Result<Vec<String>> {
        self.db
            .table_names()
            .execute()
            .await
            .map_err(|e| SafeAgentError::VectorStore(format!("failed to list tables: {e}")))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Ensure the embedding dimension is known.  If not yet cached, store it
    /// from the given observed length.  Returns the dimension.
    async fn ensure_dim(&self, observed: usize) -> Result<usize> {
        {
            let cached = self.dim.read().await;
            if let Some(d) = *cached {
                if d != observed {
                    return Err(SafeAgentError::VectorStore(format!(
                        "embedding dimension mismatch: cached {d}, got {observed}"
                    )));
                }
                return Ok(d);
            }
        }

        // First time — write the dimension.
        let mut cached = self.dim.write().await;
        // Double-check after acquiring write lock (another task may have set it).
        if let Some(d) = *cached {
            if d != observed {
                return Err(SafeAgentError::VectorStore(format!(
                    "embedding dimension mismatch: cached {d}, got {observed}"
                )));
            }
            return Ok(d);
        }

        debug!(dim = observed, "embedding dimension discovered");
        *cached = Some(observed);
        Ok(observed)
    }

    /// Insert a `RecordBatch` into a table, creating the table lazily if it
    /// does not yet exist.
    async fn upsert_into_table(
        &self,
        table_name: &str,
        schema: Arc<Schema>,
        batch: RecordBatch,
    ) -> Result<()> {
        let existing = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| SafeAgentError::VectorStore(format!("failed to list tables: {e}")))?;

        if existing.iter().any(|n| n == table_name) {
            // Table already exists — append.
            let table = self
                .db
                .open_table(table_name)
                .execute()
                .await
                .map_err(|e| {
                    SafeAgentError::VectorStore(format!(
                        "failed to open table '{table_name}': {e}"
                    ))
                })?;

            let batches = RecordBatchIterator::new(
                vec![Ok(batch)],
                schema,
            );

            table
                .add(batches)
                .execute()
                .await
                .map_err(|e| {
                    SafeAgentError::VectorStore(format!(
                        "failed to add data to '{table_name}': {e}"
                    ))
                })?;
        } else {
            // Table does not exist — create it with the initial data.
            let batches = RecordBatchIterator::new(
                vec![Ok(batch)],
                schema,
            );

            self.db
                .create_table(table_name, batches)
                .execute()
                .await
                .map_err(|e| {
                    SafeAgentError::VectorStore(format!(
                        "failed to create table '{table_name}': {e}"
                    ))
                })?;
        }

        Ok(())
    }

    /// Search a single table for vectors nearest to `query_vec`.
    async fn search_single_table(
        &self,
        table_name: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let table = self
            .db
            .open_table(table_name)
            .execute()
            .await
            .map_err(|e| {
                SafeAgentError::VectorStore(format!(
                    "failed to open table '{table_name}': {e}"
                ))
            })?;

        let query = table
            .vector_search(query_vec)
            .map_err(|e| {
                SafeAgentError::VectorStore(format!(
                    "failed to build vector query on '{table_name}': {e}"
                ))
            })?
            .limit(limit);

        let stream = query
            .execute()
            .await
            .map_err(|e| {
                SafeAgentError::VectorStore(format!(
                    "failed to execute search on '{table_name}': {e}"
                ))
            })?;

        let batches: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            SafeAgentError::VectorStore(format!(
                "failed to collect search results from '{table_name}': {e}"
            ))
        })?;

        let mut results = Vec::new();

        for batch in &batches {
            let num_rows = batch.num_rows();
            if num_rows == 0 {
                continue;
            }

            let id_col = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let content_col = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let distance_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>());

            let (id_col, content_col, distance_col) =
                match (id_col, content_col, distance_col) {
                    (Some(i), Some(c), Some(d)) => (i, c, d),
                    _ => {
                        warn!(
                            table = table_name,
                            "search result batch missing expected columns, skipping"
                        );
                        continue;
                    }
                };

            for row in 0..num_rows {
                let id = id_col.value(row).to_string();
                let content = content_col.value(row).to_string();
                let distance = distance_col.value(row);
                // Convert L2 distance to a similarity score in [0, 1].
                let score = 1.0 / (1.0 + distance);

                results.push(SearchResult {
                    id,
                    content,
                    table: table_name.to_string(),
                    score,
                });
            }
        }

        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use embed::ApiEmbedder;

    /// Helper: create an ApiEmbedder pointing at a non-existent server so
    /// that any actual API call will fail fast.
    fn dummy_embedder() -> Embedder {
        Embedder::Api(
            ApiEmbedder::new(
                "test-key".to_string(),
                None,
                Some("http://127.0.0.1:1".to_string()),
                None,
            )
            .expect("should construct dummy embedder"),
        )
    }

    #[tokio::test]
    async fn new_creates_store_in_temp_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder()).await;
        assert!(store.is_ok(), "VectorStore::new failed: {:?}", store.err());
    }

    #[tokio::test]
    async fn table_names_starts_empty() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder())
            .await
            .expect("create store");

        let names = store.table_names().await.expect("list tables");
        assert!(
            names.is_empty(),
            "expected no tables on fresh store, got: {names:?}"
        );
    }

    #[tokio::test]
    async fn remember_fails_with_bad_embedder() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder())
            .await
            .expect("create store");

        // The dummy embedder points at an unreachable server, so embed_one
        // should fail, which means remember() propagates the error.
        let result = store.remember("hello world", "test", "unit-test").await;
        assert!(result.is_err(), "expected error from bad embedder");
    }

    #[tokio::test]
    async fn search_on_missing_table_returns_empty() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder())
            .await
            .expect("create store");

        // search_single_table on a nonexistent table should be caught
        // gracefully by the search() method.
        // But since search() also calls embed_one first, it will fail at
        // embedding. Test the inner method directly instead.
        let result = store
            .search_single_table("nonexistent", &[0.0; 128], 5)
            .await;
        assert!(result.is_err(), "expected error for nonexistent table");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("was not found") || msg.contains("TableNotFound") || msg.contains("failed to open"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn ensure_dim_caches_dimension() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder())
            .await
            .expect("create store");

        // Dimension should not be cached yet (API embedder returns None).
        {
            let dim = store.dim.read().await;
            assert_eq!(*dim, None);
        }

        // Discover dimension.
        let d = store.ensure_dim(384).await.expect("set dim");
        assert_eq!(d, 384);

        // Now it should be cached.
        {
            let dim = store.dim.read().await;
            assert_eq!(*dim, Some(384));
        }

        // Same dimension should succeed.
        let d2 = store.ensure_dim(384).await.expect("same dim ok");
        assert_eq!(d2, 384);
    }

    #[tokio::test]
    async fn ensure_dim_rejects_mismatch() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = VectorStore::new(dir.path(), dummy_embedder())
            .await
            .expect("create store");

        store.ensure_dim(384).await.expect("set dim");

        let result = store.ensure_dim(512).await;
        assert!(result.is_err(), "expected dimension mismatch error");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("mismatch"), "unexpected error: {msg}");
    }

    #[test]
    fn memories_schema_has_correct_fields() {
        let schema = memories_schema(128);
        assert_eq!(schema.fields().len(), 6);
        assert!(schema.field_with_name("id").is_ok());
        assert!(schema.field_with_name("content").is_ok());
        assert!(schema.field_with_name("vector").is_ok());
        assert!(schema.field_with_name("category").is_ok());
        assert!(schema.field_with_name("source").is_ok());
        assert!(schema.field_with_name("created_at").is_ok());

        // Verify vector field is FixedSizeList(Float32, 128).
        let vector_field = schema.field_with_name("vector").unwrap();
        match vector_field.data_type() {
            DataType::FixedSizeList(inner, size) => {
                assert_eq!(*size, 128);
                assert_eq!(*inner.data_type(), DataType::Float32);
            }
            other => panic!("expected FixedSizeList, got {other:?}"),
        }
    }

    #[test]
    fn documents_schema_has_correct_fields() {
        let schema = documents_schema(256);
        assert_eq!(schema.fields().len(), 7);
        assert!(schema.field_with_name("id").is_ok());
        assert!(schema.field_with_name("content").is_ok());
        assert!(schema.field_with_name("vector").is_ok());
        assert!(schema.field_with_name("file_path").is_ok());
        assert!(schema.field_with_name("file_type").is_ok());
        assert!(schema.field_with_name("chunk_index").is_ok());
        assert!(schema.field_with_name("created_at").is_ok());

        let vector_field = schema.field_with_name("vector").unwrap();
        match vector_field.data_type() {
            DataType::FixedSizeList(inner, size) => {
                assert_eq!(*size, 256);
                assert_eq!(*inner.data_type(), DataType::Float32);
            }
            other => panic!("expected FixedSizeList, got {other:?}"),
        }
    }

    #[test]
    fn search_result_is_debug_and_clone() {
        let r = SearchResult {
            id: "abc".to_string(),
            content: "hello".to_string(),
            table: "memories".to_string(),
            score: 0.95,
        };
        let r2 = r.clone();
        assert_eq!(r2.id, "abc");
        assert_eq!(format!("{r:?}").len() > 0, true);
    }
}
