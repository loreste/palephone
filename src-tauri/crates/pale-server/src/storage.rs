use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AdminAuditEvent, CallQualityReport, CallSession, ChannelWebhook, CollaborationPolicy,
    Conference, ConferenceAttendanceRecord, DlpPolicy, DlpViolation, FileRecord,
    MessageReactionRecord, MessageRead, RetentionPolicy, Room, RoomMessage, RoutingRule,
    ScheduledMeeting, SipAccount, SipDialog, SipMessage, SipRegistration, SipTransaction, Team,
    User,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS pale_objects (
    collection TEXT NOT NULL,
    object_key TEXT NOT NULL,
    json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (collection, object_key)
);
"#;

pub struct Store {
    conn: Mutex<Connection>,
    secrets: SecretBox,
}

impl Store {
    pub fn open(data_dir: &Path, storage_key: &str) -> rusqlite::Result<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
        let conn = Connection::open(data_dir.join("pale-server.sqlite3"))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
            secrets: SecretBox::new(storage_key),
        })
    }

    pub fn put<T: Serialize>(
        &self,
        collection: &'static str,
        key: impl AsRef<str>,
        value: &T,
    ) -> rusqlite::Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
        self.put_json(collection, key.as_ref(), &json)
    }

    pub fn put_sip_account(&self, account: &SipAccount) -> rusqlite::Result<()> {
        self.put(
            "sip_accounts",
            account.aor(),
            &StoredSipAccount::from_account(account, &self.secrets)?,
        )
    }

    pub fn delete(&self, collection: &'static str, key: impl AsRef<str>) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .expect("store connection lock poisoned")
            .execute(
                "DELETE FROM pale_objects WHERE collection = ?1 AND object_key = ?2",
                params![collection, key.as_ref()],
            )?;
        Ok(())
    }

    pub fn load<T: DeserializeOwned>(&self, collection: &'static str) -> rusqlite::Result<Vec<T>> {
        let conn = self.conn.lock().expect("store connection lock poisoned");
        let mut statement = conn.prepare(
            "SELECT json FROM pale_objects WHERE collection = ?1 ORDER BY updated_at ASC",
        )?;
        let rows = statement.query_map(params![collection], |row| row.get::<_, String>(0))?;
        let mut values = Vec::new();
        for row in rows {
            let json = row?;
            let value = serde_json::from_str(&json).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            values.push(value);
        }
        Ok(values)
    }

    pub fn load_sip_accounts(&self) -> rusqlite::Result<Vec<SipAccount>> {
        Ok(self
            .load::<StoredSipAccount>("sip_accounts")?
            .into_iter()
            .map(|account| account.into_account(&self.secrets))
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn put_json(&self, collection: &'static str, key: &str, json: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .expect("store connection lock poisoned")
            .execute(
                r#"
                INSERT INTO pale_objects (collection, object_key, json, updated_at)
                VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
                ON CONFLICT(collection, object_key) DO UPDATE SET
                    json = excluded.json,
                    updated_at = CURRENT_TIMESTAMP
                "#,
                params![collection, key, json],
            )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSipAccount {
    username: String,
    domain: String,
    display_name: Option<String>,
    password_ha1_encrypted: String,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl StoredSipAccount {
    fn from_account(account: &SipAccount, secrets: &SecretBox) -> rusqlite::Result<Self> {
        Ok(Self {
            username: account.username.clone(),
            domain: account.domain.clone(),
            display_name: account.display_name.clone(),
            password_ha1_encrypted: secrets.encrypt(&account.password_ha1)?,
            enabled: account.enabled,
            created_at: account.created_at,
        })
    }

    fn into_account(self, secrets: &SecretBox) -> rusqlite::Result<SipAccount> {
        Ok(SipAccount {
            username: self.username,
            domain: self.domain,
            display_name: self.display_name,
            password_ha1: secrets.decrypt(&self.password_ha1_encrypted)?,
            enabled: self.enabled,
            created_at: self.created_at,
        })
    }
}

struct SecretBox {
    cipher: ChaCha20Poly1305,
}

impl SecretBox {
    fn new(storage_key: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(storage_key.as_bytes());
        let digest = hasher.finalize();
        Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&digest)),
        }
    }

    fn encrypt(&self, plaintext: &str) -> rusqlite::Result<String> {
        let uuid = Uuid::new_v4();
        let mut nonce_bytes = [0_u8; 12];
        nonce_bytes.copy_from_slice(&uuid.as_bytes()[..12]);
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .map_err(|_| rusqlite::Error::InvalidQuery)?;
        Ok(format!(
            "v1:{}:{}",
            BASE64.encode(nonce_bytes),
            BASE64.encode(ciphertext)
        ))
    }

    fn decrypt(&self, encoded: &str) -> rusqlite::Result<String> {
        let Some(rest) = encoded.strip_prefix("v1:") else {
            return Err(rusqlite::Error::InvalidQuery);
        };
        let Some((nonce, ciphertext)) = rest.split_once(':') else {
            return Err(rusqlite::Error::InvalidQuery);
        };
        let nonce = BASE64
            .decode(nonce)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
        let ciphertext = BASE64
            .decode(ciphertext)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
        if nonce.len() != 12 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| rusqlite::Error::InvalidQuery)?;
        String::from_utf8(plaintext)
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
    }
}

pub trait StoredObject {
    fn collection() -> &'static str;
    fn key(&self) -> String;
}

impl StoredObject for User {
    fn collection() -> &'static str {
        "users"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for SipRegistration {
    fn collection() -> &'static str {
        "registrations"
    }

    fn key(&self) -> String {
        self.aor.clone()
    }
}

impl StoredObject for SipDialog {
    fn collection() -> &'static str {
        "sip_dialogs"
    }

    fn key(&self) -> String {
        self.call_id.clone()
    }
}

impl StoredObject for SipMessage {
    fn collection() -> &'static str {
        "sip_messages"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for SipTransaction {
    fn collection() -> &'static str {
        "sip_transactions"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for Conference {
    fn collection() -> &'static str {
        "conferences"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for ConferenceAttendanceRecord {
    fn collection() -> &'static str {
        "conference_attendance"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for CallSession {
    fn collection() -> &'static str {
        "calls"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for CallQualityReport {
    fn collection() -> &'static str {
        "call_quality_reports"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for DlpPolicy {
    fn collection() -> &'static str {
        "dlp_policies"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for DlpViolation {
    fn collection() -> &'static str {
        "dlp_violations"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for FileRecord {
    fn collection() -> &'static str {
        "files"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for RoutingRule {
    fn collection() -> &'static str {
        "routing_rules"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for Room {
    fn collection() -> &'static str {
        "rooms"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for Team {
    fn collection() -> &'static str {
        "teams"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for ScheduledMeeting {
    fn collection() -> &'static str {
        "scheduled_meetings"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for RetentionPolicy {
    fn collection() -> &'static str {
        "retention_policies"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for CollaborationPolicy {
    fn collection() -> &'static str {
        "collaboration_policy"
    }

    fn key(&self) -> String {
        self.id.clone()
    }
}

impl StoredObject for ChannelWebhook {
    fn collection() -> &'static str {
        "channel_webhooks"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for RoomMessage {
    fn collection() -> &'static str {
        "room_messages"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

impl StoredObject for MessageRead {
    fn collection() -> &'static str {
        "message_reads"
    }

    fn key(&self) -> String {
        format!("{}:{}", self.message_id, self.reader_uri)
    }
}

impl StoredObject for MessageReactionRecord {
    fn collection() -> &'static str {
        "message_reactions"
    }

    fn key(&self) -> String {
        MessageReactionRecord::key(self)
    }
}

impl StoredObject for AdminAuditEvent {
    fn collection() -> &'static str {
        "admin_audit_events"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}
