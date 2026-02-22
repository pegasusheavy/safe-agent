//! Multi-user support — user management, roles, and identity mapping.
//!
//! Users can be created via config, the dashboard API, or auto-provisioned
//! when a message arrives from a mapped Telegram/WhatsApp account.

use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::crypto::FieldEncryptor;
use crate::error::{Result, SafeAgentError};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Permission role for a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Full access: manage users, approve dangerous tools, configure agent.
    Admin,
    /// Can chat, trigger tools (subject to approval), view dashboard.
    User,
    /// Read-only dashboard access; cannot send messages or trigger tools.
    Viewer,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
            Self::Viewer => "viewer",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => Self::Admin,
            "viewer" => Self::Viewer,
            _ => Self::User,
        }
    }

    /// Whether this role can send messages and trigger tool execution.
    pub fn can_chat(&self) -> bool {
        matches!(self, Self::Admin | Self::User)
    }

}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A registered user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub telegram_id: Option<i64>,
    pub whatsapp_id: Option<String>,
    pub imessage_id: Option<String>,
    pub twilio_number: Option<String>,
    pub android_sms_id: Option<String>,
    pub discord_id: Option<String>,
    pub signal_id: Option<String>,
    pub enabled: bool,
    /// IANA timezone name (e.g. "America/New_York"). Empty = use system default.
    #[serde(default)]
    pub timezone: String,
    /// BCP 47 locale tag (e.g. "en-US"). Empty = use system default.
    #[serde(default)]
    pub locale: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Context about the user who triggered an action.  Threaded through
/// `handle_message`, tool execution, and audit logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    /// Where this user connected from: "dashboard", "telegram", "whatsapp", "api".
    pub source: String,
}

impl UserContext {
    /// Create a context from a User and a source platform.
    pub fn from_user(user: &User, source: &str) -> Self {
        Self {
            user_id: user.id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role,
            source: source.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// User Manager
// ---------------------------------------------------------------------------

/// Manages user CRUD and lookup operations.
pub struct UserManager {
    pub(crate) db: Arc<Mutex<Connection>>,
    pub(crate) enc: Arc<FieldEncryptor>,
}

impl UserManager {
    pub fn new(db: Arc<Mutex<Connection>>, enc: Arc<FieldEncryptor>) -> Self {
        Self { db, enc }
    }

    /// Create a new user. Returns the created user.
    pub async fn create(&self, username: &str, display_name: &str, role: UserRole, password: &str) -> Result<User> {
        let db = self.db.lock().await;

        // Check for duplicates
        let exists: bool = db.query_row(
            "SELECT COUNT(*) > 0 FROM users WHERE username = ?1",
            [username],
            |row| row.get(0),
        )?;
        if exists {
            return Err(SafeAgentError::Config(format!("user '{username}' already exists")));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let enc_display = self.enc.encrypt(display_name);
        let enc_password = self.enc.encrypt(password);

        db.execute(
            "INSERT INTO users (id, username, display_name, role, password_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, username, enc_display, role.as_str(), enc_password],
        )?;

        info!(username, role = role.as_str(), "user created");

        self.get_by_id_sync(&db, &id)
    }

    /// Get a user by ID.
    pub async fn get_by_id(&self, user_id: &str) -> Result<User> {
        let db = self.db.lock().await;
        self.get_by_id_sync(&db, user_id)
    }

    fn get_by_id_sync(&self, db: &Connection, user_id: &str) -> Result<User> {
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE id = ?1",
            [user_id],
            row_to_user_raw,
        )
        .map(|raw| raw.decrypt(&self.enc))
        .map_err(|_| SafeAgentError::Config(format!("user not found: {user_id}")))
    }

    /// Get a user by username.
    pub async fn get_by_username(&self, username: &str) -> Option<User> {
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE username = ?1",
            [username],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Get a user by email (uses blind index for lookup).
    pub async fn get_by_email(&self, email: &str) -> Option<User> {
        let blind = self.enc.blind_index(email);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE email_blind = ?1 AND email_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by Telegram user ID (uses blind index).
    pub async fn get_by_telegram_id(&self, telegram_id: i64) -> Option<User> {
        let blind = self.enc.blind_index_i64(telegram_id);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE telegram_id_blind = ?1 AND telegram_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by WhatsApp ID (uses blind index).
    pub async fn get_by_whatsapp_id(&self, whatsapp_id: &str) -> Option<User> {
        let blind = self.enc.blind_index(whatsapp_id);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE whatsapp_id_blind = ?1 AND whatsapp_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by iMessage ID (uses blind index).
    pub async fn get_by_imessage_id(&self, imessage_id: &str) -> Option<User> {
        let blind = self.enc.blind_index(imessage_id);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE imessage_id_blind = ?1 AND imessage_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by Twilio number (uses blind index).
    pub async fn get_by_twilio_number(&self, number: &str) -> Option<User> {
        let blind = self.enc.blind_index(number);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE twilio_number_blind = ?1 AND twilio_number_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by Android SMS ID (uses blind index).
    pub async fn get_by_android_sms_id(&self, number: &str) -> Option<User> {
        let blind = self.enc.blind_index(number);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE android_sms_id_blind = ?1 AND android_sms_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by Discord ID (uses blind index).
    pub async fn get_by_discord_id(&self, discord_id: &str) -> Option<User> {
        let blind = self.enc.blind_index(discord_id);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE discord_id_blind = ?1 AND discord_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Look up a user by Signal ID / phone number (uses blind index).
    pub async fn get_by_signal_id(&self, signal_id: &str) -> Option<User> {
        let blind = self.enc.blind_index(signal_id);
        let db = self.db.lock().await;
        db.query_row(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users WHERE signal_id_blind = ?1 AND signal_id_blind != ''",
            [&blind],
            row_to_user_raw,
        )
        .ok()
        .map(|raw| raw.decrypt(&self.enc))
    }

    /// Authenticate a user by username and password. Returns the user if valid.
    ///
    /// The stored password is decrypted transparently (it's already
    /// decrypted by `row_to_user_raw.decrypt()`).
    pub async fn authenticate(&self, username: &str, password: &str) -> Option<User> {
        let user = self.get_by_username(username).await?;
        if !user.enabled {
            warn!(username, "login attempt for disabled user");
            return None;
        }
        if user.password_hash == password {
            // Update last_seen_at
            let db = self.db.lock().await;
            let _ = db.execute(
                "UPDATE users SET last_seen_at = datetime('now') WHERE id = ?1",
                [&user.id],
            );
            Some(user)
        } else {
            None
        }
    }

    /// List all users.
    pub async fn list(&self) -> Vec<User> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, username, display_name, role, email, password_hash, telegram_id, whatsapp_id, imessage_id, twilio_number, android_sms_id, discord_id, signal_id, timezone, locale, enabled, last_seen_at, created_at, updated_at
             FROM users ORDER BY created_at",
        ).unwrap();
        let enc = &self.enc;
        stmt.query_map([], row_to_user_raw)
            .unwrap()
            .filter_map(|r| r.ok())
            .map(|raw| raw.decrypt(enc))
            .collect()
    }

    /// Update a user's profile fields.
    pub async fn update(&self, user_id: &str, display_name: Option<&str>, role: Option<UserRole>, email: Option<&str>, enabled: Option<bool>) -> Result<User> {
        let db = self.db.lock().await;

        if let Some(name) = display_name {
            let enc_name = self.enc.encrypt(name);
            db.execute("UPDATE users SET display_name = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![enc_name, user_id])?;
        }
        if let Some(r) = role {
            db.execute("UPDATE users SET role = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![r.as_str(), user_id])?;
        }
        if let Some(e) = email {
            let enc_email = self.enc.encrypt(e);
            let blind = if e.is_empty() { String::new() } else { self.enc.blind_index(e) };
            db.execute("UPDATE users SET email = ?1, email_blind = ?2, updated_at = datetime('now') WHERE id = ?3", rusqlite::params![enc_email, blind, user_id])?;
        }
        if let Some(en) = enabled {
            db.execute("UPDATE users SET enabled = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![en as i32, user_id])?;
        }

        self.get_by_id_sync(&db, user_id)
    }

    /// Link a Telegram ID to a user.
    pub async fn link_telegram(&self, user_id: &str, telegram_id: i64) -> Result<()> {
        let enc_tid = self.enc.encrypt(&telegram_id.to_string());
        let blind = self.enc.blind_index_i64(telegram_id);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET telegram_id = ?1, telegram_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc_tid, blind, user_id],
        )?;
        info!(user_id, "linked Telegram ID (encrypted)");
        Ok(())
    }

    /// Link a WhatsApp ID to a user.
    pub async fn link_whatsapp(&self, user_id: &str, whatsapp_id: &str) -> Result<()> {
        let enc_wid = self.enc.encrypt(whatsapp_id);
        let blind = self.enc.blind_index(whatsapp_id);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET whatsapp_id = ?1, whatsapp_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc_wid, blind, user_id],
        )?;
        info!(user_id, "linked WhatsApp ID (encrypted)");
        Ok(())
    }

    /// Link an iMessage ID to a user.
    #[allow(dead_code)]
    pub async fn link_imessage(&self, user_id: &str, imessage_id: &str) -> Result<()> {
        let enc = self.enc.encrypt(imessage_id);
        let blind = self.enc.blind_index(imessage_id);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET imessage_id = ?1, imessage_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc, blind, user_id],
        )?;
        info!(user_id, "linked iMessage ID (encrypted)");
        Ok(())
    }

    /// Link a Twilio number to a user.
    #[allow(dead_code)]
    pub async fn link_twilio(&self, user_id: &str, number: &str) -> Result<()> {
        let enc = self.enc.encrypt(number);
        let blind = self.enc.blind_index(number);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET twilio_number = ?1, twilio_number_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc, blind, user_id],
        )?;
        info!(user_id, "linked Twilio number (encrypted)");
        Ok(())
    }

    /// Link an Android SMS ID to a user.
    #[allow(dead_code)]
    pub async fn link_android_sms(&self, user_id: &str, number: &str) -> Result<()> {
        let enc = self.enc.encrypt(number);
        let blind = self.enc.blind_index(number);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET android_sms_id = ?1, android_sms_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc, blind, user_id],
        )?;
        info!(user_id, "linked Android SMS ID (encrypted)");
        Ok(())
    }

    /// Link a Discord ID to a user.
    #[allow(dead_code)]
    pub async fn link_discord(&self, user_id: &str, discord_id: &str) -> Result<()> {
        let enc = self.enc.encrypt(discord_id);
        let blind = self.enc.blind_index(discord_id);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET discord_id = ?1, discord_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc, blind, user_id],
        )?;
        info!(user_id, "linked Discord ID (encrypted)");
        Ok(())
    }

    /// Link a Signal ID (phone number) to a user.
    #[allow(dead_code)]
    pub async fn link_signal(&self, user_id: &str, signal_id: &str) -> Result<()> {
        let enc = self.enc.encrypt(signal_id);
        let blind = self.enc.blind_index(signal_id);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET signal_id = ?1, signal_id_blind = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![enc, blind, user_id],
        )?;
        info!(user_id, "linked Signal ID (encrypted)");
        Ok(())
    }

    /// Set a user's timezone (IANA name, e.g. "America/New_York").
    pub async fn set_timezone(&self, user_id: &str, timezone: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET timezone = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![timezone, user_id],
        )?;
        info!(user_id, timezone, "user timezone updated");
        Ok(())
    }

    /// Set a user's locale (BCP 47 tag, e.g. "en-US").
    pub async fn set_locale(&self, user_id: &str, locale: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET locale = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![locale, user_id],
        )?;
        info!(user_id, locale, "user locale updated");
        Ok(())
    }

    /// Change a user's password.
    pub async fn set_password(&self, user_id: &str, password: &str) -> Result<()> {
        let enc_pw = self.enc.encrypt(password);
        let db = self.db.lock().await;
        db.execute(
            "UPDATE users SET password_hash = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![enc_pw, user_id],
        )?;
        Ok(())
    }

    /// Delete a user.
    pub async fn delete(&self, user_id: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute("DELETE FROM users WHERE id = ?1", [user_id])?;
        info!(user_id, "user deleted");
        Ok(())
    }

    /// Update last_seen_at timestamp.
    pub async fn touch(&self, user_id: &str) {
        let db = self.db.lock().await;
        let _ = db.execute(
            "UPDATE users SET last_seen_at = datetime('now') WHERE id = ?1",
            [user_id],
        );
    }

    /// Count total users.
    pub async fn count(&self) -> i64 {
        let db = self.db.lock().await;
        db.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap_or(0)
    }

    /// Migrate any plaintext PII data to encrypted form.
    ///
    /// Scans all users; for each PII field that doesn't start with `ENC$`,
    /// encrypts it in place and populates the corresponding blind index.
    /// Safe to run repeatedly — already-encrypted fields are skipped.
    pub async fn migrate_encrypt_pii(&self) -> Result<usize> {
        let db = self.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT id, display_name, email, password_hash, telegram_id, whatsapp_id, totp_secret, recovery_codes FROM users",
        )?;

        struct Row {
            id: String,
            display_name: String,
            email: String,
            password_hash: String,
            telegram_id: Option<String>,
            whatsapp_id: Option<String>,
            totp_secret: Option<String>,
            recovery_codes: Option<String>,
        }

        let rows: Vec<Row> = stmt.query_map([], |row| {
            Ok(Row {
                id: row.get(0)?,
                display_name: row.get(1)?,
                email: row.get(2)?,
                password_hash: row.get(3)?,
                telegram_id: row.get(4)?,
                whatsapp_id: row.get(5)?,
                totp_secret: row.get(6)?,
                recovery_codes: row.get(7)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        let mut migrated = 0usize;

        for row in &rows {
            let mut needs_update = false;

            let enc_display = if FieldEncryptor::is_plaintext(&row.display_name) {
                needs_update = true;
                self.enc.encrypt(&row.display_name)
            } else {
                row.display_name.clone()
            };

            let enc_email = if FieldEncryptor::is_plaintext(&row.email) {
                needs_update = true;
                self.enc.encrypt(&row.email)
            } else {
                row.email.clone()
            };

            let email_blind = if !row.email.is_empty() {
                // Compute blind index from the plaintext email
                let plain = self.enc.decrypt(&enc_email).unwrap_or(row.email.clone());
                self.enc.blind_index(&plain)
            } else {
                String::new()
            };

            let enc_pw = if FieldEncryptor::is_plaintext(&row.password_hash) {
                needs_update = true;
                self.enc.encrypt(&row.password_hash)
            } else {
                row.password_hash.clone()
            };

            let (enc_tid, tid_blind) = match &row.telegram_id {
                Some(v) if FieldEncryptor::is_plaintext(v) => {
                    needs_update = true;
                    (Some(self.enc.encrypt(v)), self.enc.blind_index(v))
                }
                Some(v) => {
                    let plain = self.enc.decrypt(v).unwrap_or(v.clone());
                    (Some(v.clone()), self.enc.blind_index(&plain))
                }
                None => (None, String::new()),
            };

            let (enc_wid, wid_blind) = match &row.whatsapp_id {
                Some(v) if FieldEncryptor::is_plaintext(v) => {
                    needs_update = true;
                    (Some(self.enc.encrypt(v)), self.enc.blind_index(v))
                }
                Some(v) => {
                    let plain = self.enc.decrypt(v).unwrap_or(v.clone());
                    (Some(v.clone()), self.enc.blind_index(&plain))
                }
                None => (None, String::new()),
            };

            let enc_totp = match &row.totp_secret {
                Some(v) if FieldEncryptor::is_plaintext(v) => {
                    needs_update = true;
                    Some(self.enc.encrypt(v))
                }
                other => other.clone(),
            };

            let enc_recovery = match &row.recovery_codes {
                Some(v) if FieldEncryptor::is_plaintext(v) => {
                    needs_update = true;
                    Some(self.enc.encrypt(v))
                }
                other => other.clone(),
            };

            if needs_update {
                db.execute(
                    "UPDATE users SET display_name=?1, email=?2, email_blind=?3, password_hash=?4,
                     telegram_id=?5, telegram_id_blind=?6, whatsapp_id=?7, whatsapp_id_blind=?8,
                     totp_secret=?9, recovery_codes=?10
                     WHERE id=?11",
                    rusqlite::params![
                        enc_display, enc_email, email_blind, enc_pw,
                        enc_tid, tid_blind, enc_wid, wid_blind,
                        enc_totp, enc_recovery,
                        row.id,
                    ],
                )?;
                migrated += 1;
            }
        }

        if migrated > 0 {
            info!(count = migrated, "encrypted PII for existing users");
        }
        Ok(migrated)
    }
}

/// Row mapper for user queries (raw — no decryption).
fn row_to_user_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawUser> {
    Ok(RawUser {
        id: row.get(0)?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        role_str: row.get(3)?,
        email: row.get(4)?,
        password_hash: row.get(5)?,
        telegram_id_str: row.get(6)?,
        whatsapp_id: row.get(7)?,
        imessage_id: row.get(8)?,
        twilio_number: row.get(9)?,
        android_sms_id: row.get(10)?,
        discord_id: row.get(11)?,
        signal_id: row.get(12)?,
        timezone: row.get(13)?,
        locale: row.get(14)?,
        enabled_int: row.get(15)?,
        last_seen_at: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

/// Intermediate row before decryption.
struct RawUser {
    id: String,
    username: String,
    display_name: String,
    role_str: String,
    email: String,
    password_hash: String,
    telegram_id_str: Option<String>,
    whatsapp_id: Option<String>,
    imessage_id: Option<String>,
    twilio_number: Option<String>,
    android_sms_id: Option<String>,
    discord_id: Option<String>,
    signal_id: Option<String>,
    timezone: String,
    locale: String,
    enabled_int: i32,
    last_seen_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawUser {
    /// Decrypt PII fields into a proper `User`.
    fn decrypt(self, enc: &FieldEncryptor) -> User {
        let display_name = enc.decrypt(&self.display_name).unwrap_or(self.display_name);
        let email = enc.decrypt(&self.email).unwrap_or(self.email);
        let password_hash = enc.decrypt(&self.password_hash).unwrap_or(self.password_hash);
        let whatsapp_id = self.whatsapp_id.map(|v| enc.decrypt(&v).unwrap_or(v));
        let imessage_id = self.imessage_id.map(|v| enc.decrypt(&v).unwrap_or(v));
        let twilio_number = self.twilio_number.map(|v| enc.decrypt(&v).unwrap_or(v));
        let android_sms_id = self.android_sms_id.map(|v| enc.decrypt(&v).unwrap_or(v));
        let discord_id = self.discord_id.map(|v| enc.decrypt(&v).unwrap_or(v));
        let signal_id = self.signal_id.map(|v| enc.decrypt(&v).unwrap_or(v));

        // telegram_id is stored as encrypted TEXT now; decrypt and parse
        let telegram_id: Option<i64> = self.telegram_id_str.and_then(|v| {
            let plain = enc.decrypt(&v).unwrap_or(v);
            plain.parse().ok()
        });

        User {
            id: self.id,
            username: self.username,
            display_name,
            role: UserRole::from_str(&self.role_str),
            email,
            password_hash,
            telegram_id,
            whatsapp_id,
            imessage_id,
            twilio_number,
            android_sms_id,
            discord_id,
            signal_id,
            timezone: self.timezone,
            locale: self.locale,
            enabled: self.enabled_int != 0,
            last_seen_at: self.last_seen_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    fn test_encryptor() -> Arc<FieldEncryptor> {
        let dir = std::env::temp_dir().join(format!("sa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        FieldEncryptor::ensure_key(&dir).unwrap()
    }

    #[tokio::test]
    async fn create_and_get_user() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("alice", "Alice", UserRole::User, "pass123").await.unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.display_name, "Alice");
        assert!(matches!(user.role, UserRole::User));
        assert!(user.enabled);

        let fetched = mgr.get_by_id(&user.id).await.unwrap();
        assert_eq!(fetched.username, "alice");
    }

    #[tokio::test]
    async fn duplicate_username_fails() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        mgr.create("bob", "Bob", UserRole::User, "pw").await.unwrap();
        let result = mgr.create("bob", "Bob2", UserRole::Admin, "pw2").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn authenticate_valid() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        mgr.create("carol", "Carol", UserRole::Admin, "secret").await.unwrap();
        let user = mgr.authenticate("carol", "secret").await;
        assert!(user.is_some());
        assert_eq!(user.unwrap().username, "carol");
    }

    #[tokio::test]
    async fn authenticate_wrong_password() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        mgr.create("dave", "Dave", UserRole::User, "pass").await.unwrap();
        assert!(mgr.authenticate("dave", "wrong").await.is_none());
    }

    #[tokio::test]
    async fn authenticate_disabled_user() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("eve", "Eve", UserRole::User, "pw").await.unwrap();
        mgr.update(&user.id, None, None, None, Some(false)).await.unwrap();
        assert!(mgr.authenticate("eve", "pw").await.is_none());
    }

    #[tokio::test]
    async fn link_telegram() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("frank", "Frank", UserRole::User, "pw").await.unwrap();
        mgr.link_telegram(&user.id, 12345678).await.unwrap();
        let found = mgr.get_by_telegram_id(12345678).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "frank");
    }

    #[tokio::test]
    async fn link_whatsapp() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("grace", "Grace", UserRole::User, "pw").await.unwrap();
        mgr.link_whatsapp(&user.id, "+15551234567").await.unwrap();
        let found = mgr.get_by_whatsapp_id("+15551234567").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "grace");
    }

    #[tokio::test]
    async fn list_users() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        mgr.create("u1", "U1", UserRole::Admin, "").await.unwrap();
        mgr.create("u2", "U2", UserRole::User, "").await.unwrap();
        mgr.create("u3", "U3", UserRole::Viewer, "").await.unwrap();
        let list = mgr.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn update_user() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("heidi", "Heidi", UserRole::User, "pw").await.unwrap();
        let updated = mgr.update(&user.id, Some("Heidi Updated"), Some(UserRole::Admin), None, None).await.unwrap();
        assert_eq!(updated.display_name, "Heidi Updated");
        assert!(matches!(updated.role, UserRole::Admin));
    }

    #[tokio::test]
    async fn delete_user() {
        let db = test_db();
        let mgr = UserManager::new(db, test_encryptor());
        let user = mgr.create("ivan", "Ivan", UserRole::User, "pw").await.unwrap();
        mgr.delete(&user.id).await.unwrap();
        assert!(mgr.get_by_username("ivan").await.is_none());
    }

    #[test]
    fn user_role_display_and_parse() {
        assert_eq!(UserRole::Admin.as_str(), "admin");
        assert_eq!(UserRole::User.as_str(), "user");
        assert_eq!(UserRole::Viewer.as_str(), "viewer");
        assert!(matches!(UserRole::from_str("admin"), UserRole::Admin));
        assert!(matches!(UserRole::from_str("viewer"), UserRole::Viewer));
        assert!(matches!(UserRole::from_str("unknown"), UserRole::User));
    }

    #[test]
    fn user_role_permissions() {
        assert!(UserRole::Admin.can_chat());
        assert!(UserRole::User.can_chat());
        assert!(!UserRole::Viewer.can_chat());
    }
}
