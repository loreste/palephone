use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};
use uuid::Uuid;

pub mod http;
pub mod ldap_auth;
pub mod metrics;
pub mod pg_store;
pub mod pjsip_runtime;
pub mod sip;
mod storage;

pub use pg_store::PgStore;
use storage::{Store, StoredObject};

const MAX_SIP_MESSAGES: usize = 10_000;
const MAX_SIP_TRANSACTIONS: usize = 20_000;
const MAX_SIP_NOTIFICATIONS: usize = 10_000;
const MAX_ADMIN_SESSIONS: usize = 10_000;

/// Role string granting administrative privileges on management endpoints.
pub const ROLE_ADMIN: &str = "admin";
/// Default role for regular (non-admin) users.
pub const ROLE_USER: &str = "user";
const MAX_USERS: usize = 100_000;
const MAX_SIP_ACCOUNTS: usize = 100_000;
const MAX_REGISTRATIONS: usize = 100_000;
const MAX_SIP_NONCES: usize = 50_000;
const MAX_SIP_DIALOGS: usize = 100_000;
const MAX_SIP_SUBSCRIPTIONS: usize = 100_000;
const MAX_CONFERENCES: usize = 50_000;
const MAX_CALLS: usize = 100_000;
const MAX_FILES: usize = 100_000;
const MAX_ROUTING_RULES: usize = 100_000;
const MAX_AUDIT_EVENTS: usize = 50_000;
const MAX_PRESENCE: usize = 100_000;
const MAX_CALL_HISTORY: usize = 100_000;
const MAX_ROOMS: usize = 50_000;
const MAX_ROOM_MESSAGES: usize = 100_000;
const SHARDED_MAP_SHARDS: usize = 32;
const DEFAULT_MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;
const MAX_LOGIN_FAILURES: u32 = 5;

struct ShardedMap<K, V> {
    shards: Vec<RwLock<HashMap<K, V>>>,
}

impl<K, V> ShardedMap<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn new() -> Self {
        Self {
            shards: (0..SHARDED_MAP_SHARDS)
                .map(|_| RwLock::new(HashMap::new()))
                .collect(),
        }
    }

    fn insert(&self, key: K, value: V) -> Option<V> {
        self.shard(&key)
            .write()
            .expect("sharded map lock poisoned")
            .insert(key, value)
    }

    fn get(&self, key: &K) -> Option<V> {
        self.shard(key)
            .read()
            .expect("sharded map lock poisoned")
            .get(key)
            .cloned()
    }

    fn remove(&self, key: &K) -> Option<V> {
        self.shard(key)
            .write()
            .expect("sharded map lock poisoned")
            .remove(key)
    }

    fn values(&self) -> Vec<V> {
        self.shards
            .iter()
            .flat_map(|shard| {
                shard
                    .read()
                    .expect("sharded map lock poisoned")
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn retain(&self, mut predicate: impl FnMut(&K, &mut V) -> bool) {
        for shard in &self.shards {
            shard
                .write()
                .expect("sharded map lock poisoned")
                .retain(|key, value| predicate(key, value));
        }
    }

    fn with_write<R>(&self, key: &K, action: impl FnOnce(&mut HashMap<K, V>) -> R) -> R {
        let mut shard = self
            .shard(key)
            .write()
            .expect("sharded map lock poisoned");
        action(&mut shard)
    }

    fn trim_to_len(&self, max_len: usize) {
        let mut overflow = self.len().saturating_sub(max_len);
        if overflow == 0 {
            return;
        }

        for shard in &self.shards {
            let mut shard = shard.write().expect("sharded map lock poisoned");
            while overflow > 0 {
                let Some(key) = shard.keys().next().cloned() else {
                    break;
                };
                shard.remove(&key);
                overflow -= 1;
            }
            if overflow == 0 {
                break;
            }
        }
    }

    fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.read().expect("sharded map lock poisoned").len())
            .sum()
    }

    fn shard(&self, key: &K) -> &RwLock<HashMap<K, V>> {
        &self.shards[Self::shard_index(key)]
    }

    fn shard_index(key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % SHARDED_MAP_SHARDS
    }
}

/// Extract user part from SIP URI: "sip:300@example.com" -> "300", "300" -> "300"
pub fn sip_user_part(uri: &str) -> &str {
    let stripped = uri.strip_prefix("sips:").or_else(|| uri.strip_prefix("sip:")).unwrap_or(uri);
    stripped.split('@').next().unwrap_or(stripped)
}

trait PersistedMapObject: StoredObject {
    type Key: Clone + Eq + Hash;

    fn map_key(&self) -> Self::Key;
}

#[derive(Clone)]
pub struct ServerConfig {
    pub http_addr: SocketAddr,
    pub sip_addr: SocketAddr,
    pub data_dir: PathBuf,
    pub http_token: String,
    pub admin_username: String,
    pub admin_password_hash: String,
    pub storage_key: String,
    pub max_upload_bytes: u64,
    pub http_tls_cert: Option<PathBuf>,
    pub http_tls_key: Option<PathBuf>,
    pub media: MediaConfig,
}

pub struct AppState {
    data_dir: PathBuf,
    http_token: String,
    admin_username: String,
    admin_password_hash: String,
    max_upload_bytes: u64,
    media: MediaConfig,
    store: Option<Arc<Store>>,
    persist_runtime_events: RwLock<bool>,
    login_attempts: RwLock<HashMap<String, LoginAttempt>>,
    admin_sessions: ShardedMap<String, AdminSession>,
    users: ShardedMap<Uuid, User>,
    sip_accounts: ShardedMap<String, SipAccount>,
    registrations: ShardedMap<String, SipRegistration>,
    sip_nonces: ShardedMap<String, DateTime<Utc>>,
    sip_dialogs: ShardedMap<String, SipDialog>,
    /// In-flight proxied INVITEs keyed by the received top-Via branch, so a
    /// CANCEL can be matched to its INVITE transaction and forwarded upstream.
    pending_invites: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
    sip_messages: RwLock<Vec<SipMessage>>,
    sip_transactions: RwLock<Vec<SipTransaction>>,
    sip_subscriptions: ShardedMap<String, SipSubscription>,
    sip_notifications: RwLock<Vec<SipNotification>>,
    conferences: ShardedMap<Uuid, Conference>,
    calls: ShardedMap<Uuid, CallSession>,
    files: ShardedMap<Uuid, FileRecord>,
    routing_rules: ShardedMap<Uuid, RoutingRule>,
    audit_events: RwLock<Vec<AdminAuditEvent>>,
    presence: ShardedMap<String, UserPresence>,
    call_history: ShardedMap<Uuid, CallHistoryEntry>,
    rooms: ShardedMap<Uuid, Room>,
    room_messages: RwLock<Vec<RoomMessage>>,
    voicemails: ShardedMap<Uuid, Voicemail>,
    recordings: ShardedMap<Uuid, CallRecording>,
    ring_groups: ShardedMap<Uuid, RingGroup>,
    ivrs: ShardedMap<Uuid, Ivr>,
    user_call_settings: ShardedMap<String, UserCallSettings>,
    call_queues: ShardedMap<Uuid, CallQueue>,
    extensions: ShardedMap<String, Extension>,
    business_hours: ShardedMap<Uuid, BusinessHours>,
    holidays: ShardedMap<Uuid, Holiday>,
    parked_calls: ShardedMap<String, ParkedCall>,
    speed_dials: RwLock<Vec<SpeedDial>>,
    cdrs: RwLock<Vec<CallDetailRecord>>,
    paging_groups: ShardedMap<Uuid, PagingGroup>,
    agent_profiles: ShardedMap<String, AgentProfile>,
    monitor_sessions: ShardedMap<Uuid, MonitorSession>,
    qa_scorecards: RwLock<Vec<QaScorecard>>,
    canned_responses: ShardedMap<Uuid, CannedResponse>,
    queue_callers: ShardedMap<Uuid, QueueCallerEntry>,
    queue_callbacks: ShardedMap<Uuid, QueueCallback>,
    vip_callers: ShardedMap<Uuid, VipCaller>,
    message_reactions: ShardedMap<Uuid, Vec<MessageReaction>>,
    user_favorites: ShardedMap<String, Vec<String>>,
    user_create_lock: std::sync::Mutex<()>,
    agent_assignment_lock: std::sync::Mutex<()>,
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    rate_limits: ShardedMap<String, RateLimitBucket>,
    rate_limit_rps: u32,
    /// Address advertised to clients as their SIP registrar. `None` when the
    /// active SIP backend cannot register clients (e.g. the pjsip backend),
    /// in which case login/provisioning responses must not advertise one.
    sip_registrar: Option<String>,
    ldap_config: std::sync::RwLock<ldap_auth::LdapConfig>,
    pg: Option<PgStore>,
    pg_failure_count: Arc<std::sync::atomic::AtomicU64>,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("http_addr", &self.http_addr)
            .field("sip_addr", &self.sip_addr)
            .field("data_dir", &self.data_dir)
            .field("http_token", &"<redacted>")
            .field("admin_password_hash", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("data_dir", &self.data_dir)
            .field("http_token", &"<redacted>")
            .field("admin_username", &self.admin_username)
            .field("admin_password_hash", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf, http_token: String, admin_password_hash: String) -> Self {
        Self::from_parts(
            data_dir,
            http_token,
            "admin".to_string(),
            admin_password_hash,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
            None,
        )
    }

    pub fn persistent(
        data_dir: PathBuf,
        http_token: String,
        admin_username: String,
        admin_password_hash: String,
        storage_key: String,
        max_upload_bytes: u64,
        media: MediaConfig,
    ) -> rusqlite::Result<Self> {
        let store = Arc::new(Store::open(&data_dir, &storage_key)?);
        let state = Self::from_parts(
            data_dir,
            http_token,
            admin_username,
            admin_password_hash,
            max_upload_bytes,
            media,
            Some(store),
        );
        state.load_persisted();
        Ok(state)
    }

    fn from_parts(
        data_dir: PathBuf,
        http_token: String,
        admin_username: String,
        admin_password_hash: String,
        max_upload_bytes: u64,
        media: MediaConfig,
        store: Option<Arc<Store>>,
    ) -> Self {
        Self {
            data_dir,
            http_token,
            admin_username,
            admin_password_hash,
            max_upload_bytes,
            media,
            store,
            persist_runtime_events: RwLock::new(true),
            login_attempts: RwLock::new(HashMap::new()),
            admin_sessions: ShardedMap::new(),
            users: ShardedMap::new(),
            sip_accounts: ShardedMap::new(),
            registrations: ShardedMap::new(),
            sip_nonces: ShardedMap::new(),
            sip_dialogs: ShardedMap::new(),
            pending_invites: std::sync::Mutex::new(HashMap::new()),
            sip_messages: RwLock::new(Vec::new()),
            sip_transactions: RwLock::new(Vec::new()),
            sip_subscriptions: ShardedMap::new(),
            sip_notifications: RwLock::new(Vec::new()),
            conferences: ShardedMap::new(),
            calls: ShardedMap::new(),
            files: ShardedMap::new(),
            routing_rules: ShardedMap::new(),
            audit_events: RwLock::new(Vec::new()),
            presence: ShardedMap::new(),
            call_history: ShardedMap::new(),
            rooms: ShardedMap::new(),
            room_messages: RwLock::new(Vec::new()),
            voicemails: ShardedMap::new(),
            recordings: ShardedMap::new(),
            ring_groups: ShardedMap::new(),
            ivrs: ShardedMap::new(),
            user_call_settings: ShardedMap::new(),
            call_queues: ShardedMap::new(),
            extensions: ShardedMap::new(),
            business_hours: ShardedMap::new(),
            holidays: ShardedMap::new(),
            parked_calls: ShardedMap::new(),
            speed_dials: RwLock::new(Vec::new()),
            cdrs: RwLock::new(Vec::new()),
            paging_groups: ShardedMap::new(),
            agent_profiles: ShardedMap::new(),
            monitor_sessions: ShardedMap::new(),
            qa_scorecards: RwLock::new(Vec::new()),
            canned_responses: ShardedMap::new(),
            queue_callers: ShardedMap::new(),
            queue_callbacks: ShardedMap::new(),
            vip_callers: ShardedMap::new(),
            message_reactions: ShardedMap::new(),
            user_favorites: ShardedMap::new(),
            user_create_lock: std::sync::Mutex::new(()),
            agent_assignment_lock: std::sync::Mutex::new(()),
            sse_tx: tokio::sync::broadcast::channel(256).0,
            rate_limits: ShardedMap::new(),
            rate_limit_rps: 100,
            sip_registrar: None,
            ldap_config: std::sync::RwLock::new(ldap_auth::LdapConfig::default()),
            pg: None,
            pg_failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn ldap_config(&self) -> ldap_auth::LdapConfig {
        self.ldap_config.read().expect("ldap config lock").clone()
    }

    pub fn set_ldap_config(&self, config: ldap_auth::LdapConfig) {
        *self.ldap_config.write().expect("ldap config lock") = config;
    }

    /// Advertise `addr` as the SIP registrar in login/provisioning responses.
    /// Only call this when the active SIP backend actually implements
    /// REGISTER (currently only the udp-parser backend does).
    pub fn set_sip_registrar(&mut self, addr: String) {
        self.sip_registrar = Some(addr);
    }

    /// Whether the active SIP backend can register clients.
    pub fn sip_registration_available(&self) -> bool {
        self.sip_registrar.is_some()
    }

    pub fn set_rate_limit_rps(&mut self, rps: u32) {
        self.rate_limit_rps = rps;
    }

    /// Token bucket rate limiter. Returns true if request is allowed.
    pub fn check_rate_limit(&self, principal: &str) -> bool {
        let now = Utc::now();
        let max_tokens = self.rate_limit_rps as f64;
        let key = principal.to_string();

        self.rate_limits.with_write(&key, |buckets| {
            let bucket = buckets.entry(key.clone()).or_insert_with(|| RateLimitBucket {
                tokens: max_tokens,
                last_refill: now,
            });

            // Refill tokens based on elapsed time
            let elapsed = (now - bucket.last_refill).num_milliseconds().max(0) as f64 / 1000.0;
            bucket.tokens = (bucket.tokens + elapsed * max_tokens).min(max_tokens);
            bucket.last_refill = now;

            // Try to consume a token
            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                true
            } else {
                false
            }
        })
    }

    pub fn pg_store(&self) -> Option<&PgStore> {
        self.pg.as_ref()
    }

    pub fn set_pg_store(&mut self, pg: PgStore) {
        self.pg = Some(pg);
    }

    pub async fn load_from_postgres(&self) {
        let Some(pg) = &self.pg else { return };

        match pg.load_users().await {
            Ok(users) => {
                for user in users {
                    self.users.insert(user.id, user);
                }
            }
            Err(e) => log::warn!("Failed to load users from Postgres: {}", e),
        }
        match pg.load_sip_accounts().await {
            Ok(accounts) => {
                for account in accounts {
                    self.sip_accounts.insert(account.aor(), account);
                }
            }
            Err(e) => log::warn!("Failed to load SIP accounts from Postgres: {}", e),
        }
        match pg.load_registrations().await {
            Ok(regs) => {
                for reg in regs {
                    self.registrations.insert(reg.aor.clone(), reg);
                }
            }
            Err(e) => log::warn!("Failed to load registrations from Postgres: {}", e),
        }
        match pg.load_routing_rules().await {
            Ok(rules) => {
                for rule in rules {
                    self.routing_rules.insert(rule.id, rule);
                }
            }
            Err(e) => log::warn!("Failed to load routing rules from Postgres: {}", e),
        }
        match pg.load_user_call_settings().await {
            Ok(settings) => {
                for s in settings {
                    self.user_call_settings.insert(s.user_sip_uri.clone(), s);
                }
            }
            Err(e) => log::warn!("Failed to load user call settings from Postgres: {}", e),
        }
        match pg.load_business_hours().await {
            Ok(hours) => {
                for bh in hours {
                    self.business_hours.insert(bh.id, bh);
                }
            }
            Err(e) => log::warn!("Failed to load business hours from Postgres: {}", e),
        }
        match pg.load_holidays().await {
            Ok(holidays) => {
                for h in holidays {
                    self.holidays.insert(h.id, h);
                }
            }
            Err(e) => log::warn!("Failed to load holidays from Postgres: {}", e),
        }
        match pg.load_cdrs().await {
            Ok(cdrs) => {
                let mut cdr_list = self.cdrs.write().expect("cdrs lock poisoned");
                for cdr in cdrs {
                    cdr_list.push(cdr);
                }
            }
            Err(e) => log::warn!("Failed to load CDRs from Postgres: {}", e),
        }
        match pg.load_recordings().await {
            Ok(recordings) => {
                for rec in recordings {
                    self.recordings.insert(rec.id, rec);
                }
            }
            Err(e) => log::warn!("Failed to load recordings from Postgres: {}", e),
        }
        match pg.load_extensions().await {
            Ok(extensions) => {
                let count = extensions.len();
                for ext in extensions {
                    self.extensions.insert(ext.extension.clone(), ext);
                }
                log::info!("Loaded {} extensions from PostgreSQL", count);
            }
            Err(e) => log::warn!("Failed to load extensions from Postgres: {}", e),
        }
        match pg.load_rooms().await {
            Ok(rooms) => {
                for room in rooms {
                    self.rooms.insert(room.id, room);
                }
            }
            Err(e) => log::warn!("Failed to load rooms from Postgres: {}", e),
        }
        match pg.load_room_messages().await {
            Ok(messages) => {
                *self.room_messages.write().expect("room messages lock poisoned") = messages;
            }
            Err(e) => log::warn!("Failed to load room messages from Postgres: {}", e),
        }
        log::info!("Loaded data from PostgreSQL into memory cache");
    }

    pub fn http_token(&self) -> &str {
        &self.http_token
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_bytes
    }

    pub fn media_config(&self) -> MediaConfig {
        self.media.clone()
    }

    pub fn set_runtime_event_persistence(&self, enabled: bool) {
        *self
            .persist_runtime_events
            .write()
            .expect("runtime event persistence lock poisoned") = enabled;
    }

    fn should_persist_runtime_events(&self) -> bool {
        *self
            .persist_runtime_events
            .read()
            .expect("runtime event persistence lock poisoned")
    }

    pub fn authenticate_admin(
        &self,
        username: &str,
        password: &str,
        source: &str,
    ) -> Result<AdminSession, AuthError> {
        if self.login_is_locked(source) {
            self.record_audit_event("anonymous", "admin.login.locked", Some(source.to_string()));
            return Err(AuthError::Locked);
        }

        if username != self.admin_username
            || !verify_password(password, &self.admin_password_hash)
        {
            self.record_login_failure(source);
            self.record_audit_event(username, "admin.login.failed", Some(source.to_string()));
            return Err(AuthError::Unauthorized);
        }

        self.clear_login_failures(source);
        let session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: self.admin_username.clone(),
            role: ROLE_ADMIN.to_string(),
            expires_at: Utc::now() + Duration::hours(12),
        };
        self.admin_sessions
            .insert(session.token.clone(), session.clone());
        self.admin_sessions.trim_to_len(MAX_ADMIN_SESSIONS);
        self.record_audit_event(
            &session.principal,
            "admin.login.succeeded",
            Some(source.to_string()),
        );
        Ok(session)
    }

    fn login_is_locked(&self, source: &str) -> bool {
        let mut attempts = self
            .login_attempts
            .write()
            .expect("login attempts lock poisoned");
        let attempt = attempts.entry(source.to_string()).or_default();
        if attempt.locked_until > Utc::now() {
            return true;
        }
        if attempt
            .last_failure_at
            .is_some_and(|last_failure| last_failure + Duration::minutes(15) <= Utc::now())
        {
            attempt.failures = 0;
        }
        false
    }

    fn record_login_failure(&self, source: &str) {
        let mut attempts = self
            .login_attempts
            .write()
            .expect("login attempts lock poisoned");
        let attempt = attempts.entry(source.to_string()).or_default();
        attempt.failures = attempt.failures.saturating_add(1);
        attempt.last_failure_at = Some(Utc::now());
        if attempt.failures >= MAX_LOGIN_FAILURES {
            attempt.locked_until = Utc::now() + Duration::minutes(15);
        }
    }

    fn clear_login_failures(&self, source: &str) {
        self.login_attempts
            .write()
            .expect("login attempts lock poisoned")
            .remove(source);
    }

    pub fn principal_for_bearer(&self, bearer: &str) -> Option<String> {
        self.principal_role_for_bearer(bearer)
            .map(|(principal, _)| principal)
    }

    /// Resolve a bearer token to `(principal, role)`. The static server token
    /// maps to the superuser admin; session tokens carry the role recorded at
    /// login time. Returns `None` for unknown or expired tokens.
    pub fn principal_role_for_bearer(&self, bearer: &str) -> Option<(String, String)> {
        if bearer == self.http_token {
            return Some((self.admin_username.clone(), ROLE_ADMIN.to_string()));
        }

        self.admin_sessions
            .retain(|_, session| session.expires_at > Utc::now());
        self.admin_sessions
            .get(&bearer.to_string())
            .map(|session| (session.principal, session.role))
    }

    pub fn refresh_admin_session(&self, old_token: &str) -> Result<AdminSession, AuthError> {
        let old_session = self
            .admin_sessions
            .remove(&old_token.to_string())
            .ok_or(AuthError::Unauthorized)?;
        if old_session.expires_at <= Utc::now() {
            return Err(AuthError::Unauthorized);
        }
        let new_session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: old_session.principal,
            role: old_session.role,
            expires_at: Utc::now() + Duration::hours(12),
        };
        self.admin_sessions
            .insert(new_session.token.clone(), new_session.clone());
        self.admin_sessions.trim_to_len(MAX_ADMIN_SESSIONS);
        Ok(new_session)
    }

    pub fn revoke_session(&self, token: &str) {
        self.admin_sessions.remove(&token.to_string());
    }

    pub fn is_admin_principal(&self, principal: &str) -> bool {
        principal == self.admin_username
    }

    pub fn record_audit_event(
        &self,
        principal: impl Into<String>,
        action: impl Into<String>,
        target: Option<String>,
    ) -> AdminAuditEvent {
        let event = AdminAuditEvent {
            id: Uuid::new_v4(),
            principal: principal.into(),
            action: action.into(),
            target,
            created_at: Utc::now(),
        };
        let mut events = self.audit_events.write().expect("audit events lock poisoned");
        events.push(event.clone());
        if events.len() > MAX_AUDIT_EVENTS {
            let overflow = events.len() - MAX_AUDIT_EVENTS;
            events.drain(..overflow);
        }
        self.persist(&event);
        let e = event.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_audit_event(&e).await }));
        event
    }

    pub fn audit_events(&self) -> Vec<AdminAuditEvent> {
        self.audit_events
            .read()
            .expect("audit events lock poisoned")
            .iter()
            .rev()
            .take(500)
            .cloned()
            .collect()
    }

    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("files")
    }

    pub fn file_path(&self, file_id: Uuid) -> PathBuf {
        self.files_dir().join(file_id.to_string())
    }

    fn load_persisted(&self) {
        let Some(store) = &self.store else {
            return;
        };

        self.load_collection::<User>(&self.users);
        match store.load_sip_accounts() {
            Ok(accounts) => {
                for account in accounts {
                    self.sip_accounts.insert(account.aor(), account);
                }
            }
            Err(err) => log::warn!("failed to load sip accounts from storage: {}", err),
        }
        self.load_collection::<SipRegistration>(&self.registrations);
        self.load_collection::<SipDialog>(&self.sip_dialogs);
        self.load_vec_collection::<SipMessage>(&self.sip_messages);
        self.load_vec_collection::<SipTransaction>(&self.sip_transactions);
        self.load_collection::<Conference>(&self.conferences);
        self.load_collection::<CallSession>(&self.calls);
        self.load_collection::<FileRecord>(&self.files);
        self.load_collection::<RoutingRule>(&self.routing_rules);
        self.load_collection::<Room>(&self.rooms);
        self.load_vec_collection::<RoomMessage>(&self.room_messages);
        self.load_vec_collection::<AdminAuditEvent>(&self.audit_events);
    }

    fn load_collection<T>(&self, map: &ShardedMap<<T as PersistedMapObject>::Key, T>)
    where
        T: PersistedMapObject + for<'de> Deserialize<'de> + Clone,
    {
        let Some(store) = &self.store else {
            return;
        };
        match store.load::<T>(T::collection()) {
            Ok(values) => {
                for value in values {
                    map.insert(value.map_key(), value);
                }
            }
            Err(err) => log::warn!("failed to load {} from storage: {}", T::collection(), err),
        }
    }

    fn load_vec_collection<T>(&self, list: &RwLock<Vec<T>>)
    where
        T: StoredObject + for<'de> Deserialize<'de> + Clone,
    {
        let Some(store) = &self.store else {
            return;
        };
        match store.load::<T>(T::collection()) {
            Ok(values) => {
                *list.write().expect("persisted list lock poisoned") = values;
            }
            Err(err) => log::warn!("failed to load {} from storage: {}", T::collection(), err),
        }
    }

    fn persist<T>(&self, value: &T)
    where
        T: StoredObject + Serialize,
    {
        if let Some(store) = &self.store {
            if let Err(err) = store.put(T::collection(), value.key(), value) {
                log::error!("failed to persist {}: {}", T::collection(), err);
            }
        }
    }

    fn persist_sip_account(&self, account: &SipAccount) {
        if let Some(store) = &self.store {
            if let Err(err) = store.put_sip_account(account) {
                log::error!("failed to persist sip account: {}", err);
            }
        }
    }

    fn delete_persisted(&self, collection: &'static str, key: impl AsRef<str>) {
        if let Some(store) = &self.store {
            if let Err(err) = store.delete(collection, key) {
                log::error!("failed to delete persisted {}: {}", collection, err);
            }
        }
    }

    pub fn user_exists(&self, sip_uri: &str) -> bool {
        let Some(normalized) = normalize_sip_uri(sip_uri) else {
            return false;
        };
        self.users
            .values()
            .iter()
            .any(|u| normalize_sip_uri(&u.sip_uri).as_deref() == Some(normalized.as_str()))
    }

    pub fn create_user(&self, input: CreateUserRequest) -> Result<User, String> {
        let normalized_sip_uri = normalize_sip_uri(&input.sip_uri)
            .ok_or_else(|| format!("Invalid SIP URI {}", input.sip_uri))?;

        let _create_guard = self
            .user_create_lock
            .lock()
            .map_err(|_| "user creation lock poisoned".to_string())?;

        if self.users.values().iter().any(|u| {
            normalize_sip_uri(&u.sip_uri).as_deref() == Some(normalized_sip_uri.as_str())
        }) {
            return Err(format!(
                "User with SIP URI {} already exists",
                normalized_sip_uri
            ));
        }

        let password_hash = input.password.as_deref().map(hash_password);

        let user = User {
            id: Uuid::new_v4(),
            display_name: input.display_name.clone(),
            sip_uri: normalized_sip_uri.clone(),
            matrix_user_id: input.matrix_user_id,
            password_hash,
            role: input.role.unwrap_or_else(|| "user".to_string()),
            created_at: Utc::now(),
            email: None,
            title: None,
            department: None,
            phone_number: None,
            status_message: None,
        };
        self.users.insert(user.id, user.clone());
        self.users.trim_to_len(MAX_USERS);
        self.persist(&user);
        let u = user.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_user(&u).await }));
        self.broadcast_sse(SseEvent {
            event_type: "user_created".to_string(),
            payload: serde_json::to_value(&user).unwrap_or_default(),
        });

        // Auto-provision SIP account when creating a user with a password
        if let Some(password) = &input.password {
            if let Some((username, domain)) = split_sip_aor_simple(&user.sip_uri) {
                self.upsert_sip_account(CreateSipAccountRequest {
                    username: username.clone(),
                    domain: domain.clone(),
                    password_ha1: sip_ha1(&username, &domain, password),
                    display_name: Some(input.display_name),
                });
                log::info!("Auto-provisioned SIP account for {}", input.sip_uri);
            }
        }

        Ok(user)
    }

    /// Authenticate a user by SIP URI and password.
    /// Tries LDAP/AD first (if configured), then falls back to local database.
    /// Auto-provisions users from AD on first login.
    pub fn authenticate_user(
        &self,
        sip_uri: &str,
        password: &str,
    ) -> Result<UserLoginResponse, AuthError> {
        // Extract username for LDAP
        let username = split_sip_aor_simple(sip_uri)
            .map(|(u, _)| u)
            .unwrap_or_else(|| sip_uri.to_string());

        // Try LDAP first if configured. Track whether LDAP actually verified
        // this password: if the LDAP server is unreachable, the bind fails, or
        // no runtime is available, we MUST fall back to verified local auth
        // instead of skipping password verification entirely (fail closed).
        let mut ldap_authenticated = false;
        let ldap_cfg = self.ldap_config();
        if ldap_cfg.enabled {
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                let ldap_result = std::thread::scope(|_| {
                    handle.block_on(ldap_auth::ldap_authenticate(&ldap_cfg, &username, password))
                });

                if let Ok(ldap_user) = ldap_result {
                    ldap_authenticated = true;
                    // Auto-provision: create local user if not exists
                    if !self.user_exists(&ldap_user.sip_uri) {
                        let _ = self.create_user(CreateUserRequest {
                            display_name: ldap_user.display_name.clone(),
                            sip_uri: ldap_user.sip_uri.clone(),
                            matrix_user_id: None,
                            password: Some(password.to_string()),
                            role: Some(if ldap_user.is_admin { "admin" } else { "user" }.to_string()),
                        });
                        log::info!("Auto-provisioned AD user: {} (admin={})", ldap_user.sip_uri, ldap_user.is_admin);
                    }
                    // Update role from AD group membership
                    let normalized_ldap_uri = normalize_sip_uri(&ldap_user.sip_uri);
                    if let Some(existing) = self.users.values().into_iter().find(|u| {
                        normalize_sip_uri(&u.sip_uri) == normalized_ldap_uri
                    }) {
                        let new_role = if ldap_user.is_admin { "admin" } else { "user" };
                        if existing.role != new_role {
                            self.update_user_role(existing.id, new_role);
                        }
                    }
                    // Continue to create session below using the local user
                }
            }
        }

        // Local auth
        let normalized_login_uri = normalize_sip_uri(sip_uri);
        let user = self
            .users
            .values()
            .into_iter()
            .find(|u| {
                normalize_sip_uri(&u.sip_uri) == normalized_login_uri
                    || split_sip_aor_simple(&u.sip_uri)
                        .map(|(u, _)| u)
                        .as_deref()
                        == Some(&username)
            })
            .ok_or(AuthError::Unauthorized)?;

        // Verify password locally unless LDAP itself verified it. LDAP being
        // merely *enabled* is not enough — an unreachable or failing
        // directory must not bypass password verification.
        if !ldap_authenticated {
            let expected_hash = user.password_hash.as_deref().ok_or(AuthError::Unauthorized)?;
            if !verify_password(password, expected_hash) {
                return Err(AuthError::Unauthorized);
            }
        }

        // Create session carrying the user's role (consulted by admin-only endpoints)
        let session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: user.sip_uri.clone(),
            role: user.role.clone(),
            expires_at: Utc::now() + Duration::hours(12),
        };
        self.admin_sessions
            .insert(session.token.clone(), session.clone());
        self.admin_sessions.trim_to_len(MAX_ADMIN_SESSIONS);

        // Get or create SIP credentials
        let sip_creds = split_sip_aor_simple(&user.sip_uri)
            .map(|(username, domain)| {
                // Auto-create SIP account if it doesn't exist
                if self.sip_account(&username, &domain).is_none() {
                    self.upsert_sip_account(CreateSipAccountRequest {
                        username: username.clone(),
                        domain: domain.clone(),
                        password_ha1: sip_ha1(&username, &domain, password),
                        display_name: Some(user.display_name.clone()),
                    });
                    log::info!("Auto-created SIP account for {} on login", user.sip_uri);
                }
                SipCredentials {
                    sip_uri: user.sip_uri.clone(),
                    registrar_uri: self
                        .sip_registrar
                        .as_ref()
                        .map(|registrar| format!("sip:{}", registrar)),
                    registration_available: self.sip_registrar.is_some(),
                    username: username.clone(),
                    password: password.to_string(),
                    transport: "udp".to_string(),
                    domain,
                }
            });

        // Set presence to online
        self.update_presence(&user.sip_uri, PresenceStatus::Online, None);

        Ok(UserLoginResponse {
            token: session.token,
            user,
            sip_credentials: sip_creds,
            expires_at: session.expires_at,
        })
    }

    pub fn users(&self) -> Vec<User> {
        self.users.values()
    }

    pub fn update_user_role(&self, id: Uuid, role: &str) -> Option<User> {
        self.users.with_write(&id, |users| {
            let user = users.get_mut(&id)?;
            user.role = role.to_string();
            Some(user.clone())
        })
    }

    /// Change a user's password. Verifies the old password, then updates to the
    /// new argon2id hash in both the in-memory store and Postgres.
    pub fn change_user_password(
        &self,
        sip_uri: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), String> {
        let user = self
            .users
            .values()
            .into_iter()
            .find(|u| u.sip_uri == sip_uri)
            .ok_or_else(|| "User not found".to_string())?;

        let stored = user
            .password_hash
            .as_deref()
            .ok_or_else(|| "No password set for user".to_string())?;

        if !verify_password(old_password, stored) {
            return Err("Current password is incorrect".to_string());
        }

        let new_hash = hash_password(new_password);
        let updated = self.users.with_write(&user.id, |users| {
            let u = users.get_mut(&user.id)?;
            u.password_hash = Some(new_hash);
            Some(u.clone())
        });

        if let Some(ref u) = updated {
            self.persist(u);
            let u2 = u.clone();
            self.pg_spawn(move |pg| Box::pin(async move { pg.update_user_password(u2.id, u2.password_hash.as_deref().unwrap_or("")).await }));

            // Also update the SIP account HA1 digest if one exists
            if let Some((username, domain)) = split_sip_aor_simple(sip_uri) {
                let ha1 = sip_ha1(&username, &domain, new_password);
                self.update_sip_account_ha1(&username, &domain, &ha1);
            }
        }

        Ok(())
    }

    /// Update the HA1 digest for a SIP account (used after password change).
    fn update_sip_account_ha1(&self, username: &str, domain: &str, ha1: &str) {
        let aor = format!("{}@{}", username, domain);
        self.sip_accounts.with_write(&aor, |accounts| {
            if let Some(account) = accounts.get_mut(&aor) {
                account.password_ha1 = ha1.to_string();
            }
        });
    }

    pub fn delete_user(&self, id: Uuid) -> Option<User> {
        let user = self.users.remove(&id);
        if user.is_some() {
            self.delete_persisted(User::collection(), id.to_string());
            self.pg_spawn(move |pg| Box::pin(async move { pg.delete_user(id).await }));
            // Orphan extensions (mirrors ON DELETE SET NULL in PG)
            for ext in self.extensions_for_user(id) {
                let mut e = ext;
                e.user_id = None;
                e.user_display_name = None;
                self.extensions.insert(e.extension.clone(), e);
            }
        }
        user
    }

    pub fn upsert_sip_account(&self, input: CreateSipAccountRequest) -> SipAccount {
        let account = SipAccount {
            username: input.username,
            domain: input.domain,
            display_name: input.display_name,
            password_ha1: input.password_ha1,
            enabled: true,
            created_at: Utc::now(),
        };
        let key = account.aor();
        self.sip_accounts.insert(key, account.clone());
        self.sip_accounts.trim_to_len(MAX_SIP_ACCOUNTS);
        self.persist_sip_account(&account);
        let a = account.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_sip_account(&a).await }));
        account
    }

    pub fn sip_account(&self, username: &str, realm: &str) -> Option<SipAccount> {
        self.sip_accounts
            .get(&format!("sip:{}@{}", username, realm))
    }

    pub fn sip_accounts(&self) -> Vec<SipAccount> {
        self.sip_accounts.values()
    }

    pub fn update_sip_account_enabled(
        &self,
        username: &str,
        domain: &str,
        enabled: bool,
    ) -> Option<SipAccount> {
        let key = format!("sip:{}@{}", username, domain);
        let account = self.sip_accounts.with_write(&key, |accounts| {
            let account = accounts.get_mut(&key)?;
            account.enabled = enabled;
            Some(account.clone())
        });
        if let Some(account) = &account {
            self.persist_sip_account(account);
        }
        account
    }

    pub fn delete_sip_account(&self, username: &str, domain: &str) -> Option<SipAccount> {
        let key = format!("sip:{}@{}", username, domain);
        let account = self.sip_accounts.remove(&key);
        if account.is_some() {
            self.delete_persisted("sip_accounts", key);
        }
        account
    }

    pub fn issue_sip_nonce(&self) -> String {
        let nonce = Uuid::new_v4().to_string();
        self.sip_nonces
            .insert(nonce.clone(), Utc::now() + Duration::minutes(5));
        self.sip_nonces.trim_to_len(MAX_SIP_NONCES);
        nonce
    }

    pub fn consume_sip_nonce(&self, nonce: &str) -> bool {
        self.sip_nonces
            .retain(|_, expires_at| *expires_at > Utc::now());
        self.sip_nonces.remove(&nonce.to_string()).is_some()
    }

    /// Register an in-flight proxied INVITE under the received top-Via
    /// branch. Returns the Notify a matching CANCEL will trigger.
    pub fn register_pending_invite(&self, received_branch: &str) -> Arc<tokio::sync::Notify> {
        let notify = Arc::new(tokio::sync::Notify::new());
        self.pending_invites
            .lock()
            .expect("pending_invites lock poisoned")
            .insert(received_branch.to_string(), notify.clone());
        notify
    }

    pub fn remove_pending_invite(&self, received_branch: &str) {
        self.pending_invites
            .lock()
            .expect("pending_invites lock poisoned")
            .remove(received_branch);
    }

    /// Signal the in-flight INVITE matching this branch to send a CANCEL
    /// upstream. Returns true when a pending transaction was found.
    pub fn cancel_pending_invite(&self, received_branch: &str) -> bool {
        let notify = self
            .pending_invites
            .lock()
            .expect("pending_invites lock poisoned")
            .get(received_branch)
            .cloned();
        match notify {
            Some(notify) => {
                notify.notify_one();
                true
            }
            None => false,
        }
    }

    pub fn upsert_registration(&self, registration: SipRegistration) {
        self.registrations
            .retain(|_, registration| registration.expires_at > Utc::now());
        let aor = registration.aor.clone();
        self.registrations
            .insert(aor.clone(), registration.clone());
        self.registrations.trim_to_len(MAX_REGISTRATIONS);
        if self.should_persist_runtime_events() {
            self.persist(&registration);
        }
        let reg = registration.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_registration(&reg).await }));
        self.update_presence(&aor, PresenceStatus::Online, None);
    }

    pub fn remove_registration(&self, aor: &str) -> Option<SipRegistration> {
        let registration = self.registrations.remove(&aor.to_string());
        if registration.is_some() {
            self.delete_persisted(SipRegistration::collection(), aor);
            self.update_presence(aor, PresenceStatus::Offline, None);
        }
        registration
    }

    pub fn registrations(&self) -> Vec<SipRegistration> {
        self.registrations
            .values()
            .into_iter()
            .filter(|registration| registration.expires_at > Utc::now())
            .collect()
    }

    pub fn registration_for(&self, aor: &str) -> Option<SipRegistration> {
        self.registrations
            .get(&aor.to_string())
            .filter(|registration| registration.expires_at > Utc::now())
    }

    pub fn upsert_sip_dialog(&self, input: UpsertSipDialog) -> SipDialog {
        let call_id = input.call_id.clone();
        let dialog = self.sip_dialogs.with_write(&call_id, |dialogs| {
            dialogs
            .entry(input.call_id.clone())
            .and_modify(|dialog| {
                dialog.from_uri = input.from_uri.clone();
                dialog.to_uri = input.to_uri.clone();
                dialog.target_contact = input.target_contact.clone();
                dialog.status = input.status.clone();
                if !input.media_types.is_empty() {
                    dialog.media_types = input.media_types.clone();
                }
                if input.peer.from_contact.is_some() {
                    dialog.from_contact = input.peer.from_contact.clone();
                }
                if input.peer.from_source.is_some() {
                    dialog.from_source = input.peer.from_source.clone();
                }
                if input.peer.to_source.is_some() {
                    dialog.to_source = input.peer.to_source.clone();
                }
                dialog.updated_at = Utc::now();
            })
            .or_insert_with(|| SipDialog {
                call_id: input.call_id,
                from_uri: input.from_uri,
                to_uri: input.to_uri,
                target_contact: input.target_contact,
                status: input.status,
                media_types: input.media_types,
                from_contact: input.peer.from_contact,
                from_source: input.peer.from_source,
                to_source: input.peer.to_source,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .clone()
        });
        self.sip_dialogs.trim_to_len(MAX_SIP_DIALOGS);
        if self.should_persist_runtime_events() {
            self.persist(&dialog);
        }
        dialog.clone()
    }

    pub fn update_sip_dialog_status(
        &self,
        call_id: &str,
        status: SipDialogStatus,
    ) -> Option<SipDialog> {
        let dialog = self.sip_dialogs
            .with_write(&call_id.to_string(), |dialogs| {
                let dialog = dialogs.get_mut(call_id)?;
                dialog.status = status;
                dialog.updated_at = Utc::now();
                Some(dialog.clone())
            });
        if let Some(dialog) = &dialog {
            if self.should_persist_runtime_events() {
                self.persist(dialog);
            }
        }
        dialog
    }

    pub fn sip_dialogs(&self) -> Vec<SipDialog> {
        self.sip_dialogs.values()
    }

    pub fn dialog_exists(&self, call_id: &str) -> bool {
        self.sip_dialogs.get(&call_id.to_string()).is_some()
    }

    pub fn dialog_for(&self, call_id: &str) -> Option<SipDialog> {
        self.sip_dialogs.get(&call_id.to_string())
    }

    pub fn store_sip_message(&self, input: StoreSipMessage) -> SipMessage {
        let message = SipMessage {
            id: Uuid::new_v4(),
            call_id: input.call_id,
            from_uri: input.from_uri,
            to_uri: input.to_uri,
            content_type: input.content_type,
            body: input.body,
            received_at: Utc::now(),
        };
        let mut messages = self.sip_messages.write().expect("sip messages lock poisoned");
        messages.push(message.clone());
        if messages.len() > MAX_SIP_MESSAGES {
            let overflow = messages.len() - MAX_SIP_MESSAGES;
            messages.drain(..overflow);
        }
        if self.should_persist_runtime_events() {
            self.persist(&message);
        }
        let m = message.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_message(&m).await }));
        self.broadcast_sse(SseEvent {
            event_type: "message".to_string(),
            payload: serde_json::to_value(&message).unwrap_or_default(),
        });
        message
    }

    pub fn sip_messages(&self) -> Vec<SipMessage> {
        self.sip_messages
            .read()
            .expect("sip messages lock poisoned")
            .clone()
    }

    pub fn store_sip_transaction(&self, input: StoreSipTransaction) -> SipTransaction {
        let transaction = SipTransaction {
            id: Uuid::new_v4(),
            method: input.method,
            uri: input.uri,
            call_id: input.call_id,
            cseq: input.cseq,
            source: input.source,
            status_code: input.status_code,
            reason: input.reason,
            created_at: Utc::now(),
        };
        let mut transactions = self
            .sip_transactions
            .write()
            .expect("sip transactions lock poisoned");
        transactions.push(transaction.clone());
        if transactions.len() > MAX_SIP_TRANSACTIONS {
            let overflow = transactions.len() - MAX_SIP_TRANSACTIONS;
            transactions.drain(..overflow);
        }
        if self.should_persist_runtime_events() {
            self.persist(&transaction);
        }
        transaction
    }

    pub fn sip_transactions(&self) -> Vec<SipTransaction> {
        self.sip_transactions
            .read()
            .expect("sip transactions lock poisoned")
            .clone()
    }

    pub fn upsert_sip_subscription(&self, input: UpsertSipSubscription) -> SipSubscription {
        self.sip_subscriptions
            .retain(|_, subscription| subscription.expires_at > Utc::now());
        let subscription_id = input.subscription_id.clone();
        let subscription = self.sip_subscriptions.with_write(&subscription_id, |subscriptions| {
            subscriptions
            .entry(input.subscription_id.clone())
            .and_modify(|subscription| {
                subscription.subscriber = input.subscriber.clone();
                subscription.target = input.target.clone();
                subscription.event = input.event.clone();
                subscription.expires_at = input.expires_at;
                subscription.updated_at = Utc::now();
            })
            .or_insert_with(|| SipSubscription {
                subscription_id: input.subscription_id,
                subscriber: input.subscriber,
                target: input.target,
                event: input.event,
                expires_at: input.expires_at,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .clone()
        });
        self.sip_subscriptions.trim_to_len(MAX_SIP_SUBSCRIPTIONS);
        subscription.clone()
    }

    pub fn remove_sip_subscription(&self, subscription_id: &str) -> Option<SipSubscription> {
        self.sip_subscriptions
            .remove(&subscription_id.to_string())
    }

    pub fn sip_subscriptions(&self) -> Vec<SipSubscription> {
        self.sip_subscriptions
            .values()
            .into_iter()
            .filter(|subscription| subscription.expires_at > Utc::now())
            .collect()
    }

    pub fn store_sip_notification(&self, input: StoreSipNotification) -> SipNotification {
        let notification = SipNotification {
            id: Uuid::new_v4(),
            subscription_id: input.subscription_id,
            notifier: input.notifier,
            target: input.target,
            event: input.event,
            subscription_state: input.subscription_state,
            content_type: input.content_type,
            body: input.body,
            received_at: Utc::now(),
        };
        let mut notifications = self
            .sip_notifications
            .write()
            .expect("sip notifications lock poisoned");
        notifications.push(notification.clone());
        if notifications.len() > MAX_SIP_NOTIFICATIONS {
            let overflow = notifications.len() - MAX_SIP_NOTIFICATIONS;
            notifications.drain(..overflow);
        }
        self.broadcast_sse(SseEvent {
            event_type: "notification".to_string(),
            payload: serde_json::to_value(&notification).unwrap_or_default(),
        });
        notification
    }

    pub fn sip_notifications(&self) -> Vec<SipNotification> {
        self.sip_notifications
            .read()
            .expect("sip notifications lock poisoned")
            .clone()
    }

    pub fn create_conference(&self, input: CreateConferenceRequest) -> Conference {
        let conference = Conference {
            id: Uuid::new_v4(),
            title: input.title,
            mode: input.mode,
            participants: Vec::new(),
            active: false,
            created_at: Utc::now(),
        };
        self.conferences.insert(conference.id, conference.clone());
        self.conferences.trim_to_len(MAX_CONFERENCES);
        self.persist(&conference);
        conference
    }

    pub fn list_conferences(&self) -> Vec<Conference> {
        self.conferences.values()
    }

    pub fn join_conference(&self, id: Uuid, input: JoinConferenceRequest) -> Option<Conference> {
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            if !conference.participants.iter().any(|p| p.user_id == input.user_id) {
                conference.participants.push(ConferenceParticipant {
                    user_id: input.user_id,
                    sip_uri: input.sip_uri,
                    role: input.role.unwrap_or(ParticipantRole::Member),
                    bridge_slot: None,
                    joined_at: Utc::now(),
                });
            }
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
        }
        conference
    }

    pub fn leave_conference(&self, id: Uuid, user_id: Uuid) -> Option<Conference> {
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            conference.participants.retain(|p| p.user_id != user_id);
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
        }
        conference
    }

    pub fn activate_conference(&self, id: Uuid) -> Option<Conference> {
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            conference.active = true;
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
        }
        conference
    }

    pub fn conference_by_uri(&self, uri: &str) -> Option<Conference> {
        let stripped = uri
            .strip_prefix("sip:")
            .or_else(|| uri.strip_prefix("sips:"))
            .unwrap_or(uri);
        let user_part = stripped.split('@').next()?;
        let uuid_str = user_part.strip_prefix("conf-")?;
        let id = Uuid::parse_str(uuid_str).ok()?;
        self.conferences.get(&id)
    }

    pub fn create_call(&self, input: CreateCallRequest) -> CallSession {
        let call = CallSession {
            id: Uuid::new_v4(),
            conference_id: input.conference_id,
            caller: input.caller,
            callees: input.callees,
            media: input.media,
            status: CallStatus::Ringing,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.calls.insert(call.id, call.clone());
        self.calls.trim_to_len(MAX_CALLS);
        self.persist(&call);
        call
    }

    pub fn update_call_status(&self, id: Uuid, status: CallStatus) -> Option<CallSession> {
        let call = self.calls.with_write(&id, |calls| {
            let call = calls.get_mut(&id)?;
            call.status = status;
            call.updated_at = Utc::now();
            Some(call.clone())
        });
        if let Some(call) = &call {
            self.persist(call);
        }
        call
    }

    pub fn calls(&self) -> Vec<CallSession> {
        self.calls.values()
    }

    pub fn put_file_record(&self, record: FileRecord) {
        self.files.insert(record.id, record.clone());
        self.files.trim_to_len(MAX_FILES);
        self.persist(&record);
    }

    pub fn delete_file_record(&self, id: Uuid) -> Option<FileRecord> {
        let record = self.files.remove(&id);
        if record.is_some() {
            self.delete_persisted(FileRecord::collection(), id.to_string());
        }
        record
    }

    pub fn file_record(&self, id: Uuid) -> Option<FileRecord> {
        self.files.get(&id)
    }

    pub fn file_records(&self) -> Vec<FileRecord> {
        self.files.values()
    }

    pub fn create_routing_rule(&self, input: CreateRoutingRuleRequest) -> RoutingRule {
        let rule = RoutingRule {
            id: Uuid::new_v4(),
            name: input.name,
            source_pattern: input.source_pattern,
            destination_pattern: input.destination_pattern,
            target: input.target,
            destination_type: input.destination_type.unwrap_or_else(default_route_destination_type),
            method_pattern: input.method_pattern.unwrap_or_else(default_route_method_pattern),
            header_conditions: input.header_conditions.unwrap_or_default(),
            header_actions: input.header_actions.unwrap_or_default(),
            stop_processing: input.stop_processing.unwrap_or(true),
            priority: input.priority,
            enabled: input.enabled,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.routing_rules.insert(rule.id, rule.clone());
        self.routing_rules.trim_to_len(MAX_ROUTING_RULES);
        self.persist(&rule);
        let r = rule.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_routing_rule(&r).await }));
        rule
    }

    pub fn routing_rules(&self) -> Vec<RoutingRule> {
        let mut rules = self.routing_rules.values();
        rules.sort_by_key(|rule| rule.priority);
        rules
    }

    pub fn resolve_routing_target(&self, source: &str, destination: &str) -> Option<String> {
        self.resolve_routing_rule(source, destination, "INVITE", &[])
            .map(|rule| rule.target)
    }

    pub fn resolve_routing_rule(
        &self,
        source: &str,
        destination: &str,
        method: &str,
        headers: &[(String, String)],
    ) -> Option<RoutingRule> {
        self.routing_rules()
            .into_iter()
            .filter(|rule| rule.enabled)
            .find(|rule| {
                pattern_matches(&rule.source_pattern, source)
                    && pattern_matches(&rule.destination_pattern, destination)
                    && route_method_matches(&rule.method_pattern, method)
                    && route_headers_match(&rule.header_conditions, headers)
            })
    }

    pub fn delete_routing_rule(&self, id: Uuid) -> Option<RoutingRule> {
        let rule = self.routing_rules.remove(&id);
        if rule.is_some() {
            self.delete_persisted(RoutingRule::collection(), id.to_string());
        }
        rule
    }

    pub fn update_routing_rule(
        &self,
        id: Uuid,
        input: CreateRoutingRuleRequest,
    ) -> Option<RoutingRule> {
        let rule = self.routing_rules.with_write(&id, |rules| {
            let rule = rules.get_mut(&id)?;
            rule.name = input.name;
            rule.source_pattern = input.source_pattern;
            rule.destination_pattern = input.destination_pattern;
            rule.target = input.target;
            rule.destination_type = input.destination_type.unwrap_or_else(default_route_destination_type);
            rule.method_pattern = input.method_pattern.unwrap_or_else(default_route_method_pattern);
            rule.header_conditions = input.header_conditions.unwrap_or_default();
            rule.header_actions = input.header_actions.unwrap_or_default();
            rule.stop_processing = input.stop_processing.unwrap_or(true);
            rule.priority = input.priority;
            rule.enabled = input.enabled;
            rule.updated_at = Utc::now();
            Some(rule.clone())
        });
        if let Some(rule) = &rule {
            self.persist(rule);
        }
        rule
    }

    // ─── Presence ───

    pub fn update_presence(
        &self,
        sip_uri: &str,
        status: PresenceStatus,
        note: Option<String>,
    ) -> UserPresence {
        let presence = UserPresence {
            sip_uri: sip_uri.to_string(),
            status,
            note,
            updated_at: Utc::now(),
            status_message: None,
        };
        self.presence
            .insert(sip_uri.to_string(), presence.clone());
        self.presence.trim_to_len(MAX_PRESENCE);
        let p = presence.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_presence(&p).await }));
        self.broadcast_sse(SseEvent {
            event_type: "presence".to_string(),
            payload: serde_json::to_value(&presence).unwrap_or_default(),
        });
        presence
    }

    pub fn presence(&self, sip_uri: &str) -> Option<UserPresence> {
        self.presence.get(&sip_uri.to_string())
    }

    pub fn all_presence(&self) -> Vec<UserPresence> {
        self.presence.values()
    }

    // ─── SSE ───

    pub fn sse_subscribe(&self) -> tokio::sync::broadcast::Receiver<SseEvent> {
        self.sse_tx.subscribe()
    }

    pub fn broadcast_sse(&self, event: SseEvent) {
        let _ = self.sse_tx.send(event);
    }

    // ─── Call Center: Agent Management ───

    pub fn create_agent_profile(&self, input: CreateAgentProfileRequest) -> Result<AgentProfile, String> {
        if self.agent_profiles.get(&input.user_sip_uri).is_some() {
            return Err("Agent profile already exists".to_string());
        }
        let profile = AgentProfile {
            id: Uuid::new_v4(),
            user_sip_uri: input.user_sip_uri.clone(),
            role: input.role.unwrap_or_else(|| "agent".to_string()),
            display_name: input.display_name.unwrap_or_default(),
            queues: input.queues.unwrap_or_default(),
            skills: input.skills.unwrap_or_default(),
            max_concurrent: input.max_concurrent.unwrap_or(1),
            auto_answer: input.auto_answer.unwrap_or(false),
            state: "offline".to_string(),
            state_reason: None,
            state_since: Utc::now(),
            total_calls: 0,
            total_talk_secs: 0,
        };
        self.agent_profiles.insert(input.user_sip_uri, profile.clone());
        Ok(profile)
    }

    pub fn list_agent_profiles(&self) -> Vec<AgentProfile> { self.agent_profiles.values() }

    pub fn agent_profile(&self, uri: &str) -> Option<AgentProfile> {
        self.agent_profiles.get(&uri.to_string())
    }

    pub fn set_agent_state(&self, uri: &str, state: &str, reason: Option<String>) -> Option<AgentProfile> {
        self.agent_profiles.with_write(&uri.to_string(), |profiles| {
            let profile = profiles.get_mut(uri)?;
            profile.state = state.to_string();
            profile.state_reason = reason;
            profile.state_since = Utc::now();
            Some(profile.clone())
        })
    }

    pub fn delete_agent_profile(&self, uri: &str) -> Option<AgentProfile> {
        self.agent_profiles.remove(&uri.to_string())
    }

    pub fn transition_agent_state(&self, uri: &str, new_state: &str, reason: Option<String>) -> Result<AgentProfile, String> {
        let profile = self.agent_profile(uri).ok_or("Agent not found")?;
        let old_state = profile.state.clone();

        let valid = match (old_state.as_str(), new_state) {
            ("offline", "available") => true,
            ("available", "on_call") | ("available", "break") | ("available", "training") |
            ("available", "meeting") | ("available", "offline") => true,
            ("on_call", "wrap_up") | ("on_call", "available") => true,
            ("wrap_up", "available") | ("wrap_up", "break") | ("wrap_up", "offline") => true,
            ("break", "available") | ("break", "offline") => true,
            ("training", "available") | ("training", "offline") => true,
            ("meeting", "available") | ("meeting", "offline") => true,
            (_, "offline") => true,
            _ => false,
        };
        if !valid {
            return Err(format!("Invalid state transition: {} -> {}", old_state, new_state));
        }

        let duration = (Utc::now() - profile.state_since).num_seconds() as i32;

        // Log state change
        let uri_owned = uri.to_string();
        let old = old_state.clone();
        let new_s = new_state.to_string();
        let r = reason.clone();
        self.pg_spawn(move |pg| Box::pin(async move {
            pg.insert_agent_state_log(&uri_owned, &old, &new_s, r.as_deref(), duration).await
        }));

        let updated = self.set_agent_state(uri, new_state, reason)
            .ok_or("Failed to update agent state")?;

        self.broadcast_sse(SseEvent {
            event_type: "agent.state".to_string(),
            payload: serde_json::json!({
                "agent_uri": uri,
                "previous_state": old_state,
                "new_state": new_state,
                "state_since": updated.state_since,
            }),
        });

        Ok(updated)
    }

    // ─── Queue Caller Tracking ───

    pub fn enqueue_caller(&self, queue_id: Uuid, caller_uri: &str, caller_name: &str) -> QueueCallerEntry {
        let position = self.queue_callers.values().into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .count() as i32 + 1;
        let caller = QueueCallerEntry {
            id: Uuid::new_v4(),
            queue_id,
            caller_uri: caller_uri.to_string(),
            caller_name: caller_name.to_string(),
            position,
            entered_at: Utc::now(),
            answered_at: None,
            answered_by: None,
            status: "waiting".to_string(),
        };
        self.queue_callers.insert(caller.id, caller.clone());
        let c = caller.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_queue_caller(&c).await }));
        self.broadcast_sse(SseEvent {
            event_type: "queue.caller_joined".to_string(),
            payload: serde_json::to_value(&caller).unwrap_or_default(),
        });
        caller
    }

    pub fn dequeue_caller(&self, caller_id: Uuid, agent_uri: &str) {
        if let Some(mut caller) = self.queue_callers.get(&caller_id) {
            caller.status = "answered".to_string();
            caller.answered_at = Some(Utc::now());
            caller.answered_by = Some(agent_uri.to_string());
            self.queue_callers.insert(caller_id, caller);
        }
    }

    pub fn abandon_caller(&self, caller_id: Uuid) {
        if let Some(mut caller) = self.queue_callers.get(&caller_id) {
            caller.status = "abandoned".to_string();
            self.queue_callers.insert(caller_id, caller.clone());
            self.broadcast_sse(SseEvent {
                event_type: "queue.caller_abandoned".to_string(),
                payload: serde_json::to_value(&caller).unwrap_or_default(),
            });
        }
    }

    pub fn queue_callers_waiting(&self, queue_id: Uuid) -> Vec<QueueCallerEntry> {
        self.queue_callers.values().into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .collect()
    }

    pub fn queue_callers_waiting_count(&self, queue_id: Uuid) -> usize {
        self.queue_callers.values().into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .count()
    }

    // ─── VIP Caller Management ───

    pub fn check_vip(&self, caller_uri: &str) -> Option<VipCaller> {
        self.vip_callers.values().into_iter()
            .find(|v| caller_uri.contains(&v.caller_pattern) || v.caller_pattern == caller_uri || caller_uri.ends_with(&v.caller_pattern))
    }

    pub fn create_vip_caller(&self, input: CreateVipCallerRequest) -> VipCaller {
        let vip = VipCaller {
            id: Uuid::new_v4(),
            caller_pattern: input.caller_pattern,
            priority: input.priority.unwrap_or(10),
            label: input.label.unwrap_or_default(),
            queue_override: input.queue_override,
            agent_override: input.agent_override,
            created_at: Utc::now(),
        };
        self.vip_callers.insert(vip.id, vip.clone());
        let v = vip.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_vip_caller(&v).await }));
        vip
    }

    pub fn list_vip_callers(&self) -> Vec<VipCaller> {
        self.vip_callers.values()
    }

    pub fn delete_vip_caller(&self, id: Uuid) -> Option<VipCaller> {
        let removed = self.vip_callers.remove(&id);
        if removed.is_some() {
            self.pg_spawn(move |pg| Box::pin(async move { pg.delete_vip_caller(id).await }));
        }
        removed
    }

    // ─── Queue Callbacks ───

    pub fn request_callback(&self, queue_id: Uuid, input: RequestCallbackInput) -> QueueCallback {
        let position = self.queue_callers_waiting_count(queue_id) as i32;
        let cb = QueueCallback {
            id: Uuid::new_v4(),
            queue_id,
            caller_uri: input.caller_uri,
            caller_name: input.caller_name.unwrap_or_default(),
            callback_number: input.callback_number,
            position,
            status: "pending".to_string(),
            requested_at: Utc::now(),
            scheduled_at: None,
            attempted_at: None,
            completed_at: None,
            attempts: 0,
            max_attempts: 3,
        };
        self.queue_callbacks.insert(cb.id, cb.clone());
        let c = cb.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_queue_callback(&c).await }));
        self.broadcast_sse(SseEvent {
            event_type: "queue.callback_requested".to_string(),
            payload: serde_json::to_value(&cb).unwrap_or_default(),
        });
        cb
    }

    pub fn list_queue_callbacks(&self, queue_id: Uuid) -> Vec<QueueCallback> {
        self.queue_callbacks.values().into_iter()
            .filter(|cb| cb.queue_id == queue_id)
            .collect()
    }

    pub fn pending_callbacks(&self, queue_id: Uuid) -> Vec<QueueCallback> {
        let mut cbs: Vec<_> = self.queue_callbacks.values().into_iter()
            .filter(|cb| cb.queue_id == queue_id && cb.status == "pending")
            .collect();
        cbs.sort_by_key(|cb| cb.position);
        cbs
    }

    pub fn queue_wallboard(&self) -> Vec<QueueMetricsSnapshot> {
        let all_dialogs = self.sip_dialogs.values();
        let cdrs = self.cdrs.read().expect("cdrs lock");
        let now = Utc::now();

        self.list_queues().into_iter().map(|q| {
            let agent_uris: Vec<&str> = q.agents.iter().map(|a| a.agent_uri.as_str()).collect();

            // Use agent profiles for real-time state where available,
            // falling back to the queue-level agent state.
            let mut available = 0i32;
            let mut busy = 0i32;
            let mut paused = 0i32;
            for qa in &q.agents {
                let state = self.agent_profiles.get(&qa.agent_uri)
                    .map(|p| p.state.clone())
                    .unwrap_or_else(|| qa.state.clone());
                match state.as_str() {
                    "available" => available += 1,
                    "busy" | "on_call" => busy += 1,
                    "paused" | "break" => paused += 1,
                    _ => {}
                }
            }

            // Active calls: dialogs where one side is a queue agent and status is confirmed
            let calls_active = all_dialogs.iter().filter(|d| {
                !matches!(d.status, SipDialogStatus::Ended | SipDialogStatus::Cancelled | SipDialogStatus::Failed)
                    && (agent_uris.contains(&d.from_uri.as_str())
                        || agent_uris.contains(&d.to_uri.as_str()))
            }).count() as i32;

            // CDR stats for this queue
            let queue_cdrs: Vec<_> = cdrs.iter()
                .filter(|c| c.queue_name.as_deref() == Some(&q.name))
                .collect();
            let answered: Vec<_> = queue_cdrs.iter().filter(|c| c.disposition == "answered").collect();
            let abandoned = queue_cdrs.iter().filter(|c| c.disposition == "abandoned").count() as i32;
            let calls_answered = answered.len() as i32;

            // Wait time stats from answered CDRs that have queue_wait_secs
            let wait_times: Vec<i32> = answered.iter()
                .filter_map(|c| c.queue_wait_secs)
                .collect();
            let avg_wait_secs = if wait_times.is_empty() { 0 } else {
                wait_times.iter().sum::<i32>() / wait_times.len() as i32
            };

            // Average talk time from answered CDRs
            let talk_times: Vec<i32> = answered.iter()
                .map(|c| c.duration_secs)
                .filter(|&d| d > 0)
                .collect();
            let avg_talk_secs = if talk_times.is_empty() { 0 } else {
                talk_times.iter().sum::<i32>() / talk_times.len() as i32
            };

            // Longest waiting: unanswered CDRs still in progress for this queue
            let longest_wait_secs = queue_cdrs.iter()
                .filter(|c| c.end_time.is_none() && c.disposition == "no_answer")
                .map(|c| (now - c.start_time).num_seconds() as i32)
                .max()
                .unwrap_or(0);

            // Calls waiting: CDRs with no end_time and no_answer disposition
            let calls_waiting = queue_cdrs.iter()
                .filter(|c| c.end_time.is_none() && c.disposition == "no_answer")
                .count() as i32;

            let total = calls_answered + abandoned;
            let sla_percentage = if total == 0 { 100.0 } else {
                (calls_answered as f32 / total as f32) * 100.0
            };

            QueueMetricsSnapshot {
                queue_id: q.id,
                queue_name: q.name,
                calls_waiting,
                calls_active,
                agents_available: available,
                agents_busy: busy,
                agents_paused: paused,
                longest_wait_secs,
                avg_wait_secs,
                avg_talk_secs,
                calls_answered,
                calls_abandoned: abandoned,
                sla_percentage,
            }
        }).collect()
    }

    // ─── Monitor Sessions ───

    pub fn start_monitor(&self, supervisor: &str, input: StartMonitorRequest) -> MonitorSession {
        let session = MonitorSession {
            id: Uuid::new_v4(),
            supervisor_uri: supervisor.to_string(),
            target_call_id: input.target_call_id,
            agent_uri: input.agent_uri,
            mode: input.mode,
            started_at: Utc::now(),
        };
        self.monitor_sessions.insert(session.id, session.clone());
        session
    }

    pub fn list_monitor_sessions(&self) -> Vec<MonitorSession> { self.monitor_sessions.values() }

    pub fn end_monitor(&self, id: Uuid) -> Option<MonitorSession> {
        self.monitor_sessions.remove(&id)
    }

    // ─── QA Scorecards ───

    pub fn create_scorecard(&self, reviewer: &str, input: CreateScorecardRequest) -> QaScorecard {
        let sc = QaScorecard {
            id: Uuid::new_v4(),
            call_id: input.call_id,
            agent_uri: input.agent_uri,
            reviewer_uri: reviewer.to_string(),
            queue_name: input.queue_name,
            scores: input.scores,
            total_score: input.total_score,
            max_score: input.max_score,
            comments: input.comments.unwrap_or_default(),
            created_at: Utc::now(),
        };
        self.qa_scorecards.write().expect("qa lock").push(sc.clone());
        sc
    }

    pub fn list_scorecards(&self) -> Vec<QaScorecard> {
        self.qa_scorecards.read().expect("qa lock").clone()
    }

    // ─── Canned Responses ───

    pub fn create_canned_response(&self, input: CreateCannedResponseRequest) -> CannedResponse {
        let cr = CannedResponse {
            id: Uuid::new_v4(),
            category: input.category.unwrap_or_else(|| "general".to_string()),
            shortcode: input.shortcode,
            title: input.title,
            body: input.body,
        };
        self.canned_responses.insert(cr.id, cr.clone());
        cr
    }

    pub fn list_canned_responses(&self) -> Vec<CannedResponse> { self.canned_responses.values() }
    pub fn delete_canned_response(&self, id: Uuid) -> Option<CannedResponse> { self.canned_responses.remove(&id) }

    // ─── User Call Settings ───

    pub fn get_user_call_settings(&self, sip_uri: &str) -> UserCallSettings {
        self.user_call_settings
            .get(&sip_uri.to_string())
            .unwrap_or_else(|| {
                let mut settings = UserCallSettings::default();
                settings.user_sip_uri = sip_uri.to_string();
                settings
            })
    }

    pub fn set_user_call_settings(&self, settings: UserCallSettings) {
        self.user_call_settings
            .insert(settings.user_sip_uri.clone(), settings);
    }

    // ─── Call Queues ───

    pub fn create_queue(&self, input: CreateQueueRequest) -> Result<CallQueue, String> {
        let queue = CallQueue {
            id: Uuid::new_v4(),
            name: input.name,
            extension: input.extension,
            strategy: input.strategy.unwrap_or_else(|| "round_robin".to_string()),
            max_wait_time: input.max_wait_time.unwrap_or(300),
            max_queue_size: input.max_queue_size.unwrap_or(50),
            wrap_up_time: input.wrap_up_time.unwrap_or(10),
            announce_position: true,
            announce_interval: 30,
            hold_music_file_id: input.hold_music_file_id,
            overflow_destination: input.overflow_destination,
            agents: input.agents.into_iter().map(|a| QueueAgent {
                agent_uri: a.agent_uri,
                priority: a.priority.unwrap_or(1),
                skills: a.skills.unwrap_or_default(),
                state: "available".to_string(),
                calls_handled: 0,
                penalty: 0,
            }).collect(),
            enabled: true,
            created_at: Utc::now(),
            callback_enabled: input.callback_enabled.unwrap_or(false),
            callback_threshold_secs: input.callback_threshold_secs.unwrap_or(120),
            sla_target_secs: input.sla_target_secs.unwrap_or(20),
        };
        self.call_queues.insert(queue.id, queue.clone());
        Ok(queue)
    }

    pub fn list_queues(&self) -> Vec<CallQueue> { self.call_queues.values() }
    pub fn queue(&self, id: Uuid) -> Option<CallQueue> { self.call_queues.get(&id) }
    pub fn delete_queue(&self, id: Uuid) -> Option<CallQueue> { self.call_queues.remove(&id) }

    pub fn queue_by_extension(&self, uri: &str) -> Option<CallQueue> {
        let user = sip_user_part(uri);
        self.call_queues.values().into_iter().find(|q| {
            (q.extension == uri || sip_user_part(&q.extension) == user) && q.enabled
        })
    }

    // ─── Extensions ───

    pub fn create_extension(&self, input: CreateExtensionRequest) -> Result<Extension, String> {
        if self.extensions.get(&input.extension).is_some() {
            return Err(format!("Extension {} already exists", input.extension));
        }
        let user_display_name = input.user_id.and_then(|uid| {
            self.users.get(&uid).map(|u| u.display_name.clone())
        });
        let ext = Extension {
            extension: input.extension.clone(),
            destination: input.destination,
            destination_type: input.destination_type.unwrap_or_else(|| "user".to_string()),
            label: input.label.unwrap_or_default(),
            user_id: input.user_id,
            user_display_name,
            is_did: input.is_did.unwrap_or(false),
        };
        self.extensions.insert(input.extension, ext.clone());
        let e = ext.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_extension(&e).await }));
        Ok(ext)
    }

    pub fn create_did(&self, input: CreateDidRequest) -> Result<Extension, String> {
        self.create_extension(CreateExtensionRequest {
            extension: normalize_did(&input.did),
            destination: input.destination,
            destination_type: Some(input.destination_type.unwrap_or_else(|| "user".to_string())),
            label: Some(input.label.unwrap_or_else(|| "DID".to_string())),
            user_id: input.user_id,
            is_did: Some(true),
        })
    }

    pub fn list_extensions(&self) -> Vec<Extension> { self.extensions.values() }
    pub fn list_dids(&self) -> Vec<Extension> {
        let mut dids: Vec<_> = self.extensions
            .values()
            .into_iter()
            .filter(|ext| ext.is_did)
            .collect();
        dids.sort_by(|a, b| a.extension.cmp(&b.extension));
        dids
    }

    pub fn resolve_extension(&self, uri: &str) -> Option<Extension> {
        let user = sip_user_part(uri);
        self.extensions.get(&uri.to_string())
            .or_else(|| self.extensions.get(&user.to_string()))
    }
    pub fn delete_extension(&self, ext: &str) -> Option<Extension> {
        let removed = self.extensions.remove(&ext.to_string());
        if removed.is_some() {
            let ext_key = ext.to_string();
            self.pg_spawn(move |pg| Box::pin(async move { pg.delete_pg_extension(&ext_key).await }));
        }
        removed
    }

    pub fn provision_user(&self, input: ProvisionUserRequest) -> Result<ProvisionUserResponse, String> {
        let default_username = input.display_name.to_lowercase().replace(' ', ".");
        let sip_username = input.extension_number.as_deref()
            .unwrap_or(&default_username);
        let sip_uri = format!("sip:{}@{}", sip_username, input.sip_domain);
        let normalized_sip_uri = normalize_sip_uri(&sip_uri)
            .ok_or_else(|| format!("Invalid SIP URI {}", sip_uri))?;

        // Check uniqueness
        if self.user_exists(&normalized_sip_uri) {
            return Err(format!("SIP URI {} already taken", normalized_sip_uri));
        }
        if let Some(ref ext) = input.extension_number {
            if self.extensions.get(&ext.to_string()).is_some() {
                return Err(format!("Extension {} already exists", ext));
            }
        }

        // Create user (auto-provisions SIP account)
        let user = self.create_user(CreateUserRequest {
            display_name: input.display_name.clone(),
            sip_uri: normalized_sip_uri.clone(),
            matrix_user_id: None,
            password: Some(input.password.clone()),
            role: input.role,
        })?;

        // Create linked extension if requested
        let extension = if let Some(ext_num) = input.extension_number {
            Some(self.create_extension(CreateExtensionRequest {
                extension: ext_num,
                destination: normalized_sip_uri.clone(),
                destination_type: Some("user".to_string()),
                label: Some(input.display_name.clone()),
                user_id: Some(user.id),
                is_did: Some(false),
            })?)
        } else {
            None
        };

        // Get SIP credentials
        let sip_creds = split_sip_aor_simple(&sip_uri).map(|(username, domain)| {
            SipCredentials {
                sip_uri: sip_uri.clone(),
                registrar_uri: self
                    .sip_registrar
                    .as_ref()
                    .map(|registrar| format!("sip:{}", registrar)),
                registration_available: self.sip_registrar.is_some(),
                username,
                password: input.password,
                transport: "udp".to_string(),
                domain,
            }
        });

        Ok(ProvisionUserResponse { user, extension, sip_credentials: sip_creds })
    }

    pub fn assign_extension(&self, ext: &str, user_id: Uuid) -> Result<Extension, String> {
        let mut extension = self.extensions.get(&ext.to_string())
            .ok_or_else(|| format!("Extension {} not found", ext))?;
        let user = self.users.get(&user_id)
            .ok_or_else(|| format!("User {} not found", user_id))?;
        extension.user_id = Some(user_id);
        extension.user_display_name = Some(user.display_name.clone());
        extension.destination = user.sip_uri.clone();
        extension.destination_type = "user".to_string();
        self.extensions.insert(ext.to_string(), extension.clone());
        let e = extension.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_extension(&e).await }));
        Ok(extension)
    }

    pub fn unassign_extension(&self, ext: &str) -> Result<Extension, String> {
        let mut extension = self.extensions.get(&ext.to_string())
            .ok_or_else(|| format!("Extension {} not found", ext))?;
        extension.user_id = None;
        extension.user_display_name = None;
        self.extensions.insert(ext.to_string(), extension.clone());
        let e = extension.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_extension(&e).await }));
        Ok(extension)
    }

    pub fn extensions_for_user(&self, user_id: Uuid) -> Vec<Extension> {
        self.extensions.values().into_iter()
            .filter(|e| e.user_id == Some(user_id))
            .collect()
    }

    pub fn list_extensions_filtered(&self, unassigned_only: bool) -> Vec<Extension> {
        if unassigned_only {
            self.extensions.values().into_iter()
                .filter(|e| e.user_id.is_none() && e.destination_type == "user")
                .collect()
        } else {
            self.list_extensions()
        }
    }

    // ─── Business Hours ───

    pub fn create_business_hours(&self, input: CreateBusinessHoursRequest) -> BusinessHours {
        let bh = BusinessHours {
            id: Uuid::new_v4(),
            name: input.name,
            timezone: input.timezone.unwrap_or_else(|| "America/New_York".to_string()),
            schedule: input.schedule,
            after_hours_destination: input.after_hours_destination,
            enabled: true,
            created_at: Utc::now(),
        };
        self.business_hours.insert(bh.id, bh.clone());
        bh
    }

    pub fn list_business_hours(&self) -> Vec<BusinessHours> { self.business_hours.values() }
    pub fn delete_business_hours(&self, id: Uuid) -> Option<BusinessHours> { self.business_hours.remove(&id) }

    // ─── Holidays ───

    pub fn create_holiday(&self, input: CreateHolidayRequest) -> Holiday {
        let h = Holiday {
            id: Uuid::new_v4(),
            name: input.name,
            date: input.date,
            recurring: input.recurring.unwrap_or(false),
            destination: input.destination,
            created_at: Utc::now(),
        };
        self.holidays.insert(h.id, h.clone());
        h
    }

    pub fn list_holidays(&self) -> Vec<Holiday> { self.holidays.values() }
    pub fn delete_holiday(&self, id: Uuid) -> Option<Holiday> { self.holidays.remove(&id) }

    /// Returns a holiday matching today's date (recurring holidays match on month/day).
    pub fn active_holiday_today(&self) -> Option<Holiday> {
        let today = chrono::Utc::now().date_naive();
        self.holidays.values().into_iter().find(|h| {
            if let Ok(hdate) = chrono::NaiveDate::parse_from_str(&h.date, "%Y-%m-%d") {
                if h.recurring {
                    hdate.month() == today.month() && hdate.day() == today.day()
                } else {
                    hdate == today
                }
            } else {
                false
            }
        })
    }

    /// Check whether the current time falls within configured business hours.
    /// Returns `(true, None)` when open or no business hours are configured,
    /// or `(false, Some(after_hours_destination))` when closed.
    pub fn is_within_business_hours(&self) -> (bool, Option<String>) {
        let enabled: Vec<_> = self.business_hours.values().into_iter().filter(|bh| bh.enabled).collect();
        if enabled.is_empty() {
            return (true, None);
        }
        let bh = &enabled[0];
        let tz: chrono_tz::Tz = bh.timezone.parse().unwrap_or(chrono_tz::America::New_York);
        let now = chrono::Utc::now().with_timezone(&tz);
        let day_key = match now.weekday() {
            Weekday::Mon => "mon",
            Weekday::Tue => "tue",
            Weekday::Wed => "wed",
            Weekday::Thu => "thu",
            Weekday::Fri => "fri",
            Weekday::Sat => "sat",
            Weekday::Sun => "sun",
        };
        if let Some(day_schedule) = bh.schedule.get(day_key) {
            if let (Some(open_str), Some(close_str)) = (
                day_schedule.get("open").and_then(|v| v.as_str()),
                day_schedule.get("close").and_then(|v| v.as_str()),
            ) {
                let current_time = now.format("%H:%M").to_string();
                if current_time.as_str() >= open_str && current_time.as_str() < close_str {
                    return (true, None);
                }
            }
        }
        (false, bh.after_hours_destination.clone())
    }

    /// Check whether the target user has Do-Not-Disturb enabled.
    /// Returns `(true, forward_to)` when DND is active, `(false, None)` otherwise.
    pub fn check_dnd(&self, target_uri: &str) -> (bool, Option<String>) {
        let settings = self.get_user_call_settings(target_uri);
        if settings.dnd_enabled {
            (true, settings.dnd_forward_to.clone())
        } else {
            (false, None)
        }
    }

    /// Resolve call forwarding for a target user based on the call state.
    /// `call_state` should be one of "always", "busy", or "no_answer".
    pub fn resolve_call_forwarding(&self, target_uri: &str, call_state: &str) -> Option<String> {
        let settings = self.get_user_call_settings(target_uri);
        match call_state {
            "always" => settings.forward_always.clone().filter(|s| !s.is_empty()),
            "busy" => settings.forward_busy.clone().filter(|s| !s.is_empty()),
            "no_answer" => settings.forward_no_answer.clone().filter(|s| !s.is_empty()),
            _ => None,
        }
    }

    /// Pick the next available agent from a queue according to its routing strategy.
    pub fn next_available_agent(&self, queue: &CallQueue) -> Option<String> {
        self.claim_next_agent(queue, &[])
    }

    pub fn claim_next_agent(&self, queue: &CallQueue, required_skills: &[String]) -> Option<String> {
        let _lock = self.agent_assignment_lock.lock().ok()?;

        let mut candidates: Vec<(usize, &QueueAgent)> = queue.agents.iter().enumerate()
            .filter(|(_, a)| a.state == "available")
            .filter(|(_, a)| {
                self.agent_profile(&a.agent_uri)
                    .map_or(false, |p| p.state == "available")
            })
            .filter(|(_, a)| {
                if required_skills.is_empty() { return true; }
                required_skills.iter().all(|s| a.skills.contains(s))
            })
            .collect();

        if candidates.is_empty() { return None; }

        // Sort by strategy
        match queue.strategy.as_str() {
            "longest_idle" => {
                candidates.sort_by(|(_, a), (_, b)| {
                    let a_since = self.agent_profile(&a.agent_uri).map(|p| p.state_since).unwrap_or_else(Utc::now);
                    let b_since = self.agent_profile(&b.agent_uri).map(|p| p.state_since).unwrap_or_else(Utc::now);
                    a_since.cmp(&b_since)
                });
            }
            "round_robin" => {
                candidates.sort_by_key(|(_, a)| a.calls_handled);
            }
            "skills_based" => {
                candidates.sort_by_key(|(_, a)| a.skills.len());
            }
            "random" => {
                use rand::seq::SliceRandom;
                candidates.shuffle(&mut rand::thread_rng());
            }
            _ => {}
        }

        // Secondary sort by penalty (lower penalty = higher priority)
        // Use stable sort to preserve strategy ordering within same penalty
        candidates.sort_by_key(|(_, a)| a.penalty);

        if let Some((idx, agent)) = candidates.first() {
            let uri = agent.agent_uri.clone();
            let queue_id = queue.id;
            let agent_idx = *idx;

            // Mark agent on_call INSIDE the lock
            self.set_agent_state(&uri, "on_call", None);

            // Increment calls_handled on the queue
            self.call_queues.with_write(&queue_id, |queues| {
                if let Some(q) = queues.get_mut(&queue_id) {
                    if let Some(qa) = q.agents.get_mut(agent_idx) {
                        qa.calls_handled += 1;
                        qa.state = "on_call".to_string();
                    }
                }
            });

            // Increment total_calls on agent profile
            self.agent_profiles.with_write(&uri, |profiles| {
                if let Some(p) = profiles.get_mut(&uri) {
                    p.total_calls += 1;
                }
            });

            return Some(uri);
        }
        None
    }

    /// Start a new CDR, persist it, and return the record.
    pub fn record_cdr_start(&self, call_id: Option<&str>, caller_uri: &str, callee_uri: &str, direction: &str) -> CallDetailRecord {
        let cdr = CallDetailRecord {
            id: Uuid::new_v4(),
            call_id: call_id.map(String::from),
            caller_uri: caller_uri.to_string(),
            callee_uri: callee_uri.to_string(),
            direction: direction.to_string(),
            start_time: Utc::now(),
            answer_time: None,
            end_time: None,
            duration_secs: 0,
            disposition: "no_answer".to_string(),
            queue_name: None,
            queue_wait_secs: None,
            recorded: false,
        };
        self.record_cdr(cdr.clone());
        cdr
    }

    /// Finalize a CDR by call_id: set end_time, disposition, and duration.
    pub fn record_cdr_end(&self, call_id: &str, disposition: &str) {
        let mut cdrs = self.cdrs.write().expect("cdrs lock");
        for cdr in cdrs.iter_mut().rev() {
            if cdr.call_id.as_deref() == Some(call_id) {
                let now = Utc::now();
                cdr.end_time = Some(now);
                cdr.disposition = disposition.to_string();
                cdr.duration_secs = (now - cdr.start_time).num_seconds() as i32;
                break;
            }
        }
    }

    /// Create a voicemail record for a user, store it, and emit an SSE event.
    pub fn create_voicemail_for_user(
        &self,
        callee_uri: &str,
        caller_uri: &str,
        caller_name: &str,
        duration_secs: i32,
        file_id: Option<Uuid>,
    ) -> Voicemail {
        let vm = Voicemail {
            id: Uuid::new_v4(),
            callee_uri: callee_uri.to_string(),
            caller_uri: caller_uri.to_string(),
            caller_name: caller_name.to_string(),
            duration_secs,
            file_id,
            listened: false,
            created_at: Utc::now(),
        };
        self.store_voicemail(vm.clone());
        vm
    }

    // ─── Call Park ───

    pub fn park_call(&self, slot: &str, call_id: &str, parked_by: &str, caller_uri: &str, caller_name: &str) -> ParkedCall {
        let pc = ParkedCall {
            slot: slot.to_string(),
            call_id: call_id.to_string(),
            parked_by: parked_by.to_string(),
            caller_uri: caller_uri.to_string(),
            caller_name: caller_name.to_string(),
            parked_at: Utc::now(),
        };
        self.parked_calls.insert(slot.to_string(), pc.clone());
        pc
    }

    pub fn pickup_parked_call(&self, slot: &str) -> Option<ParkedCall> {
        self.parked_calls.remove(&slot.to_string())
    }

    pub fn list_parked_calls(&self) -> Vec<ParkedCall> { self.parked_calls.values() }

    // ─── Speed Dial ───

    pub fn set_speed_dial(&self, owner: Option<&str>, input: CreateSpeedDialRequest) -> SpeedDial {
        let sd = SpeedDial {
            code: input.code,
            destination: input.destination,
            label: input.label.unwrap_or_default(),
            owner_uri: owner.map(String::from),
        };
        let mut dials = self.speed_dials.write().expect("speed dials lock");
        dials.retain(|d| !(d.code == sd.code && d.owner_uri == sd.owner_uri));
        dials.push(sd.clone());
        sd
    }

    pub fn speed_dials_for_user(&self, owner: &str) -> Vec<SpeedDial> {
        self.speed_dials.read().expect("speed dials lock")
            .iter()
            .filter(|d| d.owner_uri.as_deref() == Some(owner) || d.owner_uri.is_none())
            .cloned()
            .collect()
    }

    // ─── CDR ───

    pub fn record_cdr(&self, cdr: CallDetailRecord) {
        let mut cdrs = self.cdrs.write().expect("cdrs lock");
        cdrs.push(cdr);
        if cdrs.len() > 100_000 {
            let overflow = cdrs.len() - 100_000;
            cdrs.drain(..overflow);
        }
    }

    pub fn list_cdrs(&self, limit: usize) -> Vec<CallDetailRecord> {
        let cdrs = self.cdrs.read().expect("cdrs lock");
        cdrs.iter().rev().take(limit).cloned().collect()
    }

    // ─── Paging Groups ───

    pub fn create_paging_group(&self, input: CreatePagingGroupRequest) -> PagingGroup {
        let pg = PagingGroup {
            id: Uuid::new_v4(),
            name: input.name,
            extension: input.extension,
            members: input.members,
        };
        self.paging_groups.insert(pg.id, pg.clone());
        pg
    }

    pub fn list_paging_groups(&self) -> Vec<PagingGroup> { self.paging_groups.values() }
    pub fn delete_paging_group(&self, id: Uuid) -> Option<PagingGroup> { self.paging_groups.remove(&id) }

    // ─── Ring Groups ───

    pub fn create_ring_group(&self, input: CreateRingGroupRequest) -> Result<RingGroup, String> {
        if self.ring_groups.values().iter().any(|g| g.extension == input.extension) {
            return Err(format!("Ring group with extension {} already exists", input.extension));
        }
        let group = RingGroup {
            id: Uuid::new_v4(),
            name: input.name,
            extension: input.extension,
            strategy: input.strategy.unwrap_or(RingStrategy::Simultaneous),
            ring_timeout: input.ring_timeout.unwrap_or(30),
            members: input.members,
            fallback_uri: input.fallback_uri,
            enabled: true,
            created_at: Utc::now(),
        };
        self.ring_groups.insert(group.id, group.clone());
        Ok(group)
    }

    pub fn list_ring_groups(&self) -> Vec<RingGroup> {
        self.ring_groups.values()
    }

    pub fn ring_group(&self, id: Uuid) -> Option<RingGroup> {
        self.ring_groups.get(&id)
    }

    pub fn ring_group_by_extension(&self, uri: &str) -> Option<RingGroup> {
        let user = sip_user_part(uri);
        self.ring_groups.values().into_iter().find(|g| {
            (g.extension == uri || sip_user_part(&g.extension) == user) && g.enabled
        })
    }

    pub fn delete_ring_group(&self, id: Uuid) -> Option<RingGroup> {
        self.ring_groups.remove(&id)
    }

    // ─── IVR ───

    pub fn create_ivr(&self, input: CreateIvrRequest) -> Result<Ivr, String> {
        if self.ivrs.values().iter().any(|i| i.extension == input.extension) {
            return Err(format!("IVR with extension {} already exists", input.extension));
        }
        let ivr = Ivr {
            id: Uuid::new_v4(),
            name: input.name,
            extension: input.extension,
            greeting_text: input.greeting_text.unwrap_or_else(|| "Welcome.".to_string()),
            greeting_file_id: input.greeting_file_id,
            timeout_secs: input.timeout_secs.unwrap_or(10),
            max_retries: input.max_retries.unwrap_or(3),
            invalid_destination: input.invalid_destination,
            timeout_destination: input.timeout_destination,
            options: input.options,
            enabled: true,
            created_at: Utc::now(),
        };
        self.ivrs.insert(ivr.id, ivr.clone());
        Ok(ivr)
    }

    pub fn list_ivrs(&self) -> Vec<Ivr> {
        self.ivrs.values()
    }

    pub fn ivr(&self, id: Uuid) -> Option<Ivr> {
        self.ivrs.get(&id)
    }

    pub fn ivr_by_extension(&self, uri: &str) -> Option<Ivr> {
        let user = sip_user_part(uri);
        self.ivrs.values().into_iter().find(|i| {
            (i.extension == uri || sip_user_part(&i.extension) == user) && i.enabled
        })
    }

    pub fn delete_ivr(&self, id: Uuid) -> Option<Ivr> {
        self.ivrs.remove(&id)
    }

    // ─── Call Route Resolution ───

    /// Resolve where an inbound call to a URI should be routed.
    /// Checks in order: direct user registration, ring groups, IVRs, routing rules.
    pub fn resolve_inbound_route(&self, destination_uri: &str) -> ResolvedRoute {
        // 1. Check if it's a ring group extension
        if let Some(group) = self.ring_group_by_extension(destination_uri) {
            return ResolvedRoute {
                destination_type: "ring_group".to_string(),
                destination: group.extension.clone(),
                ring_group: Some(group),
                ivr: None,
            };
        }

        // 2. Check if it's an IVR extension
        if let Some(ivr) = self.ivr_by_extension(destination_uri) {
            return ResolvedRoute {
                destination_type: "ivr".to_string(),
                destination: ivr.extension.clone(),
                ring_group: None,
                ivr: Some(ivr),
            };
        }

        // 3. Check routing rules
        if let Some(target) = self.resolve_routing_target("*", destination_uri) {
            // Check if the routing target points to a ring group or IVR
            if let Some(group) = self.ring_group_by_extension(&target) {
                return ResolvedRoute {
                    destination_type: "ring_group".to_string(),
                    destination: target,
                    ring_group: Some(group),
                    ivr: None,
                };
            }
            if let Some(ivr) = self.ivr_by_extension(&target) {
                return ResolvedRoute {
                    destination_type: "ivr".to_string(),
                    destination: target,
                    ring_group: None,
                    ivr: Some(ivr),
                };
            }
            return ResolvedRoute {
                destination_type: "user".to_string(),
                destination: target,
                ring_group: None,
                ivr: None,
            };
        }

        // 4. Default: route to the user directly
        ResolvedRoute {
            destination_type: "user".to_string(),
            destination: destination_uri.to_string(),
            ring_group: None,
            ivr: None,
        }
    }

    pub fn preview_route(
        &self,
        direction: &str,
        source: &str,
        destination: &str,
        method: &str,
        headers: &[(String, String)],
    ) -> RoutePreview {
        let destination_uri = if destination.starts_with("sip:") {
            destination.to_string()
        } else {
            format!("sip:{destination}")
        };
        let matched_rule = self.resolve_routing_rule(source, &destination_uri, method, headers);
        let resolved = if direction.eq_ignore_ascii_case("outbound") {
            let target = matched_rule
                .as_ref()
                .map(|rule| rule.target.clone())
                .unwrap_or_else(|| destination_uri.clone());
            ResolvedRoute {
                destination_type: matched_rule
                    .as_ref()
                    .map(|rule| rule.destination_type.clone())
                    .unwrap_or_else(|| "external".to_string()),
                destination: target,
                ring_group: None,
                ivr: None,
            }
        } else {
            self.resolve_inbound_route(&destination_uri)
        };
        let header_actions = matched_rule
            .as_ref()
            .map(|rule| rule.header_actions.clone())
            .unwrap_or_default();

        RoutePreview {
            direction: direction.to_string(),
            source: source.to_string(),
            destination: destination_uri,
            method: method.to_string(),
            matched_rule,
            resolved,
            header_actions,
        }
    }

    // ─── Voicemail ───

    pub fn store_voicemail(&self, vm: Voicemail) {
        self.voicemails.insert(vm.id, vm.clone());
        self.broadcast_sse(SseEvent {
            event_type: "voicemail".to_string(),
            payload: serde_json::to_value(&vm).unwrap_or_default(),
        });
    }

    pub fn voicemails_for_user(&self, callee_uri: &str) -> Vec<Voicemail> {
        self.voicemails
            .values()
            .into_iter()
            .filter(|v| v.callee_uri == callee_uri)
            .collect()
    }

    pub fn mark_voicemail_listened(&self, id: Uuid) -> Option<Voicemail> {
        self.voicemails.with_write(&id, |vms| {
            let vm = vms.get_mut(&id)?;
            vm.listened = true;
            Some(vm.clone())
        })
    }

    pub fn delete_voicemail(&self, id: Uuid) -> Option<Voicemail> {
        self.voicemails.remove(&id)
    }

    // ─── Call Recordings ───

    pub fn store_recording(&self, recording: CallRecording) {
        self.recordings.insert(recording.id, recording.clone());
        self.broadcast_sse(SseEvent {
            event_type: "recording".to_string(),
            payload: serde_json::to_value(&recording).unwrap_or_default(),
        });
        let r = recording.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_recording(&r).await }));
    }

    pub fn recordings_for_user(&self, sip_uri: &str) -> Vec<CallRecording> {
        self.recordings
            .values()
            .into_iter()
            .filter(|r| r.caller_uri == sip_uri || r.callee_uri == sip_uri)
            .collect()
    }

    pub fn delete_recording(&self, id: Uuid) -> Option<CallRecording> {
        self.recordings.remove(&id)
    }

    // ─── Group Chat Rooms ───

    pub fn create_room(&self, creator: &str, input: CreateRoomRequest) -> Room {
        let is_direct = input.is_direct.unwrap_or(false);
        let mut members = input.members.clone();
        members.push(creator.to_string());
        let members = normalized_room_members(members);

        if is_direct {
            if let Some(existing) = self.rooms.values().into_iter().find(|room| {
                if !room.is_direct || room.members.len() != members.len() {
                    return false;
                }
                let mut room_members: Vec<_> = room
                    .members
                    .iter()
                    .map(|member| member.user_sip_uri.clone())
                    .collect();
                room_members.sort();
                room_members == members
            }) {
                return existing;
            }
        }

        let room = Room {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description.unwrap_or_default(),
            is_direct,
            created_by: creator.to_string(),
            members: members
                .into_iter()
                .map(|uri| {
                    let role = if uri == creator { "admin" } else { "member" };
                    RoomMember {
                        user_sip_uri: uri,
                        role: role.to_string(),
                        joined_at: Utc::now(),
                    }
                })
                .collect(),
            conference_id: None,
            call_uri: None,
            created_at: Utc::now(),
        };
        self.rooms.insert(room.id, room.clone());
        self.rooms.trim_to_len(MAX_ROOMS);
        self.persist(&room);
        let room_for_pg = room.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
        self.broadcast_sse(SseEvent {
            event_type: "room_created".to_string(),
            payload: serde_json::to_value(&room).unwrap_or_default(),
        });
        room
    }

    pub fn start_room_call(&self, room_id: Uuid, mode: RoomCallMode) -> Option<RoomCallTarget> {
        let (target, updated_room) = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            if room.is_direct {
                return None;
            }

            let conference_id = match room.conference_id.and_then(|id| self.conferences.get(&id)) {
                Some(conference) if matches_room_call_mode(&conference.mode, &mode) => conference.id,
                _ => {
                    let conference = self.create_conference(CreateConferenceRequest {
                        title: room.name.clone(),
                        mode: mode.clone().into(),
                    });
                    room.conference_id = Some(conference.id);
                    conference.id
                }
            };
            let call_uri = format!("sip:conf-{}@pale.local", conference_id);
            room.call_uri = Some(call_uri.clone());
            let target = RoomCallTarget {
                room_id,
                conference_id,
                call_uri,
                mode,
            };
            Some((target, room.clone()))
        })?;

        self.persist(&updated_room);
        let room_for_pg = updated_room.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
        let _ = self.activate_conference(target.conference_id);
        self.broadcast_sse(SseEvent {
            event_type: "room_call_started".to_string(),
            payload: serde_json::to_value(&target).unwrap_or_default(),
        });
        Some(target)
    }

    pub fn join_room_call(
        &self,
        room_id: Uuid,
        user_sip_uri: &str,
        mode: RoomCallMode,
    ) -> Option<RoomCallTarget> {
        let target = self.start_room_call(room_id, mode)?;
        let user_id = self
            .users
            .values()
            .into_iter()
            .find(|user| user.sip_uri == user_sip_uri)
            .map(|user| user.id)
            .unwrap_or_else(Uuid::nil);
        let _ = self.join_conference(
            target.conference_id,
            JoinConferenceRequest {
                user_id,
                sip_uri: user_sip_uri.to_string(),
                role: Some(ParticipantRole::Member),
            },
        );
        Some(target)
    }

    pub fn list_rooms_for_user(&self, sip_uri: &str) -> Vec<Room> {
        self.rooms
            .values()
            .into_iter()
            .filter(|r| r.members.iter().any(|m| m.user_sip_uri == sip_uri))
            .collect()
    }

    pub fn room(&self, id: Uuid) -> Option<Room> {
        self.rooms.get(&id)
    }

    pub fn add_room_member(&self, room_id: Uuid, user_sip_uri: &str) -> Option<Room> {
        let updated = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            if !room.members.iter().any(|m| m.user_sip_uri == user_sip_uri) {
                room.members.push(RoomMember {
                    user_sip_uri: user_sip_uri.to_string(),
                    role: "member".to_string(),
                    joined_at: Utc::now(),
                });
            }
            Some(room.clone())
        })?;
        self.persist(&updated);
        let room_for_pg = updated.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
        Some(updated)
    }

    pub fn remove_room_member(&self, room_id: Uuid, user_sip_uri: &str) -> Option<Room> {
        let updated = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            room.members.retain(|m| m.user_sip_uri != user_sip_uri);
            Some(room.clone())
        })?;
        self.persist(&updated);
        let room_for_pg = updated.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
        Some(updated)
    }

    pub fn send_room_message(&self, room_id: Uuid, sender_uri: &str, body: &str, reply_to: Option<Uuid>) -> RoomMessage {
        let msg = RoomMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_uri: sender_uri.to_string(),
            body: body.to_string(),
            content_type: "text/plain".to_string(),
            created_at: Utc::now(),
            reply_to,
            edited_at: None,
            pinned: false,
        };
        let mut messages = self.room_messages.write().expect("room messages lock poisoned");
        messages.push(msg.clone());
        if messages.len() > MAX_ROOM_MESSAGES {
            let overflow = messages.len() - MAX_ROOM_MESSAGES;
            messages.drain(..overflow);
        }
        self.persist(&msg);
        let msg_for_pg = msg.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_room_message(&msg_for_pg).await }));
        self.broadcast_sse(SseEvent {
            event_type: "room_message".to_string(),
            payload: serde_json::to_value(&msg).unwrap_or_default(),
        });
        msg
    }

    pub fn edit_room_message(&self, id: Uuid, new_body: &str) -> Option<RoomMessage> {
        let mut messages = self.room_messages.write().expect("room messages lock poisoned");
        let msg = messages.iter_mut().find(|m| m.id == id)?;
        msg.body = new_body.to_string();
        msg.edited_at = Some(Utc::now());
        let updated = msg.clone();
        self.broadcast_sse(SseEvent {
            event_type: "message_edited".to_string(),
            payload: serde_json::to_value(&updated).unwrap_or_default(),
        });
        self.persist(&updated);
        let body = updated.body.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.update_room_message_body(id, &body).await }));
        Some(updated)
    }

    pub fn pin_room_message(&self, id: Uuid) -> Option<RoomMessage> {
        let mut messages = self.room_messages.write().expect("room messages lock poisoned");
        let msg = messages.iter_mut().find(|m| m.id == id)?;
        msg.pinned = !msg.pinned;
        let updated = msg.clone();
        self.broadcast_sse(SseEvent {
            event_type: "message_pinned".to_string(),
            payload: serde_json::to_value(&updated).unwrap_or_default(),
        });
        self.persist(&updated);
        let pinned = updated.pinned;
        self.pg_spawn(move |pg| Box::pin(async move { pg.toggle_pin(id, pinned).await }));
        Some(updated)
    }

    pub fn delete_room_message(&self, id: Uuid) -> Option<RoomMessage> {
        let mut messages = self.room_messages.write().expect("room messages lock poisoned");
        let index = messages.iter().position(|m| m.id == id)?;
        let deleted = messages.remove(index);
        self.delete_persisted(RoomMessage::collection(), deleted.key());
        self.pg_spawn(move |pg| Box::pin(async move { pg.delete_room_message(id).await }));
        Some(deleted)
    }

    pub fn pinned_messages(&self, room_id: Uuid) -> Vec<RoomMessage> {
        self.room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|m| m.room_id == room_id && m.pinned)
            .cloned()
            .collect()
    }

    pub fn add_reaction(&self, message_id: Uuid, user_uri: &str, emoji: &str) {
        self.message_reactions.with_write(&message_id, |map| {
            let reactions = map.entry(message_id).or_insert_with(Vec::new);
            // Toggle: if same user+emoji exists, remove it
            if let Some(pos) = reactions.iter().position(|r| r.user_uri == user_uri && r.emoji == emoji) {
                reactions.remove(pos);
            } else {
                reactions.push(MessageReaction {
                    emoji: emoji.to_string(),
                    user_uri: user_uri.to_string(),
                    created_at: Utc::now(),
                });
            }
        });
        self.broadcast_sse(SseEvent {
            event_type: "reaction".to_string(),
            payload: serde_json::json!({
                "message_id": message_id,
                "emoji": emoji,
                "user": user_uri,
                "created_at": Utc::now(),
            }),
        });
    }

    pub fn message_reactions(&self, message_id: Uuid) -> Vec<MessageReaction> {
        self.message_reactions.get(&message_id).unwrap_or_default()
    }

    pub fn add_favorite(&self, user_uri: &str, favorite_uri: &str) {
        let key = user_uri.to_string();
        self.user_favorites.with_write(&key, |map| {
            let favorites = map.entry(key.clone()).or_insert_with(Vec::new);
            if !favorites.contains(&favorite_uri.to_string()) {
                favorites.push(favorite_uri.to_string());
            }
        });
    }

    pub fn remove_favorite(&self, user_uri: &str, favorite_uri: &str) {
        let key = user_uri.to_string();
        self.user_favorites.with_write(&key, |map| {
            if let Some(favorites) = map.get_mut(&key) {
                favorites.retain(|f| f != favorite_uri);
            }
        });
    }

    pub fn list_favorites(&self, user_uri: &str) -> Vec<String> {
        self.user_favorites.get(&user_uri.to_string()).unwrap_or_default()
    }

    pub fn update_user_profile(
        &self,
        id: Uuid,
        email: Option<String>,
        title: Option<String>,
        department: Option<String>,
        phone_number: Option<String>,
    ) -> Option<User> {
        self.users.with_write(&id, |map| {
            let user = map.get_mut(&id)?;
            if let Some(e) = email { user.email = Some(e); }
            if let Some(t) = title { user.title = Some(t); }
            if let Some(d) = department { user.department = Some(d); }
            if let Some(p) = phone_number { user.phone_number = Some(p); }
            Some(user.clone())
        })
    }

    pub fn room_messages(&self, room_id: Uuid) -> Vec<RoomMessage> {
        self.room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|m| m.room_id == room_id)
            .cloned()
            .collect()
    }

    pub fn room_message(&self, id: Uuid) -> Option<RoomMessage> {
        self.room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .find(|m| m.id == id)
            .cloned()
    }

    /// Returns true if Postgres is connected and healthy.
    pub fn pg_healthy(&self) -> bool {
        if self.pg.is_none() {
            return true; // No PG configured = not degraded
        }
        let failures = self
            .pg_failure_count
            .load(std::sync::atomic::Ordering::Relaxed);
        failures < 10 // Circuit breaker: open after 10 consecutive failures
    }

    /// Spawn a background Postgres write with circuit breaker.
    /// After 10 consecutive failures, stops attempting writes until a success resets the counter.
    pub fn pg_spawn<F>(&self, f: F)
    where
        F: FnOnce(PgStore) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), pg_store::PgError>> + Send>>
            + Send
            + 'static,
    {
        if let Some(pg) = self.pg.clone() {
            let failures = self.pg_failure_count.load(std::sync::atomic::Ordering::Relaxed);
            if failures >= 10 {
                // Circuit open — skip writes, log periodically
                if failures % 100 == 10 {
                    log::warn!("Postgres circuit breaker open ({} consecutive failures), skipping writes", failures);
                }
                self.pg_failure_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let counter = self.pg_failure_count.clone();
            tokio::spawn(async move {
                match f(pg).await {
                    Ok(()) => {
                        counter.store(0, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        log::error!("Postgres write failed: {}", e);
                    }
                }
            });
        }
    }

    // ─── Call History ───

    pub fn store_call_history(&self, user_sip_uri: &str, input: CallHistoryInput) -> CallHistoryEntry {
        let entry = CallHistoryEntry {
            id: Uuid::new_v4(),
            user_sip_uri: user_sip_uri.to_string(),
            direction: input.direction,
            remote_uri: input.remote_uri,
            remote_name: input.remote_name,
            start_time: input.start_time,
            duration_secs: input.duration_secs,
            answered: input.answered,
            synced_at: Utc::now(),
        };
        self.call_history.insert(entry.id, entry.clone());
        self.call_history.trim_to_len(MAX_CALL_HISTORY);
        entry
    }

    pub fn call_history_for_user(&self, sip_uri: &str) -> Vec<CallHistoryEntry> {
        self.call_history
            .values()
            .into_iter()
            .filter(|e| e.user_sip_uri == sip_uri)
            .collect()
    }

    pub fn merge_call_history(&self, user_sip_uri: &str, entries: Vec<CallHistoryInput>) -> usize {
        use std::collections::HashSet;
        let existing = self.call_history_for_user(user_sip_uri);
        let existing_set: HashSet<(i64, &str, &str)> = existing
            .iter()
            .map(|e| (e.start_time.timestamp(), e.remote_uri.as_str(), e.direction.as_str()))
            .collect();
        let mut merged = 0;
        for input in entries {
            let key = (input.start_time.timestamp(), input.remote_uri.as_str(), input.direction.as_str());
            if !existing_set.contains(&key) {
                self.store_call_history(user_sip_uri, input);
                merged += 1;
            }
        }
        merged
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
    pub sip_uri: String,
    pub matrix_user_id: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub role: String, // "admin" or "user"
    pub created_at: DateTime<Utc>,
    pub email: Option<String>,
    pub title: Option<String>,
    pub department: Option<String>,
    pub phone_number: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub display_name: String,
    pub sip_uri: String,
    pub matrix_user_id: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
}

/// Response returned after user login — contains everything the client needs
#[derive(Debug, Clone, Serialize)]
pub struct UserLoginResponse {
    pub token: String,
    pub user: User,
    pub sip_credentials: Option<SipCredentials>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SipCredentials {
    pub sip_uri: String,
    /// Registrar to REGISTER against. `None` when the active SIP backend
    /// cannot register clients — clients should skip auto-registration.
    pub registrar_uri: Option<String>,
    /// True only when the server runs a backend that implements REGISTER.
    pub registration_available: bool,
    pub username: String,
    pub password: String,
    pub transport: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSession {
    pub token: String,
    pub principal: String,
    /// Role attached to this session ("admin" or "user"). Sessions created
    /// before role separation deserialize to an empty string, which is
    /// treated as non-admin (fail closed).
    #[serde(default)]
    pub role: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Unauthorized,
    Locked,
}

#[derive(Debug, Clone)]
struct LoginAttempt {
    failures: u32,
    last_failure_at: Option<DateTime<Utc>>,
    locked_until: DateTime<Utc>,
}

impl Default for LoginAttempt {
    fn default() -> Self {
        Self {
            failures: 0,
            last_failure_at: None,
            locked_until: DateTime::<Utc>::UNIX_EPOCH,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAuditEvent {
    pub id: Uuid,
    pub principal: String,
    pub action: String,
    pub target: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipAccount {
    pub username: String,
    pub domain: String,
    pub display_name: Option<String>,
    #[serde(skip_serializing)]
    pub password_ha1: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl SipAccount {
    pub fn aor(&self) -> String {
        format!("sip:{}@{}", self.username, self.domain)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSipAccountRequest {
    pub username: String,
    pub domain: String,
    pub password_ha1: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipRegistration {
    pub aor: String,
    pub contact: String,
    pub source: String,
    pub user_agent: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipDialog {
    pub call_id: String,
    pub from_uri: String,
    pub to_uri: String,
    pub target_contact: Option<String>,
    pub status: SipDialogStatus,
    #[serde(default)]
    pub media_types: Vec<MediaKind>,
    /// Caller's Contact header (route target for requests toward the caller).
    #[serde(default)]
    pub from_contact: Option<String>,
    /// Caller's transport source address as observed by the proxy.
    #[serde(default)]
    pub from_source: Option<String>,
    /// Callee's transport address the INVITE was forwarded to.
    #[serde(default)]
    pub to_source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipMessage {
    pub id: Uuid,
    pub call_id: Option<String>,
    pub from_uri: String,
    pub to_uri: String,
    pub content_type: String,
    pub body: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipTransaction {
    pub id: Uuid,
    pub method: String,
    pub uri: String,
    pub call_id: Option<String>,
    pub cseq: Option<String>,
    pub source: String,
    pub status_code: Option<u16>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipSubscription {
    pub subscription_id: String,
    pub subscriber: String,
    pub target: String,
    pub event: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UpsertSipSubscription {
    pub subscription_id: String,
    pub subscriber: String,
    pub target: String,
    pub event: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipNotification {
    pub id: Uuid,
    pub subscription_id: Option<String>,
    pub notifier: String,
    pub target: String,
    pub event: Option<String>,
    pub subscription_state: Option<String>,
    pub content_type: String,
    pub body: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoreSipNotification {
    pub subscription_id: Option<String>,
    pub notifier: String,
    pub target: String,
    pub event: Option<String>,
    pub subscription_state: Option<String>,
    pub content_type: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Online,
    Offline,
    Busy,
    Away,
    Dnd,
    OnCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    pub sip_uri: String,
    pub status: PresenceStatus,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RateLimitBucket {
    pub tokens: f64,
    pub last_refill: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPresenceRequest {
    pub status: PresenceStatus,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHistoryEntry {
    pub id: Uuid,
    pub user_sip_uri: String,
    pub direction: String,
    pub remote_uri: String,
    pub remote_name: String,
    pub start_time: DateTime<Utc>,
    pub duration_secs: i64,
    pub answered: bool,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncCallHistoryRequest {
    pub entries: Vec<CallHistoryInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallHistoryInput {
    pub direction: String,
    pub remote_uri: String,
    pub remote_name: String,
    pub start_time: DateTime<Utc>,
    pub duration_secs: i64,
    pub answered: bool,
}

// ─── Voicemail & Call Recordings ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voicemail {
    pub id: Uuid,
    pub callee_uri: String,
    pub caller_uri: String,
    pub caller_name: String,
    pub duration_secs: i32,
    pub file_id: Option<Uuid>,
    pub listened: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecording {
    pub id: Uuid,
    pub call_id: Option<String>,
    pub caller_uri: String,
    pub callee_uri: String,
    pub duration_secs: i32,
    pub file_id: Option<Uuid>,
    pub recorded_by: String,
    pub created_at: DateTime<Utc>,
}

// ─── Group Chat Rooms ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub is_direct: bool,
    pub created_by: String,
    pub members: Vec<RoomMember>,
    #[serde(default)]
    pub conference_id: Option<Uuid>,
    #[serde(default)]
    pub call_uri: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMember {
    pub user_sip_uri: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<String>, // SIP URIs to invite
    pub is_direct: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomCallMode {
    Audio,
    Video,
}

impl From<RoomCallMode> for ConferenceMode {
    fn from(mode: RoomCallMode) -> Self {
        match mode {
            RoomCallMode::Audio => ConferenceMode::Audio,
            RoomCallMode::Video => ConferenceMode::Video,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomCallTarget {
    pub room_id: Uuid,
    pub conference_id: Uuid,
    pub call_uri: String,
    pub mode: RoomCallMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMessage {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_uri: String,
    pub body: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub reply_to: Option<Uuid>,
    pub edited_at: Option<DateTime<Utc>>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReaction {
    pub emoji: String,
    pub user_uri: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendRoomMessageRequest {
    pub body: String,
    pub reply_to: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddRoomMemberRequest {
    pub user_sip_uri: String,
}

// ─── Read Receipts ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRead {
    pub message_id: Uuid,
    pub reader_uri: String,
    pub read_at: DateTime<Utc>,
}

// ─── Search ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: Uuid,
    pub source: String, // "sip" or "room"
    pub from_uri: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub room_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct StoreSipTransaction {
    pub method: String,
    pub uri: String,
    pub call_id: Option<String>,
    pub cseq: Option<String>,
    pub source: String,
    pub status_code: Option<u16>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreSipMessage {
    pub call_id: Option<String>,
    pub from_uri: String,
    pub to_uri: String,
    pub content_type: String,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
pub struct DialogPeerInfo {
    /// Caller's Contact header value.
    pub from_contact: Option<String>,
    /// Caller's transport source address.
    pub from_source: Option<String>,
    /// Callee's transport address the INVITE was forwarded to.
    pub to_source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertSipDialog {
    pub call_id: String,
    pub from_uri: String,
    pub to_uri: String,
    pub target_contact: Option<String>,
    pub status: SipDialogStatus,
    pub media_types: Vec<MediaKind>,
    /// Peer-leg addressing. `None` fields leave any stored value untouched.
    pub peer: DialogPeerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SipDialogStatus {
    Routing,
    Ringing,
    Queued,
    Answered,
    Held,
    Cancelled,
    Ended,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConferenceMode {
    Audio,
    Video,
    Webinar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conference {
    pub id: Uuid,
    pub title: String,
    pub mode: ConferenceMode,
    pub participants: Vec<ConferenceParticipant>,
    #[serde(default)]
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConferenceRequest {
    pub title: String,
    pub mode: ConferenceMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinConferenceRequest {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub role: Option<ParticipantRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Host,
    Moderator,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceParticipant {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub role: ParticipantRole,
    #[serde(default)]
    pub bridge_slot: Option<i32>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
    ScreenShare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConfig {
    pub ice_enabled: bool,
    pub stun_servers: Vec<String>,
    pub stun_ignore_failure: bool,
    pub turn: Option<TurnConfig>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            ice_enabled: true,
            stun_servers: Vec::new(),
            stun_ignore_failure: true,
            turn: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnConfig {
    pub server: String,
    pub transport: TurnTransport,
    pub username: Option<String>,
    pub realm: Option<String>,
    #[serde(skip_serializing)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnTransport {
    Udp,
    Tcp,
    Tls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCallRequest {
    pub conference_id: Option<Uuid>,
    pub caller: String,
    pub callees: Vec<String>,
    pub media: Vec<MediaKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCallStatusRequest {
    pub status: CallStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSipAccountStatusRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSession {
    pub id: Uuid,
    pub conference_id: Option<Uuid>,
    pub caller: String,
    pub callees: Vec<String>,
    pub media: Vec<MediaKind>,
    pub status: CallStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    Ringing,
    Active,
    Held,
    Ended,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: Uuid,
    pub owner: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: Uuid,
    pub name: String,
    pub source_pattern: String,
    pub destination_pattern: String,
    pub target: String,
    #[serde(default = "default_route_destination_type")]
    pub destination_type: String,
    #[serde(default = "default_route_method_pattern")]
    pub method_pattern: String,
    #[serde(default)]
    pub header_conditions: Vec<RouteHeaderCondition>,
    #[serde(default)]
    pub header_actions: Vec<SipHeaderAction>,
    #[serde(default = "default_route_stop_processing")]
    pub stop_processing: bool,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoutingRuleRequest {
    pub name: String,
    pub source_pattern: String,
    pub destination_pattern: String,
    pub target: String,
    pub destination_type: Option<String>,
    pub method_pattern: Option<String>,
    pub header_conditions: Option<Vec<RouteHeaderCondition>>,
    pub header_actions: Option<Vec<SipHeaderAction>>,
    pub stop_processing: Option<bool>,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteHeaderCondition {
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub negate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SipHeaderActionKind {
    Add,
    Set,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SipHeaderAction {
    pub kind: SipHeaderActionKind,
    pub name: String,
    #[serde(default)]
    pub value: String,
}

fn default_route_destination_type() -> String {
    "user".to_string()
}

fn default_route_method_pattern() -> String {
    "*".to_string()
}

fn default_route_stop_processing() -> bool {
    true
}

fn normalize_did(did: &str) -> String {
    did.trim().replace([' ', '-', '(', ')'], "")
}

// ─── User Call Settings (Voicemail + Follow-Me) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCallSettings {
    pub user_sip_uri: String,

    // Voicemail
    pub voicemail_enabled: bool,
    pub voicemail_greeting_file_id: Option<Uuid>,
    pub voicemail_greeting_text: String,
    pub voicemail_timeout: i32,

    // Follow-me
    pub followme_enabled: bool,
    pub followme_numbers: Vec<FollowMeEntry>,
    pub followme_final: String, // "voicemail", "hangup", or SIP URI

    // Call forwarding
    pub forward_always: Option<String>,
    pub forward_busy: Option<String>,
    pub forward_no_answer: Option<String>,

    // DND
    pub dnd_enabled: bool,
    pub dnd_forward_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowMeEntry {
    pub number: String,      // SIP URI or phone number
    pub ring_timeout: i32,   // seconds to ring before trying next
    pub label: String,       // "Office", "Mobile", "Home"
}

impl Default for UserCallSettings {
    fn default() -> Self {
        Self {
            user_sip_uri: String::new(),
            voicemail_enabled: true,
            voicemail_greeting_file_id: None,
            voicemail_greeting_text: "Please leave a message after the tone.".to_string(),
            voicemail_timeout: 20,
            followme_enabled: false,
            followme_numbers: Vec::new(),
            followme_final: "voicemail".to_string(),
            forward_always: None,
            forward_busy: None,
            forward_no_answer: None,
            dnd_enabled: false,
            dnd_forward_to: None,
        }
    }
}

// ─── Ring Groups ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingGroup {
    pub id: Uuid,
    pub name: String,
    pub extension: String,
    pub strategy: RingStrategy,
    pub ring_timeout: i32,
    pub members: Vec<String>, // SIP URIs
    pub fallback_uri: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RingStrategy {
    Simultaneous,
    Sequential,
    Random,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRingGroupRequest {
    pub name: String,
    pub extension: String,
    pub strategy: Option<RingStrategy>,
    pub ring_timeout: Option<i32>,
    pub members: Vec<String>,
    pub fallback_uri: Option<String>,
}

// ─── IVR ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ivr {
    pub id: Uuid,
    pub name: String,
    pub extension: String,
    pub greeting_text: String,
    pub greeting_file_id: Option<Uuid>,
    pub timeout_secs: i32,
    pub max_retries: i32,
    pub invalid_destination: Option<String>,
    pub timeout_destination: Option<String>,
    pub options: Vec<IvrOption>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvrOption {
    pub digit: String,
    pub label: String,
    pub destination: String,
    pub destination_type: String, // user, ring_group, ivr, voicemail, external
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateIvrRequest {
    pub name: String,
    pub extension: String,
    pub greeting_text: Option<String>,
    pub greeting_file_id: Option<Uuid>,
    pub timeout_secs: Option<i32>,
    pub max_retries: Option<i32>,
    pub invalid_destination: Option<String>,
    pub timeout_destination: Option<String>,
    pub options: Vec<IvrOption>,
}

// ─── Call Route Resolution ───

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedRoute {
    pub destination_type: String, // user, ring_group, ivr
    pub destination: String,     // SIP URI or ID
    pub ring_group: Option<RingGroup>,
    pub ivr: Option<Ivr>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutePreview {
    pub direction: String,
    pub source: String,
    pub destination: String,
    pub method: String,
    pub matched_rule: Option<RoutingRule>,
    pub resolved: ResolvedRoute,
    pub header_actions: Vec<SipHeaderAction>,
}

// ─── Call Queues (ACD) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallQueue {
    pub id: Uuid,
    pub name: String,
    pub extension: String,
    pub strategy: String,
    pub max_wait_time: i32,
    pub max_queue_size: i32,
    pub wrap_up_time: i32,
    pub announce_position: bool,
    pub announce_interval: i32,
    pub hold_music_file_id: Option<Uuid>,
    pub overflow_destination: Option<String>,
    pub agents: Vec<QueueAgent>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub callback_enabled: bool,
    pub callback_threshold_secs: i32,
    pub sla_target_secs: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueAgent {
    pub agent_uri: String,
    pub priority: i32,
    pub skills: Vec<String>,
    pub state: String,
    pub calls_handled: i32,
    pub penalty: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateQueueRequest {
    pub name: String,
    pub extension: String,
    pub strategy: Option<String>,
    pub max_wait_time: Option<i32>,
    pub max_queue_size: Option<i32>,
    pub wrap_up_time: Option<i32>,
    pub hold_music_file_id: Option<Uuid>,
    pub overflow_destination: Option<String>,
    pub agents: Vec<QueueAgentInput>,
    pub callback_enabled: Option<bool>,
    pub callback_threshold_secs: Option<i32>,
    pub sla_target_secs: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueAgentInput {
    pub agent_uri: String,
    pub priority: Option<i32>,
    pub skills: Option<Vec<String>>,
}

// ─── Business Hours ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessHours {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub schedule: serde_json::Value,
    pub after_hours_destination: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBusinessHoursRequest {
    pub name: String,
    pub timezone: Option<String>,
    pub schedule: serde_json::Value,
    pub after_hours_destination: Option<String>,
}

// ─── Holiday Calendar ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holiday {
    pub id: Uuid,
    pub name: String,
    pub date: String,
    pub recurring: bool,
    pub destination: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateHolidayRequest {
    pub name: String,
    pub date: String,
    pub recurring: Option<bool>,
    pub destination: Option<String>,
}

// ─── Extensions ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    pub extension: String,
    pub destination: String,
    pub destination_type: String,
    pub label: String,
    pub user_id: Option<Uuid>,
    pub user_display_name: Option<String>,
    #[serde(default)]
    pub is_did: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateExtensionRequest {
    pub extension: String,
    pub destination: String,
    pub destination_type: Option<String>,
    pub label: Option<String>,
    pub user_id: Option<Uuid>,
    pub is_did: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDidRequest {
    pub did: String,
    pub destination: String,
    pub destination_type: Option<String>,
    pub label: Option<String>,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvisionUserRequest {
    pub display_name: String,
    pub password: String,
    pub role: Option<String>,
    pub extension_number: Option<String>,
    pub sip_domain: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvisionUserResponse {
    pub user: User,
    pub extension: Option<Extension>,
    pub sip_credentials: Option<SipCredentials>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignExtensionRequest {
    pub user_id: Uuid,
}

// ─── Call Park ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParkedCall {
    pub slot: String,
    pub call_id: String,
    pub parked_by: String,
    pub caller_uri: String,
    pub caller_name: String,
    pub parked_at: DateTime<Utc>,
}

// ─── Speed Dial ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedDial {
    pub code: String,
    pub destination: String,
    pub label: String,
    pub owner_uri: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpeedDialRequest {
    pub code: String,
    pub destination: String,
    pub label: Option<String>,
}

// ─── CDR ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallDetailRecord {
    pub id: Uuid,
    pub call_id: Option<String>,
    pub caller_uri: String,
    pub callee_uri: String,
    pub direction: String,
    pub start_time: DateTime<Utc>,
    pub answer_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_secs: i32,
    pub disposition: String,
    pub queue_name: Option<String>,
    pub queue_wait_secs: Option<i32>,
    pub recorded: bool,
}

// ─── Paging Groups ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagingGroup {
    pub id: Uuid,
    pub name: String,
    pub extension: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePagingGroupRequest {
    pub name: String,
    pub extension: String,
    pub members: Vec<String>,
}

// ─── Call Center: Agent Profiles ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: Uuid,
    pub user_sip_uri: String,
    pub role: String,           // agent, supervisor, qa, admin
    pub display_name: String,
    pub queues: Vec<Uuid>,
    pub skills: Vec<String>,
    pub max_concurrent: i32,
    pub auto_answer: bool,
    pub state: String,          // available, on_call, wrap_up, break, training, meeting, offline
    pub state_reason: Option<String>,
    pub state_since: DateTime<Utc>,
    pub total_calls: i32,
    pub total_talk_secs: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentProfileRequest {
    pub user_sip_uri: String,
    pub role: Option<String>,
    pub display_name: Option<String>,
    pub queues: Option<Vec<Uuid>>,
    pub skills: Option<Vec<String>>,
    pub max_concurrent: Option<i32>,
    pub auto_answer: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetAgentStateRequest {
    pub state: String,
    pub reason: Option<String>,
}

// ─── Call Center: Queue Metrics ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetricsSnapshot {
    pub queue_id: Uuid,
    pub queue_name: String,
    pub calls_waiting: i32,
    pub calls_active: i32,
    pub agents_available: i32,
    pub agents_busy: i32,
    pub agents_paused: i32,
    pub longest_wait_secs: i32,
    pub avg_wait_secs: i32,
    pub avg_talk_secs: i32,
    pub calls_answered: i32,
    pub calls_abandoned: i32,
    pub sla_percentage: f32,
}

// ─── Call Center: Queue Callers & Callbacks ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueCallerEntry {
    pub id: Uuid,
    pub queue_id: Uuid,
    pub caller_uri: String,
    pub caller_name: String,
    pub position: i32,
    pub entered_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub answered_by: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueCallback {
    pub id: Uuid,
    pub queue_id: Uuid,
    pub caller_uri: String,
    pub caller_name: String,
    pub callback_number: String,
    pub position: i32,
    pub status: String,
    pub requested_at: DateTime<Utc>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub attempted_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipCaller {
    pub id: Uuid,
    pub caller_pattern: String,
    pub priority: i32,
    pub label: String,
    pub queue_override: Option<String>,
    pub agent_override: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateVipCallerRequest {
    pub caller_pattern: String,
    pub priority: Option<i32>,
    pub label: Option<String>,
    pub queue_override: Option<String>,
    pub agent_override: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestCallbackInput {
    pub caller_uri: String,
    pub caller_name: Option<String>,
    pub callback_number: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentTransitionRequest {
    pub state: String,
    pub reason: Option<String>,
}

// ─── Call Center: Monitor Session ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorSession {
    pub id: Uuid,
    pub supervisor_uri: String,
    pub target_call_id: String,
    pub agent_uri: Option<String>,
    pub mode: String,  // listen, whisper, barge
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartMonitorRequest {
    pub target_call_id: String,
    pub agent_uri: Option<String>,
    pub mode: String,
}

// ─── Call Center: QA Scorecard ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaScorecard {
    pub id: Uuid,
    pub call_id: String,
    pub agent_uri: String,
    pub reviewer_uri: String,
    pub queue_name: Option<String>,
    pub scores: serde_json::Value,
    pub total_score: f32,
    pub max_score: f32,
    pub comments: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateScorecardRequest {
    pub call_id: String,
    pub agent_uri: String,
    pub queue_name: Option<String>,
    pub scores: serde_json::Value,
    pub total_score: f32,
    pub max_score: f32,
    pub comments: Option<String>,
}

// ─── Call Center: Canned Responses ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponse {
    pub id: Uuid,
    pub category: String,
    pub shortcode: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCannedResponseRequest {
    pub category: Option<String>,
    pub shortcode: String,
    pub title: String,
    pub body: String,
}

/// Parse "sip:user@domain" into (user, domain)
fn split_sip_aor_simple(aor: &str) -> Option<(String, String)> {
    let trimmed = aor.trim();
    let lower = trimmed.to_ascii_lowercase();
    let aor = if lower.starts_with("sip:") {
        &trimmed[4..]
    } else if lower.starts_with("sips:") {
        &trimmed[5..]
    } else {
        return None;
    };
    let bare = aor.split(';').next()?.split('?').next()?;
    let (username, domain) = bare.split_once('@')?;
    Some((username.to_string(), domain.to_string()))
}

fn normalize_sip_uri(aor: &str) -> Option<String> {
    let (username, domain) = split_sip_aor_simple(aor.trim())?;
    let username = username.trim();
    let domain = domain.trim();
    if username.is_empty() || domain.is_empty() {
        return None;
    }
    Some(format!(
        "sip:{}@{}",
        username.to_ascii_lowercase(),
        domain.to_ascii_lowercase()
    ))
}

fn normalized_room_members(members: Vec<String>) -> Vec<String> {
    let mut members: Vec<String> = members
        .into_iter()
        .filter_map(|member| normalize_sip_uri(&member))
        .collect();
    members.sort();
    members.dedup();
    members
}

fn matches_room_call_mode(conference_mode: &ConferenceMode, room_mode: &RoomCallMode) -> bool {
    matches!(
        (conference_mode, room_mode),
        (ConferenceMode::Audio, RoomCallMode::Audio) | (ConferenceMode::Video, RoomCallMode::Video)
    )
}

pub fn safe_filename(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "file".to_string())
}

pub fn sip_ha1(username: &str, realm: &str, password: &str) -> String {
    md5_hex(format!("{}:{}:{}", username, realm, password).as_bytes())
}

pub fn md5_hex(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn start_wrap_up_timer(state: Arc<AppState>, agent_uri: String, wrap_up_secs: i32) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(wrap_up_secs.max(1) as u64)).await;
        if let Some(profile) = state.agent_profile(&agent_uri) {
            if profile.state == "wrap_up" {
                let _ = state.transition_agent_state(&agent_uri, "available", Some("wrap_up_expired".to_string()));
            }
        }
    });
}

/// Hash a password for storage with argon2id (PHC string format).
pub fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing cannot fail with valid parameters")
        .to_string()
}

/// Verify a password against a stored hash. Accepts argon2 PHC strings and,
/// for records created before the argon2 migration, legacy unsalted
/// SHA-256 hex digests.
pub fn verify_password(password: &str, stored: &str) -> bool {
    if stored.starts_with("$argon2") {
        use argon2::password_hash::{PasswordHash, PasswordVerifier};
        let Ok(parsed) = PasswordHash::new(stored) else {
            return false;
        };
        return argon2::Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
    }
    // Legacy SHA-256 hex digest
    sha256_hex(password.as_bytes()) == stored
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    ShaDigest::update(&mut hasher, bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" || pattern == value {
        return true;
    }

    let mut remaining = value;
    let mut first = true;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        if first && !pattern.starts_with('*') {
            let Some(stripped) = remaining.strip_prefix(part) else {
                return false;
            };
            remaining = stripped;
        } else {
            let Some(index) = remaining.find(part) else {
                return false;
            };
            remaining = &remaining[index + part.len()..];
        }
        first = false;
    }

    pattern.ends_with('*') || remaining.is_empty()
}

fn route_method_matches(pattern: &str, method: &str) -> bool {
    pattern
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| pattern_matches(&part.to_ascii_uppercase(), &method.to_ascii_uppercase()))
}

fn route_headers_match(conditions: &[RouteHeaderCondition], headers: &[(String, String)]) -> bool {
    conditions.iter().all(|condition| {
        let matched = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case(&condition.name) && pattern_matches(&condition.pattern, value)
        });
        if condition.negate { !matched } else { matched }
    })
}

impl PersistedMapObject for User {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for SipRegistration {
    type Key = String;

    fn map_key(&self) -> Self::Key {
        self.aor.clone()
    }
}

impl PersistedMapObject for SipDialog {
    type Key = String;

    fn map_key(&self) -> Self::Key {
        self.call_id.clone()
    }
}

impl PersistedMapObject for Conference {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for CallSession {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for FileRecord {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for RoutingRule {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for Room {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for AdminAuditEvent {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_hash_roundtrip() {
        let hash = hash_password("correct horse battery staple");
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn legacy_sha256_hashes_still_verify() {
        let legacy = sha256_hex("old-password".as_bytes());
        assert!(verify_password("old-password", &legacy));
        assert!(!verify_password("not-it", &legacy));
    }

    #[test]
    fn conference_join_is_idempotent() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Ops".to_string(),
            mode: ConferenceMode::Video,
        });
        let user_id = Uuid::new_v4();
        let join = JoinConferenceRequest {
            user_id,
            sip_uri: "sip:alice@example.com".to_string(),
            role: None,
        };

        state.join_conference(conference.id, join.clone()).unwrap();
        let updated = state.join_conference(conference.id, join).unwrap();

        assert_eq!(updated.participants.len(), 1);
        assert_eq!(updated.participants[0].role, ParticipantRole::Member);
    }

    #[test]
    fn safe_filename_strips_paths() {
        assert_eq!(safe_filename("../../secret.txt"), "secret.txt");
        assert_eq!(safe_filename(""), "file");
    }

    #[test]
    fn sip_ha1_matches_rfc_digest_example() {
        assert_eq!(
            sip_ha1("Mufasa", "testrealm@host.com", "Circle Of Life"),
            "939e7578ed9e3c518a452acee763bce9"
        );
    }

    #[test]
    fn admin_login_issues_session() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let session = state
            .authenticate_admin("admin", "admin-password", "test")
            .unwrap();

        assert_eq!(state.principal_for_bearer(&session.token), Some("admin".to_string()));
        assert!(matches!(
            state.authenticate_admin("admin", "wrong", "test"),
            Err(AuthError::Unauthorized)
        ));
        assert!(
            state
                .audit_events()
                .iter()
                .any(|event| event.action == "admin.login.succeeded")
        );
    }

    #[test]
    fn admin_login_locks_after_repeated_failures() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        for _ in 0..MAX_LOGIN_FAILURES {
            assert!(matches!(
                state.authenticate_admin("admin", "wrong", "blocked-source"),
                Err(AuthError::Unauthorized)
            ));
        }

        assert!(matches!(
            state.authenticate_admin("admin", "admin-password", "blocked-source"),
            Err(AuthError::Locked)
        ));
    }

    #[test]
    fn routing_rules_are_sorted_and_removable() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let low = state.create_routing_rule(CreateRoutingRuleRequest {
            name: "fallback".to_string(),
            source_pattern: "*".to_string(),
            destination_pattern: "sip:*".to_string(),
            target: "sip:operator@example.com".to_string(),
            destination_type: None,
            method_pattern: None,
            header_conditions: None,
            header_actions: None,
            stop_processing: None,
            priority: 200,
            enabled: true,
        });
        let high = state.create_routing_rule(CreateRoutingRuleRequest {
            name: "priority".to_string(),
            source_pattern: "sip:vip@example.com".to_string(),
            destination_pattern: "sip:support@example.com".to_string(),
            target: "sip:vip-desk@example.com".to_string(),
            destination_type: None,
            method_pattern: None,
            header_conditions: None,
            header_actions: None,
            stop_processing: None,
            priority: 10,
            enabled: true,
        });

        let rules = state.routing_rules();
        assert_eq!(rules[0].id, high.id);
        assert_eq!(rules[1].id, low.id);

        assert_eq!(state.delete_routing_rule(high.id).unwrap().name, "priority");
        assert_eq!(state.routing_rules().len(), 1);
    }

    #[test]
    fn users_and_sip_accounts_are_manageable() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let user = state.create_user(CreateUserRequest {
            display_name: "Alice".to_string(),
            sip_uri: "sip:alice@example.com".to_string(),
            matrix_user_id: None,
            password: Some("test123".to_string()),
            role: None,
        }).unwrap();
        assert_eq!(state.delete_user(user.id).unwrap().display_name, "Alice");
        assert!(state.users().is_empty());

        state.upsert_sip_account(CreateSipAccountRequest {
            username: "alice".to_string(),
            domain: "example.com".to_string(),
            password_ha1: sip_ha1("alice", "example.com", "secret"),
            display_name: None,
        });
        let disabled = state
            .update_sip_account_enabled("alice", "example.com", false)
            .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(
            state
                .delete_sip_account("alice", "example.com")
                .unwrap()
                .username,
            "alice"
        );
        assert!(state.sip_accounts().is_empty());
    }

    #[test]
    fn users_are_unique_by_normalized_sip_uri() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test-unique-users"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let user = state
            .create_user(CreateUserRequest {
                display_name: "Alice".to_string(),
                sip_uri: "SIP:Alice@Example.COM;transport=tcp".to_string(),
                matrix_user_id: None,
                password: Some("test123".to_string()),
                role: None,
            })
            .unwrap();
        assert_eq!(user.sip_uri, "sip:alice@example.com");

        let duplicate = state.create_user(CreateUserRequest {
            display_name: "Alice Duplicate".to_string(),
            sip_uri: "sip:alice@example.com".to_string(),
            matrix_user_id: None,
            password: Some("test123".to_string()),
            role: None,
        });
        assert!(duplicate.is_err());
        assert_eq!(state.users().len(), 1);
    }

    #[test]
    fn direct_rooms_are_reused_for_same_users() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let first = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Bob".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(true),
            },
        );
        let second = state.create_room(
            "sip:bob@example.com",
            CreateRoomRequest {
                name: "Alice".to_string(),
                description: None,
                members: vec!["sip:alice@example.com".to_string()],
                is_direct: Some(true),
            },
        );

        assert_eq!(first.id, second.id);
        assert!(first.is_direct);
        assert_eq!(state.list_rooms_for_user("sip:alice@example.com").len(), 1);
        assert_eq!(state.list_rooms_for_user("sip:bob@example.com").len(), 1);
    }

    #[test]
    fn group_rooms_deduplicate_members_and_start_calls() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-room-call-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Project".to_string(),
                description: None,
                members: vec![
                    "sip:bob@example.com".to_string(),
                    "SIP:Bob@Example.com;transport=tcp".to_string(),
                    "sip:alice@example.com".to_string(),
                ],
                is_direct: Some(false),
            },
        );

        assert!(!room.is_direct);
        assert_eq!(room.members.len(), 2);
        assert!(room.members.iter().any(|member| member.user_sip_uri == "sip:alice@example.com"));
        assert!(room.members.iter().any(|member| member.user_sip_uri == "sip:bob@example.com"));

        let target = state
            .join_room_call(room.id, "sip:alice@example.com", RoomCallMode::Video)
            .expect("room call target");
        let conference = state.conference_by_uri(&target.call_uri).unwrap();
        assert!(target.call_uri.starts_with("sip:conf-"));
        assert_eq!(conference.mode, ConferenceMode::Video);
        assert!(conference.active);
        assert_eq!(conference.participants[0].sip_uri, "sip:alice@example.com");
    }

    #[test]
    fn rooms_messages_and_group_calls_survive_persistent_reload() {
        let data_dir = std::env::temp_dir().join(format!("pale-room-persist-{}", Uuid::new_v4()));
        let storage_key = "01234567890123456789012345678901".to_string();

        let state = AppState::persistent(
            data_dir.clone(),
            "012345678901234567890123".to_string(),
            "admin".to_string(),
            sha256_hex("admin-password".as_bytes()),
            storage_key.clone(),
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Project".to_string(),
                description: Some("Persistent room".to_string()),
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
            },
        );
        let target = state
            .start_room_call(room.id, RoomCallMode::Audio)
            .expect("room call target");
        let message = state.send_room_message(room.id, "sip:alice@example.com", "hello", None);
        let edited = state.edit_room_message(message.id, "hello team").unwrap();
        assert_eq!(edited.body, "hello team");
        let pinned = state.pin_room_message(message.id).unwrap();
        assert!(pinned.pinned);
        drop(state);

        let reloaded = AppState::persistent(
            data_dir.clone(),
            "012345678901234567890123".to_string(),
            "admin".to_string(),
            sha256_hex("admin-password".as_bytes()),
            storage_key.clone(),
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let persisted_room = reloaded.room(room.id).expect("persisted room");
        assert_eq!(persisted_room.conference_id, Some(target.conference_id));
        assert_eq!(persisted_room.call_uri, Some(target.call_uri));
        let persisted_messages = reloaded.room_messages(room.id);
        assert_eq!(persisted_messages.len(), 1);
        assert_eq!(persisted_messages[0].body, "hello team");
        assert!(persisted_messages[0].pinned);

        assert!(reloaded.delete_room_message(message.id).is_some());
        drop(reloaded);

        let reloaded_after_delete = AppState::persistent(
            data_dir.clone(),
            "012345678901234567890123".to_string(),
            "admin".to_string(),
            sha256_hex("admin-password".as_bytes()),
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        assert!(reloaded_after_delete.room_messages(room.id).is_empty());
        drop(reloaded_after_delete);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn routing_rule_can_be_updated() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let rule = state.create_routing_rule(CreateRoutingRuleRequest {
            name: "primary".to_string(),
            source_pattern: "*".to_string(),
            destination_pattern: "sip:*".to_string(),
            target: "sip:desk@example.com".to_string(),
            destination_type: None,
            method_pattern: None,
            header_conditions: None,
            header_actions: None,
            stop_processing: None,
            priority: 100,
            enabled: true,
        });

        let updated = state
            .update_routing_rule(
                rule.id,
                CreateRoutingRuleRequest {
                    name: "disabled".to_string(),
                    source_pattern: "sip:alice@example.com".to_string(),
                    destination_pattern: "sip:support@example.com".to_string(),
                    target: "sip:queue@example.com".to_string(),
                    destination_type: None,
                    method_pattern: None,
                    header_conditions: None,
                    header_actions: None,
                    stop_processing: None,
                    priority: 50,
                    enabled: false,
                },
            )
            .unwrap();

        assert_eq!(updated.name, "disabled");
        assert_eq!(updated.priority, 50);
        assert!(!updated.enabled);
    }
}
