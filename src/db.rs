use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use crate::error::Result;

pub fn open(path: &Path) -> Result<Connection> {
    info!("opening database at {}", path.display());
    let conn = Connection::open(path)?;

    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- Conversation history
        CREATE TABLE IF NOT EXISTS conversation_history (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            role        TEXT NOT NULL,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Core memory (single-row personality)
        CREATE TABLE IF NOT EXISTS core_memory (
            id          INTEGER PRIMARY KEY CHECK (id = 1),
            personality TEXT NOT NULL,
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Archival memory
        CREATE TABLE IF NOT EXISTS archival_memory (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            content     TEXT NOT NULL,
            category    TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS archival_memory_fts USING fts5(
            content,
            category,
            content='archival_memory',
            content_rowid='id'
        );

        CREATE TRIGGER IF NOT EXISTS archival_ai AFTER INSERT ON archival_memory BEGIN
            INSERT INTO archival_memory_fts(rowid, content, category)
            VALUES (new.id, new.content, new.category);
        END;

        CREATE TRIGGER IF NOT EXISTS archival_ad AFTER DELETE ON archival_memory BEGIN
            INSERT INTO archival_memory_fts(archival_memory_fts, rowid, content, category)
            VALUES ('delete', old.id, old.content, old.category);
        END;

        CREATE TRIGGER IF NOT EXISTS archival_au AFTER UPDATE ON archival_memory BEGIN
            INSERT INTO archival_memory_fts(archival_memory_fts, rowid, content, category)
            VALUES ('delete', old.id, old.content, old.category);
            INSERT INTO archival_memory_fts(rowid, content, category)
            VALUES (new.id, new.content, new.category);
        END;

        -- Activity log
        CREATE TABLE IF NOT EXISTS activity_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            action_type TEXT NOT NULL,
            summary     TEXT NOT NULL,
            detail      TEXT,
            status      TEXT NOT NULL DEFAULT 'ok',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Pending actions (approval queue)
        CREATE TABLE IF NOT EXISTS pending_actions (
            id          TEXT PRIMARY KEY,
            action_json TEXT NOT NULL,
            reasoning   TEXT NOT NULL DEFAULT '',
            context     TEXT NOT NULL DEFAULT '',
            status      TEXT NOT NULL DEFAULT 'pending',
            proposed_at TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at TEXT
        );

        -- Agent stats
        CREATE TABLE IF NOT EXISTS agent_stats (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            total_ticks     INTEGER NOT NULL DEFAULT 0,
            total_actions   INTEGER NOT NULL DEFAULT 0,
            total_approved  INTEGER NOT NULL DEFAULT 0,
            total_rejected  INTEGER NOT NULL DEFAULT 0,
            last_tick_at    TEXT,
            started_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        INSERT OR IGNORE INTO agent_stats (id) VALUES (1);

        -- Knowledge graph: nodes
        CREATE TABLE IF NOT EXISTS knowledge_nodes (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            label       TEXT NOT NULL,
            node_type   TEXT NOT NULL DEFAULT '',
            content     TEXT NOT NULL DEFAULT '',
            confidence  REAL NOT NULL DEFAULT 1.0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Knowledge graph: edges
        CREATE TABLE IF NOT EXISTS knowledge_edges (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id   INTEGER NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
            target_id   INTEGER NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
            relation    TEXT NOT NULL,
            weight      REAL NOT NULL DEFAULT 1.0,
            metadata    TEXT NOT NULL DEFAULT '{}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(source_id, target_id, relation)
        );

        -- Knowledge graph: FTS index
        CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_nodes_fts USING fts5(
            label, content, node_type,
            content='knowledge_nodes',
            content_rowid='id'
        );

        CREATE TRIGGER IF NOT EXISTS knowledge_ai AFTER INSERT ON knowledge_nodes BEGIN
            INSERT INTO knowledge_nodes_fts(rowid, label, content, node_type)
            VALUES (new.id, new.label, new.content, new.node_type);
        END;

        CREATE TRIGGER IF NOT EXISTS knowledge_ad AFTER DELETE ON knowledge_nodes BEGIN
            INSERT INTO knowledge_nodes_fts(knowledge_nodes_fts, rowid, label, content, node_type)
            VALUES ('delete', old.id, old.label, old.content, old.node_type);
        END;

        CREATE TRIGGER IF NOT EXISTS knowledge_au AFTER UPDATE ON knowledge_nodes BEGIN
            INSERT INTO knowledge_nodes_fts(knowledge_nodes_fts, rowid, label, content, node_type)
            VALUES ('delete', old.id, old.label, old.content, old.node_type);
            INSERT INTO knowledge_nodes_fts(rowid, label, content, node_type)
            VALUES (new.id, new.label, new.content, new.node_type);
        END;

        -- OAuth tokens (multi-account per provider)
        CREATE TABLE IF NOT EXISTS oauth_tokens (
            provider      TEXT NOT NULL,
            account       TEXT NOT NULL DEFAULT '',
            email         TEXT NOT NULL DEFAULT '',
            access_token  TEXT NOT NULL,
            refresh_token TEXT,
            expires_at    TEXT,
            scopes        TEXT NOT NULL DEFAULT '',
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (provider, account)
        );

        -- Cron jobs
        CREATE TABLE IF NOT EXISTS cron_jobs (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            schedule    TEXT NOT NULL,
            tool_call   TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            last_run_at TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Sessions (multi-agent)
        CREATE TABLE IF NOT EXISTS sessions (
            id          TEXT PRIMARY KEY,
            label       TEXT NOT NULL DEFAULT '',
            agent_id    TEXT NOT NULL DEFAULT 'default',
            status      TEXT NOT NULL DEFAULT 'active',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS session_messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            role        TEXT NOT NULL,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )?;

    // Migrate oauth_tokens from single-account to multi-account schema.
    // Check if the 'account' column exists; if not, recreate the table.
    let has_account_col: bool = conn
        .prepare("SELECT account FROM oauth_tokens LIMIT 0")
        .is_ok();

    if !has_account_col {
        info!("migrating oauth_tokens to multi-account schema");
        conn.execute_batch(
            "
            ALTER TABLE oauth_tokens RENAME TO oauth_tokens_old;

            CREATE TABLE oauth_tokens (
                provider      TEXT NOT NULL,
                account       TEXT NOT NULL DEFAULT '',
                email         TEXT NOT NULL DEFAULT '',
                access_token  TEXT NOT NULL,
                refresh_token TEXT,
                expires_at    TEXT,
                scopes        TEXT NOT NULL DEFAULT '',
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (provider, account)
            );

            INSERT INTO oauth_tokens (provider, account, email, access_token, refresh_token, expires_at, scopes, created_at, updated_at)
                SELECT provider, 'default', '', access_token, refresh_token, expires_at, scopes, created_at, updated_at
                FROM oauth_tokens_old;

            DROP TABLE oauth_tokens_old;
            ",
        )?;
        info!("oauth_tokens migration complete");
    }

    info!("database migrations complete");
    Ok(())
}
