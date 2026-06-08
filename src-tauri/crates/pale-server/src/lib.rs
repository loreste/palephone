use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};
use uuid::Uuid;

pub mod http;
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
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    rate_limits: ShardedMap<String, RateLimitBucket>,
    rate_limit_rps: u32,
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
            sse_tx: tokio::sync::broadcast::channel(256).0,
            rate_limits: ShardedMap::new(),
            rate_limit_rps: 100,
            pg: None,
            pg_failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
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
            || sha256_hex(password.as_bytes()) != self.admin_password_hash
        {
            self.record_login_failure(source);
            self.record_audit_event(username, "admin.login.failed", Some(source.to_string()));
            return Err(AuthError::Unauthorized);
        }

        self.clear_login_failures(source);
        let session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: self.admin_username.clone(),
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
        if bearer == self.http_token {
            return Some(self.admin_username.clone());
        }

        self.admin_sessions
            .retain(|_, session| session.expires_at > Utc::now());
        self.admin_sessions
            .get(&bearer.to_string())
            .map(|session| session.principal)
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
        self.users.values().iter().any(|u| u.sip_uri == sip_uri)
    }

    pub fn create_user(&self, input: CreateUserRequest) -> Result<User, String> {
        // Enforce unique SIP URI
        if self.user_exists(&input.sip_uri) {
            return Err(format!("User with SIP URI {} already exists", input.sip_uri));
        }

        let password_hash = input
            .password
            .as_deref()
            .map(|p| sha256_hex(p.as_bytes()));

        let user = User {
            id: Uuid::new_v4(),
            display_name: input.display_name.clone(),
            sip_uri: input.sip_uri.clone(),
            matrix_user_id: input.matrix_user_id,
            password_hash,
            role: input.role.unwrap_or_else(|| "user".to_string()),
            created_at: Utc::now(),
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
            if let Some((username, domain)) = split_sip_aor_simple(&input.sip_uri) {
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

    /// Authenticate a user (not admin) by SIP URI and password
    pub fn authenticate_user(
        &self,
        sip_uri: &str,
        password: &str,
    ) -> Result<UserLoginResponse, AuthError> {
        let user = self
            .users
            .values()
            .into_iter()
            .find(|u| u.sip_uri == sip_uri)
            .ok_or(AuthError::Unauthorized)?;

        let expected_hash = user.password_hash.as_deref().ok_or(AuthError::Unauthorized)?;
        if sha256_hex(password.as_bytes()) != expected_hash {
            return Err(AuthError::Unauthorized);
        }

        // Create session
        let session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: user.sip_uri.clone(),
            expires_at: Utc::now() + Duration::hours(12),
        };
        self.admin_sessions
            .insert(session.token.clone(), session.clone());
        self.admin_sessions.trim_to_len(MAX_ADMIN_SESSIONS);

        // Get SIP credentials
        let sip_creds = split_sip_aor_simple(&user.sip_uri)
            .and_then(|(username, domain)| {
                self.sip_account(&username, &domain).map(|_| SipCredentials {
                    sip_uri: user.sip_uri.clone(),
                    registrar_uri: domain.clone(),
                    username: username.clone(),
                    password: password.to_string(), // Return the plaintext for client SIP registration
                    transport: "udp".to_string(),
                    domain,
                })
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

    pub fn delete_user(&self, id: Uuid) -> Option<User> {
        let user = self.users.remove(&id);
        if user.is_some() {
            self.delete_persisted(User::collection(), id.to_string());
            self.pg_spawn(move |pg| Box::pin(async move { pg.delete_user(id).await }));
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
                dialog.updated_at = Utc::now();
            })
            .or_insert_with(|| SipDialog {
                call_id: input.call_id,
                from_uri: input.from_uri,
                to_uri: input.to_uri,
                target_contact: input.target_contact,
                status: input.status,
                media_types: input.media_types,
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
        self.routing_rules()
            .into_iter()
            .filter(|rule| rule.enabled)
            .find(|rule| {
                pattern_matches(&rule.source_pattern, source)
                    && pattern_matches(&rule.destination_pattern, destination)
            })
            .map(|rule| rule.target)
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

    pub fn ring_group_by_extension(&self, extension: &str) -> Option<RingGroup> {
        self.ring_groups.values().into_iter().find(|g| g.extension == extension && g.enabled)
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

    pub fn ivr_by_extension(&self, extension: &str) -> Option<Ivr> {
        self.ivrs.values().into_iter().find(|i| i.extension == extension && i.enabled)
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
        let room = Room {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description.unwrap_or_default(),
            is_direct: false,
            created_by: creator.to_string(),
            members: std::iter::once(RoomMember {
                user_sip_uri: creator.to_string(),
                role: "admin".to_string(),
                joined_at: Utc::now(),
            })
            .chain(input.members.into_iter().map(|uri| RoomMember {
                user_sip_uri: uri,
                role: "member".to_string(),
                joined_at: Utc::now(),
            }))
            .collect(),
            created_at: Utc::now(),
        };
        self.rooms.insert(room.id, room.clone());
        self.rooms.trim_to_len(MAX_ROOMS);
        room
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
        self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            if !room.members.iter().any(|m| m.user_sip_uri == user_sip_uri) {
                room.members.push(RoomMember {
                    user_sip_uri: user_sip_uri.to_string(),
                    role: "member".to_string(),
                    joined_at: Utc::now(),
                });
            }
            Some(room.clone())
        })
    }

    pub fn remove_room_member(&self, room_id: Uuid, user_sip_uri: &str) -> Option<Room> {
        self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            room.members.retain(|m| m.user_sip_uri != user_sip_uri);
            Some(room.clone())
        })
    }

    pub fn send_room_message(&self, room_id: Uuid, sender_uri: &str, body: &str) -> RoomMessage {
        let msg = RoomMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_uri: sender_uri.to_string(),
            body: body.to_string(),
            content_type: "text/plain".to_string(),
            created_at: Utc::now(),
        };
        let mut messages = self.room_messages.write().expect("room messages lock poisoned");
        messages.push(msg.clone());
        if messages.len() > MAX_ROOM_MESSAGES {
            let overflow = messages.len() - MAX_ROOM_MESSAGES;
            messages.drain(..overflow);
        }
        self.broadcast_sse(SseEvent {
            event_type: "room_message".to_string(),
            payload: serde_json::to_value(&msg).unwrap_or_default(),
        });
        msg
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
    fn pg_spawn<F>(&self, f: F)
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
    pub registrar_uri: String,
    pub username: String,
    pub password: String,
    pub transport: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSession {
    pub token: String,
    pub principal: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMessage {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_uri: String,
    pub body: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendRoomMessageRequest {
    pub body: String,
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

#[derive(Debug, Clone)]
pub struct UpsertSipDialog {
    pub call_id: String,
    pub from_uri: String,
    pub to_uri: String,
    pub target_contact: Option<String>,
    pub status: SipDialogStatus,
    pub media_types: Vec<MediaKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SipDialogStatus {
    Routing,
    Ringing,
    Held,
    Cancelled,
    Ended,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub priority: i32,
    pub enabled: bool,
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

/// Parse "sip:user@domain" into (user, domain)
fn split_sip_aor_simple(aor: &str) -> Option<(String, String)> {
    let aor = aor.strip_prefix("sip:").or_else(|| aor.strip_prefix("sips:"))?;
    let bare = aor.split(';').next()?.split('?').next()?;
    let (username, domain) = bare.split_once('@')?;
    Some((username.to_string(), domain.to_string()))
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
            priority: 200,
            enabled: true,
        });
        let high = state.create_routing_rule(CreateRoutingRuleRequest {
            name: "priority".to_string(),
            source_pattern: "sip:vip@example.com".to_string(),
            destination_pattern: "sip:support@example.com".to_string(),
            target: "sip:vip-desk@example.com".to_string(),
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
