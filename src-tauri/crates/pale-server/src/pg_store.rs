use std::str::FromStr;

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;
use uuid::Uuid;

use crate::{
    AdminAuditEvent, AdminSession, CallHistoryEntry, CallSession, Conference, FileRecord,
    RoutingRule, SipAccount, SipDialog, SipMessage, SipNotification, SipRegistration,
    SipSubscription, SipTransaction, User, UserPresence,
};

pub type PgError = Box<dyn std::error::Error + Send + Sync>;

/// PostgreSQL-backed persistent store using deadpool connection pool.
/// Write-through layer: AppState memory caches remain the primary
/// read path; PgStore is the durable source of truth.
#[derive(Clone)]
pub struct PgStore {
    pool: Pool,
}

impl PgStore {
    pub async fn connect(database_url: &str, max_connections: usize) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pg_config = tokio_postgres::Config::from_str(database_url)?;
        let mut cfg = Config::new();
        cfg.dbname = pg_config.get_dbname().map(String::from);
        cfg.host = pg_config.get_hosts().first().map(|h| {
            let debug = format!("{:?}", h);
            debug.trim_matches('"').trim_start_matches("Tcp(\"").trim_end_matches("\")").to_string()
        });
        cfg.port = pg_config.get_ports().first().copied();
        cfg.user = pg_config.get_user().map(String::from);
        cfg.password = pg_config.get_password().map(|p| String::from_utf8_lossy(p).to_string());

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        // Test connection
        let _conn = pool.get().await?;
        log::info!("PostgreSQL connection pool established (max {})", max_connections);

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await?;

        let migrations = [
            include_str!("../migrations/001_initial_schema.sql"),
            include_str!("../migrations/002_rooms_search_receipts_avatars.sql"),
            include_str!("../migrations/003_voicemail_recordings.sql"),
            include_str!("../migrations/004_dba_fixes.sql"),
        ];

        for (i, sql) in migrations.iter().enumerate() {
            client.batch_execute(sql).await?;
            log::info!("PostgreSQL migration {} applied", i + 1);
        }

        Ok(())
    }

    // ─── Users ───

    pub async fn insert_user(&self, user: &User) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO users (id, display_name, sip_uri, matrix_user_id, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET display_name = $2, sip_uri = $3, matrix_user_id = $4",
            &[&user.id, &user.display_name, &user.sip_uri, &user.matrix_user_id, &user.created_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM users WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    pub async fn load_users(&self) -> Result<Vec<User>, PgError> {
        let client = self.pool.get().await?;
        let rows = client.query(
            "SELECT id, display_name, sip_uri, matrix_user_id, created_at FROM users ORDER BY created_at",
            &[],
        ).await?;

        Ok(rows.iter().map(|r| User {
            id: r.get("id"),
            display_name: r.get("display_name"),
            sip_uri: r.get("sip_uri"),
            matrix_user_id: r.get("matrix_user_id"),
            password_hash: None,
            role: "user".to_string(),
            created_at: r.get("created_at"),
        }).collect())
    }

    // ─── SIP Accounts ───

    pub async fn upsert_sip_account(&self, account: &SipAccount) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO sip_accounts (username, domain, display_name, password_ha1, enabled, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (username, domain) DO UPDATE SET display_name = $3, password_ha1 = $4, enabled = $5",
            &[&account.username, &account.domain, &account.display_name, &account.password_ha1, &account.enabled, &account.created_at],
        ).await?;
        Ok(())
    }

    pub async fn load_sip_accounts(&self) -> Result<Vec<SipAccount>, PgError> {
        let client = self.pool.get().await?;
        let rows = client.query(
            "SELECT username, domain, display_name, password_ha1, enabled, created_at FROM sip_accounts",
            &[],
        ).await?;

        Ok(rows.iter().map(|r| SipAccount {
            username: r.get("username"),
            domain: r.get("domain"),
            display_name: r.get("display_name"),
            password_ha1: r.get("password_ha1"),
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
        }).collect())
    }

    // ─── Registrations ───

    pub async fn upsert_registration(&self, reg: &SipRegistration) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO sip_registrations (aor, contact, source, user_agent, expires_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, now())
             ON CONFLICT (aor) DO UPDATE SET contact = $2, source = $3, user_agent = $4, expires_at = $5, updated_at = now()",
            &[&reg.aor, &reg.contact, &reg.source, &reg.user_agent, &reg.expires_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_registration(&self, aor: &str) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM sip_registrations WHERE aor = $1", &[&aor]).await?;
        Ok(())
    }

    pub async fn load_registrations(&self) -> Result<Vec<SipRegistration>, PgError> {
        let client = self.pool.get().await?;
        let rows = client.query(
            "SELECT aor, contact, source, user_agent, expires_at, updated_at FROM sip_registrations WHERE expires_at > now()",
            &[],
        ).await?;

        Ok(rows.iter().map(|r| SipRegistration {
            aor: r.get("aor"),
            contact: r.get("contact"),
            source: r.get("source"),
            user_agent: r.get("user_agent"),
            expires_at: r.get("expires_at"),
            updated_at: r.get("updated_at"),
        }).collect())
    }

    // ─── Dialogs ───

    pub async fn upsert_dialog(&self, dialog: &SipDialog) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let media_json = serde_json::to_value(&dialog.media_types).unwrap_or_default();
        let status_str = serde_json::to_value(&dialog.status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "routing".to_string());

        client.execute(
            "INSERT INTO sip_dialogs (call_id, from_uri, to_uri, target_contact, status, media_types, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (call_id) DO UPDATE SET from_uri = $2, to_uri = $3, target_contact = $4, status = $5, media_types = $6, updated_at = $8",
            &[&dialog.call_id, &dialog.from_uri, &dialog.to_uri, &dialog.target_contact, &status_str, &media_json, &dialog.created_at, &dialog.updated_at],
        ).await?;
        Ok(())
    }

    // ─── Messages ───

    pub async fn insert_message(&self, msg: &SipMessage) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO sip_messages (id, call_id, from_uri, to_uri, content_type, body, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[&msg.id, &msg.call_id, &msg.from_uri, &msg.to_uri, &msg.content_type, &msg.body, &msg.received_at],
        ).await?;
        Ok(())
    }

    // ─── Transactions ───

    pub async fn insert_transaction(&self, tx: &SipTransaction) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let status_code = tx.status_code.map(|c| c as i16);
        client.execute(
            "INSERT INTO sip_transactions (id, method, uri, call_id, cseq, source, status_code, reason, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            &[&tx.id, &tx.method, &tx.uri, &tx.call_id, &tx.cseq, &tx.source, &status_code, &tx.reason, &tx.created_at],
        ).await?;
        Ok(())
    }

    // ─── Subscriptions ───

    pub async fn upsert_subscription(&self, sub: &SipSubscription) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO sip_subscriptions (subscription_id, subscriber, target, event, expires_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (subscription_id) DO UPDATE SET subscriber = $2, target = $3, event = $4, expires_at = $5, updated_at = $7",
            &[&sub.subscription_id, &sub.subscriber, &sub.target, &sub.event, &sub.expires_at, &sub.created_at, &sub.updated_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_subscription(&self, subscription_id: &str) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM sip_subscriptions WHERE subscription_id = $1", &[&subscription_id]).await?;
        Ok(())
    }

    // ─── Notifications ───

    pub async fn insert_notification(&self, notif: &SipNotification) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO sip_notifications (id, subscription_id, notifier, target, event, subscription_state, content_type, body, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            &[&notif.id, &notif.subscription_id, &notif.notifier, &notif.target, &notif.event, &notif.subscription_state, &notif.content_type, &notif.body, &notif.received_at],
        ).await?;
        Ok(())
    }

    // ─── Presence ───

    pub async fn upsert_presence(&self, presence: &UserPresence) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let status_str = serde_json::to_value(&presence.status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "offline".to_string());

        client.execute(
            "INSERT INTO presence (sip_uri, status, note, updated_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (sip_uri) DO UPDATE SET status = $2, note = $3, updated_at = $4",
            &[&presence.sip_uri, &status_str, &presence.note, &presence.updated_at],
        ).await?;
        Ok(())
    }

    // ─── Conferences ───

    pub async fn insert_conference(&self, conf: &Conference) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let mode_str = serde_json::to_value(&conf.mode)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "audio".to_string());

        client.execute(
            "INSERT INTO conferences (id, title, mode, active, created_at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET title = $2, mode = $3, active = $4",
            &[&conf.id, &conf.title, &mode_str, &conf.active, &conf.created_at],
        ).await?;
        Ok(())
    }

    // ─── Calls ───

    pub async fn upsert_call(&self, call: &CallSession) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let status_str = serde_json::to_value(&call.status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "ringing".to_string());
        let callees = serde_json::to_value(&call.callees).unwrap_or_default();
        let media = serde_json::to_value(&call.media).unwrap_or_default();

        client.execute(
            "INSERT INTO calls (id, conference_id, caller, callees, media, status, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET status = $6, updated_at = $8",
            &[&call.id, &call.conference_id, &call.caller, &callees, &media, &status_str, &call.created_at, &call.updated_at],
        ).await?;
        Ok(())
    }

    // ─── Files ───

    pub async fn insert_file(&self, file: &FileRecord) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        let size = file.size as i64;
        client.execute(
            "INSERT INTO files (id, owner, filename, content_type, size, sha256, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
            &[&file.id, &file.owner, &file.filename, &file.content_type, &size, &file.sha256, &file.created_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_file(&self, id: Uuid) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM files WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    // ─── Routing Rules ───

    pub async fn upsert_routing_rule(&self, rule: &RoutingRule) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO routing_rules (id, name, source_pattern, destination_pattern, target, priority, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET name = $2, source_pattern = $3, destination_pattern = $4, target = $5, priority = $6, enabled = $7, updated_at = $9",
            &[&rule.id, &rule.name, &rule.source_pattern, &rule.destination_pattern, &rule.target, &rule.priority, &rule.enabled, &rule.created_at, &rule.updated_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_routing_rule(&self, id: Uuid) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM routing_rules WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    pub async fn load_routing_rules(&self) -> Result<Vec<RoutingRule>, PgError> {
        let client = self.pool.get().await?;
        let rows = client.query(
            "SELECT id, name, source_pattern, destination_pattern, target, priority, enabled, created_at, updated_at FROM routing_rules ORDER BY priority ASC",
            &[],
        ).await?;

        Ok(rows.iter().map(|r| RoutingRule {
            id: r.get("id"),
            name: r.get("name"),
            source_pattern: r.get("source_pattern"),
            destination_pattern: r.get("destination_pattern"),
            target: r.get("target"),
            priority: r.get("priority"),
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }).collect())
    }

    // ─── Audit Events ───

    pub async fn insert_audit_event(&self, event: &AdminAuditEvent) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO audit_events (id, principal, action, target, created_at) VALUES ($1, $2, $3, $4, $5)",
            &[&event.id, &event.principal, &event.action, &event.target, &event.created_at],
        ).await?;
        Ok(())
    }

    // ─── Call History ───

    pub async fn insert_call_history(&self, entry: &CallHistoryEntry) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO call_history (id, user_sip_uri, direction, remote_uri, remote_name, start_time, duration_secs, answered, synced_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (user_sip_uri, start_time, remote_uri, direction) DO NOTHING",
            &[&entry.id, &entry.user_sip_uri, &entry.direction, &entry.remote_uri, &entry.remote_name, &entry.start_time, &entry.duration_secs, &entry.answered, &entry.synced_at],
        ).await?;
        Ok(())
    }

    // ─── Admin Sessions ───

    pub async fn insert_session(&self, session: &AdminSession) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute(
            "INSERT INTO admin_sessions (token, principal, expires_at) VALUES ($1, $2, $3)
             ON CONFLICT (token) DO UPDATE SET expires_at = $3",
            &[&session.token, &session.principal, &session.expires_at],
        ).await?;
        Ok(())
    }

    pub async fn delete_session(&self, token: &str) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("DELETE FROM admin_sessions WHERE token = $1", &[&token]).await?;
        Ok(())
    }

    // ─── Cleanup ───

    pub async fn cleanup_expired(&self) -> Result<(), PgError> {
        let client = self.pool.get().await?;
        client.execute("SELECT cleanup_expired()", &[]).await?;
        Ok(())
    }
}

