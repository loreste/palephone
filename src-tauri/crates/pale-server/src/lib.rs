use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub mod cli;
pub mod http;
pub mod ldap_auth;
pub mod livekit;
pub mod metrics;
pub mod pg_store;
#[cfg(feature = "native-pjsip")]
pub mod pjsip_runtime;
pub mod sip;
mod storage;
pub mod storage_backend;
pub mod voicemail_media;
pub mod web_push;

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
        let mut shard = self.shard(key).write().expect("sharded map lock poisoned");
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
    let stripped = uri
        .strip_prefix("sips:")
        .or_else(|| uri.strip_prefix("sip:"))
        .unwrap_or(uri);
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
    /// Path to a CA certificate for verifying client TLS certificates on SIP.
    pub ca_cert_path: Option<PathBuf>,
    /// When true, require and verify client certificates on SIP TLS connections.
    pub verify_client_certs: bool,
    /// Optional LiveKit SFU configuration for multi-party media.
    pub livekit: Option<livekit::LiveKitConfig>,
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
    /// Live SIP connections keyed by peer address (e.g. "1.2.3.4:5678"). Each
    /// value writes raw SIP messages onto that client's open TCP/TLS flow, so
    /// the server can forward an INVITE to a NAT'd callee over the connection
    /// it already holds instead of redirecting to an unreachable private
    /// contact. Populated by the stream handler for the lifetime of the socket.
    sip_connections: ShardedMap<String, tokio::sync::mpsc::UnboundedSender<String>>,
    /// Active proxied calls keyed by Call-ID, tracking the two flows so
    /// responses and in-dialog requests can be relayed between caller and callee.
    proxy_calls: ShardedMap<String, ProxyCall>,
    /// In-flight proxied INVITEs keyed by the received top-Via branch, so a
    /// CANCEL can be matched to its INVITE transaction and forwarded upstream.
    pending_invites: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
    sip_messages: RwLock<Vec<SipMessage>>,
    sip_transactions: RwLock<Vec<SipTransaction>>,
    sip_subscriptions: ShardedMap<String, SipSubscription>,
    sip_notifications: RwLock<Vec<SipNotification>>,
    conferences: ShardedMap<Uuid, Conference>,
    conference_attendance: RwLock<Vec<ConferenceAttendanceRecord>>,
    calls: ShardedMap<Uuid, CallSession>,
    files: ShardedMap<Uuid, FileRecord>,
    malware_quarantine: ShardedMap<Uuid, MalwareQuarantineItem>,
    routing_rules: ShardedMap<Uuid, RoutingRule>,
    audit_events: RwLock<Vec<AdminAuditEvent>>,
    presence: ShardedMap<String, UserPresence>,
    call_history: ShardedMap<Uuid, CallHistoryEntry>,
    teams: ShardedMap<Uuid, Team>,
    scheduled_meetings: ShardedMap<Uuid, ScheduledMeeting>,
    retention_policies: ShardedMap<Uuid, RetentionPolicy>,
    ediscovery_cases: ShardedMap<Uuid, EDiscoveryCase>,
    collaboration_policy: RwLock<CollaborationPolicy>,
    channel_webhooks: ShardedMap<Uuid, ChannelWebhook>,
    mention_rate_limits: ShardedMap<String, RateLimitBucket>,
    rooms: ShardedMap<Uuid, Room>,
    room_messages: RwLock<Vec<RoomMessage>>,
    message_reads: RwLock<Vec<MessageRead>>,
    voicemails: ShardedMap<Uuid, Voicemail>,
    recordings: ShardedMap<Uuid, CallRecording>,
    transcription_jobs: ShardedMap<Uuid, TranscriptionJob>,
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
    tags: ShardedMap<Uuid, Tag>,
    notification_preferences: ShardedMap<String, NotificationPreference>,
    user_favorites: ShardedMap<String, Vec<String>>,
    // Meeting lobby state keyed by conference_id
    conference_lobbies: ShardedMap<Uuid, ConferenceLobby>,
    // Raised hands keyed by conference_id
    raised_hands: ShardedMap<Uuid, Vec<HandRaise>>,
    // Polls keyed by poll_id
    meeting_polls: ShardedMap<Uuid, MeetingPoll>,
    // Q&A keyed by question_id
    qa_questions: ShardedMap<Uuid, QaQuestion>,
    // Breakout sessions keyed by session_id
    breakout_sessions: ShardedMap<Uuid, BreakoutSession>,
    // PowerPoint Live style presentation sessions keyed by session_id
    presentation_sessions: ShardedMap<Uuid, PresentationSession>,
    // Per-user media effects and conference layout state
    meeting_media_settings: ShardedMap<String, MeetingMediaSettings>,
    conference_layouts: ShardedMap<Uuid, ConferenceLayoutState>,
    stream_sessions: ShardedMap<Uuid, MeetingStreamSession>,
    town_hall_configs: ShardedMap<Uuid, TownHallConfig>,
    // Transcript segments
    transcripts: RwLock<Vec<TranscriptSegment>>,
    // Call quality reports
    call_quality_reports: RwLock<Vec<CallQualityReport>>,
    // DLP policies and violations
    dlp_policies: ShardedMap<Uuid, DlpPolicy>,
    dlp_violations: RwLock<Vec<DlpViolation>>,
    // Enterprise governance
    information_barriers: ShardedMap<Uuid, InformationBarrier>,
    sensitivity_labels: ShardedMap<Uuid, SensitivityLabel>,
    custom_roles: ShardedMap<Uuid, CustomRole>,
    policy_packages: ShardedMap<Uuid, PolicyPackage>,
    // Meeting templates
    meeting_templates: ShardedMap<Uuid, MeetingTemplate>,
    // Green room state keyed by conference_id
    green_rooms: ShardedMap<Uuid, GreenRoomState>,
    // File versioning & folders
    file_versions: RwLock<Vec<FileVersion>>,
    folders: ShardedMap<Uuid, Folder>,
    // Approvals
    approval_requests: ShardedMap<Uuid, ApprovalRequest>,
    // Recording policies & hold music
    recording_policies: ShardedMap<Uuid, RecordingPolicy>,
    hold_music: ShardedMap<Uuid, HoldMusic>,
    // Personal call groups
    personal_call_groups: ShardedMap<Uuid, PersonalCallGroup>,
    // SSO providers
    sso_providers: ShardedMap<Uuid, SsoProvider>,
    /// Pending SSO login states: state_value -> (provider_id, nonce, created_at)
    sso_pending_states: ShardedMap<String, SsoPendingState>,
    /// Cached OIDC discovery documents: issuer_url -> OidcDiscovery
    oidc_discovery_cache: ShardedMap<String, OidcDiscovery>,
    // Encryption config (BYOK)
    encryption_configs: RwLock<Vec<EncryptionConfig>>,
    // Admin elevations (PAM)
    admin_elevations: RwLock<Vec<AdminElevation>>,
    // Line delegations (boss-secretary)
    line_delegations: ShardedMap<Uuid, LineDelegation>,
    // Common area phones
    common_area_phones: ShardedMap<Uuid, CommonAreaPhone>,
    // Meeting rooms & bookings
    meeting_rooms: ShardedMap<Uuid, MeetingRoom>,
    room_bookings: ShardedMap<Uuid, RoomBooking>,
    // Provisioned devices
    provisioned_devices: ShardedMap<Uuid, ProvisionedDevice>,
    // Hot desking
    hotdesk_sessions: ShardedMap<Uuid, HotdeskSession>,
    custom_emojis: ShardedMap<Uuid, CustomEmoji>,
    wiki_pages: ShardedMap<Uuid, WikiPage>,
    task_boards: ShardedMap<Uuid, TaskBoard>,
    tasks: ShardedMap<Uuid, Task>,
    // Platform & integration
    api_clients: ShardedMap<Uuid, ApiClient>,
    api_tokens: ShardedMap<Uuid, ApiToken>,
    bots: ShardedMap<Uuid, Bot>,
    calendar_integrations: ShardedMap<Uuid, CalendarIntegration>,
    contact_sync_configs: ShardedMap<Uuid, ContactSyncConfig>,
    synced_contacts: ShardedMap<Uuid, SyncedContact>,
    connectors: ShardedMap<Uuid, Connector>,
    // Conditional access policies
    conditional_access_policies: ShardedMap<Uuid, ConditionalAccessPolicy>,
    // Webinar registrations
    webinar_registrations: ShardedMap<Uuid, WebinarRegistration>,
    // Guest users
    guest_users: ShardedMap<Uuid, GuestUser>,
    // CNAM cache
    cnam_cache: ShardedMap<String, CnamEntry>,
    cnam_providers: RwLock<Vec<CnamProviderConfig>>,
    // SIP gateways
    sip_gateways: ShardedMap<Uuid, SipGateway>,
    // Location routing rules
    location_routing_rules: ShardedMap<Uuid, LocationRoutingRule>,
    emergency_locations: ShardedMap<Uuid, EmergencyLocation>,
    emergency_assignments: ShardedMap<String, EmergencyCallingAssignment>,
    // Annotations (in-memory per conference)
    conference_annotations: ShardedMap<Uuid, Vec<Annotation>>,
    // Whiteboards
    whiteboards: ShardedMap<Uuid, Whiteboard>,
    // Scheduling panels
    scheduling_panels: ShardedMap<Uuid, SchedulingPanel>,
    // Automation rules
    automation_rules: ShardedMap<Uuid, AutomationRule>,
    // Federation
    federation_peers: ShardedMap<Uuid, FederationPeer>,
    federated_messages: RwLock<Vec<FederatedMessage>>,
    // Loop components
    loop_components: ShardedMap<Uuid, LoopComponent>,
    // Compliance
    compliance_reviews: ShardedMap<Uuid, ComplianceReview>,
    // Data residency
    data_residency_configs: ShardedMap<Uuid, DataResidencyConfig>,
    // Channel tabs
    channel_tabs: ShardedMap<Uuid, ChannelTab>,
    // Message extensions
    message_extensions: ShardedMap<Uuid, MessageExtension>,
    // App catalog
    app_catalog: ShardedMap<Uuid, AppCatalogEntry>,
    // Bandwidth policies
    bandwidth_policies: ShardedMap<Uuid, BandwidthPolicy>,
    // Signage displays
    signage_displays: ShardedMap<Uuid, SignageDisplay>,
    // External enterprise capability providers
    enterprise_integrations: ShardedMap<String, EnterpriseIntegration>,
    user_create_lock: std::sync::Mutex<()>,
    agent_assignment_lock: std::sync::Mutex<()>,
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    nats_url: Option<String>,
    rate_limits: ShardedMap<String, RateLimitBucket>,
    rate_limit_rps: u32,
    rate_limit_config: std::sync::RwLock<RateLimitConfig>,
    endpoint_rate_limits: ShardedMap<String, RateLimitBucket>,
    message_threads: ShardedMap<Uuid, MessageThread>,
    /// Address advertised to clients as their SIP registrar. `None` when the
    /// active SIP backend cannot register clients (e.g. the pjsip backend),
    /// in which case login/provisioning responses must not advertise one.
    sip_registrar: Option<String>,
    /// Transport paired with `sip_registrar` in login/provisioning responses.
    sip_registrar_transport: String,
    ldap_config: std::sync::RwLock<ldap_auth::LdapConfig>,
    pg: Option<PgStore>,
    pg_failure_count: Arc<std::sync::atomic::AtomicU64>,
    /// LiveKit SFU configuration (when set, conferences use LiveKit for media).
    livekit: Option<livekit::LiveKitConfig>,
    /// Pluggable file storage backend (local disk or S3).
    storage_client: Option<storage_backend::StorageClient>,
    /// VAPID configuration for Web Push notifications.
    vapid_config: Option<web_push::VapidConfig>,
    /// Web Push subscriptions keyed by subscription ID.
    push_subscriptions: ShardedMap<Uuid, web_push::PushSubscription>,
    /// HTTP client for sending push notifications.
    push_http_client: reqwest::Client,
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
            sip_connections: ShardedMap::new(),
            proxy_calls: ShardedMap::new(),
            pending_invites: std::sync::Mutex::new(HashMap::new()),
            sip_messages: RwLock::new(Vec::new()),
            sip_transactions: RwLock::new(Vec::new()),
            sip_subscriptions: ShardedMap::new(),
            sip_notifications: RwLock::new(Vec::new()),
            conferences: ShardedMap::new(),
            conference_attendance: RwLock::new(Vec::new()),
            calls: ShardedMap::new(),
            files: ShardedMap::new(),
            malware_quarantine: ShardedMap::new(),
            routing_rules: ShardedMap::new(),
            audit_events: RwLock::new(Vec::new()),
            presence: ShardedMap::new(),
            call_history: ShardedMap::new(),
            teams: ShardedMap::new(),
            scheduled_meetings: ShardedMap::new(),
            retention_policies: ShardedMap::new(),
            ediscovery_cases: ShardedMap::new(),
            collaboration_policy: RwLock::new(CollaborationPolicy::default()),
            channel_webhooks: ShardedMap::new(),
            mention_rate_limits: ShardedMap::new(),
            rooms: ShardedMap::new(),
            room_messages: RwLock::new(Vec::new()),
            message_reads: RwLock::new(Vec::new()),
            voicemails: ShardedMap::new(),
            recordings: ShardedMap::new(),
            transcription_jobs: ShardedMap::new(),
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
            tags: ShardedMap::new(),
            notification_preferences: ShardedMap::new(),
            user_favorites: ShardedMap::new(),
            conference_lobbies: ShardedMap::new(),
            raised_hands: ShardedMap::new(),
            meeting_polls: ShardedMap::new(),
            qa_questions: ShardedMap::new(),
            breakout_sessions: ShardedMap::new(),
            presentation_sessions: ShardedMap::new(),
            meeting_media_settings: ShardedMap::new(),
            conference_layouts: ShardedMap::new(),
            stream_sessions: ShardedMap::new(),
            town_hall_configs: ShardedMap::new(),
            transcripts: RwLock::new(Vec::new()),
            call_quality_reports: RwLock::new(Vec::new()),
            dlp_policies: ShardedMap::new(),
            dlp_violations: RwLock::new(Vec::new()),
            information_barriers: ShardedMap::new(),
            sensitivity_labels: ShardedMap::new(),
            custom_roles: ShardedMap::new(),
            policy_packages: ShardedMap::new(),
            meeting_templates: ShardedMap::new(),
            green_rooms: ShardedMap::new(),
            file_versions: RwLock::new(Vec::new()),
            folders: ShardedMap::new(),
            approval_requests: ShardedMap::new(),
            recording_policies: ShardedMap::new(),
            hold_music: ShardedMap::new(),
            personal_call_groups: ShardedMap::new(),
            sso_providers: ShardedMap::new(),
            sso_pending_states: ShardedMap::new(),
            oidc_discovery_cache: ShardedMap::new(),
            encryption_configs: RwLock::new(Vec::new()),
            admin_elevations: RwLock::new(Vec::new()),
            line_delegations: ShardedMap::new(),
            common_area_phones: ShardedMap::new(),
            meeting_rooms: ShardedMap::new(),
            room_bookings: ShardedMap::new(),
            provisioned_devices: ShardedMap::new(),
            hotdesk_sessions: ShardedMap::new(),
            custom_emojis: ShardedMap::new(),
            wiki_pages: ShardedMap::new(),
            task_boards: ShardedMap::new(),
            tasks: ShardedMap::new(),
            api_clients: ShardedMap::new(),
            api_tokens: ShardedMap::new(),
            bots: ShardedMap::new(),
            calendar_integrations: ShardedMap::new(),
            contact_sync_configs: ShardedMap::new(),
            synced_contacts: ShardedMap::new(),
            connectors: ShardedMap::new(),
            conditional_access_policies: ShardedMap::new(),
            webinar_registrations: ShardedMap::new(),
            guest_users: ShardedMap::new(),
            cnam_cache: ShardedMap::new(),
            cnam_providers: RwLock::new(Vec::new()),
            sip_gateways: ShardedMap::new(),
            location_routing_rules: ShardedMap::new(),
            emergency_locations: ShardedMap::new(),
            emergency_assignments: ShardedMap::new(),
            conference_annotations: ShardedMap::new(),
            whiteboards: ShardedMap::new(),
            scheduling_panels: ShardedMap::new(),
            automation_rules: ShardedMap::new(),
            federation_peers: ShardedMap::new(),
            federated_messages: RwLock::new(Vec::new()),
            loop_components: ShardedMap::new(),
            compliance_reviews: ShardedMap::new(),
            data_residency_configs: ShardedMap::new(),
            channel_tabs: ShardedMap::new(),
            message_extensions: ShardedMap::new(),
            app_catalog: ShardedMap::new(),
            bandwidth_policies: ShardedMap::new(),
            signage_displays: ShardedMap::new(),
            enterprise_integrations: ShardedMap::new(),
            user_create_lock: std::sync::Mutex::new(()),
            agent_assignment_lock: std::sync::Mutex::new(()),
            sse_tx: tokio::sync::broadcast::channel(256).0,
            nats_url: std::env::var("NATS_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            rate_limits: ShardedMap::new(),
            rate_limit_rps: 100,
            rate_limit_config: std::sync::RwLock::new(RateLimitConfig::default()),
            endpoint_rate_limits: ShardedMap::new(),
            message_threads: ShardedMap::new(),
            sip_registrar: None,
            sip_registrar_transport: "tls".to_string(),
            ldap_config: std::sync::RwLock::new(ldap_auth::LdapConfig::default()),
            pg: None,
            pg_failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            livekit: None,
            storage_client: None,
            vapid_config: None,
            push_subscriptions: ShardedMap::new(),
            push_http_client: reqwest::Client::new(),
        }
    }

    pub fn ldap_config(&self) -> ldap_auth::LdapConfig {
        self.ldap_config.read().expect("ldap config lock").clone()
    }

    pub fn set_ldap_config(&self, config: ldap_auth::LdapConfig) {
        *self.ldap_config.write().expect("ldap config lock") = config;
    }

    /// Advertise `addr` as the SIP registrar in login/provisioning responses.
    /// Only call this when the active SIP backend actually implements REGISTER.
    pub fn set_sip_registrar(&mut self, addr: String, transport: impl Into<String>) {
        self.sip_registrar = Some(addr);
        self.sip_registrar_transport = transport.into();
    }

    /// Whether the active SIP backend can register clients.
    pub fn sip_registration_available(&self) -> bool {
        self.sip_registrar.is_some()
    }

    pub fn sip_registrar_uri(&self) -> Option<String> {
        self.sip_registrar
            .as_ref()
            .map(|registrar| format!("sip:{}", registrar))
    }

    pub fn sip_registrar_transport(&self) -> String {
        self.sip_registrar_transport.clone()
    }

    /// Set the LiveKit configuration for media-backed conferences.
    pub fn set_livekit(&mut self, config: livekit::LiveKitConfig) {
        self.livekit = Some(config);
    }

    /// Returns the LiveKit configuration, if present.
    pub fn livekit_config(&self) -> Option<&livekit::LiveKitConfig> {
        self.livekit.as_ref()
    }

    // ─── Storage Backend ───

    /// Set the storage backend (local or S3).
    pub fn set_storage_client(&mut self, client: storage_backend::StorageClient) {
        self.storage_client = Some(client);
    }

    /// Get the storage client, falling back to a local client if none is set.
    pub fn storage_client(&self) -> storage_backend::StorageClient {
        self.storage_client
            .clone()
            .unwrap_or_else(|| storage_backend::StorageClient::local(self.files_dir()))
    }

    // ─── Web Push ───

    /// Set the VAPID configuration for Web Push.
    pub fn set_vapid_config(&mut self, config: web_push::VapidConfig) {
        self.vapid_config = Some(config);
    }

    /// Get the VAPID public key (for client subscription).
    pub fn vapid_public_key(&self) -> Option<String> {
        self.vapid_config.as_ref().map(|c| c.public_key.clone())
    }

    /// Register a push subscription for the given user.
    pub fn add_push_subscription(
        &self,
        user_uri: &str,
        request: &web_push::PushSubscribeRequest,
    ) -> web_push::PushSubscription {
        // Remove any existing subscription with the same endpoint
        let existing: Vec<Uuid> = self
            .push_subscriptions
            .values()
            .iter()
            .filter(|s| s.endpoint == request.endpoint)
            .map(|s| s.id)
            .collect();
        for id in existing {
            self.push_subscriptions.remove(&id);
        }

        let sub = web_push::PushSubscription {
            id: Uuid::new_v4(),
            user_uri: user_uri.to_string(),
            endpoint: request.endpoint.clone(),
            p256dh: request.p256dh.clone(),
            auth: request.auth.clone(),
            created_at: Utc::now(),
        };
        self.push_subscriptions.insert(sub.id, sub.clone());
        self.pg_spawn({
            let sub = sub.clone();
            move |pg| Box::pin(async move { pg.insert_push_subscription(&sub).await })
        });
        sub
    }

    /// Remove a push subscription by endpoint.
    pub fn remove_push_subscription(&self, user_uri: &str, endpoint: &str) -> bool {
        let to_remove: Vec<Uuid> = self
            .push_subscriptions
            .values()
            .iter()
            .filter(|s| s.user_uri == user_uri && s.endpoint == endpoint)
            .map(|s| s.id)
            .collect();
        let removed = !to_remove.is_empty();
        for id in to_remove {
            self.push_subscriptions.remove(&id);
            self.pg_spawn(move |pg| Box::pin(async move { pg.delete_push_subscription(id).await }));
        }
        removed
    }

    /// Remove a push subscription by ID (used when receiving a 410 Gone).
    pub fn remove_push_subscription_by_id(&self, id: Uuid) {
        self.push_subscriptions.remove(&id);
        self.pg_spawn(move |pg| Box::pin(async move { pg.delete_push_subscription(id).await }));
    }

    /// Get all push subscriptions for a user.
    pub fn push_subscriptions_for_user(&self, user_uri: &str) -> Vec<web_push::PushSubscription> {
        self.push_subscriptions
            .values()
            .into_iter()
            .filter(|s| s.user_uri == user_uri)
            .collect()
    }

    /// Send a push notification to all of a user's subscriptions.
    pub fn notify_user_push(&self, user_uri: &str, title: &str, body: &str, tag: Option<String>) {
        let Some(config) = self.vapid_config.clone() else {
            return;
        };
        let subscriptions = self.push_subscriptions_for_user(user_uri);
        if subscriptions.is_empty() {
            return;
        }
        let http_client = self.push_http_client.clone();
        let payload = web_push::PushPayload {
            title: title.to_string(),
            body: body.to_string(),
            tag,
            url: None,
        };
        for sub in subscriptions {
            let client = http_client.clone();
            let cfg = config.clone();
            let p = payload.clone();
            let sub_id = sub.id;
            // We can't borrow self in spawn, so collect IDs to remove
            tokio::spawn(async move {
                match web_push::send_push_notification(&client, &cfg, &sub, &p).await {
                    Ok(()) => {}
                    Err(web_push::PushError::Gone) => {
                        log::info!("push subscription {} is gone, will be cleaned up", sub_id);
                    }
                    Err(e) => {
                        log::warn!("push notification failed for {}: {}", sub.endpoint, e);
                    }
                }
            });
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
            let bucket = buckets
                .entry(key.clone())
                .or_insert_with(|| RateLimitBucket {
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
                *self
                    .room_messages
                    .write()
                    .expect("room messages lock poisoned") = messages;
            }
            Err(e) => log::warn!("Failed to load room messages from Postgres: {}", e),
        }
        match pg.load_message_reads().await {
            Ok(reads) => {
                *self
                    .message_reads
                    .write()
                    .expect("message reads lock poisoned") = reads;
            }
            Err(e) => log::warn!("Failed to load message reads from Postgres: {}", e),
        }
        match pg.load_message_reactions().await {
            Ok(records) => {
                for record in records {
                    self.message_reactions
                        .with_write(&record.message_id, |map| {
                            map.entry(record.message_id)
                                .or_insert_with(Vec::new)
                                .push(record.reaction);
                        });
                }
            }
            Err(e) => log::warn!("Failed to load message reactions from Postgres: {}", e),
        }
        match pg.load_business_objects::<Team>(Team::collection()).await {
            Ok(teams) => {
                for team in teams {
                    self.teams.insert(team.id, team);
                }
            }
            Err(e) => log::warn!("Failed to load teams from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<ScheduledMeeting>(ScheduledMeeting::collection())
            .await
        {
            Ok(meetings) => {
                for meeting in meetings {
                    self.scheduled_meetings.insert(meeting.id, meeting);
                }
            }
            Err(e) => log::warn!("Failed to load scheduled meetings from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<RetentionPolicy>(RetentionPolicy::collection())
            .await
        {
            Ok(policies) => {
                for policy in policies {
                    self.retention_policies.insert(policy.id, policy);
                }
            }
            Err(e) => log::warn!("Failed to load retention policies from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<CollaborationPolicy>(CollaborationPolicy::collection())
            .await
        {
            Ok(mut policies) => {
                if let Some(policy) = policies.pop() {
                    *self
                        .collaboration_policy
                        .write()
                        .expect("collaboration policy lock poisoned") = policy;
                }
            }
            Err(e) => log::warn!("Failed to load collaboration policy from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<InformationBarrier>(InformationBarrier::collection())
            .await
        {
            Ok(barriers) => {
                for barrier in barriers {
                    self.information_barriers.insert(barrier.id, barrier);
                }
            }
            Err(e) => log::warn!("Failed to load information barriers from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<SensitivityLabel>(SensitivityLabel::collection())
            .await
        {
            Ok(labels) => {
                for label in labels {
                    self.sensitivity_labels.insert(label.id, label);
                }
            }
            Err(e) => log::warn!("Failed to load sensitivity labels from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<CustomRole>(CustomRole::collection())
            .await
        {
            Ok(roles) => {
                for role in roles {
                    self.custom_roles.insert(role.id, role);
                }
            }
            Err(e) => log::warn!("Failed to load custom roles from Postgres: {}", e),
        }
        match pg
            .load_business_objects::<PolicyPackage>(PolicyPackage::collection())
            .await
        {
            Ok(packages) => {
                for pkg in packages {
                    self.policy_packages.insert(pkg.id, pkg);
                }
            }
            Err(e) => log::warn!("Failed to load policy packages from Postgres: {}", e),
        }
        match pg.load_line_delegations().await {
            Ok(delegations) => {
                for d in delegations {
                    self.line_delegations.insert(d.id, d);
                }
            }
            Err(e) => log::warn!("Failed to load line delegations from Postgres: {}", e),
        }
        match pg.load_common_area_phones().await {
            Ok(phones) => {
                for p in phones {
                    self.common_area_phones.insert(p.id, p);
                }
            }
            Err(e) => log::warn!("Failed to load common area phones from Postgres: {}", e),
        }
        match pg.load_meeting_rooms().await {
            Ok(rooms) => {
                for r in rooms {
                    self.meeting_rooms.insert(r.id, r);
                }
            }
            Err(e) => log::warn!("Failed to load meeting rooms from Postgres: {}", e),
        }
        match pg.load_room_bookings().await {
            Ok(bookings) => {
                for b in bookings {
                    self.room_bookings.insert(b.id, b);
                }
            }
            Err(e) => log::warn!("Failed to load room bookings from Postgres: {}", e),
        }
        match pg.load_provisioned_devices().await {
            Ok(devices) => {
                for d in devices {
                    self.provisioned_devices.insert(d.id, d);
                }
            }
            Err(e) => log::warn!("Failed to load provisioned devices from Postgres: {}", e),
        }
        match pg.load_hotdesk_sessions().await {
            Ok(sessions) => {
                for s in sessions {
                    self.hotdesk_sessions.insert(s.id, s);
                }
            }
            Err(e) => log::warn!("Failed to load hotdesk sessions from Postgres: {}", e),
        }
        match pg.load_push_subscriptions().await {
            Ok(subs) => {
                let count = subs.len();
                for sub in subs {
                    self.push_subscriptions.insert(sub.id, sub);
                }
                if count > 0 {
                    log::info!("Loaded {} push subscriptions from PostgreSQL", count);
                }
            }
            Err(e) => log::warn!("Failed to load push subscriptions from Postgres: {}", e),
        }
        log::info!("Loaded data from PostgreSQL into memory cache");
    }

    pub fn http_token(&self) -> &str {
        &self.http_token
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_bytes
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn media_config(&self) -> MediaConfig {
        self.media.clone()
    }

    pub fn meeting_media_settings(&self, user_uri: &str) -> MeetingMediaSettings {
        let normalized =
            normalize_emergency_user_uri(user_uri).unwrap_or_else(|| user_uri.to_string());
        let mut settings = self
            .meeting_media_settings
            .get(&normalized)
            .unwrap_or_else(|| MeetingMediaSettings {
                user_uri: normalized.clone(),
                echo_cancellation: true,
                noise_suppression: true,
                auto_gain: false,
                background_mode: "none".to_string(),
                background_image_url: None,
                noise_suppression_configured: false,
                virtual_backgrounds_configured: false,
                updated_at: Utc::now(),
            });
        settings.noise_suppression_configured =
            self.enterprise_capability_available("noise_suppression");
        settings.virtual_backgrounds_configured =
            self.enterprise_capability_available("virtual_backgrounds");
        settings
    }

    pub fn update_meeting_media_settings(
        &self,
        user_uri: &str,
        input: UpdateMeetingMediaSettingsRequest,
    ) -> MeetingMediaSettings {
        let normalized =
            normalize_emergency_user_uri(user_uri).unwrap_or_else(|| user_uri.to_string());
        let mut settings = self.meeting_media_settings(&normalized);
        if let Some(value) = input.echo_cancellation {
            settings.echo_cancellation = value;
        }
        if let Some(value) = input.noise_suppression {
            settings.noise_suppression = value;
        }
        if let Some(value) = input.auto_gain {
            settings.auto_gain = value;
        }
        if let Some(mode) = input.background_mode {
            settings.background_mode = normalize_media_background_mode(&mode);
        }
        if let Some(url) = input.background_image_url {
            settings.background_image_url = non_empty_string(url);
        }
        if settings.background_mode != "image" {
            settings.background_image_url = None;
        }
        settings.updated_at = Utc::now();
        self.meeting_media_settings
            .insert(normalized, settings.clone());
        settings
    }

    pub fn conference_layout_state(&self, conference_id: Uuid) -> Option<ConferenceLayoutState> {
        let conference = self.conferences.get(&conference_id)?;
        let mut layout = self
            .conference_layouts
            .get(&conference_id)
            .unwrap_or_else(|| ConferenceLayoutState {
                conference_id,
                mode: "gallery".to_string(),
                max_visible: 9,
                together_scene: None,
                stage_participant_ids: Vec::new(),
                sfu_layout_configured: false,
                gallery_capacity: 9,
                together_scene_supported: true,
                layout_blockers: Vec::new(),
                updated_by: None,
                updated_at: Utc::now(),
            });
        layout.sfu_layout_configured = self.enterprise_capability_available("together_gallery");
        apply_layout_readiness(&mut layout, &conference.participants);
        Some(layout)
    }

    pub fn update_conference_layout_state(
        &self,
        conference_id: Uuid,
        updated_by: &str,
        input: UpdateConferenceLayoutRequest,
    ) -> Option<ConferenceLayoutState> {
        let conference = self.conferences.get(&conference_id)?;
        let mut layout = self.conference_layout_state(conference_id)?;
        if let Some(mode) = input.mode {
            layout.mode = normalize_conference_layout_mode(&mode);
        }
        if let Some(max_visible) = input.max_visible {
            layout.max_visible = max_visible.clamp(1, 49);
        }
        if let Some(scene) = input.together_scene {
            layout.together_scene = non_empty_string(scene);
        }
        if let Some(stage_ids) = input.stage_participant_ids {
            layout.stage_participant_ids = stage_ids
                .into_iter()
                .filter(|id| {
                    conference
                        .participants
                        .iter()
                        .any(|participant| participant.user_id == *id && !participant.removed)
                })
                .take(12)
                .collect();
        }
        if layout.mode != "together" {
            layout.together_scene = None;
        }
        layout.sfu_layout_configured = self.enterprise_capability_available("together_gallery");
        apply_layout_readiness(&mut layout, &conference.participants);
        layout.updated_by = Some(updated_by.to_string());
        layout.updated_at = Utc::now();
        self.conference_layouts
            .insert(conference_id, layout.clone());
        Some(layout)
    }

    pub fn list_stream_sessions(&self, conference_id: Uuid) -> Vec<MeetingStreamSession> {
        let mut sessions: Vec<_> = self
            .stream_sessions
            .values()
            .into_iter()
            .filter(|session| session.conference_id == conference_id)
            .map(|mut session| {
                session.gateway_configured =
                    self.enterprise_capability_available("ndi_rtmp_streaming");
                session.destination = redact_stream_destination(&session.destination);
                session
            })
            .collect();
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        sessions
    }

    pub fn start_stream_session(
        &self,
        conference_id: Uuid,
        started_by: &str,
        input: CreateMeetingStreamRequest,
    ) -> Result<MeetingStreamSession, String> {
        self.conferences
            .get(&conference_id)
            .ok_or_else(|| "conference_not_found".to_string())?;
        let gateway_configured = self.enterprise_capability_available("ndi_rtmp_streaming");
        if !gateway_configured {
            return Err("streaming_gateway_not_configured".to_string());
        }
        let destination = input.destination.trim();
        if !valid_stream_destination(&input.target_kind, destination) {
            return Err("invalid_stream_destination".to_string());
        }
        let now = Utc::now();
        let session = MeetingStreamSession {
            id: Uuid::new_v4(),
            conference_id,
            target_kind: input.target_kind,
            name: non_empty_string(input.name).unwrap_or_else(|| "Meeting stream".to_string()),
            destination: destination.to_string(),
            status: StreamStatus::Live,
            started_by: started_by.to_string(),
            started_at: now,
            stopped_at: None,
            health: "egress_requested".to_string(),
            gateway_configured,
        };
        self.stream_sessions.insert(session.id, session.clone());
        let mut redacted = session;
        redacted.destination = redact_stream_destination(&redacted.destination);
        Ok(redacted)
    }

    pub fn stop_stream_session(&self, id: Uuid) -> Option<MeetingStreamSession> {
        let mut session = self.stream_sessions.get(&id)?;
        if session.status == StreamStatus::Live {
            session.status = StreamStatus::Stopped;
            session.stopped_at = Some(Utc::now());
            session.health = "stopped".to_string();
            self.stream_sessions.insert(id, session.clone());
        }
        session.gateway_configured = self.enterprise_capability_available("ndi_rtmp_streaming");
        session.destination = redact_stream_destination(&session.destination);
        Some(session)
    }

    pub fn stream_session(&self, id: Uuid) -> Option<MeetingStreamSession> {
        self.stream_sessions.get(&id).map(|mut session| {
            session.gateway_configured = self.enterprise_capability_available("ndi_rtmp_streaming");
            session.destination = redact_stream_destination(&session.destination);
            session
        })
    }

    pub fn town_hall_config(&self, conference_id: Uuid) -> Option<TownHallConfig> {
        self.conferences.get(&conference_id)?;
        let mut config = self
            .town_hall_configs
            .get(&conference_id)
            .unwrap_or_else(|| TownHallConfig {
                conference_id,
                enabled: false,
                max_viewers: 10000,
                registration_required: true,
                presenter_only_video: true,
                attendee_mic_disabled: true,
                qna_moderation_required: true,
                overflow_url: None,
                broadcast_provider_configured: false,
                broadcast_capacity: 1000,
                attendee_mode: "interactive".to_string(),
                broadcast_ready: false,
                broadcast_blockers: Vec::new(),
                updated_by: None,
                updated_at: Utc::now(),
            });
        config.broadcast_provider_configured =
            self.enterprise_capability_available("town_hall_broadcast");
        apply_town_hall_readiness(&mut config);
        Some(config)
    }

    pub fn update_town_hall_config(
        &self,
        conference_id: Uuid,
        updated_by: &str,
        input: UpdateTownHallConfigRequest,
    ) -> Result<TownHallConfig, String> {
        let conference = self
            .conferences
            .get(&conference_id)
            .ok_or_else(|| "conference_not_found".to_string())?;
        if conference.mode != ConferenceMode::Webinar {
            return Err("town_hall_requires_webinar".to_string());
        }
        let mut config = self
            .town_hall_config(conference_id)
            .ok_or_else(|| "conference_not_found".to_string())?;
        if let Some(enabled) = input.enabled {
            config.enabled = enabled;
        }
        if let Some(max_viewers) = input.max_viewers {
            config.max_viewers = max_viewers.clamp(1, 100000);
        }
        if let Some(required) = input.registration_required {
            config.registration_required = required;
        }
        if let Some(value) = input.presenter_only_video {
            config.presenter_only_video = value;
        }
        if let Some(value) = input.attendee_mic_disabled {
            config.attendee_mic_disabled = value;
        }
        if let Some(value) = input.qna_moderation_required {
            config.qna_moderation_required = value;
        }
        if let Some(url) = input.overflow_url {
            config.overflow_url = non_empty_string(url);
        }
        config.broadcast_provider_configured =
            self.enterprise_capability_available("town_hall_broadcast");
        apply_town_hall_readiness(&mut config);
        config.updated_by = Some(updated_by.to_string());
        config.updated_at = Utc::now();
        self.town_hall_configs.insert(conference_id, config.clone());
        Ok(config)
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

        if username != self.admin_username || !verify_password(password, &self.admin_password_hash)
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
        let session = self.admin_sessions.get(&bearer.to_string())?;
        if self
            .user_by_sip_uri(&session.principal)
            .is_some_and(|user| !user.active)
        {
            self.admin_sessions.remove(&bearer.to_string());
            return None;
        }
        Some((session.principal, session.role))
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
        let principal = principal.into();
        let action = action.into();
        let created_at = Utc::now();
        let id = Uuid::new_v4();

        // Compute HMAC-like integrity hash: SHA-256(storage_key | id | fields)
        let integrity = {
            let msg = format!(
                "{}|{}|{}|{}|{}",
                id,
                principal,
                action,
                target.as_deref().unwrap_or(""),
                created_at.to_rfc3339(),
            );
            let keyed = format!("{}{}", self.http_token, msg);
            sha256_hex(keyed.as_bytes())
        };

        let event = AdminAuditEvent {
            id,
            principal,
            action,
            target,
            created_at,
            integrity: Some(integrity),
        };
        let mut events = self
            .audit_events
            .write()
            .expect("audit events lock poisoned");
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

    pub fn search_audit_events(&self, query: AdminAuditQuery) -> Vec<AdminAuditEvent> {
        let principal = query
            .principal
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let action = query
            .action
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let target = query
            .target
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let mut events: Vec<_> = self
            .audit_events
            .read()
            .expect("audit events lock poisoned")
            .iter()
            .filter(|event| {
                if let Some(from) = query.from {
                    if event.created_at < from {
                        return false;
                    }
                }
                if let Some(to) = query.to {
                    if event.created_at > to {
                        return false;
                    }
                }
                if let Some(principal) = &principal {
                    if !event.principal.to_ascii_lowercase().contains(principal) {
                        return false;
                    }
                }
                if let Some(action) = &action {
                    if !event.action.to_ascii_lowercase().contains(action) {
                        return false;
                    }
                }
                if let Some(target) = &target {
                    if !event
                        .target
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(target)
                    {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        events.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let limit = query.limit.unwrap_or(500).clamp(1, 5000);
        events.truncate(limit);
        events
    }

    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("files")
    }

    pub fn file_path(&self, file_id: Uuid) -> PathBuf {
        self.files_dir().join(file_id.to_string())
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.data_dir.join("quarantine")
    }

    pub fn quarantine_path(&self, id: Uuid) -> PathBuf {
        self.quarantine_dir().join(id.to_string())
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
        self.load_vec_collection::<ConferenceAttendanceRecord>(&self.conference_attendance);
        self.load_collection::<CallSession>(&self.calls);
        self.load_collection::<FileRecord>(&self.files);
        self.load_collection::<MalwareQuarantineItem>(&self.malware_quarantine);
        self.load_collection::<RoutingRule>(&self.routing_rules);
        self.load_collection::<Team>(&self.teams);
        self.load_collection::<ScheduledMeeting>(&self.scheduled_meetings);
        self.load_collection::<RetentionPolicy>(&self.retention_policies);
        self.load_collection::<EDiscoveryCase>(&self.ediscovery_cases);
        self.load_singleton::<CollaborationPolicy>(&self.collaboration_policy);
        self.load_collection::<ChannelWebhook>(&self.channel_webhooks);
        self.load_collection::<Room>(&self.rooms);
        self.load_vec_collection::<RoomMessage>(&self.room_messages);
        self.load_vec_collection::<MessageRead>(&self.message_reads);
        self.load_message_reactions();
        self.load_vec_collection::<AdminAuditEvent>(&self.audit_events);
        self.load_vec_collection::<CallQualityReport>(&self.call_quality_reports);
        self.load_collection::<DlpPolicy>(&self.dlp_policies);
        self.load_vec_collection::<DlpViolation>(&self.dlp_violations);
        self.load_collection::<EnterpriseIntegration>(&self.enterprise_integrations);
        self.load_collection::<EmergencyLocation>(&self.emergency_locations);
        self.load_collection::<EmergencyCallingAssignment>(&self.emergency_assignments);
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

    fn load_message_reactions(&self) {
        let Some(store) = &self.store else {
            return;
        };
        match store.load::<MessageReactionRecord>(MessageReactionRecord::collection()) {
            Ok(records) => {
                for record in records {
                    self.message_reactions
                        .with_write(&record.message_id, |map| {
                            map.entry(record.message_id)
                                .or_insert_with(Vec::new)
                                .push(record.reaction);
                        });
                }
            }
            Err(err) => log::warn!(
                "failed to load {} from storage: {}",
                MessageReactionRecord::collection(),
                err
            ),
        }
    }

    fn load_singleton<T>(&self, value: &RwLock<T>)
    where
        T: StoredObject + for<'de> Deserialize<'de> + Clone,
    {
        let Some(store) = &self.store else {
            return;
        };
        match store.load::<T>(T::collection()) {
            Ok(values) => {
                if let Some(stored) = values.into_iter().next() {
                    *value.write().expect("persisted singleton lock poisoned") = stored;
                }
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

    pub fn user_by_sip_uri(&self, sip_uri: &str) -> Option<User> {
        let normalized = normalize_sip_uri(sip_uri)?;
        self.users
            .values()
            .into_iter()
            .find(|user| normalize_sip_uri(&user.sip_uri).as_deref() == Some(normalized.as_str()))
    }

    pub fn create_user(&self, input: CreateUserRequest) -> Result<User, String> {
        let normalized_sip_uri = normalize_sip_uri(&input.sip_uri)
            .ok_or_else(|| format!("Invalid SIP URI {}", input.sip_uri))?;

        let _create_guard = self
            .user_create_lock
            .lock()
            .map_err(|_| "user creation lock poisoned".to_string())?;

        if self
            .users
            .values()
            .iter()
            .any(|u| normalize_sip_uri(&u.sip_uri).as_deref() == Some(normalized_sip_uri.as_str()))
        {
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
            active: true,
            deactivated_at: None,
            deactivated_by: None,
            email: None,
            title: None,
            department: None,
            phone_number: None,
            status_message: None,
            out_of_office_message: None,
            out_of_office_until: None,
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
        client_ip: &str,
        device_type: &str,
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
                            role: Some(
                                if ldap_user.is_admin { "admin" } else { "user" }.to_string(),
                            ),
                        });
                        log::info!(
                            "Auto-provisioned AD user: {} (admin={})",
                            ldap_user.sip_uri,
                            ldap_user.is_admin
                        );
                    }
                    // Update role from AD group membership
                    let normalized_ldap_uri = normalize_sip_uri(&ldap_user.sip_uri);
                    if let Some(existing) = self
                        .users
                        .values()
                        .into_iter()
                        .find(|u| normalize_sip_uri(&u.sip_uri) == normalized_ldap_uri)
                    {
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
                    || split_sip_aor_simple(&u.sip_uri).map(|(u, _)| u).as_deref()
                        == Some(&username)
            })
            .ok_or(AuthError::Unauthorized)?;

        if !user.active {
            return Err(AuthError::Unauthorized);
        }

        // Verify password locally unless LDAP itself verified it. LDAP being
        // merely *enabled* is not enough — an unreachable or failing
        // directory must not bypass password verification.
        if !ldap_authenticated {
            let expected_hash = user
                .password_hash
                .as_deref()
                .ok_or(AuthError::Unauthorized)?;
            if !verify_password(password, expected_hash) {
                return Err(AuthError::Unauthorized);
            }
        }

        // Evaluate conditional access policies before creating session
        let ca_result = self.evaluate_conditional_access(client_ip, device_type, &[]);
        if ca_result.block {
            return Err(AuthError::Unauthorized);
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
        let sip_creds = split_sip_aor_simple(&user.sip_uri).map(|(username, domain)| {
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
                transport: self.sip_registrar_transport.clone(),
                domain,
            }
        });

        // Check if MFA is enabled for this user
        let mfa_required = self.is_mfa_enabled(user.id);

        if mfa_required {
            // Return a limited MFA token — the user must validate the TOTP code
            // before getting full access. Mark the session role as "mfa_pending"
            // so it cannot pass authenticated_principal checks.
            let mfa_session = AdminSession {
                token: Uuid::new_v4().to_string(),
                principal: user.sip_uri.clone(),
                role: "mfa_pending".to_string(),
                expires_at: Utc::now() + Duration::minutes(5),
            };
            self.admin_sessions
                .insert(mfa_session.token.clone(), mfa_session.clone());
            return Ok(UserLoginResponse {
                token: mfa_session.token,
                user,
                sip_credentials: sip_creds,
                expires_at: mfa_session.expires_at,
                mfa_required: true,
            });
        }

        // Set presence to online
        self.update_presence(&user.sip_uri, PresenceStatus::Online, None);

        // Track session
        self.track_session(user.id, &session.token, device_type, device_type, client_ip);

        Ok(UserLoginResponse {
            token: session.token,
            user,
            sip_credentials: sip_creds,
            expires_at: session.expires_at,
            mfa_required: false,
        })
    }

    // ─── MFA / TOTP ───

    /// Check if MFA/TOTP is enabled for a user (by SIP URI lookup).
    pub fn is_mfa_enabled(&self, user_id: Uuid) -> bool {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let query = move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()
                    .and_then(|rt| rt.block_on(pg.get_totp_secret(user_id)).ok())
                    .flatten()
                    .map(|(_, enabled, _)| enabled)
                    .unwrap_or(false)
            };
            if tokio::runtime::Handle::try_current().is_ok() {
                return std::thread::spawn(query).join().unwrap_or(false);
            }
            return query();
        }
        false
    }

    /// Generate a TOTP secret for MFA setup. Returns provisioning URI + backup codes.
    pub fn mfa_setup(&self, user_id: Uuid, sip_uri: &str) -> Result<MfaSetupResponse, String> {
        use totp_rs::{Algorithm, Secret, TOTP};

        let secret = Secret::generate_secret();
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret.to_bytes().map_err(|e| e.to_string())?,
            Some("Pale Softphone".to_string()),
            sip_uri.to_string(),
        )
        .map_err(|e| e.to_string())?;

        let provisioning_uri = totp.get_url();
        let secret_base32 = secret.to_encoded().to_string();

        // Generate backup codes
        let mut backup_codes = Vec::new();
        for _ in 0..8 {
            let code = format!("{:08x}", rand::random::<u32>());
            backup_codes.push(code);
        }

        // Store encrypted secret (not yet enabled)
        let encrypted = secret_base32.clone(); // Will be stored encrypted in PG
        let codes_json = serde_json::to_string(&backup_codes).unwrap_or_default();
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let encrypted = encrypted.clone();
            let codes_json = codes_json.clone();
            tokio::spawn(async move {
                let _ = pg
                    .upsert_totp_secret(user_id, &encrypted, false, &codes_json)
                    .await;
            });
        }

        Ok(MfaSetupResponse {
            provisioning_uri,
            secret_base32,
            backup_codes,
        })
    }

    /// Verify a TOTP code to enable MFA for a user.
    pub fn mfa_verify_enable(&self, user_id: Uuid, code: &str) -> Result<(), String> {
        use totp_rs::{Algorithm, Secret, TOTP};

        let (secret_b32, _enabled, backup_codes_json) = self
            .get_totp_data(user_id)
            .ok_or_else(|| "MFA not set up".to_string())?;

        let secret_bytes = Secret::Encoded(secret_b32.clone())
            .to_bytes()
            .map_err(|e| e.to_string())?;
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes, None, String::new())
            .map_err(|e| e.to_string())?;

        if !totp.check_current(code).map_err(|e| e.to_string())? {
            return Err("Invalid TOTP code".to_string());
        }

        // Enable MFA
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let secret_b32 = secret_b32.clone();
            let backup_codes_json = backup_codes_json.clone();
            tokio::spawn(async move {
                let _ = pg
                    .upsert_totp_secret(user_id, &secret_b32, true, &backup_codes_json)
                    .await;
            });
        }

        Ok(())
    }

    /// Validate a TOTP code during login. Also accepts backup codes.
    pub fn mfa_validate(&self, user_id: Uuid, code: &str) -> Result<bool, String> {
        use totp_rs::{Algorithm, Secret, TOTP};

        let (secret_b32, enabled, backup_codes_json) = self
            .get_totp_data(user_id)
            .ok_or_else(|| "MFA not configured".to_string())?;

        if !enabled {
            return Err("MFA not enabled".to_string());
        }

        // Check backup codes first
        if let Ok(mut backup_codes) = serde_json::from_str::<Vec<String>>(&backup_codes_json) {
            if let Some(pos) = backup_codes.iter().position(|c| c == code) {
                backup_codes.remove(pos);
                let new_codes_json = serde_json::to_string(&backup_codes).unwrap_or_default();
                if let Some(pg) = &self.pg {
                    let pg = pg.clone();
                    let secret_b32 = secret_b32.clone();
                    tokio::spawn(async move {
                        let _ = pg
                            .upsert_totp_secret(user_id, &secret_b32, true, &new_codes_json)
                            .await;
                    });
                }
                return Ok(true);
            }
        }

        // Validate TOTP
        let secret_bytes = Secret::Encoded(secret_b32)
            .to_bytes()
            .map_err(|e| e.to_string())?;
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes, None, String::new())
            .map_err(|e| e.to_string())?;

        totp.check_current(code).map_err(|e| e.to_string())
    }

    /// Disable MFA for a user.
    pub fn mfa_disable(&self, user_id: Uuid) {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            tokio::spawn(async move {
                let _ = pg.delete_totp_secret(user_id).await;
            });
        }
    }

    fn get_totp_data(&self, user_id: Uuid) -> Option<(String, bool, String)> {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                if let Ok(data) =
                    std::thread::scope(|_| handle.block_on(pg.get_totp_secret(user_id)))
                {
                    return data;
                }
            }
        }
        None
    }

    // ─── Session Management ───

    /// Record a new session in the user_sessions table.
    pub fn track_session(
        &self,
        user_id: Uuid,
        token: &str,
        device_name: &str,
        device_type: &str,
        ip_address: &str,
    ) {
        let token_hash = Self::hash_token(token);
        let session_id = Uuid::new_v4();
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let token_hash = token_hash.clone();
            let device_name = device_name.to_string();
            let device_type = device_type.to_string();
            let ip_address = ip_address.to_string();
            tokio::spawn(async move {
                let _ = pg
                    .insert_user_session(
                        session_id,
                        user_id,
                        &token_hash,
                        &device_name,
                        &device_type,
                        &ip_address,
                    )
                    .await;
            });
        }
    }

    /// List active sessions for a user.
    pub fn list_sessions(&self, user_id: Uuid, current_token: &str) -> Vec<UserSessionInfo> {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let rt = tokio::runtime::Handle::try_current();
            let _current_hash = Self::hash_token(current_token);
            if let Ok(handle) = rt {
                if let Ok(sessions) =
                    std::thread::scope(|_| handle.block_on(pg.list_user_sessions(user_id)))
                {
                    return sessions
                        .into_iter()
                        .map(|s| {
                            let id = s["id"].as_str().unwrap_or("").to_string();
                            UserSessionInfo {
                                id,
                                device_name: s["device_name"]
                                    .as_str()
                                    .unwrap_or("Unknown")
                                    .to_string(),
                                device_type: s["device_type"]
                                    .as_str()
                                    .unwrap_or("desktop")
                                    .to_string(),
                                ip_address: s["ip_address"]
                                    .as_str()
                                    .unwrap_or("unknown")
                                    .to_string(),
                                created_at: s["created_at"].as_str().unwrap_or("").to_string(),
                                last_active: s["last_active"].as_str().unwrap_or("").to_string(),
                                current: false, // Set below
                            }
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    /// Revoke a specific session by ID.
    pub fn revoke_session_by_id(&self, session_id: Uuid) -> bool {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                // Get the token_hash for this session so we can also remove it from admin_sessions
                if let Ok(Some(token_hash)) = std::thread::scope(|_| {
                    handle.block_on(pg.get_session_token_hash_for_id(session_id))
                }) {
                    // Remove from in-memory session map
                    self.admin_sessions
                        .retain(|_, s| Self::hash_token(&s.token) != token_hash);
                }
                if let Ok(revoked) =
                    std::thread::scope(|_| handle.block_on(pg.revoke_user_session(session_id)))
                {
                    return revoked;
                }
            }
        }
        false
    }

    /// Revoke all sessions for a user except the current one.
    pub fn revoke_all_sessions(&self, user_id: Uuid, current_token: &str) -> u64 {
        let current_hash = Self::hash_token(current_token);
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                if let Ok(count) = std::thread::scope(|_| {
                    handle.block_on(pg.revoke_all_user_sessions(user_id, &current_hash))
                }) {
                    // Also remove from in-memory map
                    self.admin_sessions.retain(|_, s| {
                        let h = Self::hash_token(&s.token);
                        h == current_hash
                            || s.principal != self.user_sip_uri_for_id(user_id).unwrap_or_default()
                    });
                    return count;
                }
            }
        }
        0
    }

    fn user_sip_uri_for_id(&self, user_id: Uuid) -> Option<String> {
        self.users.get(&user_id).map(|u| u.sip_uri.clone())
    }

    fn hash_token(token: &str) -> String {
        use sha2::Digest as _;
        let digest = Sha256::digest(token.as_bytes());
        hex::encode(digest)
    }

    pub fn users(&self) -> Vec<User> {
        self.users
            .values()
            .into_iter()
            .filter(|user| user.active)
            .collect()
    }

    pub fn all_users(&self) -> Vec<User> {
        self.users.values()
    }

    pub fn update_user_role(&self, id: Uuid, role: &str) -> Option<User> {
        let updated = self.users.with_write(&id, |users| {
            let user = users.get_mut(&id)?;
            user.role = role.to_string();
            Some(user.clone())
        });
        if let Some(user) = &updated {
            self.persist_user(user);
            self.revoke_sessions_for_principal(&user.sip_uri);
            self.broadcast_sse(SseEvent {
                event_type: "user_updated".to_string(),
                payload: serde_json::to_value(user).unwrap_or_default(),
            });
        }
        updated
    }

    pub fn set_user_active(&self, id: Uuid, active: bool, actor: &str) -> Option<User> {
        let updated = self.users.with_write(&id, |users| {
            let user = users.get_mut(&id)?;
            user.active = active;
            if active {
                user.deactivated_at = None;
                user.deactivated_by = None;
            } else {
                user.deactivated_at = Some(Utc::now());
                user.deactivated_by = Some(actor.to_string());
            }
            Some(user.clone())
        });
        if let Some(user) = &updated {
            self.persist_user(user);
            if !active {
                self.revoke_sessions_for_principal(&user.sip_uri);
                self.update_presence(&user.sip_uri, PresenceStatus::Offline, None);
            }
            self.broadcast_sse(SseEvent {
                event_type: if active {
                    "user_activated"
                } else {
                    "user_deactivated"
                }
                .to_string(),
                payload: serde_json::to_value(user).unwrap_or_default(),
            });
        }
        updated
    }

    fn persist_user(&self, user: &User) {
        self.persist(user);
        let user_for_pg = user.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_user(&user_for_pg).await }));
    }

    fn revoke_sessions_for_principal(&self, principal: &str) {
        let principal = principal.to_string();
        self.admin_sessions
            .retain(|_, session| session.principal != principal);
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
            self.pg_spawn(move |pg| {
                Box::pin(async move {
                    pg.update_user_password(u2.id, u2.password_hash.as_deref().unwrap_or(""))
                        .await
                })
            });

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
        let user = self.set_user_active(id, false, "delete");
        if user.is_some() {
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
        self.registrations.insert(aor.clone(), registration.clone());
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

    /// Register a live SIP flow so the server can push messages to this client.
    pub fn add_sip_connection(
        &self,
        peer: String,
        tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) {
        self.sip_connections.insert(peer, tx);
    }

    /// Drop a SIP flow when its socket closes.
    pub fn remove_sip_connection(&self, peer: &str) {
        self.sip_connections.remove(&peer.to_string());
    }

    /// Sender for the live flow at `peer`, if the socket is still open.
    pub fn sip_connection(
        &self,
        peer: &str,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<String>> {
        self.sip_connections.get(&peer.to_string())
    }

    pub fn upsert_proxy_call(&self, call: ProxyCall) {
        self.proxy_calls.insert(call.call_id.clone(), call);
    }

    pub fn proxy_call(&self, call_id: &str) -> Option<ProxyCall> {
        self.proxy_calls.get(&call_id.to_string())
    }

    pub fn remove_proxy_call(&self, call_id: &str) {
        self.proxy_calls.remove(&call_id.to_string());
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
        let dialog = self
            .sip_dialogs
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
        let mut messages = self
            .sip_messages
            .write()
            .expect("sip messages lock poisoned");
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
        let subscription = self
            .sip_subscriptions
            .with_write(&subscription_id, |subscriptions| {
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
        self.sip_subscriptions.remove(&subscription_id.to_string())
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
            locked: false,
            active: false,
            created_at: Utc::now(),
            spotlight_participant_id: None,
            green_room_enabled: false,
            chat_room_id: None,
            registration_enabled: input.registration_enabled.unwrap_or(false),
            max_registrations: input.max_registrations,
            registration_fields: input.registration_fields,
            livekit_room: None,
            livekit_egress_id: None,
        };
        self.conferences.insert(conference.id, conference.clone());
        self.conferences.trim_to_len(MAX_CONFERENCES);
        self.persist(&conference);
        conference
    }

    pub fn list_conferences(&self) -> Vec<Conference> {
        self.conferences.values()
    }

    pub fn get_conference(&self, id: Uuid) -> Option<Conference> {
        self.conferences.get(&id)
    }

    pub fn join_conference(
        &self,
        id: Uuid,
        input: JoinConferenceRequest,
        bypass_lock: bool,
    ) -> Result<Conference, JoinConferenceError> {
        let mut joined: Option<ConferenceParticipant> = None;
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            let existing = conference
                .participants
                .iter()
                .find(|p| p.user_id == input.user_id);
            if conference.locked
                && !bypass_lock
                && !existing.is_some_and(|participant| !participant.removed)
            {
                return Some(Err(JoinConferenceError::Locked));
            }
            if existing.is_none() {
                if let Some(config) = self.town_hall_configs.get(&id) {
                    let attendee_count = conference
                        .participants
                        .iter()
                        .filter(|participant| !participant.removed)
                        .count();
                    let requested_role = input.role.clone().unwrap_or(ParticipantRole::Member);
                    if config.enabled
                        && requested_role == ParticipantRole::Member
                        && attendee_count
                            >= if self.enterprise_capability_available("town_hall_broadcast") {
                                config.max_viewers
                            } else {
                                config.max_viewers.min(1000)
                            }
                    {
                        return Some(Err(JoinConferenceError::CapacityReached));
                    }
                }
            }
            if !conference
                .participants
                .iter()
                .any(|p| p.user_id == input.user_id)
            {
                let participant = ConferenceParticipant {
                    user_id: input.user_id,
                    sip_uri: input.sip_uri,
                    role: input.role.unwrap_or(ParticipantRole::Member),
                    bridge_slot: None,
                    muted: false,
                    removed: false,
                    removed_at: None,
                    removed_by: None,
                    removal_reason: None,
                    joined_at: Utc::now(),
                };
                joined = Some(participant.clone());
                conference.participants.push(participant);
            }
            Some(Ok(conference.clone()))
        });
        let conference = conference.ok_or(JoinConferenceError::NotFound)??;
        self.persist(&conference);
        if let Some(participant) = joined {
            self.open_attendance_record(id, &participant);
        }
        Ok(conference)
    }

    /// Ensure a LiveKit room exists for the given conference, setting
    /// `livekit_room` on the conference if not already set.  This is a
    /// synchronous state update; the actual room creation is done via the
    /// async `livekit::create_room` by the caller (HTTP handler).
    pub fn ensure_livekit_room_name(&self, conference_id: Uuid) -> Option<String> {
        self.conferences.with_write(&conference_id, |conferences| {
            let conference = conferences.get_mut(&conference_id)?;
            if conference.livekit_room.is_none() {
                let room = livekit::room_name_for_conference(conference_id);
                conference.livekit_room = Some(room.clone());
                Some(room)
            } else {
                conference.livekit_room.clone()
            }
        })
    }

    /// Generate a LiveKit access token for a participant.
    pub fn generate_livekit_token(
        &self,
        conference_id: Uuid,
        identity: &str,
        display_name: &str,
    ) -> Result<(String, String), String> {
        let config = self
            .livekit_config()
            .ok_or_else(|| "LiveKit not configured".to_string())?;
        let room_name = self
            .conferences
            .get(&conference_id)
            .and_then(|c| c.livekit_room.clone())
            .ok_or_else(|| "Conference has no LiveKit room".to_string())?;
        let grant = livekit::VideoGrant::participant(&room_name);
        let token = livekit::generate_token(
            config,
            identity,
            display_name,
            grant,
            livekit::DEFAULT_TOKEN_TTL_SECS,
        )?;
        Ok((config.url.clone(), token))
    }

    /// Store the LiveKit egress ID on a conference for recording tracking.
    pub fn set_livekit_egress_id(&self, conference_id: Uuid, egress_id: Option<String>) {
        self.conferences.with_write(&conference_id, |conferences| {
            if let Some(conference) = conferences.get_mut(&conference_id) {
                conference.livekit_egress_id = egress_id;
                // persist handled by caller
            }
        });
        if let Some(conference) = self.conferences.get(&conference_id) {
            self.persist(&conference);
        }
    }

    pub fn can_moderate_conference(
        &self,
        conference_id: Uuid,
        principal: &str,
        is_admin: bool,
    ) -> bool {
        if is_admin || self.is_admin_principal(principal) {
            return true;
        }
        self.conferences
            .get(&conference_id)
            .is_some_and(|conference| {
                conference.participants.iter().any(|participant| {
                    participant.sip_uri == principal
                        && matches!(
                            participant.role,
                            ParticipantRole::Host | ParticipantRole::Moderator
                        )
                        && !participant.removed
                })
            })
    }

    pub fn conference_visible_to(&self, conference_id: Uuid, principal: &str) -> bool {
        if self.is_admin_principal(principal) {
            return true;
        }
        self.conferences
            .get(&conference_id)
            .is_some_and(|conference| {
                conference
                    .participants
                    .iter()
                    .any(|participant| participant.sip_uri == principal && !participant.removed)
            })
    }

    pub fn update_conference_participant(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        input: UpdateConferenceParticipantRequest,
        actor: &str,
    ) -> Option<Conference> {
        let conference = self.conferences.with_write(&conference_id, |conferences| {
            let conference = conferences.get_mut(&conference_id)?;
            let participant = conference
                .participants
                .iter_mut()
                .find(|participant| participant.user_id == user_id)?;
            if let Some(role) = input.role {
                participant.role = role;
            }
            if let Some(muted) = input.muted {
                participant.muted = muted;
            }
            if let Some(removed) = input.removed {
                participant.removed = removed;
                if removed {
                    participant.removed_at = Some(Utc::now());
                    participant.removed_by = Some(actor.to_string());
                    participant.removal_reason = input.removal_reason;
                    participant.muted = true;
                    self.close_attendance_record(
                        conference_id,
                        user_id,
                        AttendanceLeaveReason::Removed,
                        participant.removed_by.clone(),
                    );
                } else {
                    participant.removed_at = None;
                    participant.removed_by = None;
                    participant.removal_reason = None;
                    self.reopen_attendance_record(conference_id, participant);
                }
            }
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
            self.broadcast_sse(SseEvent {
                event_type: "conference_participant_updated".to_string(),
                payload: serde_json::to_value(conference).unwrap_or_default(),
            });
        }
        conference
    }

    pub fn set_conference_locked(&self, conference_id: Uuid, locked: bool) -> Option<Conference> {
        let conference = self.conferences.with_write(&conference_id, |conferences| {
            let conference = conferences.get_mut(&conference_id)?;
            conference.locked = locked;
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
            self.broadcast_sse(SseEvent {
                event_type: "conference_participant_updated".to_string(),
                payload: serde_json::to_value(conference).unwrap_or_default(),
            });
        }
        conference
    }

    pub fn leave_conference(&self, id: Uuid, user_id: Uuid) -> Option<Conference> {
        let mut left = false;
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            left = conference
                .participants
                .iter()
                .any(|participant| participant.user_id == user_id);
            conference.participants.retain(|p| p.user_id != user_id);
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
        }
        if left {
            self.close_attendance_record(id, user_id, AttendanceLeaveReason::Left, None);
        }
        conference
    }

    pub fn conference_attendance(&self, conference_id: Uuid) -> Vec<ConferenceAttendanceRecord> {
        let mut records: Vec<_> = self
            .conference_attendance
            .read()
            .expect("conference attendance lock")
            .iter()
            .filter(|record| record.conference_id == conference_id)
            .cloned()
            .collect();
        records.sort_by(|left, right| left.joined_at.cmp(&right.joined_at));
        records
    }

    fn open_attendance_record(&self, conference_id: Uuid, participant: &ConferenceParticipant) {
        let record = ConferenceAttendanceRecord {
            id: Uuid::new_v4(),
            conference_id,
            user_id: participant.user_id,
            sip_uri: participant.sip_uri.clone(),
            role: participant.role.clone(),
            joined_at: participant.joined_at,
            left_at: None,
            duration_secs: None,
            leave_reason: None,
            removed_by: None,
        };
        self.conference_attendance
            .write()
            .expect("conference attendance lock")
            .push(record.clone());
        self.persist(&record);
    }

    fn reopen_attendance_record(&self, conference_id: Uuid, participant: &ConferenceParticipant) {
        if self
            .conference_attendance(conference_id)
            .iter()
            .any(|record| record.user_id == participant.user_id && record.left_at.is_none())
        {
            return;
        }
        self.open_attendance_record(conference_id, participant);
    }

    fn close_attendance_record(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        reason: AttendanceLeaveReason,
        removed_by: Option<String>,
    ) {
        let updated = {
            let mut records = self
                .conference_attendance
                .write()
                .expect("conference attendance lock");
            let record = records.iter_mut().rev().find(|record| {
                record.conference_id == conference_id
                    && record.user_id == user_id
                    && record.left_at.is_none()
            });
            let Some(record) = record else {
                return;
            };
            let left_at = Utc::now();
            record.left_at = Some(left_at);
            record.duration_secs = Some((left_at - record.joined_at).num_seconds().max(0));
            record.leave_reason = Some(reason);
            record.removed_by = removed_by;
            record.clone()
        };
        self.persist(&updated);
    }

    fn close_active_attendance_for_conference(
        &self,
        conference_id: Uuid,
        reason: AttendanceLeaveReason,
    ) {
        let updated = {
            let mut records = self
                .conference_attendance
                .write()
                .expect("conference attendance lock");
            let left_at = Utc::now();
            let mut updated = Vec::new();
            for record in records
                .iter_mut()
                .filter(|record| record.conference_id == conference_id && record.left_at.is_none())
            {
                record.left_at = Some(left_at);
                record.duration_secs = Some((left_at - record.joined_at).num_seconds().max(0));
                record.leave_reason = Some(reason.clone());
                updated.push(record.clone());
            }
            updated
        };
        for record in updated {
            self.persist(&record);
        }
    }

    pub fn conference_participant(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
    ) -> Option<ConferenceParticipant> {
        self.conferences.get(&conference_id).and_then(|conference| {
            conference
                .participants
                .into_iter()
                .find(|participant| participant.user_id == user_id)
        })
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

    pub fn deactivate_conference(&self, id: Uuid) -> Option<Conference> {
        let conference = self.conferences.with_write(&id, |conferences| {
            let conference = conferences.get_mut(&id)?;
            conference.active = false;
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
            self.close_active_attendance_for_conference(id, AttendanceLeaveReason::Ended);
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

    // ── Meeting templates ──────────────────────────────────────────

    pub fn list_meeting_templates(&self) -> Vec<MeetingTemplate> {
        self.meeting_templates.values()
    }

    pub fn get_meeting_template(&self, id: Uuid) -> Option<MeetingTemplate> {
        self.meeting_templates.get(&id)
    }

    pub fn create_meeting_template(
        &self,
        principal: &str,
        input: CreateMeetingTemplateRequest,
    ) -> MeetingTemplate {
        let template = MeetingTemplate {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description.unwrap_or_default(),
            default_lobby: input.default_lobby,
            default_mute_on_join: input.default_mute_on_join,
            default_allow_reactions: input.default_allow_reactions,
            default_recording: input.default_recording,
            max_participants: input.max_participants,
            allowed_roles: input.allowed_roles,
            created_at: Utc::now(),
            created_by: principal.to_string(),
        };
        self.meeting_templates.insert(template.id, template.clone());
        self.persist(&template);
        let template_for_pg = template.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(
                    MeetingTemplate::collection(),
                    template_for_pg.key(),
                    &template_for_pg,
                )
                .await
            })
        });
        template
    }

    pub fn update_meeting_template(
        &self,
        id: Uuid,
        input: UpdateMeetingTemplateRequest,
    ) -> Option<MeetingTemplate> {
        let template = {
            let mut t = self.meeting_templates.get(&id)?;
            if let Some(name) = input.name {
                t.name = name;
            }
            if let Some(description) = input.description {
                t.description = description;
            }
            if let Some(v) = input.default_lobby {
                t.default_lobby = v;
            }
            if let Some(v) = input.default_mute_on_join {
                t.default_mute_on_join = v;
            }
            if let Some(v) = input.default_allow_reactions {
                t.default_allow_reactions = v;
            }
            if let Some(v) = input.default_recording {
                t.default_recording = v;
            }
            if let Some(v) = input.max_participants {
                t.max_participants = v;
            }
            if let Some(v) = input.allowed_roles {
                t.allowed_roles = v;
            }
            t
        };
        self.meeting_templates.insert(template.id, template.clone());
        self.persist(&template);
        let template_for_pg = template.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(
                    MeetingTemplate::collection(),
                    template_for_pg.key(),
                    &template_for_pg,
                )
                .await
            })
        });
        Some(template)
    }

    pub fn delete_meeting_template(&self, id: Uuid) -> bool {
        self.meeting_templates.remove(&id).is_some()
    }

    // ── Spotlight ─────────────────────────────────────────────────

    pub fn set_spotlight(
        &self,
        conference_id: Uuid,
        participant_id: Option<Uuid>,
    ) -> Option<Conference> {
        let conference = self.conferences.with_write(&conference_id, |conferences| {
            let conference = conferences.get_mut(&conference_id)?;
            conference.spotlight_participant_id = participant_id;
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
            self.broadcast_sse(SseEvent {
                event_type: "spotlight_changed".to_string(),
                payload: serde_json::json!({
                    "conference_id": conference_id,
                    "participant_id": participant_id,
                }),
            });
        }
        conference
    }

    // ── Meeting reactions ─────────────────────────────────────────

    pub fn broadcast_meeting_reaction(&self, conference_id: Uuid, user_uri: &str, emoji: &str) {
        let reaction = MeetingReaction {
            user_id: user_uri.to_string(),
            user_name: user_uri
                .strip_prefix("sip:")
                .unwrap_or(user_uri)
                .to_string(),
            emoji: emoji.to_string(),
            timestamp: Utc::now(),
        };
        self.broadcast_sse(SseEvent {
            event_type: "meeting_reaction".to_string(),
            payload: serde_json::json!({
                "conference_id": conference_id,
                "reaction": reaction,
            }),
        });
    }

    // ── Persistent meeting chat ───────────────────────────────────

    pub fn ensure_meeting_chat_room(
        &self,
        conference_id: Uuid,
        title: &str,
        organizer_uri: &str,
    ) -> Uuid {
        // Check if conference already has a linked chat room
        if let Some(conference) = self.conferences.get(&conference_id) {
            if let Some(room_id) = conference.chat_room_id {
                return room_id;
            }
        }
        // Create a new room for the meeting chat
        let room_id = Uuid::new_v4();
        let room = Room {
            id: room_id,
            name: format!("Meeting: {}", title),
            description: format!("Chat for meeting: {}", title),
            is_direct: false,
            created_by: organizer_uri.to_string(),
            members: vec![RoomMember {
                user_sip_uri: organizer_uri.to_string(),
                role: "owner".to_string(),
                joined_at: Utc::now(),
            }],
            conference_id: Some(conference_id),
            call_uri: None,
            team_id: None,
            channel_name: None,
            channel_type: "standard".to_string(),
            channel_owners: Vec::new(),
            posting_policy: "members".to_string(),
            created_at: Utc::now(),
        };
        self.rooms.insert(room_id, room.clone());
        self.persist(&room);
        let room_for_pg = room.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));

        // Link the chat room to the conference
        self.conferences.with_write(&conference_id, |conferences| {
            if let Some(conference) = conferences.get_mut(&conference_id) {
                conference.chat_room_id = Some(room_id);
            }
        });
        if let Some(conference) = self.conferences.get(&conference_id) {
            self.persist(&conference);
        }

        room_id
    }

    // ── Green room ────────────────────────────────────────────────

    pub fn get_green_room(&self, conference_id: Uuid) -> GreenRoomState {
        let conference = self.conferences.get(&conference_id);
        let enabled = conference
            .as_ref()
            .map(|c| c.green_room_enabled)
            .unwrap_or(false);
        self.green_rooms
            .get(&conference_id)
            .unwrap_or_else(|| GreenRoomState {
                conference_id,
                enabled,
                participants: Vec::new(),
            })
    }

    pub fn set_green_room_enabled(&self, conference_id: Uuid, enabled: bool) -> Option<Conference> {
        let conference = self.conferences.with_write(&conference_id, |conferences| {
            let conference = conferences.get_mut(&conference_id)?;
            conference.green_room_enabled = enabled;
            Some(conference.clone())
        });
        if let Some(conference) = &conference {
            self.persist(conference);
        }
        conference
    }

    pub fn join_green_room(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        sip_uri: String,
    ) -> GreenRoomState {
        self.green_rooms.with_write(&conference_id, |rooms| {
            let state = rooms
                .entry(conference_id)
                .or_insert_with(|| GreenRoomState {
                    conference_id,
                    enabled: true,
                    participants: Vec::new(),
                });
            if !state.participants.iter().any(|p| p.user_id == user_id) {
                state.participants.push(GreenRoomParticipant {
                    user_id,
                    sip_uri,
                    ready: false,
                    joined_at: Utc::now(),
                });
            }
            state.clone()
        })
    }

    pub fn set_green_room_ready(&self, conference_id: Uuid, user_id: Uuid) -> GreenRoomState {
        self.green_rooms.with_write(&conference_id, |rooms| {
            let state = rooms
                .entry(conference_id)
                .or_insert_with(|| GreenRoomState {
                    conference_id,
                    enabled: true,
                    participants: Vec::new(),
                });
            if let Some(p) = state.participants.iter_mut().find(|p| p.user_id == user_id) {
                p.ready = true;
            }
            state.clone()
        })
    }

    // ── Out-of-office ─────────────────────────────────────────────

    pub fn get_out_of_office(&self, user_uri: &str) -> OutOfOfficeSettings {
        // Find user by SIP URI to get OOO settings
        let users = self.users.values();
        let user = users.into_iter().find(|u| u.sip_uri == user_uri);
        match user {
            Some(u) => OutOfOfficeSettings {
                message: u.out_of_office_message,
                until: u.out_of_office_until,
            },
            None => OutOfOfficeSettings {
                message: None,
                until: None,
            },
        }
    }

    pub fn set_out_of_office(
        &self,
        user_uri: &str,
        input: SetOutOfOfficeRequest,
    ) -> OutOfOfficeSettings {
        let users = self.users.values();
        if let Some(mut user) = users.into_iter().find(|u| u.sip_uri == user_uri) {
            user.out_of_office_message = input.message.clone();
            user.out_of_office_until = input.until;
            self.users.insert(user.id, user.clone());
            self.persist(&user);
            let user_for_pg = user.clone();
            self.pg_spawn(move |pg| Box::pin(async move { pg.insert_user(&user_for_pg).await }));
        }
        OutOfOfficeSettings {
            message: input.message,
            until: input.until,
        }
    }

    pub fn check_out_of_office_auto_reply(&self, recipient_uri: &str) -> Option<String> {
        let ooo = self.get_out_of_office(recipient_uri);
        if let Some(message) = &ooo.message {
            if !message.is_empty() {
                // Check if OOO has expired
                if let Some(until) = ooo.until {
                    if Utc::now() > until {
                        return None;
                    }
                }
                return Some(message.clone());
            }
        }
        None
    }

    // ── Attendance CSV export ─────────────────────────────────────

    pub fn export_attendance_csv(&self, conference_id: Uuid) -> String {
        let records = self.conference_attendance(conference_id);
        let mut csv = String::from("participant,join_time,leave_time,duration,leave_reason\n");
        for record in &records {
            let participant = record
                .sip_uri
                .strip_prefix("sip:")
                .unwrap_or(&record.sip_uri);
            let join_time = record.joined_at.to_rfc3339();
            let leave_time = record.left_at.map(|t| t.to_rfc3339()).unwrap_or_default();
            let duration = record.duration_secs.unwrap_or(0);
            let leave_reason = record
                .leave_reason
                .as_ref()
                .map(|r| {
                    serde_json::to_value(r)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            csv.push_str(&format!(
                "\"{}\",\"{}\",\"{}\",{},\"{}\"\n",
                participant, join_time, leave_time, duration, leave_reason
            ));
        }
        csv
    }

    // ── Lobby methods ──────────────────────────────────────────────

    pub fn get_lobby(&self, conference_id: Uuid) -> ConferenceLobby {
        self.conference_lobbies
            .get(&conference_id)
            .unwrap_or_else(|| ConferenceLobby {
                conference_id,
                enabled: false,
                participants: Vec::new(),
            })
    }

    pub fn set_lobby_enabled(&self, conference_id: Uuid, enabled: bool) -> ConferenceLobby {
        let lobby = self
            .conference_lobbies
            .with_write(&conference_id, |lobbies| {
                let lobby = lobbies
                    .entry(conference_id)
                    .or_insert_with(|| ConferenceLobby {
                        conference_id,
                        enabled: false,
                        participants: Vec::new(),
                    });
                lobby.enabled = enabled;
                lobby.clone()
            });
        lobby
    }

    pub fn join_lobby(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        sip_uri: String,
        display_name: String,
    ) -> ConferenceLobby {
        self.conference_lobbies
            .with_write(&conference_id, |lobbies| {
                let lobby = lobbies
                    .entry(conference_id)
                    .or_insert_with(|| ConferenceLobby {
                        conference_id,
                        enabled: true,
                        participants: Vec::new(),
                    });
                if !lobby.participants.iter().any(|p| p.user_id == user_id) {
                    lobby.participants.push(LobbyParticipant {
                        user_id,
                        sip_uri,
                        display_name,
                        state: LobbyParticipantState::Waiting,
                        requested_at: Utc::now(),
                    });
                }
                lobby.clone()
            })
    }

    pub fn admit_lobby_participant(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        admit: bool,
    ) -> Option<ConferenceLobby> {
        let lobby = self
            .conference_lobbies
            .with_write(&conference_id, |lobbies| {
                let lobby = lobbies.get_mut(&conference_id)?;
                if let Some(p) = lobby.participants.iter_mut().find(|p| p.user_id == user_id) {
                    p.state = if admit {
                        LobbyParticipantState::Admitted
                    } else {
                        LobbyParticipantState::Rejected
                    };
                }
                Some(lobby.clone())
            });
        lobby
    }

    pub fn admit_all_lobby(&self, conference_id: Uuid) -> Option<ConferenceLobby> {
        self.conference_lobbies
            .with_write(&conference_id, |lobbies| {
                let lobby = lobbies.get_mut(&conference_id)?;
                for p in &mut lobby.participants {
                    if p.state == LobbyParticipantState::Waiting {
                        p.state = LobbyParticipantState::Admitted;
                    }
                }
                Some(lobby.clone())
            })
    }

    // ── Raise hand methods ─────────────────────────────────────────

    pub fn get_raised_hands(&self, conference_id: Uuid) -> Vec<HandRaise> {
        self.raised_hands.get(&conference_id).unwrap_or_default()
    }

    pub fn raise_hand(
        &self,
        conference_id: Uuid,
        user_id: Uuid,
        sip_uri: String,
    ) -> Vec<HandRaise> {
        self.raised_hands.with_write(&conference_id, |hands| {
            let list = hands.entry(conference_id).or_default();
            if !list.iter().any(|h| h.user_id == user_id) {
                list.push(HandRaise {
                    user_id,
                    sip_uri,
                    raised_at: Utc::now(),
                });
            }
            list.clone()
        })
    }

    pub fn lower_hand(&self, conference_id: Uuid, user_id: Uuid) -> Vec<HandRaise> {
        self.raised_hands.with_write(&conference_id, |hands| {
            let list = hands.entry(conference_id).or_default();
            list.retain(|h| h.user_id != user_id);
            list.clone()
        })
    }

    pub fn lower_all_hands(&self, conference_id: Uuid) -> Vec<HandRaise> {
        self.raised_hands.with_write(&conference_id, |hands| {
            let list = hands.entry(conference_id).or_default();
            list.clear();
            list.clone()
        })
    }

    // ── Poll methods ───────────────────────────────────────────────

    pub fn create_poll(
        &self,
        conference_id: Uuid,
        principal: &str,
        input: CreatePollRequest,
    ) -> MeetingPoll {
        let poll = MeetingPoll {
            id: Uuid::new_v4(),
            conference_id,
            question: input.question,
            options: input
                .options
                .into_iter()
                .map(|text| PollOption {
                    id: Uuid::new_v4(),
                    text,
                    votes: Vec::new(),
                })
                .collect(),
            status: PollStatus::Draft,
            anonymous: input.anonymous,
            multi_select: input.multi_select,
            created_by: principal.to_string(),
            created_at: Utc::now(),
        };
        self.meeting_polls.insert(poll.id, poll.clone());
        poll
    }

    pub fn launch_poll(&self, poll_id: Uuid) -> Option<MeetingPoll> {
        self.meeting_polls.with_write(&poll_id, |polls| {
            let poll = polls.get_mut(&poll_id)?;
            poll.status = PollStatus::Active;
            Some(poll.clone())
        })
    }

    pub fn close_poll(&self, poll_id: Uuid) -> Option<MeetingPoll> {
        self.meeting_polls.with_write(&poll_id, |polls| {
            let poll = polls.get_mut(&poll_id)?;
            poll.status = PollStatus::Closed;
            Some(poll.clone())
        })
    }

    pub fn cast_vote(
        &self,
        poll_id: Uuid,
        voter_uri: &str,
        option_ids: Vec<Uuid>,
    ) -> Option<MeetingPoll> {
        self.meeting_polls.with_write(&poll_id, |polls| {
            let poll = polls.get_mut(&poll_id)?;
            if poll.status != PollStatus::Active {
                return None;
            }
            // Remove previous votes by this voter
            for opt in &mut poll.options {
                opt.votes.retain(|v| v != voter_uri);
            }
            // Cast new votes
            for opt in &mut poll.options {
                if option_ids.contains(&opt.id) {
                    if poll.multi_select || option_ids.len() == 1 {
                        opt.votes.push(voter_uri.to_string());
                    }
                }
            }
            Some(poll.clone())
        })
    }

    pub fn list_polls(&self, conference_id: Uuid) -> Vec<MeetingPoll> {
        self.meeting_polls
            .values()
            .into_iter()
            .filter(|p| p.conference_id == conference_id)
            .collect()
    }

    // ── Q&A methods ────────────────────────────────────────────────

    pub fn ask_question(&self, conference_id: Uuid, asked_by: &str, text: String) -> QaQuestion {
        let q = QaQuestion {
            id: Uuid::new_v4(),
            conference_id,
            text,
            asked_by: asked_by.to_string(),
            upvotes: Vec::new(),
            answered: false,
            answer: None,
            created_at: Utc::now(),
        };
        self.qa_questions.insert(q.id, q.clone());
        q
    }

    pub fn upvote_question(&self, question_id: Uuid, voter_uri: &str) -> Option<QaQuestion> {
        self.qa_questions.with_write(&question_id, |questions| {
            let q = questions.get_mut(&question_id)?;
            if !q.upvotes.contains(&voter_uri.to_string()) {
                q.upvotes.push(voter_uri.to_string());
            }
            Some(q.clone())
        })
    }

    pub fn answer_question(&self, question_id: Uuid, answer: String) -> Option<QaQuestion> {
        self.qa_questions.with_write(&question_id, |questions| {
            let q = questions.get_mut(&question_id)?;
            q.answered = true;
            q.answer = Some(answer);
            Some(q.clone())
        })
    }

    pub fn list_questions(&self, conference_id: Uuid) -> Vec<QaQuestion> {
        self.qa_questions
            .values()
            .into_iter()
            .filter(|q| q.conference_id == conference_id)
            .collect()
    }

    // ── Breakout room methods ──────────────────────────────────────

    pub fn create_breakout_session(
        &self,
        conference_id: Uuid,
        input: CreateBreakoutRequest,
    ) -> BreakoutSession {
        let session = BreakoutSession {
            id: Uuid::new_v4(),
            conference_id,
            rooms: input
                .rooms
                .into_iter()
                .map(|r| BreakoutRoom {
                    id: Uuid::new_v4(),
                    name: r.name,
                    participants: r.participants,
                })
                .collect(),
            status: BreakoutStatus::Pending,
            duration_secs: input.duration_secs,
            created_at: Utc::now(),
        };
        self.breakout_sessions.insert(session.id, session.clone());
        session
    }

    pub fn start_breakout(&self, session_id: Uuid) -> Option<BreakoutSession> {
        self.breakout_sessions.with_write(&session_id, |sessions| {
            let session = sessions.get_mut(&session_id)?;
            session.status = BreakoutStatus::Active;
            Some(session.clone())
        })
    }

    pub fn close_breakout(&self, session_id: Uuid) -> Option<BreakoutSession> {
        self.breakout_sessions.with_write(&session_id, |sessions| {
            let session = sessions.get_mut(&session_id)?;
            session.status = BreakoutStatus::Closed;
            Some(session.clone())
        })
    }

    pub fn list_breakouts(&self, conference_id: Uuid) -> Vec<BreakoutSession> {
        self.breakout_sessions
            .values()
            .into_iter()
            .filter(|s| s.conference_id == conference_id)
            .collect()
    }

    // ── PowerPoint Live / presentation sessions ────────────────────

    pub fn create_presentation_session(
        &self,
        conference_id: Uuid,
        presenter_uri: &str,
        input: CreatePresentationSessionRequest,
    ) -> Option<PresentationSession> {
        self.conferences.get(&conference_id)?;
        let slides: Vec<_> = input
            .slides
            .into_iter()
            .enumerate()
            .filter_map(|(index, slide)| {
                let title = slide.title.trim();
                if title.is_empty() {
                    return None;
                }
                Some(PresentationSlide {
                    index,
                    title: title.to_string(),
                    notes: non_empty_string(slide.notes.unwrap_or_default()),
                    render_url: non_empty_string(slide.render_url.unwrap_or_default()),
                })
            })
            .collect();
        if slides.is_empty() {
            return None;
        }
        let now = Utc::now();
        let title = input.title.trim();
        let session = PresentationSession {
            id: Uuid::new_v4(),
            conference_id,
            title: if title.is_empty() {
                "Presentation".to_string()
            } else {
                title.to_string()
            },
            source_file_id: input.source_file_id,
            presenter_uri: normalize_emergency_user_uri(presenter_uri)
                .unwrap_or_else(|| presenter_uri.to_string()),
            slides,
            current_slide: 0,
            attendee_navigation_enabled: input.attendee_navigation_enabled,
            renderer_configured: self.enterprise_capability_available("powerpoint_live"),
            ended_at: None,
            created_at: now,
            updated_at: now,
        };
        self.presentation_sessions
            .insert(session.id, session.clone());
        Some(session)
    }

    pub fn list_presentation_sessions(&self, conference_id: Uuid) -> Vec<PresentationSession> {
        let mut sessions: Vec<_> = self
            .presentation_sessions
            .values()
            .into_iter()
            .filter(|session| session.conference_id == conference_id)
            .map(|mut session| {
                session.renderer_configured =
                    self.enterprise_capability_available("powerpoint_live");
                session
            })
            .collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    pub fn presentation_session(&self, id: Uuid) -> Option<PresentationSession> {
        self.presentation_sessions.get(&id).map(|mut session| {
            session.renderer_configured = self.enterprise_capability_available("powerpoint_live");
            session
        })
    }

    pub fn update_presentation_session(
        &self,
        id: Uuid,
        input: UpdatePresentationSessionRequest,
    ) -> Option<PresentationSession> {
        let mut session = self.presentation_sessions.get(&id)?;
        if session.ended_at.is_some() {
            return Some(session);
        }
        if let Some(slide) = input.current_slide {
            session.current_slide = slide.min(session.slides.len().saturating_sub(1));
        }
        if let Some(enabled) = input.attendee_navigation_enabled {
            session.attendee_navigation_enabled = enabled;
        }
        if let Some(presenter) = input.presenter_uri {
            if let Some(normalized) = normalize_emergency_user_uri(&presenter) {
                session.presenter_uri = normalized;
            }
        }
        session.renderer_configured = self.enterprise_capability_available("powerpoint_live");
        session.updated_at = Utc::now();
        self.presentation_sessions.insert(id, session.clone());
        Some(session)
    }

    pub fn end_presentation_session(&self, id: Uuid) -> Option<PresentationSession> {
        let mut session = self.presentation_sessions.get(&id)?;
        let now = Utc::now();
        session.ended_at = Some(now);
        session.updated_at = now;
        session.renderer_configured = self.enterprise_capability_available("powerpoint_live");
        self.presentation_sessions.insert(id, session.clone());
        Some(session)
    }

    // ── Transcript / live captions methods ─────────────────────────

    pub fn post_transcript(
        &self,
        conference_id: Uuid,
        input: PostTranscriptRequest,
    ) -> TranscriptSegment {
        let segment = TranscriptSegment {
            id: Uuid::new_v4(),
            conference_id,
            speaker_uri: input.speaker_uri,
            speaker_name: input.speaker_name,
            text: input.text,
            timestamp: Utc::now(),
            is_final: input.is_final,
            language: input.language.or_else(|| Some("en".to_string())),
        };
        {
            let mut transcripts = self.transcripts.write().expect("transcripts lock");
            transcripts.push(segment.clone());
            // Keep last 50000 segments
            if transcripts.len() > 50000 {
                let drain_count = transcripts.len() - 50000;
                transcripts.drain(..drain_count);
            }
        }
        segment
    }

    pub fn get_transcript(&self, conference_id: Uuid) -> Vec<TranscriptSegment> {
        self.transcripts
            .read()
            .expect("transcripts lock")
            .iter()
            .filter(|s| s.conference_id == conference_id)
            .cloned()
            .collect()
    }

    pub fn export_transcript(&self, conference_id: Uuid) -> TranscriptExport {
        let segments = self.get_transcript(conference_id);
        let title = self
            .conferences
            .get(&conference_id)
            .map(|c| c.title)
            .unwrap_or_else(|| format!("Conference {}", conference_id));
        TranscriptExport {
            conference_id,
            title,
            segments,
            exported_at: Utc::now(),
        }
    }

    pub fn meeting_assistant_report(&self, conference_id: Uuid) -> Option<MeetingAssistantReport> {
        let title = self.conferences.get(&conference_id)?.title;
        let segments: Vec<_> = self
            .get_transcript(conference_id)
            .into_iter()
            .filter(|segment| segment.is_final)
            .collect();
        let ai_provider_configured =
            self.enterprise_capability_report()
                .capabilities
                .iter()
                .any(|capability| {
                    matches!(capability.id.as_str(), "meeting_assistant" | "copilot")
                        && capability.status == "available"
                });
        let speaker_stats = meeting_speaker_stats(&segments);
        let summary = meeting_summary(&segments);
        Some(MeetingAssistantReport {
            conference_id,
            title,
            generated_at: Utc::now(),
            transcript_segments: segments.len(),
            ai_provider_configured,
            summary,
            key_topics: meeting_key_topics(&segments),
            action_items: meeting_action_items(&segments),
            decisions: meeting_lines_matching(
                &segments,
                &["decided", "decision", "approved", "agreed", "we will"],
            ),
            risks: meeting_lines_matching(
                &segments,
                &["risk", "blocked", "blocker", "concern", "issue", "problem"],
            ),
            open_questions: meeting_open_questions(&segments),
            speaker_stats,
        })
    }

    // ── Call quality methods ───────────────────────────────────────

    pub fn post_call_quality(
        &self,
        principal: &str,
        input: PostCallQualityRequest,
    ) -> CallQualityReport {
        let diagnostics = call_quality_diagnostics(
            input.mos_score,
            input.jitter_ms,
            input.packet_loss_pct,
            input.round_trip_ms,
        );
        let report = CallQualityReport {
            id: Uuid::new_v4(),
            call_id: input.call_id,
            user_sip_uri: principal.to_string(),
            codec: input.codec,
            jitter_ms: input.jitter_ms,
            packet_loss_pct: input.packet_loss_pct,
            round_trip_ms: input.round_trip_ms,
            mos_score: input.mos_score,
            bytes_sent: input.bytes_sent,
            bytes_received: input.bytes_received,
            rating: diagnostics.rating,
            issues: diagnostics.issues,
            recommended_action: diagnostics.recommended_action,
            reported_at: Utc::now(),
        };
        {
            let mut reports = self.call_quality_reports.write().expect("cqd lock");
            reports.push(report.clone());
            if reports.len() > 100000 {
                let drain_count = reports.len() - 100000;
                reports.drain(..drain_count);
            }
        }
        self.persist(&report);
        report
    }

    pub fn list_call_quality(&self) -> Vec<CallQualityReport> {
        self.call_quality_reports.read().expect("cqd lock").clone()
    }

    pub fn search_call_quality(&self, query: CallQualityQuery) -> Vec<CallQualityReport> {
        let user_filter = query
            .user_sip_uri
            .as_ref()
            .map(|value| value.trim().trim_start_matches("sip:").to_ascii_lowercase());
        let mut reports: Vec<_> = self
            .call_quality_reports
            .read()
            .expect("cqd lock")
            .iter()
            .filter(|report| {
                if let Some(call_id) = query.call_id {
                    if report.call_id != call_id {
                        return false;
                    }
                }
                if let Some(rating) = query.rating {
                    if report.rating != rating {
                        return false;
                    }
                }
                if let Some(from) = query.from {
                    if report.reported_at < from {
                        return false;
                    }
                }
                if let Some(to) = query.to {
                    if report.reported_at > to {
                        return false;
                    }
                }
                if let Some(user_filter) = &user_filter {
                    let report_user = report
                        .user_sip_uri
                        .trim_start_matches("sip:")
                        .to_ascii_lowercase();
                    if !report_user.contains(user_filter) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        reports.sort_by(|left, right| left.reported_at.cmp(&right.reported_at));
        if let Some(limit) = query.limit.filter(|limit| *limit > 0) {
            if reports.len() > limit {
                reports.drain(0..reports.len() - limit);
            }
        }
        reports
    }

    pub fn call_quality_summary(&self) -> CallQualitySummary {
        let reports = self.call_quality_reports.read().expect("cqd lock");
        let total = reports.len();
        if total == 0 {
            return CallQualitySummary {
                total_reports: 0,
                avg_mos: 0.0,
                avg_jitter_ms: 0.0,
                avg_packet_loss_pct: 0.0,
                avg_round_trip_ms: 0.0,
                poor_quality_calls: 0,
                warning_quality_calls: 0,
                worst_mos: 0.0,
            };
        }
        let n = total as f64;
        CallQualitySummary {
            total_reports: total,
            avg_mos: reports.iter().map(|r| r.mos_score).sum::<f64>() / n,
            avg_jitter_ms: reports.iter().map(|r| r.jitter_ms).sum::<f64>() / n,
            avg_packet_loss_pct: reports.iter().map(|r| r.packet_loss_pct).sum::<f64>() / n,
            avg_round_trip_ms: reports.iter().map(|r| r.round_trip_ms).sum::<f64>() / n,
            poor_quality_calls: reports
                .iter()
                .filter(|r| r.rating == CallQualityRating::Poor)
                .count(),
            warning_quality_calls: reports
                .iter()
                .filter(|r| r.rating == CallQualityRating::Warning)
                .count(),
            worst_mos: reports
                .iter()
                .map(|r| r.mos_score)
                .fold(f64::INFINITY, f64::min),
        }
    }

    // ── DLP methods ────────────────────────────────────────────────

    pub fn create_dlp_policy(
        &self,
        principal: &str,
        input: CreateDlpPolicyRequest,
    ) -> Result<DlpPolicy, String> {
        validate_dlp_pattern(&input.pattern)?;
        let policy = DlpPolicy {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description.unwrap_or_default(),
            pattern: input.pattern,
            action: input.action,
            enabled: input.enabled,
            created_by: principal.to_string(),
            created_at: Utc::now(),
        };
        self.dlp_policies.insert(policy.id, policy.clone());
        self.persist(&policy);
        Ok(policy)
    }

    pub fn delete_dlp_policy(&self, id: Uuid) -> bool {
        if self.dlp_policies.remove(&id).is_some() {
            self.delete_persisted(DlpPolicy::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    pub fn update_dlp_policy(
        &self,
        id: Uuid,
        input: UpdateDlpPolicyRequest,
    ) -> Result<Option<DlpPolicy>, String> {
        if let Some(pattern) = input.pattern.as_deref() {
            validate_dlp_pattern(pattern)?;
        }
        let updated = self.dlp_policies.with_write(&id, |policies| {
            let policy = policies.get_mut(&id)?;
            if let Some(name) = input.name {
                policy.name = name;
            }
            if let Some(description) = input.description {
                policy.description = description;
            }
            if let Some(pattern) = input.pattern {
                policy.pattern = pattern;
            }
            if let Some(action) = input.action {
                policy.action = action;
            }
            if let Some(enabled) = input.enabled {
                policy.enabled = enabled;
            }
            Some(policy.clone())
        });
        if let Some(policy) = &updated {
            self.persist(policy);
        }
        Ok(updated)
    }

    pub fn list_dlp_policies(&self) -> Vec<DlpPolicy> {
        self.dlp_policies.values()
    }

    pub fn scan_content_dlp(&self, user_uri: &str, content: &str) -> DlpScanResult {
        self.evaluate_dlp_content(user_uri, content, true)
    }

    pub fn preview_content_dlp(&self, user_uri: &str, content: &str) -> DlpScanResult {
        self.evaluate_dlp_content(user_uri, content, false)
    }

    fn evaluate_dlp_content(&self, user_uri: &str, content: &str, record: bool) -> DlpScanResult {
        let policies = self.dlp_policies.values();
        let mut violations = Vec::new();
        for policy in &policies {
            if !policy.enabled {
                continue;
            }
            if let Ok(re) = regex::Regex::new(&policy.pattern) {
                if re.is_match(content) {
                    let violation = DlpViolation {
                        id: Uuid::new_v4(),
                        policy_id: policy.id,
                        policy_name: policy.name.clone(),
                        user_uri: user_uri.to_string(),
                        action_taken: policy.action.clone(),
                        content_snippet: dlp_content_snippet(content),
                        detected_at: Utc::now(),
                    };
                    violations.push(violation);
                }
            }
        }

        let blocked = violations
            .iter()
            .any(|v| v.action_taken == DlpAction::Block);

        if record && !violations.is_empty() {
            let mut stored = self.dlp_violations.write().expect("dlp violations lock");
            stored.extend(violations.clone());
            if stored.len() > 50000 {
                let drain_count = stored.len() - 50000;
                stored.drain(..drain_count);
            }
            for violation in &violations {
                self.persist(violation);
            }
        }

        DlpScanResult {
            allowed: !blocked,
            violations,
        }
    }

    pub fn list_dlp_violations(&self) -> Vec<DlpViolation> {
        self.dlp_violations
            .read()
            .expect("dlp violations lock")
            .clone()
    }

    pub fn search_dlp_violations(&self, query: DlpViolationQuery) -> Vec<DlpViolation> {
        let policy = query
            .policy
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let user_uri = query
            .user_uri
            .as_ref()
            .map(|value| value.trim().trim_start_matches("sip:").to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let action = query.action;
        let mut violations: Vec<_> = self
            .dlp_violations
            .read()
            .expect("dlp violations lock")
            .iter()
            .filter(|violation| {
                if let Some(from) = query.from {
                    if violation.detected_at < from {
                        return false;
                    }
                }
                if let Some(to) = query.to {
                    if violation.detected_at > to {
                        return false;
                    }
                }
                if let Some(action) = &action {
                    if &violation.action_taken != action {
                        return false;
                    }
                }
                if let Some(policy) = &policy {
                    let policy_id = violation.policy_id.to_string();
                    if !violation.policy_name.to_ascii_lowercase().contains(policy)
                        && !policy_id.contains(policy)
                    {
                        return false;
                    }
                }
                if let Some(user_uri) = &user_uri {
                    let violation_user = violation
                        .user_uri
                        .trim_start_matches("sip:")
                        .to_ascii_lowercase();
                    if !violation_user.contains(user_uri) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        violations.sort_by(|left, right| right.detected_at.cmp(&left.detected_at));
        let limit = query.limit.unwrap_or(500).clamp(1, 5000);
        violations.truncate(limit);
        violations
    }

    // ── Information Barriers ──────────────────────────────────────

    pub fn list_barriers(&self) -> Vec<InformationBarrier> {
        self.information_barriers.values()
    }

    pub fn create_barrier(&self, input: CreateInformationBarrierRequest) -> InformationBarrier {
        let barrier = InformationBarrier {
            id: Uuid::new_v4(),
            name: input.name,
            segment1_name: input.segment1_name,
            segment1_users: input.segment1_users,
            segment2_name: input.segment2_name,
            segment2_users: input.segment2_users,
            block_chat: input.block_chat,
            block_call: input.block_call,
            enabled: input.enabled,
            created_at: Utc::now(),
        };
        self.information_barriers
            .insert(barrier.id, barrier.clone());
        self.persist(&barrier);
        barrier
    }

    pub fn update_barrier(
        &self,
        id: Uuid,
        input: UpdateInformationBarrierRequest,
    ) -> Option<InformationBarrier> {
        let updated = self.information_barriers.with_write(&id, |barriers| {
            let barrier = barriers.get_mut(&id)?;
            if let Some(name) = input.name {
                barrier.name = name;
            }
            if let Some(s) = input.segment1_name {
                barrier.segment1_name = s;
            }
            if let Some(users) = input.segment1_users {
                barrier.segment1_users = users;
            }
            if let Some(s) = input.segment2_name {
                barrier.segment2_name = s;
            }
            if let Some(users) = input.segment2_users {
                barrier.segment2_users = users;
            }
            if let Some(v) = input.block_chat {
                barrier.block_chat = v;
            }
            if let Some(v) = input.block_call {
                barrier.block_call = v;
            }
            if let Some(v) = input.enabled {
                barrier.enabled = v;
            }
            Some(barrier.clone())
        });
        if let Some(barrier) = &updated {
            self.persist(barrier);
        }
        updated
    }

    pub fn delete_barrier(&self, id: Uuid) -> bool {
        if self.information_barriers.remove(&id).is_some() {
            self.delete_persisted(InformationBarrier::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    /// Check whether communication between two user URIs is blocked by a barrier.
    pub fn check_barrier(&self, user_a: &str, user_b: &str, is_call: bool) -> BarrierCheckResult {
        let barriers = self.information_barriers.values();
        for barrier in &barriers {
            if !barrier.enabled {
                continue;
            }
            let blocked_type = if is_call {
                barrier.block_call
            } else {
                barrier.block_chat
            };
            if !blocked_type {
                continue;
            }
            let a_in_seg1 = barrier.segment1_users.iter().any(|u| u == user_a);
            let a_in_seg2 = barrier.segment2_users.iter().any(|u| u == user_a);
            let b_in_seg1 = barrier.segment1_users.iter().any(|u| u == user_b);
            let b_in_seg2 = barrier.segment2_users.iter().any(|u| u == user_b);
            if (a_in_seg1 && b_in_seg2) || (a_in_seg2 && b_in_seg1) {
                return BarrierCheckResult {
                    blocked: true,
                    barrier_id: Some(barrier.id),
                    barrier_name: Some(barrier.name.clone()),
                };
            }
        }
        BarrierCheckResult {
            blocked: false,
            barrier_id: None,
            barrier_name: None,
        }
    }

    // ── Sensitivity Labels ────────────────────────────────────────

    pub fn list_labels(&self) -> Vec<SensitivityLabel> {
        let mut labels = self.sensitivity_labels.values();
        labels.sort_by(|a, b| b.priority.cmp(&a.priority));
        labels
    }

    pub fn create_label(&self, input: CreateSensitivityLabelRequest) -> SensitivityLabel {
        let label = SensitivityLabel {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description,
            color: input.color,
            priority: input.priority,
            encrypt_content: input.encrypt_content,
            restrict_sharing: input.restrict_sharing,
            watermark: input.watermark,
            created_at: Utc::now(),
        };
        self.sensitivity_labels.insert(label.id, label.clone());
        self.persist(&label);
        label
    }

    pub fn update_label(
        &self,
        id: Uuid,
        input: UpdateSensitivityLabelRequest,
    ) -> Option<SensitivityLabel> {
        let updated = self.sensitivity_labels.with_write(&id, |labels| {
            let label = labels.get_mut(&id)?;
            if let Some(name) = input.name {
                label.name = name;
            }
            if let Some(desc) = input.description {
                label.description = desc;
            }
            if let Some(color) = input.color {
                label.color = color;
            }
            if let Some(priority) = input.priority {
                label.priority = priority;
            }
            if let Some(v) = input.encrypt_content {
                label.encrypt_content = v;
            }
            if let Some(v) = input.restrict_sharing {
                label.restrict_sharing = v;
            }
            if let Some(v) = input.watermark {
                label.watermark = v;
            }
            Some(label.clone())
        });
        if let Some(label) = &updated {
            self.persist(label);
        }
        updated
    }

    pub fn delete_label(&self, id: Uuid) -> bool {
        if self.sensitivity_labels.remove(&id).is_some() {
            self.delete_persisted(SensitivityLabel::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    // ── Custom RBAC Roles ─────────────────────────────────────────

    pub fn list_custom_roles(&self) -> Vec<CustomRole> {
        self.custom_roles.values()
    }

    pub fn create_custom_role(&self, input: CreateCustomRoleRequest) -> Result<CustomRole, String> {
        // Validate permissions
        let valid = permissions::all();
        for perm in &input.permissions {
            if !valid.contains(&perm.as_str()) {
                return Err(format!("unknown permission: {}", perm));
            }
        }
        let role = CustomRole {
            id: Uuid::new_v4(),
            name: input.name,
            permissions: input.permissions,
            created_at: Utc::now(),
        };
        self.custom_roles.insert(role.id, role.clone());
        self.persist(&role);
        Ok(role)
    }

    pub fn update_custom_role(
        &self,
        id: Uuid,
        input: UpdateCustomRoleRequest,
    ) -> Result<Option<CustomRole>, String> {
        if let Some(perms) = &input.permissions {
            let valid = permissions::all();
            for perm in perms {
                if !valid.contains(&perm.as_str()) {
                    return Err(format!("unknown permission: {}", perm));
                }
            }
        }
        let updated = self.custom_roles.with_write(&id, |roles| {
            let role = roles.get_mut(&id)?;
            if let Some(name) = input.name {
                role.name = name;
            }
            if let Some(perms) = input.permissions {
                role.permissions = perms;
            }
            Some(role.clone())
        });
        if let Some(role) = &updated {
            self.persist(role);
        }
        Ok(updated)
    }

    pub fn delete_custom_role(&self, id: Uuid) -> bool {
        if self.custom_roles.remove(&id).is_some() {
            self.delete_persisted(CustomRole::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    // ── Policy Packages ───────────────────────────────────────────

    pub fn list_policy_packages(&self) -> Vec<PolicyPackage> {
        self.policy_packages.values()
    }

    pub fn create_policy_package(&self, input: CreatePolicyPackageRequest) -> PolicyPackage {
        let pkg = PolicyPackage {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description,
            policies: input.policies,
            created_at: Utc::now(),
        };
        self.policy_packages.insert(pkg.id, pkg.clone());
        self.persist(&pkg);
        pkg
    }

    pub fn update_policy_package(
        &self,
        id: Uuid,
        input: UpdatePolicyPackageRequest,
    ) -> Option<PolicyPackage> {
        let updated = self.policy_packages.with_write(&id, |packages| {
            let pkg = packages.get_mut(&id)?;
            if let Some(name) = input.name {
                pkg.name = name;
            }
            if let Some(desc) = input.description {
                pkg.description = desc;
            }
            if let Some(policies) = input.policies {
                pkg.policies = policies;
            }
            Some(pkg.clone())
        });
        if let Some(pkg) = &updated {
            self.persist(pkg);
        }
        updated
    }

    pub fn delete_policy_package(&self, id: Uuid) -> bool {
        if self.policy_packages.remove(&id).is_some() {
            self.delete_persisted(PolicyPackage::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    // ── Bulk User Operations ──────────────────────────────────────

    pub fn export_users_csv(&self) -> String {
        let users = self.users.values();
        let mut csv = "id,display_name,sip_uri,role,active,created_at\n".to_string();
        for user in &users {
            csv.push_str(&format!(
                "{},{},{},{},{},{}\n",
                user.id,
                csv_escape_field(&user.display_name),
                csv_escape_field(&user.sip_uri),
                user.role,
                user.active,
                user.created_at.to_rfc3339(),
            ));
        }
        csv
    }

    pub fn import_users_csv(&self, csv_data: &str) -> BulkImportResult {
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        let lines: Vec<&str> = csv_data.lines().collect();
        if lines.is_empty() {
            return BulkImportResult {
                imported,
                skipped,
                errors,
            };
        }
        // Skip header
        for (i, line) in lines.iter().enumerate().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 3 {
                errors.push(format!("line {}: not enough fields", i + 1));
                continue;
            }
            let display_name = fields[0].trim().trim_matches('"').to_string();
            let sip_uri = fields[1].trim().trim_matches('"').to_string();
            let password = if fields.len() > 2 && !fields[2].trim().is_empty() {
                Some(fields[2].trim().trim_matches('"').to_string())
            } else {
                None
            };
            let role = if fields.len() > 3 && !fields[3].trim().is_empty() {
                Some(fields[3].trim().trim_matches('"').to_string())
            } else {
                None
            };
            if self.user_by_sip_uri(&sip_uri).is_some() {
                skipped += 1;
                continue;
            }
            match self.create_user(CreateUserRequest {
                display_name,
                sip_uri,
                matrix_user_id: None,
                password,
                role,
            }) {
                Ok(_) => imported += 1,
                Err(err) => errors.push(format!("line {}: {}", i + 1, err)),
            }
        }
        BulkImportResult {
            imported,
            skipped,
            errors,
        }
    }

    // ── Usage Analytics ───────────────────────────────────────────

    pub fn usage_analytics(&self) -> UsageAnalytics {
        let users = self.users.values();
        let active_users = users.iter().filter(|u| u.active).count();
        let total_messages = self.room_messages.read().expect("room messages lock").len();
        let total_calls = self.calls.len();
        let total_meetings = self.scheduled_meetings.len();
        let files = self.files.values();
        let total_storage: u64 = files.iter().map(|f| f.size).sum();
        let online_users = self
            .presence
            .values()
            .iter()
            .filter(|p| p.status != PresenceStatus::Offline)
            .count();

        UsageAnalytics {
            total_users: users.len(),
            active_users,
            total_messages,
            total_calls,
            total_meetings,
            total_files: files.len(),
            total_storage_bytes: total_storage,
            online_users,
        }
    }

    pub fn security_posture_report(&self) -> SecurityPostureReport {
        let users = self.users();
        let active_users: Vec<_> = users.iter().filter(|user| user.active).collect();
        let active_user_count = active_users.len();
        let mfa_enabled_users = active_users
            .iter()
            .filter(|user| self.is_mfa_enabled(user.id))
            .count();
        let sso_providers = self.list_sso_providers();
        let enabled_sso_providers = sso_providers
            .iter()
            .filter(|provider| provider.enabled)
            .count();
        let conditional_access = self.list_conditional_access_policies();
        let enabled_conditional_access_policies = conditional_access
            .iter()
            .filter(|policy| policy.enabled)
            .count();
        let mfa_required_by_policy = conditional_access
            .iter()
            .any(|policy| policy.enabled && policy.actions.require_mfa);
        let dlp_policies = self.list_dlp_policies();
        let enabled_dlp_policies = dlp_policies.iter().filter(|policy| policy.enabled).count();
        let retention_policies = self.retention_policies();
        let legal_hold_policies = retention_policies
            .iter()
            .filter(|policy| policy.legal_hold)
            .count();
        let information_barriers = self.list_barriers();
        let enabled_information_barriers = information_barriers
            .iter()
            .filter(|barrier| barrier.enabled)
            .count();
        let sensitivity_labels = self.list_labels().len();
        let encryption_keys = self
            .encryption_configs
            .read()
            .expect("encryption_configs lock")
            .len();
        let enabled_data_residency_regions = self
            .list_data_residency_configs()
            .iter()
            .filter(|config| config.enabled)
            .count();
        let audit_events = self.audit_events().len();
        let pending_compliance_reviews = self
            .list_compliance_reviews()
            .iter()
            .filter(|review| review.status == "pending")
            .count();

        let mfa_score = if mfa_required_by_policy {
            15
        } else if active_user_count == 0 {
            0
        } else {
            ((mfa_enabled_users as f64 / active_user_count as f64) * 15.0).round() as u32
        };

        let mut controls = Vec::new();
        push_security_control(
            &mut controls,
            "identity.sso",
            "Identity",
            "Single sign-on provider",
            enabled_sso_providers > 0,
            if enabled_sso_providers > 0 { 10 } else { 0 },
            10,
            format!("{enabled_sso_providers} enabled SSO provider(s)"),
            "Configure at least one enabled OIDC/SAML provider for centralized identity.",
        );
        push_security_control(
            &mut controls,
            "identity.mfa",
            "Identity",
            "MFA enforcement",
            mfa_score == 15,
            mfa_score,
            15,
            if mfa_required_by_policy {
                "Conditional access requires MFA".to_string()
            } else {
                format!("{mfa_enabled_users}/{active_user_count} active users have MFA enabled")
            },
            "Enable MFA for all active users or enforce MFA through conditional access.",
        );
        push_security_control(
            &mut controls,
            "access.conditional",
            "Access",
            "Conditional access",
            enabled_conditional_access_policies > 0,
            if enabled_conditional_access_policies > 0 {
                10
            } else {
                0
            },
            10,
            format!("{enabled_conditional_access_policies} enabled conditional access policy(ies)"),
            "Create enabled policies for MFA, risky networks, device posture, or blocked contexts.",
        );
        push_security_control(
            &mut controls,
            "protection.dlp",
            "Information Protection",
            "Data loss prevention",
            enabled_dlp_policies > 0,
            if enabled_dlp_policies > 0 { 10 } else { 0 },
            10,
            format!("{enabled_dlp_policies} enabled DLP policy(ies)"),
            "Enable DLP policies that block or warn on regulated data in messages and files.",
        );
        push_security_control(
            &mut controls,
            "compliance.retention",
            "Compliance",
            "Retention policies",
            !retention_policies.is_empty(),
            if retention_policies.is_empty() { 0 } else { 10 },
            10,
            format!("{} retention policy(ies)", retention_policies.len()),
            "Define retention policies for chats, channels, meetings, files, and recordings.",
        );
        push_security_control(
            &mut controls,
            "compliance.legal_hold",
            "Compliance",
            "Legal hold readiness",
            legal_hold_policies > 0,
            if legal_hold_policies > 0 { 8 } else { 0 },
            8,
            format!("{legal_hold_policies} legal hold policy(ies)"),
            "Create at least one legal-hold policy for litigation preservation workflows.",
        );
        push_security_control(
            &mut controls,
            "compliance.reviews",
            "Compliance",
            "Communication compliance reviews",
            pending_compliance_reviews == 0,
            if pending_compliance_reviews == 0 {
                7
            } else {
                3
            },
            7,
            format!("{pending_compliance_reviews} pending compliance review(s)"),
            "Review or resolve pending communication compliance items.",
        );
        push_security_control(
            &mut controls,
            "barriers.enabled",
            "Information Protection",
            "Information barriers",
            enabled_information_barriers > 0,
            if enabled_information_barriers > 0 {
                8
            } else {
                0
            },
            8,
            format!("{enabled_information_barriers} enabled barrier(s)"),
            "Configure barriers for departments or regulated groups that must not communicate.",
        );
        push_security_control(
            &mut controls,
            "labels.sensitivity",
            "Information Protection",
            "Sensitivity labels",
            sensitivity_labels > 0,
            if sensitivity_labels > 0 { 7 } else { 0 },
            7,
            format!("{sensitivity_labels} sensitivity label(s)"),
            "Create labels for privacy, guest access, encryption, and sharing restrictions.",
        );
        push_security_control(
            &mut controls,
            "encryption.byok",
            "Encryption",
            "Encryption key rotation",
            encryption_keys > 0,
            if encryption_keys > 0 { 7 } else { 0 },
            7,
            format!("{encryption_keys} configured encryption key(s)"),
            "Rotate the service encryption key and use customer-provided keys where required.",
        );
        push_security_control(
            &mut controls,
            "residency.enabled",
            "Data Governance",
            "Data residency",
            enabled_data_residency_regions > 0,
            if enabled_data_residency_regions > 0 {
                5
            } else {
                0
            },
            5,
            format!("{enabled_data_residency_regions} enabled residency region(s)"),
            "Enable data residency configurations for regulated tenant regions.",
        );
        push_security_control(
            &mut controls,
            "audit.activity",
            "Audit",
            "Audit activity",
            audit_events > 0,
            if audit_events > 0 { 3 } else { 0 },
            3,
            format!("{audit_events} audit event(s) retained"),
            "Confirm admin actions are generating and retaining audit events.",
        );

        let score: u32 = controls.iter().map(|control| control.score).sum();
        let max_score: u32 = controls.iter().map(|control| control.max_score).sum();
        let posture = if score * 100 >= max_score * 85 {
            "strong"
        } else if score * 100 >= max_score * 60 {
            "moderate"
        } else {
            "needs_attention"
        }
        .to_string();
        let mut recommendations: Vec<_> = controls
            .iter()
            .filter(|control| control.score < control.max_score)
            .map(|control| SecurityPostureRecommendation {
                control_id: control.id.clone(),
                priority: if control.score == 0 {
                    "high".to_string()
                } else {
                    "medium".to_string()
                },
                title: control.title.clone(),
                action: control.remediation.clone(),
            })
            .collect();
        recommendations.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.title.cmp(&b.title)));

        SecurityPostureReport {
            score,
            max_score,
            posture,
            generated_at: Utc::now(),
            controls,
            recommendations,
            counts: SecurityPostureCounts {
                active_users: active_user_count,
                mfa_enabled_users,
                enabled_sso_providers,
                enabled_conditional_access_policies,
                enabled_dlp_policies,
                retention_policies: retention_policies.len(),
                legal_hold_policies,
                enabled_information_barriers,
                sensitivity_labels,
                encryption_keys,
                enabled_data_residency_regions,
                audit_events,
                pending_compliance_reviews,
            },
        }
    }

    pub fn create_call(&self, input: CreateCallRequest) -> Result<CallSession, String> {
        self.authorize_call_policy(&input)?;
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
        // Push notifications to callees for incoming calls
        for callee in &call.callees {
            self.notify_user_push(
                callee,
                &format!("Incoming call from {}", sip_user_part(&call.caller)),
                "Tap to answer",
                Some(format!("call-{}", call.id)),
            );
        }
        Ok(call)
    }

    fn authorize_call_policy(&self, input: &CreateCallRequest) -> Result<(), String> {
        let settings = self.get_user_call_settings(&input.caller);
        if input.conference_id.is_none() && !settings.allow_private_calls {
            return Err("private calls are disabled by policy".to_string());
        }
        if !settings.allow_external_calls
            && input
                .callees
                .iter()
                .any(|callee| is_external_call_target(&input.caller, callee))
        {
            return Err("external calling is disabled by policy".to_string());
        }
        Ok(())
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
        let file_for_pg = record.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_file(&file_for_pg).await }));
    }

    pub fn delete_file_record(&self, id: Uuid) -> Option<FileRecord> {
        let record = self.files.remove(&id);
        if record.is_some() {
            self.delete_persisted(FileRecord::collection(), id.to_string());
        }
        self.pg_spawn(move |pg| Box::pin(async move { pg.delete_file(id).await }));
        record
    }

    pub fn mark_file_deleted(&self, id: Uuid, deleted_by: &str) -> Option<FileRecord> {
        let record = self.files.with_write(&id, |files| {
            let file = files.get_mut(&id)?;
            file.deleted_at = Some(Utc::now());
            file.deleted_by = Some(deleted_by.to_string());
            Some(file.clone())
        })?;
        self.persist(&record);
        let file_for_pg = record.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_file(&file_for_pg).await }));
        Some(record)
    }

    pub fn file_record(&self, id: Uuid) -> Option<FileRecord> {
        self.files.get(&id)
    }

    pub fn file_records(&self) -> Vec<FileRecord> {
        self.files
            .values()
            .into_iter()
            .filter(|file| file.deleted_at.is_none())
            .collect()
    }

    pub fn cloud_storage_status(&self) -> CloudStorageStatus {
        let integration = self
            .list_enterprise_integrations()
            .into_iter()
            .find(|integration| integration.id == "cloud_storage");
        let provider_configured = self.enterprise_capability_available("cloud_storage");
        let files = self.file_records();
        let total_storage_bytes = files.iter().map(|file| file.size).sum();
        let mut warnings = Vec::new();
        if !provider_configured {
            warnings.push(
                "cloud_storage integration is not available; files are retained in local server storage"
                    .to_string(),
            );
        }
        if integration
            .as_ref()
            .and_then(|item| item.endpoint_url.as_ref())
            .is_none()
        {
            warnings.push(
                "configure a WebDAV or S3-compatible endpoint such as Nextcloud, ownCloud, or MinIO"
                    .to_string(),
            );
        }
        CloudStorageStatus {
            provider_configured,
            provider_name: integration
                .as_ref()
                .map(|item| item.default_provider.clone())
                .unwrap_or_else(|| "WebDAV/S3-compatible storage".to_string()),
            open_source_options: integration
                .as_ref()
                .map(|item| {
                    item.open_source_option
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        "Nextcloud".to_string(),
                        "ownCloud".to_string(),
                        "MinIO".to_string(),
                    ]
                }),
            endpoint_url: integration
                .as_ref()
                .and_then(|item| item.endpoint_url.clone()),
            admin_url: integration.as_ref().and_then(|item| item.admin_url.clone()),
            sync_mode: if provider_configured {
                "external_provider_ready".to_string()
            } else {
                "local_server_storage".to_string()
            },
            local_file_count: files.len(),
            total_storage_bytes,
            warnings,
        }
    }

    pub fn discovery_file_records(&self) -> Vec<FileDiscoveryRecord> {
        self.files
            .values()
            .into_iter()
            .map(|file| FileDiscoveryRecord {
                id: file.id,
                owner: file.owner,
                filename: file.filename,
                content_type: file.content_type,
                size: file.size,
                sha256: file.sha256,
                created_at: file.created_at,
                dlp_status: file.dlp_status,
                dlp_violation_count: file.dlp_violation_count,
                legal_hold: file.legal_hold,
                deleted_at: file.deleted_at,
                deleted_by: file.deleted_by,
            })
            .collect()
    }

    pub async fn file_governance_for_upload(
        &self,
        owner: &str,
        filename: &str,
        content_type: &str,
        body: &[u8],
    ) -> FileGovernanceDecision {
        let scan_content = if is_textual_content(content_type) {
            format!("{}\n{}", filename, String::from_utf8_lossy(body))
        } else {
            filename.to_string()
        };
        let dlp = self.scan_content_dlp(owner, &scan_content);
        let malware_detected = if self.advanced_threat_protection_available() {
            malware_scan_async(filename, content_type, body).await
        } else {
            false
        };
        FileGovernanceDecision {
            allowed: dlp.allowed && !malware_detected,
            dlp_status: if malware_detected {
                "malware_blocked"
            } else if dlp.allowed {
                "clean"
            } else {
                "blocked"
            }
            .to_string(),
            dlp_violation_count: dlp.violations.len() + usize::from(malware_detected),
            legal_hold: self.file_on_legal_hold(),
        }
    }

    pub fn advanced_threat_protection_available(&self) -> bool {
        self.enterprise_capability_report()
            .capabilities
            .into_iter()
            .any(|capability| {
                capability.id == "advanced_threat_protection" && capability.status == "available"
            })
    }

    pub fn quarantine_malware_upload(
        &self,
        owner: &str,
        filename: &str,
        content_type: &str,
        size: u64,
        sha256: &str,
        reason: &str,
    ) -> MalwareQuarantineItem {
        let item = MalwareQuarantineItem {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size,
            sha256: sha256.to_string(),
            reason: reason.to_string(),
            status: MalwareQuarantineStatus::Quarantined,
            detected_at: Utc::now(),
            reviewed_by: None,
            reviewed_at: None,
            review_notes: None,
        };
        self.malware_quarantine.insert(item.id, item.clone());
        self.persist(&item);
        item
    }

    pub fn list_malware_quarantine(&self) -> Vec<MalwareQuarantineItem> {
        let mut items = self.malware_quarantine.values();
        items.sort_by(|left, right| right.detected_at.cmp(&left.detected_at));
        items
    }

    pub fn review_malware_quarantine(
        &self,
        id: Uuid,
        reviewer: &str,
        input: ReviewMalwareQuarantineRequest,
    ) -> Option<MalwareQuarantineItem> {
        let mut item = self.malware_quarantine.get(&id)?;
        item.status = input.status;
        item.reviewed_by = Some(reviewer.to_string());
        item.reviewed_at = Some(Utc::now());
        item.review_notes = input.notes.and_then(non_empty_string);
        self.malware_quarantine.insert(id, item.clone());
        self.persist(&item);
        Some(item)
    }

    pub fn casb_available(&self) -> bool {
        self.enterprise_capability_available("casb")
    }

    pub fn casb_file_access_decision(
        &self,
        _actor: &str,
        action: &str,
        file: &FileRecord,
    ) -> CasbAccessDecision {
        if !self.casb_available() {
            return CasbAccessDecision {
                allowed: true,
                enforced: false,
                reason: "casb_not_configured".to_string(),
            };
        }
        let dlp_status = file.dlp_status.to_ascii_lowercase();
        if matches!(action, "download" | "share" | "export")
            && matches!(dlp_status.as_str(), "blocked" | "malware_blocked")
        {
            return CasbAccessDecision {
                allowed: false,
                enforced: true,
                reason: format!("file_{}", dlp_status),
            };
        }
        CasbAccessDecision {
            allowed: true,
            enforced: true,
            reason: "allowed".to_string(),
        }
    }

    pub fn file_on_legal_hold(&self) -> bool {
        self.retention_policies().into_iter().any(|policy| {
            policy.legal_hold && matches!(policy.scope.as_str(), "global" | "files" | "file")
        })
    }

    // ─── File Versioning ───

    pub fn add_file_version(&self, version: FileVersion) {
        let mut versions = self.file_versions.write().expect("file_versions lock");
        versions.push(version);
    }

    pub fn file_versions(&self, file_id: Uuid) -> Vec<FileVersion> {
        let versions = self.file_versions.read().expect("file_versions lock");
        let mut result: Vec<_> = versions
            .iter()
            .filter(|v| v.file_id == file_id)
            .cloned()
            .collect();
        result.sort_by_key(|v| v.version_number);
        result
    }

    pub fn file_version_path(&self, version_id: Uuid) -> PathBuf {
        self.files_dir().join(format!("version_{}", version_id))
    }

    // ─── Folders ───

    pub fn put_folder(&self, folder: Folder) {
        self.folders.insert(folder.id, folder);
    }

    pub fn folder(&self, id: Uuid) -> Option<Folder> {
        self.folders.get(&id)
    }

    pub fn folders_for_room(&self, room_id: Uuid, parent_id: Option<Uuid>) -> Vec<Folder> {
        self.folders
            .values()
            .into_iter()
            .filter(|f| f.room_id == room_id && f.parent_id == parent_id)
            .collect()
    }

    pub fn delete_folder(&self, id: Uuid) -> Option<Folder> {
        self.folders.remove(&id)
    }

    // ─── File Lock ───

    pub fn lock_file(&self, id: Uuid, user: &str) -> Option<FileRecord> {
        self.files.with_write(&id, |files| {
            let file = files.get_mut(&id)?;
            if file.locked_by.is_some() {
                return None; // already locked
            }
            file.locked_by = Some(user.to_string());
            file.locked_at = Some(Utc::now());
            let record = file.clone();
            Some(record)
        })
    }

    pub fn unlock_file(&self, id: Uuid, user: &str) -> Option<FileRecord> {
        self.files.with_write(&id, |files| {
            let file = files.get_mut(&id)?;
            if file.locked_by.as_deref() != Some(user) {
                return None; // not locked by this user
            }
            file.locked_by = None;
            file.locked_at = None;
            let record = file.clone();
            Some(record)
        })
    }

    // ─── Approvals ───

    pub fn put_approval(&self, approval: ApprovalRequest) {
        self.approval_requests.insert(approval.id, approval);
    }

    pub fn approval(&self, id: Uuid) -> Option<ApprovalRequest> {
        self.approval_requests.get(&id)
    }

    pub fn approvals(&self) -> Vec<ApprovalRequest> {
        let mut list = self.approval_requests.values();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    pub fn update_approval(
        &self,
        id: Uuid,
        updater: impl FnOnce(&mut ApprovalRequest),
    ) -> Option<ApprovalRequest> {
        self.approval_requests.with_write(&id, |map| {
            let approval = map.get_mut(&id)?;
            updater(approval);
            Some(approval.clone())
        })
    }

    // ─── Recording Policies ───

    pub fn put_recording_policy(&self, policy: RecordingPolicy) {
        self.recording_policies.insert(policy.id, policy);
    }

    pub fn recording_policy(&self, id: Uuid) -> Option<RecordingPolicy> {
        self.recording_policies.get(&id)
    }

    pub fn recording_policies_list(&self) -> Vec<RecordingPolicy> {
        self.recording_policies.values()
    }

    pub fn delete_recording_policy(&self, id: Uuid) -> Option<RecordingPolicy> {
        self.recording_policies.remove(&id)
    }

    /// Check if a call should be auto-recorded based on policies.
    pub fn should_auto_record(&self, caller_uri: &str, callee_uri: &str) -> bool {
        for policy in self.recording_policies_list() {
            if !policy.enabled {
                continue;
            }
            match policy.trigger.as_str() {
                "all_calls" => return true,
                "all_external" => {
                    // External if callee doesn't match any registered account
                    let callee_user = sip_user_part(callee_uri);
                    let is_internal = self
                        .sip_accounts
                        .values()
                        .iter()
                        .any(|a| a.username == callee_user);
                    if !is_internal {
                        return true;
                    }
                }
                "specific_users" => {
                    let caller_user = sip_user_part(caller_uri);
                    let callee_user = sip_user_part(callee_uri);
                    if policy
                        .target_ids
                        .iter()
                        .any(|t| t == caller_user || t == callee_user)
                    {
                        return true;
                    }
                }
                "specific_queues" => {
                    // Check if callee is in a targeted queue
                    if policy
                        .target_ids
                        .iter()
                        .any(|t| callee_uri.contains(t.as_str()))
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    // ─── Hold Music ───

    pub fn put_hold_music(&self, music: HoldMusic) {
        self.hold_music.insert(music.id, music);
    }

    pub fn hold_music_list(&self) -> Vec<HoldMusic> {
        self.hold_music.values()
    }

    pub fn delete_hold_music(&self, id: Uuid) -> Option<HoldMusic> {
        self.hold_music.remove(&id)
    }

    // ─── Personal Call Groups ───

    pub fn put_personal_call_group(&self, group: PersonalCallGroup) {
        self.personal_call_groups.insert(group.id, group);
    }

    pub fn personal_call_groups_for_user(&self, user_id: &str) -> Vec<PersonalCallGroup> {
        self.personal_call_groups
            .values()
            .into_iter()
            .filter(|g| g.user_id == user_id)
            .collect()
    }

    pub fn personal_call_group(&self, id: Uuid) -> Option<PersonalCallGroup> {
        self.personal_call_groups.get(&id)
    }

    pub fn delete_personal_call_group(&self, id: Uuid) -> Option<PersonalCallGroup> {
        self.personal_call_groups.remove(&id)
    }

    // ─── SSO Providers ───

    pub fn list_sso_providers(&self) -> Vec<SsoProvider> {
        self.sso_providers.values()
    }

    pub fn create_sso_provider(&self, input: CreateSsoProviderRequest) -> SsoProvider {
        let provider = SsoProvider {
            id: Uuid::new_v4(),
            name: input.name,
            provider_type: input.provider_type,
            client_id: input.client_id,
            client_secret_enc: input.client_secret,
            issuer_url: input.issuer_url,
            redirect_uri: input.redirect_uri,
            enabled: input.enabled,
            created_at: Utc::now(),
        };
        self.sso_providers.insert(provider.id, provider.clone());
        self.persist_pg_sso_provider(&provider);
        provider
    }

    pub fn update_sso_provider(
        &self,
        id: Uuid,
        input: UpdateSsoProviderRequest,
    ) -> Option<SsoProvider> {
        let updated = self.sso_providers.with_write(&id, |providers| {
            let provider = providers.get_mut(&id)?;
            if let Some(name) = input.name {
                provider.name = name;
            }
            if let Some(pt) = input.provider_type {
                provider.provider_type = pt;
            }
            if let Some(cid) = input.client_id {
                provider.client_id = cid;
            }
            if let Some(cs) = input.client_secret {
                provider.client_secret_enc = cs;
            }
            if let Some(iu) = input.issuer_url {
                provider.issuer_url = iu;
            }
            if let Some(ru) = input.redirect_uri {
                provider.redirect_uri = ru;
            }
            if let Some(en) = input.enabled {
                provider.enabled = en;
            }
            Some(provider.clone())
        });
        if let Some(ref p) = updated {
            self.persist_pg_sso_provider(p);
        }
        updated
    }

    pub fn delete_sso_provider(&self, id: Uuid) -> bool {
        let removed = self.sso_providers.remove(&id).is_some();
        if removed {
            if let Some(pg) = &self.pg {
                let pg = pg.clone();
                tokio::spawn(async move {
                    let _ = pg.delete_sso_provider(id).await;
                });
            }
        }
        removed
    }

    pub fn get_sso_provider(&self, id: Uuid) -> Option<SsoProvider> {
        self.sso_providers.get(&id)
    }

    fn persist_pg_sso_provider(&self, p: &SsoProvider) {
        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let p = p.clone();
            tokio::spawn(async move {
                let _ = pg.upsert_sso_provider(&p).await;
            });
        }
    }

    /// Build OIDC authorization URL with state and nonce parameters.
    /// The state and nonce are stored server-side so the callback can verify them.
    pub fn sso_login_url(&self, provider_id: Uuid) -> Option<(String, String, String)> {
        let provider = self.sso_providers.get(&provider_id)?;
        if !provider.enabled {
            return None;
        }
        let state = Uuid::new_v4().to_string();
        let nonce = Uuid::new_v4().to_string();

        // Store state -> (provider_id, nonce) for callback verification
        self.sso_pending_states.insert(
            state.clone(),
            SsoPendingState {
                provider_id,
                nonce: nonce.clone(),
                created_at: Utc::now(),
            },
        );
        // Prune expired pending states (older than 10 minutes)
        let cutoff = Utc::now() - Duration::minutes(10);
        self.sso_pending_states
            .retain(|_k, v| v.created_at > cutoff);

        // Use discovered authorization_endpoint if cached, else fall back to
        // the provider issuer_url with /authorize appended.
        let auth_endpoint = self
            .oidc_discovery_cache
            .get(&provider.issuer_url)
            .map(|d| d.authorization_endpoint.clone())
            .unwrap_or_else(|| format!("{}/authorize", provider.issuer_url.trim_end_matches('/')));

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&nonce={}",
            auth_endpoint,
            urlencoding::encode(&provider.client_id),
            urlencoding::encode(&provider.redirect_uri),
            urlencoding::encode(&state),
            urlencoding::encode(&nonce),
        );
        Some((url, state, nonce))
    }

    /// Consume a pending SSO state, returning the stored provider_id and nonce.
    /// Returns `None` if the state is unknown or expired (>10 min).
    pub fn consume_sso_state(&self, state: &str) -> Option<SsoPendingState> {
        let pending = self.sso_pending_states.remove(&state.to_string())?;
        let age = Utc::now() - pending.created_at;
        if age > Duration::minutes(10) {
            return None;
        }
        Some(pending)
    }

    /// Fetch and cache OIDC discovery document from the provider's well-known endpoint.
    pub async fn discover_oidc_config(&self, issuer_url: &str) -> Result<OidcDiscovery, String> {
        // Return cached if fresh (< 1 hour)
        if let Some(cached) = self.oidc_discovery_cache.get(&issuer_url.to_string()) {
            let age = Utc::now() - cached.fetched_at;
            if age < Duration::hours(1) {
                return Ok(cached);
            }
        }

        let well_known_url = format!(
            "{}/.well-known/openid-configuration",
            issuer_url.trim_end_matches('/')
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let resp = client
            .get(&well_known_url)
            .send()
            .await
            .map_err(|e| format!("OIDC discovery fetch failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("OIDC discovery returned status {}", resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("OIDC discovery parse failed: {e}"))?;

        let discovery = OidcDiscovery {
            issuer: body["issuer"].as_str().unwrap_or(issuer_url).to_string(),
            authorization_endpoint: body["authorization_endpoint"]
                .as_str()
                .ok_or("missing authorization_endpoint")?
                .to_string(),
            token_endpoint: body["token_endpoint"]
                .as_str()
                .ok_or("missing token_endpoint")?
                .to_string(),
            jwks_uri: body["jwks_uri"]
                .as_str()
                .ok_or("missing jwks_uri")?
                .to_string(),
            userinfo_endpoint: body["userinfo_endpoint"].as_str().map(|s| s.to_string()),
            fetched_at: Utc::now(),
        };

        self.oidc_discovery_cache
            .insert(issuer_url.to_string(), discovery.clone());
        Ok(discovery)
    }

    /// Fetch the JWKS from the provider and build a `DecodingKey` for the given
    /// key ID (`kid`). Returns the key and the algorithm.
    pub async fn fetch_jwks_key(
        &self,
        jwks_uri: &str,
        kid: &str,
    ) -> Result<(jsonwebtoken::DecodingKey, jsonwebtoken::Algorithm), String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let resp = client
            .get(jwks_uri)
            .send()
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("JWKS returned status {}", resp.status()));
        }
        let jwks: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JWKS parse failed: {e}"))?;

        let keys = jwks["keys"].as_array().ok_or("JWKS missing keys array")?;

        let jwk = keys
            .iter()
            .find(|k| k["kid"].as_str() == Some(kid))
            .ok_or_else(|| format!("No JWKS key found for kid={kid}"))?;

        let alg_str = jwk["alg"].as_str().unwrap_or("RS256");
        let algorithm = match alg_str {
            "RS256" => jsonwebtoken::Algorithm::RS256,
            "RS384" => jsonwebtoken::Algorithm::RS384,
            "RS512" => jsonwebtoken::Algorithm::RS512,
            "ES256" => jsonwebtoken::Algorithm::ES256,
            "ES384" => jsonwebtoken::Algorithm::ES384,
            other => return Err(format!("Unsupported JWKS algorithm: {other}")),
        };

        // Build the decoding key from the JWK components
        let kty = jwk["kty"].as_str().unwrap_or("");
        let decoding_key = match kty {
            "RSA" => {
                let n = jwk["n"].as_str().ok_or("RSA JWK missing 'n'")?;
                let e = jwk["e"].as_str().ok_or("RSA JWK missing 'e'")?;
                jsonwebtoken::DecodingKey::from_rsa_components(n, e)
                    .map_err(|e| format!("Failed to build RSA key: {e}"))?
            }
            "EC" => {
                let x = jwk["x"].as_str().ok_or("EC JWK missing 'x'")?;
                let y = jwk["y"].as_str().ok_or("EC JWK missing 'y'")?;
                jsonwebtoken::DecodingKey::from_ec_components(x, y)
                    .map_err(|e| format!("Failed to build EC key: {e}"))?
            }
            other => return Err(format!("Unsupported JWK key type: {other}")),
        };

        Ok((decoding_key, algorithm))
    }

    /// Authenticate a user via SSO/OIDC: exchange the authorization code for
    /// tokens, validate the ID token, and create a session.
    pub async fn sso_authenticate(
        &self,
        code: &str,
        state: &str,
        client_ip: &str,
    ) -> Result<UserLoginResponse, String> {
        // 1. Consume and verify the pending state
        let pending = self
            .consume_sso_state(state)
            .ok_or("Invalid or expired SSO state")?;

        let provider = self
            .sso_providers
            .get(&pending.provider_id)
            .ok_or("SSO provider not found")?;
        if !provider.enabled {
            return Err("SSO provider is disabled".into());
        }

        // 2. Discover OIDC endpoints
        let discovery = self.discover_oidc_config(&provider.issuer_url).await?;

        // 3. Exchange the authorization code for tokens
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let token_resp = client
            .post(&discovery.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &provider.redirect_uri),
                ("client_id", &provider.client_id),
                ("client_secret", &provider.client_secret_enc),
            ])
            .send()
            .await
            .map_err(|e| format!("Token exchange failed: {e}"))?;

        if !token_resp.status().is_success() {
            let status = token_resp.status();
            let body = token_resp.text().await.unwrap_or_default();
            return Err(format!("Token endpoint returned {status}: {body}"));
        }

        let token_body: serde_json::Value = token_resp
            .json()
            .await
            .map_err(|e| format!("Token response parse failed: {e}"))?;

        let id_token_str = token_body["id_token"]
            .as_str()
            .ok_or("Token response missing id_token")?;

        // 4. Decode the JWT header to get the kid
        let jwt_header = jsonwebtoken::decode_header(id_token_str)
            .map_err(|e| format!("Failed to decode JWT header: {e}"))?;
        let kid = jwt_header.kid.ok_or("ID token JWT header missing kid")?;

        // 5. Fetch the signing key from JWKS and validate signature
        let (decoding_key, algorithm) = self.fetch_jwks_key(&discovery.jwks_uri, &kid).await?;

        let mut validation = jsonwebtoken::Validation::new(algorithm);
        validation.set_audience(&[&provider.client_id]);
        validation.set_issuer(&[&discovery.issuer]);
        validation.validate_exp = true;

        let token_data =
            jsonwebtoken::decode::<OidcIdTokenClaims>(id_token_str, &decoding_key, &validation)
                .map_err(|e| format!("ID token validation failed: {e}"))?;
        let claims = token_data.claims;

        // 6. Verify nonce
        if claims.nonce.as_deref() != Some(&pending.nonce) {
            return Err("ID token nonce mismatch".into());
        }

        // 7. Extract user identity (email or sub)
        let email = claims
            .email
            .as_deref()
            .or(claims.preferred_username.as_deref())
            .ok_or("ID token missing email and preferred_username")?;
        let display_name = claims.name.unwrap_or_else(|| email.to_string());

        // 8. Look up or auto-provision local user
        let sip_uri = format!("sip:{}@sso", email.split('@').next().unwrap_or(email));
        if !self.user_exists(&sip_uri) {
            let _ = self.create_user(CreateUserRequest {
                display_name: display_name.clone(),
                sip_uri: sip_uri.clone(),
                matrix_user_id: None,
                password: None,
                role: Some("user".to_string()),
            });
            log::info!("Auto-provisioned SSO user: {sip_uri} from {email}");
        }

        let user = self
            .user_by_sip_uri(&sip_uri)
            .ok_or("Failed to find or provision SSO user")?;
        if !user.active {
            return Err("User account is deactivated".into());
        }

        // 9. Evaluate conditional access
        let ca_result = self.evaluate_conditional_access(client_ip, "desktop", &[]);
        if ca_result.block {
            return Err("Blocked by conditional access policy".into());
        }

        // 10. Create session
        let session = AdminSession {
            token: Uuid::new_v4().to_string(),
            principal: user.sip_uri.clone(),
            role: user.role.clone(),
            expires_at: Utc::now() + Duration::hours(12),
        };
        self.admin_sessions
            .insert(session.token.clone(), session.clone());
        self.admin_sessions.trim_to_len(MAX_ADMIN_SESSIONS);

        // Track session
        self.track_session(user.id, &session.token, "SSO", "desktop", client_ip);

        self.update_presence(&user.sip_uri, PresenceStatus::Online, None);

        self.record_audit_event(&user.sip_uri, "user.sso_login", Some(provider.name.clone()));

        Ok(UserLoginResponse {
            token: session.token,
            user,
            sip_credentials: None,
            expires_at: session.expires_at,
            mfa_required: false,
        })
    }

    // ─── Encryption Config (BYOK) ───

    pub fn encryption_status(&self) -> serde_json::Value {
        let configs = self
            .encryption_configs
            .read()
            .expect("encryption_configs lock");
        let active = configs.first();
        serde_json::json!({
            "active": active.is_some(),
            "key_source": active.map(|c| c.key_source.as_str()).unwrap_or("server"),
            "key_id": active.map(|c| c.key_id.as_str()).unwrap_or(""),
            "rotated_at": active.and_then(|c| c.rotated_at.map(|t| t.to_rfc3339())),
            "total_keys": configs.len(),
        })
    }

    pub fn rotate_encryption_key(&self, input: RotateEncryptionKeyRequest) -> EncryptionConfig {
        let key_source = if input.customer_key_base64.is_some() {
            "customer"
        } else {
            "server"
        };
        let key_id = Uuid::new_v4().to_string();
        // In production: wrap the DEK with customer key or generate server key.
        // For now, generate a key ID and record the config.
        let wrapped = input.customer_key_base64.unwrap_or_else(|| {
            use base64::Engine;
            let mut key = [0u8; 32];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut key);
            base64::engine::general_purpose::STANDARD.encode(key)
        });

        let config = EncryptionConfig {
            id: Uuid::new_v4(),
            key_id,
            key_source: key_source.to_string(),
            wrapped_key_enc: wrapped,
            created_at: Utc::now(),
            rotated_at: Some(Utc::now()),
        };

        {
            let mut configs = self
                .encryption_configs
                .write()
                .expect("encryption_configs lock");
            configs.insert(0, config.clone());
        }

        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let c = config.clone();
            tokio::spawn(async move {
                let _ = pg.upsert_encryption_config(&c).await;
            });
        }

        config
    }

    // ─── Admin Elevations (PAM) ───

    pub fn list_admin_elevations(&self) -> Vec<AdminElevation> {
        let elevations = self.admin_elevations.read().expect("admin_elevations lock");
        elevations.clone()
    }

    pub fn active_admin_elevations(&self) -> Vec<AdminElevation> {
        let now = Utc::now();
        let elevations = self.admin_elevations.read().expect("admin_elevations lock");
        elevations
            .iter()
            .filter(|e| e.revoked_at.is_none() && e.expires_at > now)
            .cloned()
            .collect()
    }

    pub fn create_admin_elevation(
        &self,
        input: CreateAdminElevationRequest,
        granted_by: &str,
    ) -> AdminElevation {
        let duration_minutes = input.duration_minutes.unwrap_or(60);
        let elevation = AdminElevation {
            id: Uuid::new_v4(),
            user_id: input.user_id,
            reason: input.reason,
            granted_by: granted_by.to_string(),
            granted_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(duration_minutes),
            revoked_at: None,
        };

        {
            let mut elevations = self
                .admin_elevations
                .write()
                .expect("admin_elevations lock");
            elevations.push(elevation.clone());
        }

        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let e = elevation.clone();
            tokio::spawn(async move {
                let _ = pg.insert_admin_elevation(&e).await;
            });
        }

        elevation
    }

    pub fn revoke_admin_elevation(&self, id: Uuid) -> Option<AdminElevation> {
        let mut elevations = self
            .admin_elevations
            .write()
            .expect("admin_elevations lock");
        let e = elevations
            .iter_mut()
            .find(|e| e.id == id && e.revoked_at.is_none())?;
        e.revoked_at = Some(Utc::now());
        let result = e.clone();

        if let Some(pg) = &self.pg {
            let pg = pg.clone();
            let e = result.clone();
            tokio::spawn(async move {
                let _ = pg.insert_admin_elevation(&e).await;
            });
        }

        Some(result)
    }

    /// Expire admin elevations that have passed their deadline.
    pub fn expire_admin_elevations(&self) {
        let now = Utc::now();
        let mut elevations = self
            .admin_elevations
            .write()
            .expect("admin_elevations lock");
        for e in elevations.iter_mut() {
            if e.revoked_at.is_none() && e.expires_at <= now {
                e.revoked_at = Some(now);
            }
        }
    }

    /// Check if a user has an active admin elevation.
    pub fn has_active_elevation(&self, user_id: Uuid) -> bool {
        let now = Utc::now();
        let elevations = self.admin_elevations.read().expect("admin_elevations lock");
        elevations
            .iter()
            .any(|e| e.user_id == user_id && e.revoked_at.is_none() && e.expires_at > now)
    }

    // ─── Application-layer encryption helpers ───

    /// Encrypt a plaintext string for storage (wraps ChaCha20Poly1305).
    pub fn encrypt_field(&self, plaintext: &str) -> String {
        use base64::Engine;
        use chacha20poly1305::{
            aead::{Aead, KeyInit},
            ChaCha20Poly1305, Key, Nonce,
        };

        // Derive key from storage key embedded in the store, or use a fixed fallback
        let key_material = self.http_token.as_bytes();
        let mut hasher = Sha256::new();
        hasher.update(key_material);
        let digest = hasher.finalize();
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&digest));

        let uuid = Uuid::new_v4();
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&uuid.as_bytes()[..12]);

        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .unwrap_or_else(|_| plaintext.as_bytes().to_vec());

        format!(
            "enc:{}:{}",
            base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
            base64::engine::general_purpose::STANDARD.encode(ciphertext)
        )
    }

    /// Decrypt an encrypted field. Returns plaintext if input is not encrypted.
    pub fn decrypt_field(&self, encoded: &str) -> String {
        use base64::Engine;
        use chacha20poly1305::{
            aead::{Aead, KeyInit},
            ChaCha20Poly1305, Key, Nonce,
        };

        let Some(rest) = encoded.strip_prefix("enc:") else {
            return encoded.to_string();
        };
        let Some((nonce_b64, ct_b64)) = rest.split_once(':') else {
            return encoded.to_string();
        };

        let key_material = self.http_token.as_bytes();
        let mut hasher = Sha256::new();
        hasher.update(key_material);
        let digest = hasher.finalize();
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&digest));

        let nonce = match base64::engine::general_purpose::STANDARD.decode(nonce_b64) {
            Ok(n) if n.len() == 12 => n,
            _ => return encoded.to_string(),
        };
        let ciphertext = match base64::engine::general_purpose::STANDARD.decode(ct_b64) {
            Ok(c) => c,
            _ => return encoded.to_string(),
        };

        cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .ok()
            .and_then(|p| String::from_utf8(p).ok())
            .unwrap_or_else(|| encoded.to_string())
    }

    // ─── Line Delegations (Boss-Secretary) ───

    pub fn put_line_delegation(&self, d: LineDelegation) {
        self.line_delegations.insert(d.id, d);
    }

    pub fn delegations_for_owner(&self, owner_uri: &str) -> Vec<LineDelegation> {
        self.line_delegations
            .values()
            .into_iter()
            .filter(|d| d.owner_uri == owner_uri)
            .collect()
    }

    pub fn delegations_for_delegate(&self, delegate_uri: &str) -> Vec<LineDelegation> {
        self.line_delegations
            .values()
            .into_iter()
            .filter(|d| d.delegate_uri == delegate_uri)
            .collect()
    }

    pub fn line_delegation(&self, id: Uuid) -> Option<LineDelegation> {
        self.line_delegations.get(&id)
    }

    pub fn delete_line_delegation(&self, id: Uuid) -> Option<LineDelegation> {
        self.line_delegations.remove(&id)
    }

    /// Check if delegate_uri can answer calls for target_uri.
    pub fn can_delegate_answer(&self, target_uri: &str, delegate_uri: &str) -> bool {
        self.line_delegations
            .values()
            .into_iter()
            .any(|d| d.owner_uri == target_uri && d.delegate_uri == delegate_uri && d.can_answer)
    }

    // ─── Common Area Phones ───

    pub fn put_common_area_phone(&self, phone: CommonAreaPhone) {
        self.common_area_phones.insert(phone.id, phone);
    }

    pub fn common_area_phone_list(&self) -> Vec<CommonAreaPhone> {
        self.common_area_phones.values()
    }

    pub fn common_area_phone(&self, id: Uuid) -> Option<CommonAreaPhone> {
        self.common_area_phones.get(&id)
    }

    pub fn delete_common_area_phone(&self, id: Uuid) -> Option<CommonAreaPhone> {
        self.common_area_phones.remove(&id)
    }

    // ─── Meeting Rooms ───

    pub fn put_meeting_room(&self, room: MeetingRoom) {
        self.meeting_rooms.insert(room.id, room);
    }

    pub fn meeting_room_list(&self) -> Vec<MeetingRoom> {
        self.meeting_rooms.values()
    }

    pub fn meeting_room(&self, id: Uuid) -> Option<MeetingRoom> {
        self.meeting_rooms.get(&id)
    }

    pub fn delete_meeting_room(&self, id: Uuid) -> Option<MeetingRoom> {
        self.meeting_rooms.remove(&id)
    }

    pub fn put_room_booking(&self, booking: RoomBooking) {
        self.room_bookings.insert(booking.id, booking);
    }

    pub fn room_bookings_for_room(&self, room_id: Uuid) -> Vec<RoomBooking> {
        self.room_bookings
            .values()
            .into_iter()
            .filter(|b| b.room_id == room_id)
            .collect()
    }

    pub fn available_rooms(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<MeetingRoom> {
        let bookings = self.room_bookings.values();
        self.meeting_rooms
            .values()
            .into_iter()
            .filter(|room| {
                room.bookable
                    && !bookings
                        .iter()
                        .any(|b| b.room_id == room.id && b.start_time < end && b.end_time > start)
            })
            .collect()
    }

    pub fn delete_room_booking(&self, id: Uuid) -> Option<RoomBooking> {
        self.room_bookings.remove(&id)
    }

    // ─── Provisioned Devices ───

    pub fn put_provisioned_device(&self, device: ProvisionedDevice) {
        self.provisioned_devices.insert(device.id, device);
    }

    pub fn provisioned_device_list(&self) -> Vec<ProvisionedDevice> {
        self.provisioned_devices.values()
    }

    pub fn provisioned_device(&self, id: Uuid) -> Option<ProvisionedDevice> {
        self.provisioned_devices.get(&id)
    }

    pub fn provisioned_device_by_mac(&self, mac: &str) -> Option<ProvisionedDevice> {
        let mac_lower = mac.to_lowercase().replace([':', '-'], "");
        self.provisioned_devices
            .values()
            .into_iter()
            .find(|d| d.mac_address.to_lowercase().replace([':', '-'], "") == mac_lower)
    }

    pub fn delete_provisioned_device(&self, id: Uuid) -> Option<ProvisionedDevice> {
        self.provisioned_devices.remove(&id)
    }

    // ─── Hot Desking ───

    pub fn put_hotdesk_session(&self, session: HotdeskSession) {
        self.hotdesk_sessions.insert(session.id, session);
    }

    pub fn active_hotdesk_for_device(&self, device_id: Uuid) -> Option<HotdeskSession> {
        self.hotdesk_sessions
            .values()
            .into_iter()
            .find(|s| s.device_id == device_id && s.logged_out_at.is_none())
    }

    pub fn hotdesk_logout(&self, device_id: Uuid) -> Option<HotdeskSession> {
        let session = self.active_hotdesk_for_device(device_id)?;
        let mut updated = session.clone();
        updated.logged_out_at = Some(Utc::now());
        self.hotdesk_sessions.insert(updated.id, updated.clone());
        Some(updated)
    }

    // ─── Custom Emojis ───

    pub fn custom_emojis_for_team(&self, team_id: Uuid) -> Vec<CustomEmoji> {
        self.custom_emojis
            .values()
            .into_iter()
            .filter(|e| e.team_id == team_id)
            .collect()
    }

    pub fn put_custom_emoji(&self, emoji: CustomEmoji) {
        self.custom_emojis.insert(emoji.id, emoji);
    }

    pub fn delete_custom_emoji(&self, id: Uuid) -> Option<CustomEmoji> {
        self.custom_emojis.remove(&id)
    }

    // ─── Wiki Pages ───

    pub fn wiki_pages_for_team(&self, team_id: Uuid) -> Vec<WikiPage> {
        let mut pages: Vec<_> = self
            .wiki_pages
            .values()
            .into_iter()
            .filter(|p| p.team_id == team_id)
            .collect();
        pages.sort_by_key(|p| p.created_at);
        pages
    }

    pub fn wiki_page(&self, id: Uuid) -> Option<WikiPage> {
        self.wiki_pages.get(&id)
    }

    pub fn put_wiki_page(&self, page: WikiPage) {
        self.wiki_pages.insert(page.id, page);
    }

    pub fn delete_wiki_page(&self, id: Uuid) -> Option<WikiPage> {
        self.wiki_pages.remove(&id)
    }

    // ─── Task Boards ───

    pub fn task_boards_for_team(&self, team_id: Uuid) -> Vec<TaskBoard> {
        let mut boards: Vec<_> = self
            .task_boards
            .values()
            .into_iter()
            .filter(|b| b.team_id == team_id)
            .collect();
        boards.sort_by_key(|b| b.created_at);
        boards
    }

    pub fn task_board(&self, id: Uuid) -> Option<TaskBoard> {
        self.task_boards.get(&id)
    }

    pub fn put_task_board(&self, board: TaskBoard) {
        self.task_boards.insert(board.id, board);
    }

    pub fn delete_task_board(&self, id: Uuid) -> Option<TaskBoard> {
        self.task_boards.remove(&id)
    }

    pub fn tasks_for_board(&self, board_id: Uuid) -> Vec<Task> {
        let mut tasks: Vec<_> = self
            .tasks
            .values()
            .into_iter()
            .filter(|t| t.board_id == board_id)
            .collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    pub fn task(&self, id: Uuid) -> Option<Task> {
        self.tasks.get(&id)
    }

    pub fn put_task(&self, task: Task) {
        self.tasks.insert(task.id, task);
    }

    pub fn delete_task(&self, id: Uuid) -> Option<Task> {
        self.tasks.remove(&id)
    }

    // ─── Call Analytics ───

    pub fn user_call_analytics(&self, user_sip_uri: &str) -> serde_json::Value {
        let cdrs = self.cdrs.read().expect("cdrs lock");
        let user_cdrs: Vec<_> = cdrs
            .iter()
            .filter(|c| c.caller_uri == user_sip_uri || c.callee_uri == user_sip_uri)
            .collect();
        let total_calls = user_cdrs.len();
        let answered_calls = user_cdrs
            .iter()
            .filter(|c| c.disposition == "answered")
            .count();
        let total_duration: i32 = user_cdrs.iter().map(|c| c.duration_secs).sum();
        let avg_duration = if total_calls > 0 {
            total_duration as f64 / total_calls as f64
        } else {
            0.0
        };

        // MOS from call quality reports
        let reports = self.call_quality_reports.read().expect("cqr lock");
        let user_reports: Vec<_> = reports
            .iter()
            .filter(|r| r.user_sip_uri == user_sip_uri)
            .collect();
        let avg_mos = if user_reports.is_empty() {
            0.0
        } else {
            user_reports.iter().map(|r| r.mos_score).sum::<f64>() / user_reports.len() as f64
        };
        let avg_packet_loss = if user_reports.is_empty() {
            0.0
        } else {
            user_reports.iter().map(|r| r.packet_loss_pct).sum::<f64>() / user_reports.len() as f64
        };

        serde_json::json!({
            "user_sip_uri": user_sip_uri,
            "total_calls": total_calls,
            "answered_calls": answered_calls,
            "avg_duration_secs": avg_duration,
            "avg_mos": avg_mos,
            "avg_packet_loss": avg_packet_loss,
            "total_duration_secs": total_duration,
            "total_quality_reports": user_reports.len(),
        })
    }

    pub fn create_routing_rule(&self, input: CreateRoutingRuleRequest) -> RoutingRule {
        let rule = RoutingRule {
            id: Uuid::new_v4(),
            name: input.name,
            source_pattern: input.source_pattern,
            destination_pattern: input.destination_pattern,
            target: input.target,
            destination_type: input
                .destination_type
                .unwrap_or_else(default_route_destination_type),
            method_pattern: input
                .method_pattern
                .unwrap_or_else(default_route_method_pattern),
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
            rule.destination_type = input
                .destination_type
                .unwrap_or_else(default_route_destination_type);
            rule.method_pattern = input
                .method_pattern
                .unwrap_or_else(default_route_method_pattern);
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
        self.presence.insert(sip_uri.to_string(), presence.clone());
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
        let _ = self.sse_tx.send(event.clone());
        self.publish_nats_event(event);
    }

    fn publish_nats_event(&self, event: SseEvent) {
        let Some(url) = self.nats_url.clone() else {
            return;
        };
        let Ok(payload) = serde_json::to_vec(&event) else {
            return;
        };
        let subject = format!("pale.events.{}", nats_subject_token(&event.event_type));
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(err) = publish_nats_message(&url, &subject, &payload).await {
                    log::warn!("failed to publish NATS event {}: {}", subject, err);
                }
            });
        }
    }

    // ─── Call Center: Agent Management ───

    pub fn create_agent_profile(
        &self,
        input: CreateAgentProfileRequest,
    ) -> Result<AgentProfile, String> {
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
        self.agent_profiles
            .insert(input.user_sip_uri, profile.clone());
        Ok(profile)
    }

    pub fn list_agent_profiles(&self) -> Vec<AgentProfile> {
        self.agent_profiles.values()
    }

    pub fn agent_profile(&self, uri: &str) -> Option<AgentProfile> {
        self.agent_profiles.get(&uri.to_string())
    }

    pub fn set_agent_state(
        &self,
        uri: &str,
        state: &str,
        reason: Option<String>,
    ) -> Option<AgentProfile> {
        self.agent_profiles
            .with_write(&uri.to_string(), |profiles| {
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

    pub fn transition_agent_state(
        &self,
        uri: &str,
        new_state: &str,
        reason: Option<String>,
    ) -> Result<AgentProfile, String> {
        let profile = self.agent_profile(uri).ok_or("Agent not found")?;
        let old_state = profile.state.clone();

        let valid = match (old_state.as_str(), new_state) {
            ("offline", "available") => true,
            ("available", "on_call")
            | ("available", "break")
            | ("available", "training")
            | ("available", "meeting")
            | ("available", "offline") => true,
            ("on_call", "wrap_up") | ("on_call", "available") => true,
            ("wrap_up", "available") | ("wrap_up", "break") | ("wrap_up", "offline") => true,
            ("break", "available") | ("break", "offline") => true,
            ("training", "available") | ("training", "offline") => true,
            ("meeting", "available") | ("meeting", "offline") => true,
            (_, "offline") => true,
            _ => false,
        };
        if !valid {
            return Err(format!(
                "Invalid state transition: {} -> {}",
                old_state, new_state
            ));
        }

        let duration = (Utc::now() - profile.state_since).num_seconds() as i32;

        // Log state change
        let uri_owned = uri.to_string();
        let old = old_state.clone();
        let new_s = new_state.to_string();
        let r = reason.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.insert_agent_state_log(&uri_owned, &old, &new_s, r.as_deref(), duration)
                    .await
            })
        });

        let updated = self
            .set_agent_state(uri, new_state, reason)
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

    pub fn enqueue_caller(
        &self,
        queue_id: Uuid,
        caller_uri: &str,
        caller_name: &str,
    ) -> QueueCallerEntry {
        let position = self
            .queue_callers
            .values()
            .into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .count() as i32
            + 1;
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
        self.queue_callers
            .values()
            .into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .collect()
    }

    pub fn queue_callers_waiting_count(&self, queue_id: Uuid) -> usize {
        self.queue_callers
            .values()
            .into_iter()
            .filter(|c| c.queue_id == queue_id && c.status == "waiting")
            .count()
    }

    // ─── VIP Caller Management ───

    pub fn check_vip(&self, caller_uri: &str) -> Option<VipCaller> {
        self.vip_callers.values().into_iter().find(|v| {
            caller_uri.contains(&v.caller_pattern)
                || v.caller_pattern == caller_uri
                || caller_uri.ends_with(&v.caller_pattern)
        })
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
        self.queue_callbacks
            .values()
            .into_iter()
            .filter(|cb| cb.queue_id == queue_id)
            .collect()
    }

    pub fn pending_callbacks(&self, queue_id: Uuid) -> Vec<QueueCallback> {
        let mut cbs: Vec<_> = self
            .queue_callbacks
            .values()
            .into_iter()
            .filter(|cb| cb.queue_id == queue_id && cb.status == "pending")
            .collect();
        cbs.sort_by_key(|cb| cb.position);
        cbs
    }

    pub fn queue_wallboard(&self) -> Vec<QueueMetricsSnapshot> {
        let all_dialogs = self.sip_dialogs.values();
        let cdrs = self.cdrs.read().expect("cdrs lock");
        let now = Utc::now();

        self.list_queues()
            .into_iter()
            .map(|q| {
                let agent_uris: Vec<&str> = q.agents.iter().map(|a| a.agent_uri.as_str()).collect();

                // Use agent profiles for real-time state where available,
                // falling back to the queue-level agent state.
                let mut available = 0i32;
                let mut busy = 0i32;
                let mut paused = 0i32;
                for qa in &q.agents {
                    let state = self
                        .agent_profiles
                        .get(&qa.agent_uri)
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
                let calls_active = all_dialogs
                    .iter()
                    .filter(|d| {
                        !matches!(
                            d.status,
                            SipDialogStatus::Ended
                                | SipDialogStatus::Cancelled
                                | SipDialogStatus::Failed
                        ) && (agent_uris.contains(&d.from_uri.as_str())
                            || agent_uris.contains(&d.to_uri.as_str()))
                    })
                    .count() as i32;

                // CDR stats for this queue
                let queue_cdrs: Vec<_> = cdrs
                    .iter()
                    .filter(|c| c.queue_name.as_deref() == Some(&q.name))
                    .collect();
                let answered: Vec<_> = queue_cdrs
                    .iter()
                    .filter(|c| c.disposition == "answered")
                    .collect();
                let abandoned = queue_cdrs
                    .iter()
                    .filter(|c| c.disposition == "abandoned")
                    .count() as i32;
                let calls_answered = answered.len() as i32;

                // Wait time stats from answered CDRs that have queue_wait_secs
                let wait_times: Vec<i32> =
                    answered.iter().filter_map(|c| c.queue_wait_secs).collect();
                let avg_wait_secs = if wait_times.is_empty() {
                    0
                } else {
                    wait_times.iter().sum::<i32>() / wait_times.len() as i32
                };

                // Average talk time from answered CDRs
                let talk_times: Vec<i32> = answered
                    .iter()
                    .map(|c| c.duration_secs)
                    .filter(|&d| d > 0)
                    .collect();
                let avg_talk_secs = if talk_times.is_empty() {
                    0
                } else {
                    talk_times.iter().sum::<i32>() / talk_times.len() as i32
                };

                // Longest waiting: unanswered CDRs still in progress for this queue
                let longest_wait_secs = queue_cdrs
                    .iter()
                    .filter(|c| c.end_time.is_none() && c.disposition == "no_answer")
                    .map(|c| (now - c.start_time).num_seconds() as i32)
                    .max()
                    .unwrap_or(0);

                // Calls waiting: CDRs with no end_time and no_answer disposition
                let calls_waiting = queue_cdrs
                    .iter()
                    .filter(|c| c.end_time.is_none() && c.disposition == "no_answer")
                    .count() as i32;

                let total = calls_answered + abandoned;
                let sla_percentage = if total == 0 {
                    100.0
                } else {
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
            })
            .collect()
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

    pub fn list_monitor_sessions(&self) -> Vec<MonitorSession> {
        self.monitor_sessions.values()
    }

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
        self.qa_scorecards
            .write()
            .expect("qa lock")
            .push(sc.clone());
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

    pub fn list_canned_responses(&self) -> Vec<CannedResponse> {
        self.canned_responses.values()
    }
    pub fn delete_canned_response(&self, id: Uuid) -> Option<CannedResponse> {
        self.canned_responses.remove(&id)
    }

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
            agents: input
                .agents
                .into_iter()
                .map(|a| QueueAgent {
                    agent_uri: a.agent_uri,
                    priority: a.priority.unwrap_or(1),
                    skills: a.skills.unwrap_or_default(),
                    state: "available".to_string(),
                    calls_handled: 0,
                    penalty: 0,
                })
                .collect(),
            enabled: true,
            created_at: Utc::now(),
            callback_enabled: input.callback_enabled.unwrap_or(false),
            callback_threshold_secs: input.callback_threshold_secs.unwrap_or(120),
            sla_target_secs: input.sla_target_secs.unwrap_or(20),
        };
        self.call_queues.insert(queue.id, queue.clone());
        Ok(queue)
    }

    pub fn list_queues(&self) -> Vec<CallQueue> {
        self.call_queues.values()
    }
    pub fn queue(&self, id: Uuid) -> Option<CallQueue> {
        self.call_queues.get(&id)
    }
    pub fn delete_queue(&self, id: Uuid) -> Option<CallQueue> {
        self.call_queues.remove(&id)
    }

    pub fn queue_by_extension(&self, uri: &str) -> Option<CallQueue> {
        let user = sip_user_part(uri);
        self.call_queues
            .values()
            .into_iter()
            .find(|q| (q.extension == uri || sip_user_part(&q.extension) == user) && q.enabled)
    }

    // ─── Extensions ───

    pub fn create_extension(&self, input: CreateExtensionRequest) -> Result<Extension, String> {
        if self.extensions.get(&input.extension).is_some() {
            return Err(format!("Extension {} already exists", input.extension));
        }
        let user_display_name = input
            .user_id
            .and_then(|uid| self.users.get(&uid).map(|u| u.display_name.clone()));
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

    pub fn list_extensions(&self) -> Vec<Extension> {
        self.extensions.values()
    }
    pub fn list_dids(&self) -> Vec<Extension> {
        let mut dids: Vec<_> = self
            .extensions
            .values()
            .into_iter()
            .filter(|ext| ext.is_did)
            .collect();
        dids.sort_by(|a, b| a.extension.cmp(&b.extension));
        dids
    }

    pub fn resolve_extension(&self, uri: &str) -> Option<Extension> {
        let user = sip_user_part(uri);
        self.extensions
            .get(&uri.to_string())
            .or_else(|| self.extensions.get(&user.to_string()))
    }
    pub fn delete_extension(&self, ext: &str) -> Option<Extension> {
        let removed = self.extensions.remove(&ext.to_string());
        if removed.is_some() {
            let ext_key = ext.to_string();
            self.pg_spawn(move |pg| {
                Box::pin(async move { pg.delete_pg_extension(&ext_key).await })
            });
        }
        removed
    }

    pub fn provision_user(
        &self,
        input: ProvisionUserRequest,
    ) -> Result<ProvisionUserResponse, String> {
        let default_username = input.display_name.to_lowercase().replace(' ', ".");
        let sip_username = input
            .extension_number
            .as_deref()
            .unwrap_or(&default_username);
        let sip_uri = format!("sip:{}@{}", sip_username, input.sip_domain);
        let normalized_sip_uri =
            normalize_sip_uri(&sip_uri).ok_or_else(|| format!("Invalid SIP URI {}", sip_uri))?;

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
        let sip_creds = split_sip_aor_simple(&sip_uri).map(|(username, domain)| SipCredentials {
            sip_uri: sip_uri.clone(),
            registrar_uri: self
                .sip_registrar
                .as_ref()
                .map(|registrar| format!("sip:{}", registrar)),
            registration_available: self.sip_registrar.is_some(),
            username,
            password: input.password,
            transport: self.sip_registrar_transport.clone(),
            domain,
        });

        Ok(ProvisionUserResponse {
            user,
            extension,
            sip_credentials: sip_creds,
        })
    }

    pub fn assign_extension(&self, ext: &str, user_id: Uuid) -> Result<Extension, String> {
        let mut extension = self
            .extensions
            .get(&ext.to_string())
            .ok_or_else(|| format!("Extension {} not found", ext))?;
        let user = self
            .users
            .get(&user_id)
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
        let mut extension = self
            .extensions
            .get(&ext.to_string())
            .ok_or_else(|| format!("Extension {} not found", ext))?;
        extension.user_id = None;
        extension.user_display_name = None;
        self.extensions.insert(ext.to_string(), extension.clone());
        let e = extension.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_extension(&e).await }));
        Ok(extension)
    }

    pub fn extensions_for_user(&self, user_id: Uuid) -> Vec<Extension> {
        self.extensions
            .values()
            .into_iter()
            .filter(|e| e.user_id == Some(user_id))
            .collect()
    }

    pub fn list_extensions_filtered(&self, unassigned_only: bool) -> Vec<Extension> {
        if unassigned_only {
            self.extensions
                .values()
                .into_iter()
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
            timezone: input
                .timezone
                .unwrap_or_else(|| "America/New_York".to_string()),
            schedule: input.schedule,
            after_hours_destination: input.after_hours_destination,
            enabled: true,
            created_at: Utc::now(),
        };
        self.business_hours.insert(bh.id, bh.clone());
        bh
    }

    pub fn list_business_hours(&self) -> Vec<BusinessHours> {
        self.business_hours.values()
    }
    pub fn delete_business_hours(&self, id: Uuid) -> Option<BusinessHours> {
        self.business_hours.remove(&id)
    }

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

    pub fn list_holidays(&self) -> Vec<Holiday> {
        self.holidays.values()
    }
    pub fn delete_holiday(&self, id: Uuid) -> Option<Holiday> {
        self.holidays.remove(&id)
    }

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
        let enabled: Vec<_> = self
            .business_hours
            .values()
            .into_iter()
            .filter(|bh| bh.enabled)
            .collect();
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
            (
                true,
                if settings.allow_call_forwarding {
                    settings.dnd_forward_to.clone()
                } else {
                    None
                },
            )
        } else {
            (false, None)
        }
    }

    /// Resolve call forwarding for a target user based on the call state.
    /// `call_state` should be one of "always", "busy", or "no_answer".
    pub fn resolve_call_forwarding(&self, target_uri: &str, call_state: &str) -> Option<String> {
        let settings = self.get_user_call_settings(target_uri);
        if !settings.allow_call_forwarding {
            return None;
        }
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

    pub fn claim_next_agent(
        &self,
        queue: &CallQueue,
        required_skills: &[String],
    ) -> Option<String> {
        let _lock = self.agent_assignment_lock.lock().ok()?;

        let mut candidates: Vec<(usize, &QueueAgent)> = queue
            .agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.state == "available")
            .filter(|(_, a)| {
                self.agent_profile(&a.agent_uri)
                    .map_or(false, |p| p.state == "available")
            })
            .filter(|(_, a)| {
                if required_skills.is_empty() {
                    return true;
                }
                required_skills.iter().all(|s| a.skills.contains(s))
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort by strategy
        match queue.strategy.as_str() {
            "longest_idle" => {
                candidates.sort_by(|(_, a), (_, b)| {
                    let a_since = self
                        .agent_profile(&a.agent_uri)
                        .map(|p| p.state_since)
                        .unwrap_or_else(Utc::now);
                    let b_since = self
                        .agent_profile(&b.agent_uri)
                        .map(|p| p.state_since)
                        .unwrap_or_else(Utc::now);
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
    pub fn record_cdr_start(
        &self,
        call_id: Option<&str>,
        caller_uri: &str,
        callee_uri: &str,
        direction: &str,
    ) -> CallDetailRecord {
        // Check recording policies for auto-record
        let auto_record = self.should_auto_record(caller_uri, callee_uri);
        if auto_record {
            log::info!(
                "Auto-recording call {} -> {} per recording policy",
                caller_uri,
                callee_uri
            );
        }
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
            recorded: auto_record,
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
        // Extract a readable caller name from the URI if not provided
        let name = if caller_name.is_empty() {
            sip_user_part(caller_uri).to_string()
        } else {
            caller_name.to_string()
        };

        let vm = Voicemail {
            id: Uuid::new_v4(),
            callee_uri: callee_uri.to_string(),
            caller_uri: caller_uri.to_string(),
            caller_name: name.clone(),
            duration_secs,
            file_id,
            listened: false,
            created_at: Utc::now(),
        };
        self.store_voicemail(vm.clone());

        // Send push notification for new voicemail
        self.notify_user_push(
            callee_uri,
            &format!("Voicemail from {}", name),
            "You have a new voicemail message",
            Some(format!("voicemail-{}", vm.id)),
        );

        // Broadcast SSE event so the client shows the voicemail immediately
        self.broadcast_sse(SseEvent {
            event_type: "voicemail".to_string(),
            payload: serde_json::to_value(&vm).unwrap_or_default(),
        });

        vm
    }

    // ─── Call Park ───

    pub fn park_call(
        &self,
        slot: &str,
        call_id: &str,
        parked_by: &str,
        caller_uri: &str,
        caller_name: &str,
    ) -> ParkedCall {
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

    pub fn list_parked_calls(&self) -> Vec<ParkedCall> {
        self.parked_calls.values()
    }

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
        self.speed_dials
            .read()
            .expect("speed dials lock")
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

    pub fn list_paging_groups(&self) -> Vec<PagingGroup> {
        self.paging_groups.values()
    }
    pub fn delete_paging_group(&self, id: Uuid) -> Option<PagingGroup> {
        self.paging_groups.remove(&id)
    }

    // ─── Ring Groups ───

    pub fn create_ring_group(&self, input: CreateRingGroupRequest) -> Result<RingGroup, String> {
        if self
            .ring_groups
            .values()
            .iter()
            .any(|g| g.extension == input.extension)
        {
            return Err(format!(
                "Ring group with extension {} already exists",
                input.extension
            ));
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
        self.ring_groups
            .values()
            .into_iter()
            .find(|g| (g.extension == uri || sip_user_part(&g.extension) == user) && g.enabled)
    }

    pub fn delete_ring_group(&self, id: Uuid) -> Option<RingGroup> {
        self.ring_groups.remove(&id)
    }

    // ─── IVR ───

    pub fn create_ivr(&self, input: CreateIvrRequest) -> Result<Ivr, String> {
        if self
            .ivrs
            .values()
            .iter()
            .any(|i| i.extension == input.extension)
        {
            return Err(format!(
                "IVR with extension {} already exists",
                input.extension
            ));
        }
        let ivr = Ivr {
            id: Uuid::new_v4(),
            name: input.name,
            extension: input.extension,
            greeting_text: input
                .greeting_text
                .unwrap_or_else(|| "Welcome.".to_string()),
            greeting_file_id: input.greeting_file_id,
            timeout_secs: input.timeout_secs.unwrap_or(10),
            max_retries: input.max_retries.unwrap_or(3),
            invalid_destination: input.invalid_destination,
            timeout_destination: input.timeout_destination,
            options: input.options,
            speech_enabled: input.speech_enabled.unwrap_or(false),
            speech_language: input
                .speech_language
                .and_then(non_empty_string)
                .unwrap_or_else(|| "en-US".to_string()),
            speech_provider_configured: self.enterprise_capability_available("speech_ivr"),
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
        self.ivrs.get(&id).map(|mut ivr| {
            ivr.speech_provider_configured = self.enterprise_capability_available("speech_ivr");
            ivr
        })
    }

    pub fn ivr_by_extension(&self, uri: &str) -> Option<Ivr> {
        let user = sip_user_part(uri);
        self.ivrs
            .values()
            .into_iter()
            .find(|i| (i.extension == uri || sip_user_part(&i.extension) == user) && i.enabled)
    }

    pub fn delete_ivr(&self, id: Uuid) -> Option<Ivr> {
        self.ivrs.remove(&id)
    }

    pub fn resolve_ivr_speech(
        &self,
        id: Uuid,
        input: ResolveIvrSpeechRequest,
    ) -> Option<IvrSpeechResolution> {
        let mut ivr = self.ivr(id)?;
        let provider_configured = self.enterprise_capability_available("speech_ivr");
        ivr.speech_provider_configured = provider_configured;
        let utterance = input.utterance.trim().to_string();
        if !ivr.speech_enabled {
            return Some(IvrSpeechResolution {
                ivr_id: id,
                utterance,
                language: input.language.or_else(|| Some(ivr.speech_language.clone())),
                provider_configured,
                matched: false,
                reason: "speech_ivr_disabled".to_string(),
                option: None,
                route: ivr
                    .invalid_destination
                    .clone()
                    .map(|destination| ResolvedRoute {
                        destination_type: "invalid".to_string(),
                        destination,
                        ring_group: None,
                        ivr: None,
                    }),
            });
        }
        if !provider_configured {
            return Some(IvrSpeechResolution {
                ivr_id: id,
                utterance,
                language: input.language.or_else(|| Some(ivr.speech_language.clone())),
                provider_configured,
                matched: false,
                reason: "speech_ivr_provider_not_configured".to_string(),
                option: None,
                route: ivr
                    .invalid_destination
                    .clone()
                    .map(|destination| ResolvedRoute {
                        destination_type: "invalid".to_string(),
                        destination,
                        ring_group: None,
                        ivr: None,
                    }),
            });
        }

        let normalized = normalize_speech_utterance(&utterance);
        let matched = ivr
            .options
            .iter()
            .find(|option| ivr_option_matches_speech(option, &normalized));
        let route = matched.map(|option| ResolvedRoute {
            destination_type: option.destination_type.clone(),
            destination: option.destination.clone(),
            ring_group: (option.destination_type == "ring_group")
                .then(|| self.ring_group_by_extension(&option.destination))
                .flatten(),
            ivr: (option.destination_type == "ivr")
                .then(|| self.ivr_by_extension(&option.destination))
                .flatten(),
        });
        Some(IvrSpeechResolution {
            ivr_id: id,
            utterance,
            language: input.language.or_else(|| Some(ivr.speech_language.clone())),
            provider_configured,
            matched: matched.is_some(),
            reason: if matched.is_some() {
                "matched_configured_phrase".to_string()
            } else {
                "no_configured_phrase_matched".to_string()
            },
            option: matched.cloned(),
            route,
        })
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

    pub fn store_recording(&self, recording: CallRecording) -> Result<CallRecording, String> {
        if !self.collaboration_policy().meeting_recording_enabled {
            return Err("meeting recording is disabled by policy".to_string());
        }
        if !self
            .get_user_call_settings(&recording.recorded_by)
            .allow_call_recording
        {
            return Err("call recording is disabled by policy".to_string());
        }
        let mut recording = recording;
        if let Some(conference_id) = recording.conference_id {
            recording.transcript_segment_count = self.get_transcript(conference_id).len();
        }
        self.recordings.insert(recording.id, recording.clone());
        self.broadcast_sse(SseEvent {
            event_type: "recording".to_string(),
            payload: serde_json::to_value(&recording).unwrap_or_default(),
        });
        let r = recording.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_recording(&r).await }));
        if recording.conference_id.is_some() {
            let _ = self.queue_transcription_job(recording.id, &recording.recorded_by, None);
        }
        Ok(recording)
    }

    pub fn transcription_jobs_for_recording(&self, recording_id: Uuid) -> Vec<TranscriptionJob> {
        let mut jobs: Vec<_> = self
            .transcription_jobs
            .values()
            .into_iter()
            .filter(|job| job.recording_id == recording_id)
            .map(|mut job| {
                job.provider_configured =
                    self.enterprise_capability_available("auto_transcription");
                job
            })
            .collect();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs
    }

    pub fn transcription_job(&self, id: Uuid) -> Option<TranscriptionJob> {
        self.transcription_jobs.get(&id).map(|mut job| {
            job.provider_configured = self.enterprise_capability_available("auto_transcription");
            job
        })
    }

    pub fn recording(&self, id: Uuid) -> Option<CallRecording> {
        self.recordings.get(&id)
    }

    pub fn queue_transcription_job(
        &self,
        recording_id: Uuid,
        requested_by: &str,
        language: Option<String>,
    ) -> Option<TranscriptionJob> {
        let recording = self.recordings.get(&recording_id)?;
        if recording.deleted_at.is_some() {
            return None;
        }
        let provider_configured = self.enterprise_capability_available("auto_transcription");
        let error = if !provider_configured {
            Some("auto_transcription_provider_not_configured".to_string())
        } else if recording.file_id.is_none() {
            Some("recording_file_missing".to_string())
        } else if recording.conference_id.is_none() {
            Some("conference_missing".to_string())
        } else {
            None
        };
        let now = Utc::now();
        let job = TranscriptionJob {
            id: Uuid::new_v4(),
            recording_id,
            conference_id: recording.conference_id,
            status: if error.is_some() {
                TranscriptionJobStatus::Blocked
            } else {
                TranscriptionJobStatus::Queued
            },
            language: language.and_then(non_empty_string),
            provider_configured,
            requested_by: requested_by.to_string(),
            error,
            transcript_segment_count: 0,
            created_at: now,
            updated_at: now,
        };
        self.transcription_jobs.insert(job.id, job.clone());
        Some(job)
    }

    pub fn start_transcription_job(&self, id: Uuid) -> Option<TranscriptionJob> {
        let mut job = self.transcription_jobs.get(&id)?;
        if job.status == TranscriptionJobStatus::Queued {
            job.status = TranscriptionJobStatus::Processing;
            job.updated_at = Utc::now();
            self.transcription_jobs.insert(id, job.clone());
        }
        Some(job)
    }

    pub fn complete_transcription_job(
        &self,
        id: Uuid,
        segments: Vec<PostTranscriptRequest>,
    ) -> Option<TranscriptionJob> {
        let mut job = self.transcription_jobs.get(&id)?;
        let conference_id = job.conference_id?;
        if !matches!(
            job.status,
            TranscriptionJobStatus::Queued | TranscriptionJobStatus::Processing
        ) {
            return Some(job);
        }
        let mut count = 0;
        for mut segment in segments {
            if segment.language.is_none() {
                segment.language = job.language.clone();
            }
            self.post_transcript(conference_id, segment);
            count += 1;
        }
        job.status = TranscriptionJobStatus::Completed;
        job.error = None;
        job.transcript_segment_count = count;
        job.updated_at = Utc::now();
        self.transcription_jobs.insert(id, job.clone());
        if let Some(mut recording) = self.recordings.get(&job.recording_id) {
            recording.transcript_segment_count = self.get_transcript(conference_id).len();
            self.recordings.insert(recording.id, recording.clone());
            let recording_for_pg = recording.clone();
            self.pg_spawn(move |pg| {
                Box::pin(async move { pg.insert_recording(&recording_for_pg).await })
            });
        }
        Some(job)
    }

    pub fn recordings_for_user(&self, sip_uri: &str) -> Vec<CallRecording> {
        self.recordings
            .values()
            .into_iter()
            .filter(|r| r.deleted_at.is_none())
            .filter(|r| r.caller_uri == sip_uri || r.callee_uri == sip_uri)
            .collect()
    }

    pub fn delete_recording(&self, id: Uuid, deleted_by: &str) -> Option<CallRecording> {
        let recording = self.recordings.get(&id)?;
        if recording.legal_hold || self.recording_on_legal_hold() {
            self.mark_recording_deleted(id, deleted_by)
        } else {
            self.recordings.remove(&id)
        }
    }

    fn mark_recording_deleted(&self, id: Uuid, deleted_by: &str) -> Option<CallRecording> {
        let mut recording = self.recordings.get(&id)?;
        recording.deleted_at = Some(Utc::now());
        recording.deleted_by = Some(deleted_by.to_string());
        self.recordings.insert(id, recording.clone());
        let recording_for_pg = recording.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move { pg.insert_recording(&recording_for_pg).await })
        });
        Some(recording)
    }

    fn recording_on_legal_hold(&self) -> bool {
        self.retention_policies.values().into_iter().any(|policy| {
            policy.legal_hold
                && matches!(policy.scope.as_str(), "global" | "recordings" | "recording")
        })
    }

    fn discovery_recordings(&self) -> Vec<CallRecording> {
        self.recordings.values()
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
            team_id: input.team_id,
            channel_name: input.channel_name,
            channel_type: normalize_channel_type(input.channel_type.as_deref()),
            channel_owners: normalized_room_members(
                input
                    .channel_owners
                    .into_iter()
                    .chain(std::iter::once(creator.to_string()))
                    .collect(),
            ),
            posting_policy: normalize_posting_policy(input.posting_policy.as_deref()),
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

    pub fn create_team(&self, creator: &str, input: CreateTeamRequest) -> Team {
        let mut members = input.members;
        members.push(creator.to_string());
        let members = normalized_room_members(members);
        let team = Team {
            id: Uuid::new_v4(),
            name: input.name,
            description: input.description.unwrap_or_default(),
            owner_uri: creator.to_string(),
            members: members
                .into_iter()
                .map(|uri| TeamMember {
                    role: if uri == creator { "owner" } else { "member" }.to_string(),
                    user_sip_uri: uri,
                    joined_at: Utc::now(),
                })
                .collect(),
            created_at: Utc::now(),
        };
        self.teams.insert(team.id, team.clone());
        self.persist(&team);
        let team_for_pg = team.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(Team::collection(), team_for_pg.key(), &team_for_pg)
                    .await
            })
        });
        self.broadcast_sse(SseEvent {
            event_type: "team_created".to_string(),
            payload: serde_json::to_value(&team).unwrap_or_default(),
        });
        team
    }

    pub fn list_teams_for_user(&self, sip_uri: &str) -> Vec<Team> {
        self.teams
            .values()
            .into_iter()
            .filter(|team| {
                team.members
                    .iter()
                    .any(|member| member.user_sip_uri == sip_uri)
            })
            .collect()
    }

    pub fn team(&self, id: Uuid) -> Option<Team> {
        self.teams.get(&id)
    }

    pub fn add_team_member(
        &self,
        team_id: Uuid,
        user_sip_uri: &str,
        role: Option<String>,
    ) -> Option<Team> {
        let updated = self.teams.with_write(&team_id, |teams| {
            let team = teams.get_mut(&team_id)?;
            if !team
                .members
                .iter()
                .any(|member| member.user_sip_uri == user_sip_uri)
            {
                team.members.push(TeamMember {
                    user_sip_uri: user_sip_uri.to_string(),
                    role: role.unwrap_or_else(|| "member".to_string()),
                    joined_at: Utc::now(),
                });
            }
            Some(team.clone())
        })?;
        self.persist(&updated);
        let team_for_pg = updated.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(Team::collection(), team_for_pg.key(), &team_for_pg)
                    .await
            })
        });
        Some(updated)
    }

    pub fn create_team_channel(
        &self,
        creator: &str,
        team_id: Uuid,
        input: CreateRoomRequest,
    ) -> Option<Room> {
        let team = self.team(team_id)?;
        if !team
            .members
            .iter()
            .any(|member| member.user_sip_uri == creator)
        {
            return None;
        }
        let channel_type = normalize_channel_type(input.channel_type.as_deref());
        let mut explicit_members = input.members;
        explicit_members.push(creator.to_string());
        let mut channel_members: Vec<String> = if channel_type == "private" {
            explicit_members
        } else {
            team.members
                .iter()
                .map(|member| member.user_sip_uri.clone())
                .chain(explicit_members)
                .collect()
        };
        channel_members.sort();
        channel_members.dedup();
        Some(self.create_room(
            creator,
            CreateRoomRequest {
                name: input.name,
                description: input.description,
                members: channel_members,
                is_direct: Some(false),
                team_id: Some(team_id),
                channel_name: input.channel_name,
                channel_type: Some(channel_type),
                channel_owners: input.channel_owners,
                posting_policy: input.posting_policy,
            },
        ))
    }

    pub fn start_room_call(&self, room_id: Uuid, mode: RoomCallMode) -> Option<RoomCallTarget> {
        let (target, updated_room) = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            if room.is_direct {
                return None;
            }

            let conference_id = match room.conference_id.and_then(|id| self.conferences.get(&id)) {
                Some(conference) if matches_room_call_mode(&conference.mode, &mode) => {
                    conference.id
                }
                _ => {
                    let conference = self.create_conference(CreateConferenceRequest {
                        title: room.name.clone(),
                        mode: mode.clone().into(),
                        registration_enabled: None,
                        max_registrations: None,
                        registration_fields: None,
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
            true,
        );
        Some(target)
    }

    pub fn end_room_call(&self, room_id: Uuid) -> Option<RoomCallEnded> {
        let (ended, updated_room) = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            let conference_id = room.conference_id?;
            let call_uri = room.call_uri.take()?;
            room.conference_id = None;
            Some((
                RoomCallEnded {
                    room_id,
                    conference_id,
                    call_uri,
                },
                room.clone(),
            ))
        })?;

        self.persist(&updated_room);
        let room_for_pg = updated_room.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
        let _ = self.deactivate_conference(ended.conference_id);
        self.broadcast_sse(SseEvent {
            event_type: "room_call_ended".to_string(),
            payload: serde_json::to_value(&ended).unwrap_or_default(),
        });
        Some(ended)
    }

    pub fn create_scheduled_meeting(
        &self,
        organizer_uri: &str,
        input: CreateScheduledMeetingRequest,
    ) -> Result<ScheduledMeeting, String> {
        if input.ends_at <= input.starts_at {
            return Err("meeting end time must be after start time".to_string());
        }
        let recurrence = normalize_meeting_recurrence(input.recurrence, input.starts_at)?;
        if let Some(room_id) = input.room_id {
            let room = self
                .room(room_id)
                .ok_or_else(|| "room not found".to_string())?;
            if !room
                .members
                .iter()
                .any(|member| member.user_sip_uri == organizer_uri)
            {
                return Err("organizer is not a room member".to_string());
            }
        }
        let conference = self.create_conference(CreateConferenceRequest {
            title: input.title.clone(),
            mode: input.mode.unwrap_or(RoomCallMode::Video).into(),
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let mut participants = input.participants;
        participants.push(organizer_uri.to_string());
        let meeting = ScheduledMeeting {
            id: Uuid::new_v4(),
            title: input.title,
            description: input.description.unwrap_or_default(),
            organizer_uri: organizer_uri.to_string(),
            room_id: input.room_id,
            conference_id: Some(conference.id),
            participants: normalized_room_members(participants),
            starts_at: input.starts_at,
            ends_at: input.ends_at,
            recurrence,
            status: MeetingStatus::Scheduled,
            cancelled_at: None,
            updated_at: Some(Utc::now()),
            created_at: Utc::now(),
        };
        self.persist_scheduled_meeting(&meeting);
        self.broadcast_sse(SseEvent {
            event_type: "meeting_scheduled".to_string(),
            payload: serde_json::to_value(&meeting).unwrap_or_default(),
        });
        Ok(meeting)
    }

    fn persist_scheduled_meeting(&self, meeting: &ScheduledMeeting) {
        self.scheduled_meetings.insert(meeting.id, meeting.clone());
        self.persist(meeting);
        let meeting_for_pg = meeting.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(
                    ScheduledMeeting::collection(),
                    meeting_for_pg.key(),
                    &meeting_for_pg,
                )
                .await
            })
        });
    }

    pub fn update_scheduled_meeting(
        &self,
        id: Uuid,
        principal: &str,
        input: UpdateScheduledMeetingRequest,
    ) -> Result<ScheduledMeeting, String> {
        let mut meeting = self
            .scheduled_meetings
            .get(&id)
            .ok_or_else(|| "meeting not found".to_string())?;
        if meeting.organizer_uri != principal {
            return Err("only the organizer can update this meeting".to_string());
        }
        if meeting.status == MeetingStatus::Cancelled {
            return Err("cancelled meetings cannot be updated".to_string());
        }
        if let Some(title) = input.title {
            meeting.title = title;
        }
        if let Some(description) = input.description {
            meeting.description = description;
        }
        if let Some(participants) = input.participants {
            let mut participants = participants;
            participants.push(meeting.organizer_uri.clone());
            meeting.participants = normalized_room_members(participants);
        }
        if let Some(starts_at) = input.starts_at {
            meeting.starts_at = starts_at;
        }
        if let Some(ends_at) = input.ends_at {
            meeting.ends_at = ends_at;
        }
        if meeting.ends_at <= meeting.starts_at {
            return Err("meeting end time must be after start time".to_string());
        }
        if let Some(recurrence) = input.recurrence {
            meeting.recurrence = normalize_meeting_recurrence(recurrence, meeting.starts_at)?;
        }
        meeting.updated_at = Some(Utc::now());
        self.persist_scheduled_meeting(&meeting);
        self.broadcast_sse(SseEvent {
            event_type: "meeting_updated".to_string(),
            payload: serde_json::to_value(&meeting).unwrap_or_default(),
        });
        Ok(meeting)
    }

    pub fn cancel_scheduled_meeting(
        &self,
        id: Uuid,
        principal: &str,
    ) -> Result<ScheduledMeeting, String> {
        let mut meeting = self
            .scheduled_meetings
            .get(&id)
            .ok_or_else(|| "meeting not found".to_string())?;
        if meeting.organizer_uri != principal {
            return Err("only the organizer can cancel this meeting".to_string());
        }
        meeting.status = MeetingStatus::Cancelled;
        meeting.cancelled_at = Some(Utc::now());
        meeting.updated_at = Some(Utc::now());
        self.persist_scheduled_meeting(&meeting);
        self.broadcast_sse(SseEvent {
            event_type: "meeting_cancelled".to_string(),
            payload: serde_json::to_value(&meeting).unwrap_or_default(),
        });
        Ok(meeting)
    }

    pub fn meeting_ics(&self, id: Uuid, principal: &str) -> Option<String> {
        let meeting = self.scheduled_meetings.get(&id)?;
        if meeting.organizer_uri != principal
            && !meeting
                .participants
                .iter()
                .any(|participant| participant == principal)
        {
            return None;
        }
        Some(meeting_to_ics(&meeting))
    }

    pub fn list_meetings_for_user(&self, sip_uri: &str) -> Vec<ScheduledMeeting> {
        self.scheduled_meetings
            .values()
            .into_iter()
            .filter(|meeting| {
                meeting.organizer_uri == sip_uri
                    || meeting
                        .participants
                        .iter()
                        .any(|participant| participant == sip_uri)
                    || meeting
                        .room_id
                        .and_then(|room_id| self.room(room_id))
                        .is_some_and(|room| {
                            room.members
                                .iter()
                                .any(|member| member.user_sip_uri == sip_uri)
                        })
            })
            .collect()
    }

    pub fn search_collaboration(
        &self,
        sip_uri: &str,
        query: &str,
        limit: usize,
    ) -> Vec<CollaborationSearchResult> {
        let term = query.trim().to_lowercase();
        if term.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut results = Vec::new();
        for room in self.list_rooms_for_user(sip_uri) {
            let mut haystack = vec![
                room.name.clone(),
                room.description.clone(),
                room.channel_name.clone().unwrap_or_default(),
                room.call_uri.clone().unwrap_or_default(),
            ];
            if let Some(team_id) = room.team_id {
                if let Some(team) = self.team(team_id) {
                    haystack.push(team.name);
                    haystack.push(team.description);
                }
            }
            haystack.extend(
                room.members
                    .iter()
                    .map(|member| member.user_sip_uri.clone()),
            );
            if !collaboration_matches(&haystack, &term) {
                continue;
            }
            results.push(CollaborationSearchResult {
                kind: if room.team_id.is_some() {
                    "channel".to_string()
                } else if room.is_direct {
                    "direct".to_string()
                } else {
                    "room".to_string()
                },
                id: room.id,
                title: room.name,
                subtitle: room.description,
                room_id: Some(room.id),
                team_id: room.team_id,
                conference_id: room.conference_id,
                call_uri: room.call_uri,
                updated_at: room.created_at,
            });
        }

        for team in self.list_teams_for_user(sip_uri) {
            let mut haystack = vec![
                team.name.clone(),
                team.description.clone(),
                team.owner_uri.clone(),
            ];
            haystack.extend(
                team.members
                    .iter()
                    .map(|member| member.user_sip_uri.clone()),
            );
            if !collaboration_matches(&haystack, &term) {
                continue;
            }
            results.push(CollaborationSearchResult {
                kind: "team".to_string(),
                id: team.id,
                title: team.name,
                subtitle: team.description,
                room_id: None,
                team_id: Some(team.id),
                conference_id: None,
                call_uri: None,
                updated_at: team.created_at,
            });
        }

        for meeting in self.list_meetings_for_user(sip_uri) {
            let mut haystack = vec![
                meeting.title.clone(),
                meeting.description.clone(),
                meeting.organizer_uri.clone(),
            ];
            haystack.extend(meeting.participants.iter().cloned());
            if !collaboration_matches(&haystack, &term) {
                continue;
            }
            results.push(CollaborationSearchResult {
                kind: "meeting".to_string(),
                id: meeting.id,
                title: meeting.title,
                subtitle: meeting.description,
                room_id: meeting.room_id,
                team_id: meeting
                    .room_id
                    .and_then(|room_id| self.room(room_id).and_then(|room| room.team_id)),
                conference_id: meeting.conference_id,
                call_uri: meeting
                    .conference_id
                    .map(|id| format!("sip:conf-{}@pale.local", id)),
                updated_at: meeting.starts_at,
            });
        }

        for conference in self.list_conferences().into_iter().filter(|conference| {
            conference
                .participants
                .iter()
                .any(|participant| participant.sip_uri == sip_uri)
        }) {
            let mut haystack = vec![conference.title.clone(), format!("{:?}", conference.mode)];
            haystack.extend(
                conference
                    .participants
                    .iter()
                    .map(|participant| participant.sip_uri.clone()),
            );
            if !collaboration_matches(&haystack, &term) {
                continue;
            }
            results.push(CollaborationSearchResult {
                kind: "conference".to_string(),
                id: conference.id,
                title: conference.title,
                subtitle: format!("{:?} conference", conference.mode).to_lowercase(),
                room_id: None,
                team_id: None,
                conference_id: Some(conference.id),
                call_uri: Some(format!("sip:conf-{}@pale.local", conference.id)),
                updated_at: conference.created_at,
            });
        }

        results.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.title.cmp(&right.title))
        });
        results.truncate(limit);
        results
    }

    pub fn unified_search(
        &self,
        principal: &str,
        query: &str,
        limit: usize,
    ) -> Vec<UnifiedSearchResult> {
        let term = query.trim().to_ascii_lowercase();
        if term.is_empty() || limit == 0 {
            return Vec::new();
        }
        let limit = limit.clamp(1, 100);
        let mut results = Vec::new();

        for message in self.sip_messages() {
            if !self.is_admin_principal(principal) && !sip_message_visible_to(&message, principal) {
                continue;
            }
            let fields = vec![
                message.body.clone(),
                message.from_uri.clone(),
                message.to_uri.clone(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: message.id.to_string(),
                kind: "message".to_string(),
                title: "Direct message".to_string(),
                snippet: message.body,
                source: message.from_uri.clone(),
                url: None,
                room_id: None,
                team_id: None,
                conference_id: None,
                user_uri: Some(message.from_uri),
                file_id: None,
                app_id: None,
                score: score_match(&fields[0], &term) + 18,
                updated_at: message.received_at,
            });
        }

        for message in self
            .room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|message| room_visible_to(self, message.room_id, principal))
        {
            let body = self.decrypt_field(&message.body);
            if !body.to_ascii_lowercase().contains(&term)
                && !message.sender_uri.to_ascii_lowercase().contains(&term)
            {
                continue;
            }
            let room_title = self
                .room(message.room_id)
                .map(|room| room.name)
                .unwrap_or_else(|| "Room message".to_string());
            results.push(UnifiedSearchResult {
                id: message.id.to_string(),
                kind: "message".to_string(),
                title: room_title,
                snippet: body.chars().take(240).collect(),
                source: message.sender_uri.clone(),
                url: None,
                room_id: Some(message.room_id),
                team_id: self.room(message.room_id).and_then(|room| room.team_id),
                conference_id: None,
                user_uri: Some(message.sender_uri.clone()),
                file_id: None,
                app_id: None,
                score: score_match(&body, &term) + 20,
                updated_at: message.edited_at.unwrap_or(message.created_at),
            });
        }

        for room in self.list_rooms_for_user(principal) {
            let fields = vec![
                room.name.clone(),
                room.description.clone(),
                room.channel_name.clone().unwrap_or_default(),
                room.call_uri.clone().unwrap_or_default(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: room.id.to_string(),
                kind: if room.team_id.is_some() {
                    "channel".to_string()
                } else if room.is_direct {
                    "direct".to_string()
                } else {
                    "room".to_string()
                },
                title: room.name,
                snippet: room.description,
                source: "chat".to_string(),
                url: None,
                room_id: Some(room.id),
                team_id: room.team_id,
                conference_id: room.conference_id,
                user_uri: None,
                file_id: None,
                app_id: None,
                score: 90,
                updated_at: room.created_at,
            });
        }

        for team in self.list_teams_for_user(principal) {
            let fields = vec![
                team.name.clone(),
                team.description.clone(),
                team.owner_uri.clone(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: team.id.to_string(),
                kind: "team".to_string(),
                title: team.name,
                snippet: team.description,
                source: team.owner_uri,
                url: None,
                room_id: None,
                team_id: Some(team.id),
                conference_id: None,
                user_uri: None,
                file_id: None,
                app_id: None,
                score: 85,
                updated_at: team.created_at,
            });
        }

        for user in self.users().into_iter().filter(|user| user.active) {
            let fields = vec![
                user.display_name.clone(),
                user.sip_uri.clone(),
                user.email.clone().unwrap_or_default(),
                user.department.clone().unwrap_or_default(),
                user.title.clone().unwrap_or_default(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: user.id.to_string(),
                kind: "user".to_string(),
                title: user.display_name,
                snippet: user
                    .title
                    .or(user.department)
                    .unwrap_or_else(|| user.sip_uri.clone()),
                source: "directory".to_string(),
                url: None,
                room_id: None,
                team_id: None,
                conference_id: None,
                user_uri: Some(user.sip_uri),
                file_id: None,
                app_id: None,
                score: 80,
                updated_at: user.created_at,
            });
        }

        for meeting in self.list_meetings_for_user(principal) {
            let fields = vec![
                meeting.title.clone(),
                meeting.description.clone(),
                meeting.organizer_uri.clone(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: meeting.id.to_string(),
                kind: "meeting".to_string(),
                title: meeting.title,
                snippet: meeting.description,
                source: meeting.organizer_uri,
                url: None,
                room_id: meeting.room_id,
                team_id: meeting
                    .room_id
                    .and_then(|room_id| self.room(room_id).and_then(|room| room.team_id)),
                conference_id: meeting.conference_id,
                user_uri: None,
                file_id: None,
                app_id: None,
                score: 75,
                updated_at: meeting.starts_at,
            });
        }

        for file in self.file_records() {
            if !self.is_admin_principal(principal) && file.owner != principal {
                continue;
            }
            let fields = vec![
                file.filename.clone(),
                file.content_type.clone(),
                file.sha256.clone(),
                file.dlp_status.clone(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: file.id.to_string(),
                kind: "file".to_string(),
                title: file.filename,
                snippet: format!("{} - {} bytes", file.content_type, file.size),
                source: file.owner,
                url: None,
                room_id: None,
                team_id: None,
                conference_id: None,
                user_uri: None,
                file_id: Some(file.id),
                app_id: None,
                score: 70,
                updated_at: file.created_at,
            });
        }

        for recording in self.discovery_recordings() {
            if recording.deleted_at.is_some() {
                continue;
            }
            if !self.is_admin_principal(principal) && !recording_visible_to(&recording, principal) {
                continue;
            }
            let transcript_hit = recording.conference_id.is_some_and(|conference_id| {
                self.get_transcript(conference_id).iter().any(|segment| {
                    segment.text.to_ascii_lowercase().contains(&term)
                        || segment.speaker_uri.to_ascii_lowercase().contains(&term)
                        || segment.speaker_name.to_ascii_lowercase().contains(&term)
                })
            });
            let fields = vec![
                recording.call_id.clone().unwrap_or_default(),
                recording.caller_uri.clone(),
                recording.callee_uri.clone(),
                recording.recorded_by.clone(),
            ];
            if !transcript_hit && !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: recording.id.to_string(),
                kind: "recording".to_string(),
                title: recording
                    .call_id
                    .clone()
                    .unwrap_or_else(|| "Call recording".to_string()),
                snippet: format!("{} -> {}", recording.caller_uri, recording.callee_uri),
                source: recording.recorded_by,
                url: None,
                room_id: None,
                team_id: None,
                conference_id: recording.conference_id,
                user_uri: None,
                file_id: recording.file_id,
                app_id: None,
                score: if transcript_hit { 72 } else { 62 },
                updated_at: recording.created_at,
            });
        }

        for app in self
            .list_app_catalog()
            .into_iter()
            .filter(|app| app.installed)
        {
            let fields = vec![
                app.name.clone(),
                app.description.clone(),
                app.category.clone(),
                app.version.clone().unwrap_or_default(),
            ];
            if !collaboration_matches(&fields, &term) {
                continue;
            }
            results.push(UnifiedSearchResult {
                id: app.id.to_string(),
                kind: "app".to_string(),
                title: app.name,
                snippet: app.description,
                source: app.category,
                url: app.manifest_url.clone(),
                room_id: None,
                team_id: None,
                conference_id: None,
                user_uri: None,
                file_id: None,
                app_id: Some(app.id),
                score: 45,
                updated_at: app.installed_at.unwrap_or(app.created_at),
            });
        }

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(right.updated_at.cmp(&left.updated_at))
        });
        results.truncate(limit);
        results
    }

    pub fn copilot_query(
        &self,
        principal: &str,
        input: CreateCopilotQueryRequest,
    ) -> Result<CopilotAnswer, String> {
        let question = input.question.trim();
        if question.is_empty() {
            return Err("question is required".to_string());
        }
        let context_query = input
            .context_query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(question);
        let limit = input.limit.unwrap_or(8).clamp(1, 12);
        let results = self.unified_search(principal, context_query, limit);
        let provider_configured = self.enterprise_capability_available("copilot");
        let citations: Vec<_> = results
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, result)| CopilotCitation {
                index: index + 1,
                result: result.clone(),
            })
            .collect();
        let answer = grounded_copilot_answer(question, provider_configured, &citations);
        Ok(CopilotAnswer {
            question: question.to_string(),
            generated_at: Utc::now(),
            provider_configured,
            grounded: !citations.is_empty(),
            answer,
            citations,
            suggested_prompts: copilot_suggested_prompts(&results),
            governance: vec![
                "uses only content returned by governed enterprise search".to_string(),
                "citations are limited to resources visible to the caller".to_string(),
                "external LLM/RAG readiness is reported separately from local grounding"
                    .to_string(),
            ],
        })
    }

    pub fn start_scheduled_meeting(&self, id: Uuid, user_sip_uri: &str) -> Option<RoomCallTarget> {
        let meeting = self.scheduled_meetings.get(&id)?;
        if meeting.status == MeetingStatus::Cancelled {
            return None;
        }
        if !meeting
            .participants
            .iter()
            .any(|participant| participant == user_sip_uri)
            && meeting.organizer_uri != user_sip_uri
        {
            return None;
        }
        if let Some(room_id) = meeting.room_id {
            self.join_room_call(room_id, user_sip_uri, RoomCallMode::Video)
        } else {
            let conference_id = meeting.conference_id?;
            let _ = self.activate_conference(conference_id);
            let user_id = self
                .users
                .values()
                .into_iter()
                .find(|user| user.sip_uri == user_sip_uri)
                .map(|user| user.id)
                .unwrap_or_else(Uuid::nil);
            let _ = self.join_conference(
                conference_id,
                JoinConferenceRequest {
                    user_id,
                    sip_uri: user_sip_uri.to_string(),
                    role: Some(ParticipantRole::Member),
                },
                true,
            );
            Some(RoomCallTarget {
                room_id: Uuid::nil(),
                conference_id,
                call_uri: format!("sip:conf-{}@pale.local", conference_id),
                mode: RoomCallMode::Video,
            })
        }
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

    pub fn send_room_message(
        &self,
        room_id: Uuid,
        sender_uri: &str,
        body: &str,
        reply_to: Option<Uuid>,
        priority: Option<String>,
    ) -> Result<RoomMessage, String> {
        let room = self.room(room_id);
        let (mentions, mentioned_user_uris) = room
            .as_ref()
            .map(|room| self.resolve_message_mentions(room, body))
            .unwrap_or_default();
        if let Some(room) = room.as_ref() {
            // Enforce information barriers
            for member in &room.members {
                if member.user_sip_uri != sender_uri {
                    let result = self.check_barrier(sender_uri, &member.user_sip_uri, false);
                    if result.blocked {
                        return Err(format!(
                            "communication blocked by information barrier: {}",
                            result.barrier_name.unwrap_or_default()
                        ));
                    }
                }
            }
            if room.posting_policy == "owners"
                && !room.channel_owners.iter().any(|owner| owner == sender_uri)
                && !room.members.iter().any(|member| {
                    member.user_sip_uri == sender_uri
                        && matches!(member.role.as_str(), "owner" | "admin")
                })
            {
                return Err("only channel owners can post in this channel".to_string());
            }
            self.authorize_message_mentions(room, sender_uri, &mentions)?;
        }
        let priority = normalize_message_priority(priority.as_deref());
        if priority == "urgent" && !self.collaboration_policy().urgent_messages_enabled {
            return Err("urgent messages are disabled by policy".to_string());
        }
        let encrypted_body = self.encrypt_field(body);
        let msg = RoomMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_uri: sender_uri.to_string(),
            body: encrypted_body,
            content_type: "text/plain".to_string(),
            created_at: Utc::now(),
            reply_to,
            edited_at: None,
            pinned: false,
            mentions,
            mentioned_user_uris,
            priority,
            saved_by: Vec::new(),
            scheduled_at: None,
            delivered: true,
            delivery_status: "sent".to_string(),
            card_payload: None,
            thread_id: None,
        };
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        messages.push(msg.clone());
        if messages.len() > MAX_ROOM_MESSAGES {
            let overflow = messages.len() - MAX_ROOM_MESSAGES;
            messages.drain(..overflow);
        }
        self.persist(&msg);
        let msg_for_pg = msg.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_room_message(&msg_for_pg).await }));
        let mut decrypted_msg = msg.clone();
        decrypted_msg.body = self.decrypt_field(&decrypted_msg.body);
        self.broadcast_sse(SseEvent {
            event_type: "room_message".to_string(),
            payload: serde_json::to_value(&decrypted_msg).unwrap_or_default(),
        });
        // Push notifications to mentioned users and DM recipients
        if let Some(room) = room.as_ref() {
            let plain_body = self.decrypt_field(&msg.body);
            // Notify mentioned users
            for mentioned_uri in &msg.mentioned_user_uris {
                if mentioned_uri != sender_uri {
                    self.notify_user_push(
                        mentioned_uri,
                        &format!("Mentioned by {}", sip_user_part(sender_uri)),
                        &plain_body,
                        Some(format!("room-{}", room_id)),
                    );
                }
            }
            // For DMs (2-member rooms), notify the other party
            if room.members.len() == 2 && msg.mentioned_user_uris.is_empty() {
                for member in &room.members {
                    if member.user_sip_uri != sender_uri {
                        self.notify_user_push(
                            &member.user_sip_uri,
                            &format!("Message from {}", sip_user_part(sender_uri)),
                            &plain_body,
                            Some(format!("dm-{}", room_id)),
                        );
                    }
                }
            }
        }
        Ok(decrypted_msg)
    }

    /// Send a room message with optional adaptive card payload.
    pub fn send_room_message_with_card(
        &self,
        room_id: Uuid,
        sender_uri: &str,
        body: &str,
        reply_to: Option<Uuid>,
        priority: Option<String>,
        card_payload: Option<AdaptiveCard>,
    ) -> Result<RoomMessage, String> {
        let mut msg = self.send_room_message(room_id, sender_uri, body, reply_to, priority)?;
        if let Some(card) = card_payload {
            msg.card_payload = Some(card);
            // Update the message in-place
            let mut messages = self
                .room_messages
                .write()
                .expect("room messages lock poisoned");
            if let Some(existing) = messages.iter_mut().find(|m| m.id == msg.id) {
                existing.card_payload = msg.card_payload.clone();
            }
        }
        Ok(msg)
    }

    /// Schedule a message for future delivery.
    pub fn schedule_room_message(
        &self,
        room_id: Uuid,
        sender_uri: &str,
        body: &str,
        scheduled_at: DateTime<Utc>,
        reply_to: Option<Uuid>,
        priority: Option<String>,
    ) -> Result<RoomMessage, String> {
        let room = self.room(room_id);
        let (mentions, mentioned_user_uris) = room
            .as_ref()
            .map(|room| self.resolve_message_mentions(room, body))
            .unwrap_or_default();
        if let Some(room) = room.as_ref() {
            // Enforce information barriers
            for member in &room.members {
                if member.user_sip_uri != sender_uri {
                    let result = self.check_barrier(sender_uri, &member.user_sip_uri, false);
                    if result.blocked {
                        return Err(format!(
                            "communication blocked by information barrier: {}",
                            result.barrier_name.unwrap_or_default()
                        ));
                    }
                }
            }
            if room.posting_policy == "owners"
                && !room.channel_owners.iter().any(|owner| owner == sender_uri)
                && !room.members.iter().any(|member| {
                    member.user_sip_uri == sender_uri
                        && matches!(member.role.as_str(), "owner" | "admin")
                })
            {
                return Err("only channel owners can post in this channel".to_string());
            }
            self.authorize_message_mentions(room, sender_uri, &mentions)?;
        }
        let priority = normalize_message_priority(priority.as_deref());
        let encrypted_body = self.encrypt_field(body);
        let msg = RoomMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_uri: sender_uri.to_string(),
            body: encrypted_body,
            content_type: "text/plain".to_string(),
            created_at: Utc::now(),
            reply_to,
            edited_at: None,
            pinned: false,
            mentions,
            mentioned_user_uris,
            priority,
            saved_by: Vec::new(),
            scheduled_at: Some(scheduled_at),
            delivered: false,
            delivery_status: "pending".to_string(),
            card_payload: None,
            thread_id: None,
        };
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        messages.push(msg.clone());
        if messages.len() > MAX_ROOM_MESSAGES {
            let overflow = messages.len() - MAX_ROOM_MESSAGES;
            messages.drain(..overflow);
        }
        self.persist(&msg);
        let msg_for_pg = msg.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.insert_room_message(&msg_for_pg).await }));
        Ok(msg)
    }

    /// Deliver scheduled messages that are due. Called by background task.
    pub fn deliver_scheduled_messages(&self) -> Vec<RoomMessage> {
        let now = Utc::now();
        let mut delivered = Vec::new();
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        for msg in messages.iter_mut() {
            if !msg.delivered {
                if let Some(scheduled_at) = msg.scheduled_at {
                    if scheduled_at <= now {
                        msg.delivered = true;
                        msg.delivery_status = "sent".to_string();
                        delivered.push(msg.clone());
                    }
                }
            }
        }
        drop(messages);
        for msg in &delivered {
            self.persist(msg);
            let msg_for_pg = msg.clone();
            self.pg_spawn(move |pg| {
                Box::pin(async move { pg.insert_room_message(&msg_for_pg).await })
            });
            let mut decrypted_msg = msg.clone();
            decrypted_msg.body = self.decrypt_field(&decrypted_msg.body);
            self.broadcast_sse(SseEvent {
                event_type: "room_message".to_string(),
                payload: serde_json::to_value(&decrypted_msg).unwrap_or_default(),
            });
            self.broadcast_sse(SseEvent {
                event_type: "scheduled_message_delivered".to_string(),
                payload: serde_json::to_value(&decrypted_msg).unwrap_or_default(),
            });
        }
        delivered
    }

    // ─── Tags ───

    pub fn create_tag(
        &self,
        team_id: Uuid,
        name: &str,
        members: Vec<String>,
    ) -> Result<Tag, String> {
        // Check team exists
        if self.teams.get(&team_id).is_none() {
            return Err("team not found".to_string());
        }
        // Check duplicate name
        let duplicate = self
            .tags
            .values()
            .iter()
            .any(|t| t.team_id == team_id && t.name.eq_ignore_ascii_case(name));
        if duplicate {
            return Err("tag name already exists in this team".to_string());
        }
        let tag = Tag {
            id: Uuid::new_v4(),
            team_id,
            name: name.to_string(),
            members,
            created_at: Utc::now(),
        };
        self.tags.insert(tag.id, tag.clone());
        Ok(tag)
    }

    pub fn list_tags(&self, team_id: Uuid) -> Vec<Tag> {
        self.tags
            .values()
            .into_iter()
            .filter(|t| t.team_id == team_id)
            .collect()
    }

    pub fn update_tag(
        &self,
        tag_id: Uuid,
        name: Option<String>,
        members: Option<Vec<String>>,
    ) -> Option<Tag> {
        let mut tag = self.tags.get(&tag_id)?;
        if let Some(new_name) = name {
            tag.name = new_name;
        }
        if let Some(new_members) = members {
            tag.members = new_members;
        }
        self.tags.insert(tag_id, tag.clone());
        Some(tag)
    }

    pub fn delete_tag(&self, tag_id: Uuid) -> Option<Tag> {
        self.tags.remove(&tag_id)
    }

    // ─── Notification Preferences ───

    pub fn get_notification_preference(
        &self,
        room_id: Uuid,
        user_uri: &str,
    ) -> NotificationPreference {
        let key = format!("{}:{}", room_id, user_uri);
        self.notification_preferences
            .get(&key)
            .unwrap_or(NotificationPreference {
                room_id,
                user_uri: user_uri.to_string(),
                notification_level: "all".to_string(),
                updated_at: Utc::now(),
            })
    }

    pub fn set_notification_preference(
        &self,
        room_id: Uuid,
        user_uri: &str,
        level: &str,
    ) -> NotificationPreference {
        let valid_level = match level {
            "all" | "mentions" | "muted" => level.to_string(),
            _ => "all".to_string(),
        };
        let pref = NotificationPreference {
            room_id,
            user_uri: user_uri.to_string(),
            notification_level: valid_level,
            updated_at: Utc::now(),
        };
        let key = format!("{}:{}", room_id, user_uri);
        self.notification_preferences.insert(key, pref.clone());
        pref
    }

    pub fn edit_room_message(
        &self,
        id: Uuid,
        editor_uri: &str,
        new_body: &str,
    ) -> Result<RoomMessage, String> {
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        let msg = messages
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| "message not found".to_string())?;
        msg.body = new_body.to_string();
        msg.edited_at = Some(Utc::now());
        if let Some(room) = self.room(msg.room_id) {
            let (mentions, mentioned_user_uris) = self.resolve_message_mentions(&room, new_body);
            self.authorize_message_mentions(&room, editor_uri, &mentions)?;
            msg.mentions = mentions;
            msg.mentioned_user_uris = mentioned_user_uris;
        }
        let updated = msg.clone();
        self.broadcast_sse(SseEvent {
            event_type: "message_edited".to_string(),
            payload: serde_json::to_value(&updated).unwrap_or_default(),
        });
        self.persist(&updated);
        let updated_for_pg = updated.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move { pg.insert_room_message(&updated_for_pg).await })
        });
        Ok(updated)
    }

    fn resolve_message_mentions(
        &self,
        room: &Room,
        body: &str,
    ) -> (Vec<MessageMention>, Vec<String>) {
        let policy = self.collaboration_policy();
        if !policy.structured_mentions_enabled {
            return (Vec::new(), Vec::new());
        }
        let normalized_body = body.to_lowercase();
        let mut mentions = Vec::new();
        let mut mentioned_user_uris = Vec::new();

        if normalized_body.contains("@channel") {
            mentions.push(MessageMention {
                kind: "channel".to_string(),
                token: "channel".to_string(),
                user_sip_uri: None,
            });
            mentioned_user_uris.extend(
                room.members
                    .iter()
                    .map(|member| member.user_sip_uri.clone()),
            );
        }

        if normalized_body.contains("@team") {
            mentions.push(MessageMention {
                kind: "team".to_string(),
                token: "team".to_string(),
                user_sip_uri: None,
            });
            mentioned_user_uris.extend(
                room.members
                    .iter()
                    .map(|member| member.user_sip_uri.clone()),
            );
        }

        // Resolve @tag mentions
        if let Some(team_id) = room.team_id {
            for tag in self.tags.values() {
                if tag.team_id != team_id {
                    continue;
                }
                let tag_token = format!("@{}", tag.name.to_lowercase());
                if normalized_body.contains(&tag_token) {
                    mentions.push(MessageMention {
                        kind: "tag".to_string(),
                        token: tag.name.clone(),
                        user_sip_uri: None,
                    });
                    mentioned_user_uris.extend(tag.members.clone());
                }
            }
        }

        for member in &room.members {
            let Some(user) = self
                .users
                .values()
                .into_iter()
                .find(|user| user.sip_uri == member.user_sip_uri)
            else {
                continue;
            };
            let display_token = format!("@{}", user.display_name.to_lowercase());
            let sip_user_token = format!("@{}", sip_user_part(&user.sip_uri).to_lowercase());
            if normalized_body.contains(&display_token) || normalized_body.contains(&sip_user_token)
            {
                mentions.push(MessageMention {
                    kind: "user".to_string(),
                    token: user.display_name.clone(),
                    user_sip_uri: Some(user.sip_uri.clone()),
                });
                mentioned_user_uris.push(user.sip_uri);
            }
        }

        mentioned_user_uris.sort();
        mentioned_user_uris.dedup();
        mentions.sort_by(|left, right| {
            (
                left.kind.as_str(),
                left.token.as_str(),
                left.user_sip_uri.as_deref().unwrap_or(""),
            )
                .cmp(&(
                    right.kind.as_str(),
                    right.token.as_str(),
                    right.user_sip_uri.as_deref().unwrap_or(""),
                ))
        });
        mentions.dedup();
        (mentions, mentioned_user_uris)
    }

    fn authorize_message_mentions(
        &self,
        room: &Room,
        sender_uri: &str,
        mentions: &[MessageMention],
    ) -> Result<(), String> {
        let has_broad_mention = mentions
            .iter()
            .any(|mention| mention.kind == "channel" || mention.kind == "team");
        if !has_broad_mention {
            return Ok(());
        }
        let policy = self.collaboration_policy();
        if !policy.broad_mentions_enabled {
            return Err("broad mentions are disabled".to_string());
        }
        let member_role = room
            .members
            .iter()
            .find(|member| member.user_sip_uri == sender_uri)
            .map(|member| member.role.as_str())
            .unwrap_or("member");
        let allowed = policy
            .broad_mentions_allowed_roles
            .iter()
            .any(|role| role.eq_ignore_ascii_case(member_role));
        if !allowed {
            return Err("sender is not allowed to use broad mentions".to_string());
        }
        if !self.check_broad_mention_rate_limit(sender_uri, policy.broad_mentions_per_minute) {
            return Err("broad mention rate limit exceeded".to_string());
        }
        Ok(())
    }

    fn check_broad_mention_rate_limit(&self, principal: &str, max_per_minute: u32) -> bool {
        if max_per_minute == 0 {
            return false;
        }
        let key = principal.to_string();
        let max_tokens = max_per_minute as f64;
        let now = Utc::now();
        self.mention_rate_limits.with_write(&key, |buckets| {
            let bucket = buckets
                .entry(key.clone())
                .or_insert_with(|| RateLimitBucket {
                    tokens: max_tokens,
                    last_refill: now,
                });
            let elapsed_minutes =
                (now - bucket.last_refill).num_milliseconds().max(0) as f64 / 60_000.0;
            bucket.tokens = (bucket.tokens + elapsed_minutes * max_tokens).min(max_tokens);
            bucket.last_refill = now;
            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                true
            } else {
                false
            }
        })
    }

    pub fn pin_room_message(&self, id: Uuid) -> Option<RoomMessage> {
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
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

    pub fn set_message_saved(&self, id: Uuid, user_uri: &str, saved: bool) -> Option<RoomMessage> {
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        let msg = messages.iter_mut().find(|msg| msg.id == id)?;
        if saved {
            if !msg.saved_by.iter().any(|user| user == user_uri) {
                msg.saved_by.push(user_uri.to_string());
            }
        } else {
            msg.saved_by.retain(|user| user != user_uri);
        }
        let updated = msg.clone();
        drop(messages);
        self.persist(&updated);
        let updated_for_pg = updated.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move { pg.insert_room_message(&updated_for_pg).await })
        });
        self.broadcast_sse(SseEvent {
            event_type: "message_saved".to_string(),
            payload: serde_json::to_value(&updated).unwrap_or_default(),
        });
        Some(updated)
    }

    pub fn delete_room_message(&self, id: Uuid) -> Option<RoomMessage> {
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        let index = messages.iter().position(|m| m.id == id)?;
        let deleted = messages.remove(index);
        self.delete_persisted(RoomMessage::collection(), deleted.key());
        self.delete_message_reads_for_message(id);
        self.delete_message_reactions_for_message(id);
        self.pg_spawn(move |pg| Box::pin(async move { pg.delete_room_message(id).await }));
        Some(deleted)
    }

    pub fn upsert_retention_policy(
        &self,
        principal: &str,
        input: UpsertRetentionPolicyRequest,
    ) -> RetentionPolicy {
        let id = input.id.unwrap_or_else(Uuid::new_v4);
        let created_by = self
            .retention_policies
            .get(&id)
            .map(|policy| policy.created_by)
            .unwrap_or_else(|| principal.to_string());
        let policy = RetentionPolicy {
            id,
            name: input.name,
            scope: input.scope,
            room_id: input.room_id,
            retain_days: input.retain_days,
            legal_hold: input.legal_hold.unwrap_or(false),
            export_enabled: input.export_enabled.unwrap_or(true),
            created_by,
            updated_at: Utc::now(),
        };
        self.retention_policies.insert(policy.id, policy.clone());
        self.persist(&policy);
        let policy_for_pg = policy.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(
                    RetentionPolicy::collection(),
                    policy_for_pg.key(),
                    &policy_for_pg,
                )
                .await
            })
        });
        policy
    }

    pub fn retention_policies(&self) -> Vec<RetentionPolicy> {
        self.retention_policies.values()
    }

    pub fn delete_retention_policy(&self, id: Uuid) -> bool {
        if self.retention_policies.remove(&id).is_some() {
            self.delete_persisted(RetentionPolicy::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    pub fn collaboration_policy(&self) -> CollaborationPolicy {
        self.collaboration_policy
            .read()
            .expect("collaboration policy lock poisoned")
            .clone()
    }

    pub fn update_collaboration_policy(
        &self,
        principal: &str,
        input: UpdateCollaborationPolicyRequest,
    ) -> CollaborationPolicy {
        let mut policy = self
            .collaboration_policy
            .write()
            .expect("collaboration policy lock poisoned");
        if let Some(enabled) = input.structured_mentions_enabled {
            policy.structured_mentions_enabled = enabled;
        }
        if let Some(enabled) = input.broad_mentions_enabled {
            policy.broad_mentions_enabled = enabled;
        }
        if let Some(roles) = input.broad_mentions_allowed_roles {
            policy.broad_mentions_allowed_roles = roles
                .into_iter()
                .map(|role| role.trim().to_ascii_lowercase())
                .filter(|role| !role.is_empty())
                .collect();
        }
        if let Some(limit) = input.broad_mentions_per_minute {
            policy.broad_mentions_per_minute = limit.min(60);
        }
        if let Some(enabled) = input.external_access_enabled {
            policy.external_access_enabled = enabled;
        }
        if let Some(domains) = input.allowed_external_domains {
            policy.allowed_external_domains = normalized_policy_domains(domains);
        }
        if let Some(enabled) = input.urgent_messages_enabled {
            policy.urgent_messages_enabled = enabled;
        }
        if let Some(enabled) = input.meeting_recording_enabled {
            policy.meeting_recording_enabled = enabled;
        }
        policy.updated_by = Some(principal.to_string());
        policy.updated_at = Utc::now();
        let updated = policy.clone();
        drop(policy);

        self.persist(&updated);
        let policy_for_pg = updated.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                pg.upsert_business_object(
                    CollaborationPolicy::collection(),
                    policy_for_pg.key(),
                    &policy_for_pg,
                )
                .await
            })
        });
        self.broadcast_sse(SseEvent {
            event_type: "collaboration_policy_updated".to_string(),
            payload: serde_json::to_value(&updated).unwrap_or_default(),
        });
        updated
    }

    pub fn list_channel_webhooks(&self, room_id: Uuid) -> Vec<ChannelWebhook> {
        self.channel_webhooks
            .values()
            .into_iter()
            .filter(|webhook| webhook.room_id == room_id)
            .collect()
    }

    pub fn create_channel_webhook(
        &self,
        room_id: Uuid,
        creator_uri: &str,
        input: CreateChannelWebhookRequest,
    ) -> Result<CreateChannelWebhookResponse, String> {
        let room = self
            .room(room_id)
            .ok_or_else(|| "room not found".to_string())?;
        if room.is_direct {
            return Err("connectors can only be added to rooms and channels".to_string());
        }
        let name = input.name.trim();
        if name.is_empty() {
            return Err("webhook name is required".to_string());
        }
        let token = format!("wh_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let webhook = ChannelWebhook {
            id: Uuid::new_v4(),
            room_id,
            name: name.chars().take(80).collect(),
            description: input
                .description
                .unwrap_or_default()
                .trim()
                .chars()
                .take(240)
                .collect(),
            token_hash: sha256_hex(token.as_bytes()),
            enabled: true,
            created_by: creator_uri.to_string(),
            created_at: Utc::now(),
            last_used_at: None,
        };
        self.channel_webhooks.insert(webhook.id, webhook.clone());
        self.persist(&webhook);
        self.ensure_webhook_room_principal(&room, &webhook);
        Ok(CreateChannelWebhookResponse {
            webhook: webhook.into(),
            token,
        })
    }

    pub fn delete_channel_webhook(
        &self,
        room_id: Uuid,
        webhook_id: Uuid,
    ) -> Option<ChannelWebhook> {
        let webhook = self.channel_webhooks.get(&webhook_id)?;
        if webhook.room_id != room_id {
            return None;
        }
        let deleted = self.channel_webhooks.remove(&webhook_id)?;
        self.delete_persisted(ChannelWebhook::collection(), deleted.key());
        self.remove_webhook_room_principal(room_id, &deleted.principal_uri());
        Some(deleted)
    }

    pub fn set_channel_webhook_enabled(
        &self,
        room_id: Uuid,
        webhook_id: Uuid,
        enabled: bool,
    ) -> Option<ChannelWebhook> {
        let webhook = self.channel_webhooks.with_write(&webhook_id, |webhooks| {
            let webhook = webhooks.get_mut(&webhook_id)?;
            if webhook.room_id != room_id {
                return None;
            }
            webhook.enabled = enabled;
            Some(webhook.clone())
        })?;
        self.persist(&webhook);
        Some(webhook)
    }

    pub fn post_channel_webhook(
        &self,
        token: &str,
        input: PostChannelWebhookRequest,
    ) -> Result<RoomMessage, String> {
        let token_hash = sha256_hex(token.as_bytes());
        let mut webhook = self
            .channel_webhooks
            .values()
            .into_iter()
            .find(|webhook| webhook.token_hash == token_hash)
            .ok_or_else(|| "webhook not found".to_string())?;
        if !webhook.enabled {
            return Err("webhook is disabled".to_string());
        }
        let text = input.text.trim();
        if text.is_empty() {
            return Err("message text is required".to_string());
        }
        if text.chars().count() > 4000 {
            return Err("message text is too long".to_string());
        }
        let body = if let Some(title) = input
            .title
            .as_ref()
            .map(|title| title.trim())
            .filter(|title| !title.is_empty())
        {
            format!(
                "**{}**\n{}",
                title.chars().take(120).collect::<String>(),
                text
            )
        } else {
            text.to_string()
        };
        let message = self.send_room_message(
            webhook.room_id,
            &webhook.principal_uri(),
            &body,
            None,
            Some("normal".to_string()),
        )?;
        webhook.last_used_at = Some(Utc::now());
        self.channel_webhooks.insert(webhook.id, webhook.clone());
        self.persist(&webhook);
        Ok(message)
    }

    fn ensure_webhook_room_principal(&self, room: &Room, webhook: &ChannelWebhook) {
        let principal = webhook.principal_uri();
        let Some(updated) = self.rooms.with_write(&room.id, |rooms| {
            let room = rooms.get_mut(&room.id)?;
            if !room
                .members
                .iter()
                .any(|member| member.user_sip_uri == principal)
            {
                room.members.push(RoomMember {
                    user_sip_uri: principal.clone(),
                    role: "admin".to_string(),
                    joined_at: Utc::now(),
                });
            }
            if !room.channel_owners.iter().any(|owner| owner == &principal) {
                room.channel_owners.push(principal);
                room.channel_owners.sort();
                room.channel_owners.dedup();
            }
            Some(room.clone())
        }) else {
            return;
        };
        self.persist(&updated);
        let room_for_pg = updated.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
    }

    fn remove_webhook_room_principal(&self, room_id: Uuid, principal: &str) {
        let Some(updated) = self.rooms.with_write(&room_id, |rooms| {
            let room = rooms.get_mut(&room_id)?;
            room.members
                .retain(|member| member.user_sip_uri != principal);
            room.channel_owners.retain(|owner| owner != principal);
            Some(room.clone())
        }) else {
            return;
        };
        self.persist(&updated);
        let room_for_pg = updated.clone();
        self.pg_spawn(move |pg| Box::pin(async move { pg.upsert_room(&room_for_pg).await }));
    }

    pub fn authorize_external_participants(
        &self,
        actor_uri: &str,
        participants: &[String],
    ) -> Result<(), String> {
        let Some(actor_domain) = sip_domain(actor_uri) else {
            return Ok(());
        };
        let policy = self.collaboration_policy();
        for participant in participants {
            let Some(domain) = sip_domain(participant) else {
                continue;
            };
            if domain == actor_domain {
                continue;
            }
            if !policy.external_access_enabled {
                return Err("external access is disabled by policy".to_string());
            }
            if !policy.allowed_external_domains.is_empty()
                && !policy
                    .allowed_external_domains
                    .iter()
                    .any(|allowed| allowed == &domain)
            {
                return Err(format!("external domain {domain} is not allowed by policy"));
            }
        }
        Ok(())
    }

    pub fn discovery_export(&self, room_id: Option<Uuid>) -> DiscoveryExport {
        let messages = self
            .room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|message| room_id.is_none_or(|id| message.room_id == id))
            .cloned()
            .map(|mut message| {
                message.body = self.decrypt_field(&message.body);
                message
            })
            .collect();
        DiscoveryExport {
            exported_at: Utc::now(),
            room_id,
            messages,
            files: if room_id.is_none() {
                self.discovery_file_records()
            } else {
                Vec::new()
            },
            recordings: if room_id.is_none() {
                self.discovery_recordings()
            } else {
                Vec::new()
            },
        }
    }

    pub fn discovery_search(&self, query: DiscoverySearchQuery) -> DiscoveryExport {
        let term = query
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let user = query
            .user_uri
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let limit = query.limit.unwrap_or(250).clamp(1, 1000);

        let mut messages: Vec<RoomMessage> = self
            .room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|message| query.room_id.is_none_or(|id| message.room_id == id))
            .filter(|message| query.from.is_none_or(|from| message.created_at >= from))
            .filter(|message| query.to.is_none_or(|to| message.created_at <= to))
            .filter(|message| {
                user.as_ref()
                    .is_none_or(|user| message.sender_uri.to_ascii_lowercase().contains(user))
            })
            .cloned()
            .map(|mut message| {
                message.body = self.decrypt_field(&message.body);
                message
            })
            .filter(|message| {
                term.as_ref()
                    .is_none_or(|term| message.body.to_ascii_lowercase().contains(term))
            })
            .collect();
        messages.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        messages.truncate(limit);

        let files = if query.room_id.is_none() {
            let mut files: Vec<FileDiscoveryRecord> = self
                .discovery_file_records()
                .into_iter()
                .filter(|file| query.from.is_none_or(|from| file.created_at >= from))
                .filter(|file| query.to.is_none_or(|to| file.created_at <= to))
                .filter(|file| {
                    user.as_ref()
                        .is_none_or(|user| file.owner.to_ascii_lowercase().contains(user))
                })
                .filter(|file| {
                    term.as_ref().is_none_or(|term| {
                        file.filename.to_ascii_lowercase().contains(term)
                            || file.content_type.to_ascii_lowercase().contains(term)
                            || file.sha256.to_ascii_lowercase().contains(term)
                            || file.dlp_status.to_ascii_lowercase().contains(term)
                    })
                })
                .collect();
            files.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            files.truncate(limit);
            files
        } else {
            Vec::new()
        };

        let mut recordings: Vec<CallRecording> = self
            .discovery_recordings()
            .into_iter()
            .filter(|recording| query.from.is_none_or(|from| recording.created_at >= from))
            .filter(|recording| query.to.is_none_or(|to| recording.created_at <= to))
            .filter(|recording| {
                user.as_ref().is_none_or(|user| {
                    recording.caller_uri.to_ascii_lowercase().contains(user)
                        || recording.callee_uri.to_ascii_lowercase().contains(user)
                        || recording.recorded_by.to_ascii_lowercase().contains(user)
                })
            })
            .filter(|recording| {
                term.as_ref().is_none_or(|term| {
                    recording
                        .call_id
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(term)
                        || recording.caller_uri.to_ascii_lowercase().contains(term)
                        || recording.callee_uri.to_ascii_lowercase().contains(term)
                        || recording.recorded_by.to_ascii_lowercase().contains(term)
                        || recording.conference_id.is_some_and(|conference_id| {
                            self.get_transcript(conference_id).iter().any(|segment| {
                                segment.text.to_ascii_lowercase().contains(term)
                                    || segment.speaker_uri.to_ascii_lowercase().contains(term)
                                    || segment.speaker_name.to_ascii_lowercase().contains(term)
                            })
                        })
                })
            })
            .collect();
        recordings.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        recordings.truncate(limit);

        DiscoveryExport {
            exported_at: Utc::now(),
            room_id: query.room_id,
            messages,
            files,
            recordings,
        }
    }

    pub fn list_ediscovery_cases(&self) -> Vec<EDiscoveryCase> {
        let mut cases = self.ediscovery_cases.values();
        cases.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        cases
    }

    pub fn create_ediscovery_case(
        &self,
        principal: &str,
        input: CreateEDiscoveryCaseRequest,
    ) -> Result<EDiscoveryCase, String> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err("case name is required".to_string());
        }
        let now = Utc::now();
        let case = EDiscoveryCase {
            id: Uuid::new_v4(),
            name: name.chars().take(120).collect(),
            description: input.description.trim().chars().take(1000).collect(),
            status: EDiscoveryCaseStatus::Open,
            custodians: normalized_case_custodians(input.custodians),
            query: normalized_case_query(input.query),
            created_by: principal.to_string(),
            created_at: now,
            updated_at: now,
            last_exported_at: None,
            last_exported_by: None,
            last_export_count: 0,
        };
        self.ediscovery_cases.insert(case.id, case.clone());
        self.persist(&case);
        Ok(case)
    }

    pub fn update_ediscovery_case(
        &self,
        id: Uuid,
        input: UpdateEDiscoveryCaseRequest,
    ) -> Result<EDiscoveryCase, String> {
        let updated = self
            .ediscovery_cases
            .with_write(&id, |cases| {
                let case = cases.get_mut(&id)?;
                if let Some(name) = input.name {
                    let name = name.trim();
                    if name.is_empty() {
                        return None;
                    }
                    case.name = name.chars().take(120).collect();
                }
                if let Some(description) = input.description {
                    case.description = description.trim().chars().take(1000).collect();
                }
                if let Some(status) = input.status {
                    case.status = status;
                }
                if let Some(custodians) = input.custodians {
                    case.custodians = normalized_case_custodians(custodians);
                }
                if let Some(query) = input.query {
                    case.query = normalized_case_query(query);
                }
                case.updated_at = Utc::now();
                Some(case.clone())
            })
            .ok_or_else(|| "case not found or invalid".to_string())?;
        self.persist(&updated);
        Ok(updated)
    }

    pub fn delete_ediscovery_case(&self, id: Uuid) -> bool {
        if self.ediscovery_cases.remove(&id).is_some() {
            self.delete_persisted(EDiscoveryCase::collection(), id.to_string());
            true
        } else {
            false
        }
    }

    pub fn export_ediscovery_case(
        &self,
        id: Uuid,
        principal: &str,
    ) -> Result<DiscoveryExport, String> {
        let case = self
            .ediscovery_cases
            .get(&id)
            .ok_or_else(|| "case not found".to_string())?;
        let export = self.discovery_case_export(&case);
        let count = export.messages.len() + export.files.len() + export.recordings.len();
        let updated = self
            .ediscovery_cases
            .with_write(&id, |cases| {
                let case = cases.get_mut(&id)?;
                case.last_exported_at = Some(export.exported_at);
                case.last_exported_by = Some(principal.to_string());
                case.last_export_count = count;
                case.updated_at = Utc::now();
                Some(case.clone())
            })
            .ok_or_else(|| "case not found".to_string())?;
        self.persist(&updated);
        Ok(export)
    }

    fn discovery_case_export(&self, case: &EDiscoveryCase) -> DiscoveryExport {
        if case.custodians.len() <= 1 || case.query.user_uri.is_some() {
            return self.discovery_search(case.to_discovery_query());
        }

        let mut messages: HashMap<Uuid, RoomMessage> = HashMap::new();
        let mut files: HashMap<Uuid, FileDiscoveryRecord> = HashMap::new();
        let mut recordings: HashMap<Uuid, CallRecording> = HashMap::new();
        for custodian in &case.custodians {
            let mut query = case.query.clone();
            query.user_uri = Some(custodian.clone());
            let export = self.discovery_search(DiscoverySearchQuery {
                q: query.q,
                user_uri: query.user_uri,
                room_id: query.room_id,
                from: query.from,
                to: query.to,
                limit: query.limit,
            });
            for message in export.messages {
                messages.insert(message.id, message);
            }
            for file in export.files {
                files.insert(file.id, file);
            }
            for recording in export.recordings {
                recordings.insert(recording.id, recording);
            }
        }

        let mut messages: Vec<_> = messages.into_values().collect();
        messages.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut files: Vec<_> = files.into_values().collect();
        files.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut recordings: Vec<_> = recordings.into_values().collect();
        recordings.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let limit = case.query.limit.unwrap_or(250).clamp(1, 1000);
        messages.truncate(limit);
        files.truncate(limit);
        recordings.truncate(limit);

        DiscoveryExport {
            exported_at: Utc::now(),
            room_id: case.query.room_id,
            messages,
            files,
            recordings,
        }
    }

    pub fn enforce_retention(&self, dry_run: bool) -> RetentionEnforcementResult {
        let evaluated_at = Utc::now();
        let policies = self.retention_policies();
        let messages = self
            .room_messages
            .read()
            .expect("room messages lock poisoned")
            .clone();
        let files = self.files.values();
        let recordings = self.recordings.values();
        let mut policy_results = Vec::new();
        let mut skipped_legal_hold_policies = Vec::new();
        let mut ids_to_delete = Vec::new();
        let mut file_ids_to_delete = Vec::new();
        let mut recording_ids_to_delete = Vec::new();

        for policy in policies {
            if policy.legal_hold {
                skipped_legal_hold_policies.push(policy.id);
                policy_results.push(RetentionPolicyResult {
                    policy_id: policy.id,
                    room_id: policy.room_id,
                    retain_days: policy.retain_days,
                    matched_messages: 0,
                    deleted_messages: 0,
                    matched_files: 0,
                    deleted_files: 0,
                    matched_recordings: 0,
                    deleted_recordings: 0,
                    legal_hold: true,
                });
                continue;
            }
            let Some(retain_days) = policy.retain_days else {
                policy_results.push(RetentionPolicyResult {
                    policy_id: policy.id,
                    room_id: policy.room_id,
                    retain_days: None,
                    matched_messages: 0,
                    deleted_messages: 0,
                    matched_files: 0,
                    deleted_files: 0,
                    matched_recordings: 0,
                    deleted_recordings: 0,
                    legal_hold: false,
                });
                continue;
            };
            let cutoff = evaluated_at - Duration::days(retain_days.max(0));
            let applies_to_messages = matches!(
                policy.scope.as_str(),
                "global" | "messages" | "rooms" | "room"
            );
            let applies_to_files = policy.room_id.is_none()
                && matches!(policy.scope.as_str(), "global" | "files" | "file");
            let applies_to_recordings = policy.room_id.is_none()
                && matches!(policy.scope.as_str(), "global" | "recordings" | "recording");
            let matched: Vec<Uuid> = if applies_to_messages {
                messages
                    .iter()
                    .filter(|message| {
                        policy
                            .room_id
                            .is_none_or(|room_id| message.room_id == room_id)
                    })
                    .filter(|message| message.created_at < cutoff)
                    .map(|message| message.id)
                    .collect()
            } else {
                Vec::new()
            };
            let matched_files: Vec<Uuid> = if applies_to_files {
                files
                    .iter()
                    .filter(|file| file.deleted_at.is_none())
                    .filter(|file| !file.legal_hold)
                    .filter(|file| file.created_at < cutoff)
                    .map(|file| file.id)
                    .collect()
            } else {
                Vec::new()
            };
            let matched_recordings: Vec<Uuid> = if applies_to_recordings {
                recordings
                    .iter()
                    .filter(|recording| recording.deleted_at.is_none())
                    .filter(|recording| !recording.legal_hold)
                    .filter(|recording| recording.created_at < cutoff)
                    .map(|recording| recording.id)
                    .collect()
            } else {
                Vec::new()
            };
            let deleted_messages = matched.len();
            let deleted_files = matched_files.len();
            let deleted_recordings = matched_recordings.len();
            ids_to_delete.extend(matched.iter().copied());
            file_ids_to_delete.extend(matched_files.iter().copied());
            recording_ids_to_delete.extend(matched_recordings.iter().copied());
            policy_results.push(RetentionPolicyResult {
                policy_id: policy.id,
                room_id: policy.room_id,
                retain_days: Some(retain_days),
                matched_messages: deleted_messages,
                deleted_messages: if dry_run { 0 } else { deleted_messages },
                matched_files: deleted_files,
                deleted_files: if dry_run { 0 } else { deleted_files },
                matched_recordings: deleted_recordings,
                deleted_recordings: if dry_run { 0 } else { deleted_recordings },
                legal_hold: false,
            });
        }

        ids_to_delete.sort();
        ids_to_delete.dedup();
        file_ids_to_delete.sort();
        file_ids_to_delete.dedup();
        recording_ids_to_delete.sort();
        recording_ids_to_delete.dedup();
        let matched_messages = ids_to_delete.len();
        let matched_files = file_ids_to_delete.len();
        let matched_recordings = recording_ids_to_delete.len();
        let mut deleted_messages = 0;
        let mut deleted_files = 0;
        let mut deleted_recordings = 0;
        if !dry_run {
            for id in ids_to_delete {
                if self.delete_room_message(id).is_some() {
                    deleted_messages += 1;
                }
            }
            for id in file_ids_to_delete {
                if self.mark_file_deleted(id, "retention").is_some() {
                    deleted_files += 1;
                }
            }
            for id in recording_ids_to_delete {
                if self.mark_recording_deleted(id, "retention").is_some() {
                    deleted_recordings += 1;
                }
            }
        }

        let result = RetentionEnforcementResult {
            evaluated_at,
            dry_run,
            matched_messages: matched_messages + matched_files + matched_recordings,
            deleted_messages: deleted_messages + deleted_files + deleted_recordings,
            skipped_legal_hold_policies,
            policy_results,
        };
        self.broadcast_sse(SseEvent {
            event_type: if dry_run {
                "retention_previewed".to_string()
            } else {
                "retention_enforced".to_string()
            },
            payload: serde_json::to_value(&result).unwrap_or_default(),
        });
        result
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

    pub fn toggle_message_reaction(
        &self,
        message_id: Uuid,
        user_uri: &str,
        emoji: &str,
    ) -> Option<MessageReactionToggle> {
        let room_id = self.room_message(message_id)?.room_id;
        let created_at = Utc::now();
        let added = self.message_reactions.with_write(&message_id, |map| {
            let reactions = map.entry(message_id).or_insert_with(Vec::new);
            if let Some(pos) = reactions
                .iter()
                .position(|r| r.user_uri == user_uri && r.emoji == emoji)
            {
                reactions.remove(pos);
                false
            } else {
                reactions.push(MessageReaction {
                    emoji: emoji.to_string(),
                    user_uri: user_uri.to_string(),
                    created_at,
                });
                true
            }
        });
        let record = MessageReactionRecord {
            message_id,
            reaction: MessageReaction {
                emoji: emoji.to_string(),
                user_uri: user_uri.to_string(),
                created_at,
            },
        };
        if added {
            self.persist(&record);
        } else {
            self.delete_persisted(MessageReactionRecord::collection(), record.key());
        }
        self.broadcast_sse(SseEvent {
            event_type: "reaction".to_string(),
            payload: serde_json::json!({
                "message_id": message_id,
                "room_id": room_id,
                "emoji": emoji,
                "user": user_uri,
                "added": added,
                "created_at": created_at,
            }),
        });
        let toggle = MessageReactionToggle {
            message_id,
            room_id,
            emoji: emoji.to_string(),
            user_uri: user_uri.to_string(),
            added,
            created_at,
        };
        let pg_emoji = toggle.emoji.clone();
        let pg_user_uri = toggle.user_uri.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move {
                if added {
                    pg.insert_reaction(message_id, &pg_user_uri, &pg_emoji)
                        .await
                } else {
                    pg.delete_reaction(message_id, &pg_user_uri, &pg_emoji)
                        .await
                }
            })
        });
        Some(toggle)
    }

    pub fn message_reactions(&self, message_id: Uuid) -> Vec<MessageReaction> {
        self.message_reactions.get(&message_id).unwrap_or_default()
    }

    pub fn room_message_state(&self, room_id: Uuid) -> Vec<RoomMessageState> {
        self.room_messages(room_id)
            .into_iter()
            .map(|message| RoomMessageState {
                message_id: message.id,
                reactions: self.message_reactions(message.id),
                reads: self.message_reads(message.id),
            })
            .collect()
    }

    pub fn mark_room_message_read(
        &self,
        message_id: Uuid,
        reader_uri: &str,
    ) -> Option<MessageRead> {
        let room_id = self.room_message(message_id)?.room_id;
        let read = MessageRead {
            message_id,
            reader_uri: reader_uri.to_string(),
            read_at: Utc::now(),
        };
        {
            let mut reads = self
                .message_reads
                .write()
                .expect("message reads lock poisoned");
            if let Some(existing) = reads.iter_mut().find(|existing| {
                existing.message_id == message_id && existing.reader_uri == reader_uri
            }) {
                *existing = read.clone();
            } else {
                reads.push(read.clone());
            }
        }
        self.persist(&read);
        let read_for_pg = read.clone();
        self.pg_spawn(move |pg| {
            Box::pin(async move { pg.upsert_message_read(&read_for_pg).await })
        });
        self.broadcast_sse(SseEvent {
            event_type: "read_receipt".to_string(),
            payload: serde_json::json!({
                "message_id": read.message_id,
                "room_id": room_id,
                "reader_uri": read.reader_uri,
                "read_at": read.read_at,
            }),
        });
        Some(read)
    }

    pub fn message_reads(&self, message_id: Uuid) -> Vec<MessageRead> {
        let mut reads: Vec<_> = self
            .message_reads
            .read()
            .expect("message reads lock poisoned")
            .iter()
            .filter(|read| read.message_id == message_id)
            .cloned()
            .collect();
        reads.sort_by(|left, right| left.read_at.cmp(&right.read_at));
        reads
    }

    fn delete_message_reads_for_message(&self, message_id: Uuid) {
        let deleted_keys = {
            let mut reads = self
                .message_reads
                .write()
                .expect("message reads lock poisoned");
            let mut deleted_keys = Vec::new();
            reads.retain(|read| {
                if read.message_id == message_id {
                    deleted_keys.push(read.key());
                    false
                } else {
                    true
                }
            });
            deleted_keys
        };
        for key in deleted_keys {
            self.delete_persisted(MessageRead::collection(), key);
        }
    }

    fn delete_message_reactions_for_message(&self, message_id: Uuid) {
        let deleted = self.message_reactions.with_write(&message_id, |map| {
            map.remove(&message_id).unwrap_or_default()
        });
        for reaction in deleted {
            self.delete_persisted(
                MessageReactionRecord::collection(),
                MessageReactionRecord {
                    message_id,
                    reaction,
                }
                .key(),
            );
        }
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
        self.user_favorites
            .get(&user_uri.to_string())
            .unwrap_or_default()
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
            if let Some(e) = email {
                user.email = Some(e);
            }
            if let Some(t) = title {
                user.title = Some(t);
            }
            if let Some(d) = department {
                user.department = Some(d);
            }
            if let Some(p) = phone_number {
                user.phone_number = Some(p);
            }
            Some(user.clone())
        })
    }

    pub fn room_messages(&self, room_id: Uuid) -> Vec<RoomMessage> {
        self.room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|m| m.room_id == room_id && m.delivered)
            .cloned()
            .map(|mut m| {
                m.body = self.decrypt_field(&m.body);
                m
            })
            .collect()
    }

    /// Return scheduled (not yet delivered) messages for a room by the sender.
    pub fn scheduled_room_messages(&self, room_id: Uuid, sender_uri: &str) -> Vec<RoomMessage> {
        self.room_messages
            .read()
            .expect("room messages lock poisoned")
            .iter()
            .filter(|m| m.room_id == room_id && !m.delivered && m.sender_uri == sender_uri)
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

    #[cfg(test)]
    fn set_room_message_created_at_for_test(
        &self,
        id: Uuid,
        created_at: DateTime<Utc>,
    ) -> Option<RoomMessage> {
        let mut messages = self
            .room_messages
            .write()
            .expect("room messages lock poisoned");
        let msg = messages.iter_mut().find(|m| m.id == id)?;
        msg.created_at = created_at;
        Some(msg.clone())
    }

    // ─── Message Threads ───

    /// List threads in a room, sorted by last activity.
    pub fn room_threads(&self, room_id: Uuid) -> Vec<MessageThread> {
        let mut threads: Vec<MessageThread> = self
            .message_threads
            .values()
            .into_iter()
            .filter(|t| t.room_id == room_id)
            .collect();
        threads.sort_by(|a, b| b.last_reply_at.cmp(&a.last_reply_at));
        threads
    }

    /// Get a single thread by ID.
    pub fn get_thread(&self, thread_id: Uuid) -> Option<MessageThread> {
        self.message_threads.get(&thread_id)
    }

    /// Get all messages in a thread.
    pub fn thread_messages(&self, thread_id: Uuid) -> Vec<RoomMessage> {
        let thread = match self.message_threads.get(&thread_id) {
            Some(t) => t,
            None => return Vec::new(),
        };
        let messages = self
            .room_messages
            .read()
            .expect("room messages lock poisoned");
        // Include the root message and all replies
        let mut result: Vec<RoomMessage> = messages
            .iter()
            .filter(|m| m.id == thread.root_message_id || m.thread_id == Some(thread_id))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        for msg in &mut result {
            msg.body = self.decrypt_field(&msg.body);
        }
        result
    }

    /// Reply to a thread. If the target message has no thread, one is created.
    pub fn reply_to_thread(
        &self,
        root_message_id: Uuid,
        sender_uri: &str,
        body: &str,
        priority: Option<String>,
    ) -> Result<(RoomMessage, MessageThread), String> {
        // Find the root message
        let root = self
            .room_message(root_message_id)
            .ok_or_else(|| "root message not found".to_string())?;

        let room_id = root.room_id;

        // Look up or create thread
        let thread_id = {
            // Check if there's already a thread for this root message
            let existing: Option<MessageThread> = self
                .message_threads
                .values()
                .into_iter()
                .find(|t| t.root_message_id == root_message_id);

            if let Some(t) = existing {
                t.id
            } else {
                // Create a new thread
                let tid = Uuid::new_v4();
                let thread = MessageThread {
                    id: tid,
                    room_id,
                    root_message_id,
                    reply_count: 0,
                    last_reply_at: root.created_at,
                    participants: vec![root.sender_uri.clone()],
                    created_at: Utc::now(),
                };
                self.message_threads.insert(tid, thread.clone());
                self.broadcast_sse(SseEvent {
                    event_type: "thread_created".to_string(),
                    payload: serde_json::to_value(&thread).unwrap_or_default(),
                });
                tid
            }
        };

        // Send the reply message via normal send path
        let mut msg =
            self.send_room_message(room_id, sender_uri, body, Some(root_message_id), priority)?;

        // Stamp thread_id on the reply
        msg.thread_id = Some(thread_id);
        {
            let mut messages = self
                .room_messages
                .write()
                .expect("room messages lock poisoned");
            if let Some(existing) = messages.iter_mut().find(|m| m.id == msg.id) {
                existing.thread_id = Some(thread_id);
            }
        }

        // Update thread metadata
        let now = Utc::now();
        let updated_thread = self.message_threads.get(&thread_id).map(|mut thread| {
            thread.reply_count += 1;
            thread.last_reply_at = now;
            if !thread.participants.contains(&sender_uri.to_string()) {
                thread.participants.push(sender_uri.to_string());
            }
            self.message_threads.insert(thread_id, thread.clone());
            thread
        });

        let thread =
            updated_thread.unwrap_or_else(|| self.message_threads.get(&thread_id).unwrap());

        self.broadcast_sse(SseEvent {
            event_type: "thread_reply".to_string(),
            payload: serde_json::json!({
                "thread": thread,
                "message": msg,
            }),
        });

        Ok((msg, thread))
    }

    /// Count replies to a given root message.
    pub fn reply_count_for_message(&self, message_id: Uuid) -> i32 {
        self.message_threads
            .values()
            .into_iter()
            .find(|t| t.root_message_id == message_id)
            .map(|t| t.reply_count)
            .unwrap_or(0)
    }

    // ─── Per-endpoint rate limiting ───

    /// Get the current rate limit configuration.
    pub fn rate_limit_config(&self) -> RateLimitConfig {
        self.rate_limit_config
            .read()
            .expect("rate limit config lock")
            .clone()
    }

    /// Update rate limit configuration at runtime.
    pub fn set_rate_limit_config(&self, config: RateLimitConfig) {
        *self
            .rate_limit_config
            .write()
            .expect("rate limit config lock") = config;
    }

    /// Check per-endpoint-group rate limit. Returns true if allowed.
    pub fn check_endpoint_rate_limit(&self, principal: &str, group: EndpointGroup) -> bool {
        let config = self.rate_limit_config();
        let max_per_minute = match group {
            EndpointGroup::Default => config.default_rpm,
            EndpointGroup::Auth => config.auth_rpm,
            EndpointGroup::FileUpload => config.file_upload_rpm,
            EndpointGroup::MessageSend => config.message_send_rpm,
            EndpointGroup::SseConnect => config.sse_connections,
        };
        let max_tokens = max_per_minute as f64;
        let refill_rate = max_tokens / 60.0; // tokens per second
        let key = format!("{}::{:?}", principal, group);

        self.endpoint_rate_limits.with_write(&key, |buckets| {
            let now = Utc::now();
            let bucket = buckets
                .entry(key.clone())
                .or_insert_with(|| RateLimitBucket {
                    tokens: max_tokens,
                    last_refill: now,
                });

            let elapsed = (now - bucket.last_refill).num_milliseconds().max(0) as f64 / 1000.0;
            bucket.tokens = (bucket.tokens + elapsed * refill_rate).min(max_tokens);
            bucket.last_refill = now;

            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                true
            } else {
                false
            }
        })
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
        F: FnOnce(
                PgStore,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), pg_store::PgError>> + Send>,
            > + Send
            + 'static,
    {
        if let Some(pg) = self.pg.clone() {
            let failures = self
                .pg_failure_count
                .load(std::sync::atomic::Ordering::Relaxed);
            if failures >= 10 {
                // Circuit open — skip writes, log periodically
                if failures % 100 == 10 {
                    log::warn!(
                        "Postgres circuit breaker open ({} consecutive failures), skipping writes",
                        failures
                    );
                }
                self.pg_failure_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

    pub fn store_call_history(
        &self,
        user_sip_uri: &str,
        input: CallHistoryInput,
    ) -> CallHistoryEntry {
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
            .map(|e| {
                (
                    e.start_time.timestamp(),
                    e.remote_uri.as_str(),
                    e.direction.as_str(),
                )
            })
            .collect();
        let mut merged = 0;
        for input in entries {
            let key = (
                input.start_time.timestamp(),
                input.remote_uri.as_str(),
                input.direction.as_str(),
            );
            if !existing_set.contains(&key) {
                self.store_call_history(user_sip_uri, input);
                merged += 1;
            }
        }
        merged
    }

    // ─── OAuth API Clients ───

    pub fn create_api_client(
        &self,
        input: CreateApiClientRequest,
        principal: &str,
    ) -> CreateApiClientResponse {
        let raw_id = Uuid::new_v4().to_string();
        let raw_secret = Uuid::new_v4().to_string();
        let client = ApiClient {
            id: Uuid::new_v4(),
            name: input.name,
            client_id: raw_id.clone(),
            client_secret_hash: sha256_hex(raw_secret.as_bytes()),
            scopes: input.scopes,
            redirect_uris: input.redirect_uris,
            created_by: principal.to_string(),
            created_at: Utc::now(),
        };
        self.api_clients.insert(client.id, client.clone());
        CreateApiClientResponse {
            client,
            client_secret: raw_secret,
        }
    }

    pub fn list_api_clients(&self) -> Vec<ApiClient> {
        self.api_clients.values()
    }

    pub fn delete_api_client(&self, id: Uuid) -> bool {
        // Remove associated tokens
        let token_ids: Vec<Uuid> = self
            .api_tokens
            .values()
            .into_iter()
            .filter(|t| t.client_id == id)
            .map(|t| t.id)
            .collect();
        for tid in token_ids {
            self.api_tokens.remove(&tid);
        }
        self.api_clients.remove(&id).is_some()
    }

    pub fn api_client_by_client_id(&self, client_id: &str) -> Option<ApiClient> {
        self.api_clients
            .values()
            .into_iter()
            .find(|c| c.client_id == client_id)
    }

    pub fn create_oauth_token(&self, input: OAuthTokenRequest) -> Option<OAuthTokenResponse> {
        let client = self.api_client_by_client_id(&input.client_id)?;
        if client.client_secret_hash != sha256_hex(input.client_secret.as_bytes()) {
            return None;
        }
        match input.grant_type.as_str() {
            "client_credentials" => {
                let scopes = input
                    .scope
                    .map(|s| s.split_whitespace().map(String::from).collect::<Vec<_>>())
                    .unwrap_or_else(|| client.scopes.clone());
                let raw_token = Uuid::new_v4().to_string();
                let token = ApiToken {
                    id: Uuid::new_v4(),
                    client_id: client.id,
                    user_uri: None,
                    scopes: scopes.clone(),
                    token_hash: sha256_hex(raw_token.as_bytes()),
                    expires_at: Utc::now() + Duration::hours(1),
                    created_at: Utc::now(),
                };
                self.api_tokens.insert(token.id, token);
                Some(OAuthTokenResponse {
                    access_token: raw_token,
                    token_type: "Bearer".to_string(),
                    expires_in: 3600,
                    scope: scopes.join(" "),
                })
            }
            _ => None,
        }
    }

    // ─── Bots ───

    pub fn create_bot(&self, input: CreateBotRequest, owner_uri: &str) -> Bot {
        let bot = Bot {
            id: Uuid::new_v4(),
            name: input.name,
            webhook_url: input.webhook_url,
            events: input.events,
            owner_uri: owner_uri.to_string(),
            api_token: Uuid::new_v4().to_string(),
            allowed_rooms: Vec::new(),
            enabled: true,
            created_at: Utc::now(),
        };
        self.bots.insert(bot.id, bot.clone());
        bot
    }

    pub fn list_bots(&self) -> Vec<Bot> {
        self.bots.values()
    }

    pub fn update_bot(&self, id: Uuid, input: UpdateBotRequest) -> Option<Bot> {
        self.bots.with_write(&id, |bots| {
            let bot = bots.get_mut(&id)?;
            if let Some(name) = input.name {
                bot.name = name;
            }
            if let Some(url) = input.webhook_url {
                bot.webhook_url = url;
            }
            if let Some(events) = input.events {
                bot.events = events;
            }
            if let Some(enabled) = input.enabled {
                bot.enabled = enabled;
            }
            Some(bot.clone())
        })
    }

    pub fn delete_bot(&self, id: Uuid) -> bool {
        self.bots.remove(&id).is_some()
    }

    pub fn bot_by_token(&self, token: &str) -> Option<Bot> {
        self.bots
            .values()
            .into_iter()
            .find(|b| b.api_token == token && b.enabled)
    }

    pub fn fire_bot_event(&self, event_type: &str, payload: serde_json::Value) {
        let bots: Vec<Bot> = self
            .bots
            .values()
            .into_iter()
            .filter(|b| b.enabled && b.events.iter().any(|e| e == event_type || e == "*"))
            .collect();
        for bot in bots {
            let url = bot.webhook_url.clone();
            let payload = payload.clone();
            let event = event_type.to_string();
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                let _ = client
                    .post(&url)
                    .json(&serde_json::json!({ "event": event, "data": payload }))
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await;
            });
        }
    }

    // ─── Calendar Integration ───

    pub fn create_calendar_integration(
        &self,
        user_uri: &str,
        input: CreateCalendarIntegrationRequest,
    ) -> CalendarIntegration {
        let integration = CalendarIntegration {
            id: Uuid::new_v4(),
            user_uri: user_uri.to_string(),
            provider: input.provider,
            access_token_enc: input.access_token,
            refresh_token_enc: input.refresh_token,
            calendar_id: input.calendar_id,
            enabled: true,
            last_sync: None,
        };
        self.calendar_integrations
            .insert(integration.id, integration.clone());
        integration
    }

    pub fn list_calendar_integrations(&self, user_uri: &str) -> Vec<CalendarIntegration> {
        self.calendar_integrations
            .values()
            .into_iter()
            .filter(|c| c.user_uri == user_uri)
            .collect()
    }

    pub fn delete_calendar_integration(&self, id: Uuid) -> bool {
        self.calendar_integrations.remove(&id).is_some()
    }

    pub fn calendar_events_local(&self, user_uri: &str) -> Vec<CalendarEvent> {
        let meetings = self.scheduled_meetings.values();
        meetings
            .into_iter()
            .filter(|m| m.organizer_uri == user_uri || m.participants.iter().any(|p| p == user_uri))
            .map(|m| CalendarEvent {
                id: m.id.to_string(),
                title: m.title.clone(),
                start: m.starts_at,
                end: m.ends_at,
                source: "local".to_string(),
            })
            .collect()
    }

    pub async fn calendar_events_synced(&self, user_uri: &str) -> Vec<CalendarEvent> {
        let mut events = self.calendar_events_local(user_uri);
        let integrations = self.list_calendar_integrations(user_uri);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        for integration in integrations {
            if !integration.enabled {
                continue;
            }
            let synced = match integration.provider.as_str() {
                "google" => {
                    sync_google_calendar(&client, &integration)
                        .await
                        .map(|(evts, refreshed)| {
                            // Persist refreshed access token if we got one
                            if let Some(new_token) = refreshed {
                                if let Some(mut updated) =
                                    self.calendar_integrations.get(&integration.id)
                                {
                                    updated.access_token_enc = new_token;
                                    self.calendar_integrations.insert(integration.id, updated);
                                }
                            }
                            evts
                        })
                }
                "caldav" => sync_caldav_calendar(&client, &integration).await,
                _ => {
                    log::warn!("Unknown calendar provider: {}", integration.provider);
                    continue;
                }
            };
            match synced {
                Ok(external_events) => {
                    // Update last_sync timestamp
                    if let Some(mut updated) = self.calendar_integrations.get(&integration.id) {
                        updated.last_sync = Some(Utc::now());
                        self.calendar_integrations.insert(integration.id, updated);
                    }
                    // Merge: skip duplicates by id
                    for ev in external_events {
                        if !events.iter().any(|e| e.id == ev.id) {
                            events.push(ev);
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Calendar sync failed for {} ({}): {}",
                        integration.provider,
                        integration.id,
                        e
                    );
                }
            }
        }
        events.sort_by_key(|e| e.start);
        events
    }

    // ─── Contact Sync ───

    pub fn create_contact_sync(
        &self,
        user_uri: &str,
        input: CreateContactSyncRequest,
    ) -> ContactSyncConfig {
        let config = ContactSyncConfig {
            id: Uuid::new_v4(),
            user_uri: user_uri.to_string(),
            provider: input.provider,
            access_token_enc: input.access_token,
            last_sync: None,
            enabled: true,
        };
        self.contact_sync_configs.insert(config.id, config.clone());
        config
    }

    pub fn list_contact_sync_configs(&self, user_uri: &str) -> Vec<ContactSyncConfig> {
        self.contact_sync_configs
            .values()
            .into_iter()
            .filter(|c| c.user_uri == user_uri)
            .collect()
    }

    pub fn delete_contact_sync(&self, id: Uuid) -> bool {
        self.contact_sync_configs.remove(&id).is_some()
    }

    pub fn list_contacts_merged(&self, user_uri: &str) -> Vec<SyncedContact> {
        self.synced_contacts
            .values()
            .into_iter()
            .filter(|c| c.user_uri == user_uri)
            .collect()
    }

    // ─── Connectors ───

    pub fn create_connector(&self, input: CreateConnectorRequest, principal: &str) -> Connector {
        let connector = Connector {
            id: Uuid::new_v4(),
            name: input.name,
            connector_type: input.connector_type,
            webhook_url: input.webhook_url,
            events: input.events,
            auth_header: input.auth_header,
            enabled: true,
            created_by: principal.to_string(),
            created_at: Utc::now(),
        };
        self.connectors.insert(connector.id, connector.clone());
        connector
    }

    pub fn list_connectors(&self) -> Vec<Connector> {
        self.connectors.values()
    }

    pub fn update_connector(&self, id: Uuid, input: UpdateConnectorRequest) -> Option<Connector> {
        self.connectors.with_write(&id, |connectors| {
            let c = connectors.get_mut(&id)?;
            if let Some(name) = input.name {
                c.name = name;
            }
            if let Some(url) = input.webhook_url {
                c.webhook_url = url;
            }
            if let Some(events) = input.events {
                c.events = events;
            }
            if let Some(auth) = input.auth_header {
                c.auth_header = Some(auth);
            }
            if let Some(enabled) = input.enabled {
                c.enabled = enabled;
            }
            Some(c.clone())
        })
    }

    pub fn delete_connector(&self, id: Uuid) -> bool {
        self.connectors.remove(&id).is_some()
    }

    pub fn fire_connector_event(&self, event_type: &str, payload: serde_json::Value) {
        let connectors: Vec<Connector> = self
            .connectors
            .values()
            .into_iter()
            .filter(|c| c.enabled && c.events.iter().any(|e| e == event_type || e == "*"))
            .collect();
        for connector in connectors {
            let url = connector.webhook_url.clone();
            let auth = connector.auth_header.clone();
            let payload = payload.clone();
            let event = event_type.to_string();
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                let mut req = client
                    .post(&url)
                    .json(&serde_json::json!({ "event": event, "data": payload }))
                    .timeout(std::time::Duration::from_secs(10));
                if let Some(header) = auth {
                    req = req.header("Authorization", header);
                }
                let _ = req.send().await;
            });
        }
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
    #[serde(default = "default_true")]
    pub active: bool,
    pub deactivated_at: Option<DateTime<Utc>>,
    pub deactivated_by: Option<String>,
    pub email: Option<String>,
    pub title: Option<String>,
    pub department: Option<String>,
    pub phone_number: Option<String>,
    pub status_message: Option<String>,
    #[serde(default)]
    pub out_of_office_message: Option<String>,
    #[serde(default)]
    pub out_of_office_until: Option<DateTime<Utc>>,
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
    /// When true, the token is a temporary MFA token that must be exchanged
    /// via POST /v1/mfa/validate before it grants access to other endpoints.
    #[serde(default)]
    pub mfa_required: bool,
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
    /// HMAC-SHA256 integrity signature over id|principal|action|target|created_at.
    /// Computed using the server storage key so tampered records can be detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AdminAuditQuery {
    pub principal: Option<String>,
    pub action: Option<String>,
    pub target: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
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

/// A call the server is proxying between two locally-registered flows. Tracks
/// both peer addresses (the sockets to write to) and both AORs so responses and
/// in-dialog requests can be relayed to the opposite party.
#[derive(Debug, Clone)]
pub struct ProxyCall {
    pub call_id: String,
    pub caller_peer: String,
    pub callee_peer: String,
    pub caller_aor: String,
    pub callee_aor: String,
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
    #[serde(default)]
    pub conference_id: Option<Uuid>,
    #[serde(default)]
    pub transcript_segment_count: usize,
    #[serde(default)]
    pub legal_hold: bool,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionJobStatus {
    Blocked,
    Queued,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionJob {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub conference_id: Option<Uuid>,
    pub status: TranscriptionJobStatus,
    pub language: Option<String>,
    pub provider_configured: bool,
    pub requested_by: String,
    pub error: Option<String>,
    pub transcript_segment_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTranscriptionJobRequest {
    pub language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteTranscriptionJobRequest {
    pub segments: Vec<PostTranscriptRequest>,
}

// ─── Group Chat Rooms ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub owner_uri: String,
    pub members: Vec<TeamMember>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub user_sip_uri: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddTeamMemberRequest {
    pub user_sip_uri: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: Uuid,
    #[serde(default)]
    pub team_id: Option<Uuid>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default = "default_channel_type")]
    pub channel_type: String,
    #[serde(default)]
    pub channel_owners: Vec<String>,
    #[serde(default = "default_posting_policy")]
    pub posting_policy: String,
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
    #[serde(default)]
    pub team_id: Option<Uuid>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub channel_type: Option<String>,
    #[serde(default)]
    pub channel_owners: Vec<String>,
    #[serde(default)]
    pub posting_policy: Option<String>,
}

fn default_channel_type() -> String {
    "standard".to_string()
}

fn default_posting_policy() -> String {
    "members".to_string()
}

fn normalize_channel_type(value: Option<&str>) -> String {
    match value
        .unwrap_or("standard")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "private" => "private".to_string(),
        "shared" => "shared".to_string(),
        _ => "standard".to_string(),
    }
}

fn normalize_posting_policy(value: Option<&str>) -> String {
    match value
        .unwrap_or("members")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "owners" | "owners_only" => "owners".to_string(),
        _ => "members".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMeeting {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub organizer_uri: String,
    pub room_id: Option<Uuid>,
    pub conference_id: Option<Uuid>,
    pub participants: Vec<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub recurrence: Option<MeetingRecurrence>,
    #[serde(default = "default_meeting_status")]
    pub status: MeetingStatus,
    #[serde(default)]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateScheduledMeetingRequest {
    pub title: String,
    pub description: Option<String>,
    pub room_id: Option<Uuid>,
    pub participants: Vec<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub mode: Option<RoomCallMode>,
    pub recurrence: Option<MeetingRecurrence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateScheduledMeetingRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub participants: Option<Vec<String>>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub recurrence: Option<Option<MeetingRecurrence>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Scheduled,
    Cancelled,
}

fn default_meeting_status() -> MeetingStatus {
    MeetingStatus::Scheduled
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingRecurrence {
    pub frequency: MeetingRecurrenceFrequency,
    pub interval: u32,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub id: Uuid,
    pub name: String,
    pub scope: String,
    pub room_id: Option<Uuid>,
    pub retain_days: Option<i64>,
    pub legal_hold: bool,
    pub export_enabled: bool,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertRetentionPolicyRequest {
    pub id: Option<Uuid>,
    pub name: String,
    pub scope: String,
    pub room_id: Option<Uuid>,
    pub retain_days: Option<i64>,
    pub legal_hold: Option<bool>,
    pub export_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationPolicy {
    pub id: String,
    pub structured_mentions_enabled: bool,
    pub broad_mentions_enabled: bool,
    pub broad_mentions_allowed_roles: Vec<String>,
    pub broad_mentions_per_minute: u32,
    #[serde(default = "default_true")]
    pub external_access_enabled: bool,
    #[serde(default)]
    pub allowed_external_domains: Vec<String>,
    #[serde(default = "default_true")]
    pub urgent_messages_enabled: bool,
    #[serde(default = "default_true")]
    pub meeting_recording_enabled: bool,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

impl Default for CollaborationPolicy {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            structured_mentions_enabled: true,
            broad_mentions_enabled: true,
            broad_mentions_allowed_roles: vec!["owner".to_string(), "admin".to_string()],
            broad_mentions_per_minute: 3,
            external_access_enabled: true,
            allowed_external_domains: Vec::new(),
            urgent_messages_enabled: true,
            meeting_recording_enabled: true,
            updated_by: None,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCollaborationPolicyRequest {
    pub structured_mentions_enabled: Option<bool>,
    pub broad_mentions_enabled: Option<bool>,
    pub broad_mentions_allowed_roles: Option<Vec<String>>,
    pub broad_mentions_per_minute: Option<u32>,
    pub external_access_enabled: Option<bool>,
    pub allowed_external_domains: Option<Vec<String>>,
    pub urgent_messages_enabled: Option<bool>,
    pub meeting_recording_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelWebhook {
    pub id: Uuid,
    pub room_id: Uuid,
    pub name: String,
    pub description: String,
    pub token_hash: String,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
}

impl ChannelWebhook {
    pub fn principal_uri(&self) -> String {
        format!("sip:webhook-{}@pale.local", self.id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelWebhookSummary {
    pub id: Uuid,
    pub room_id: Uuid,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl From<ChannelWebhook> for ChannelWebhookSummary {
    fn from(webhook: ChannelWebhook) -> Self {
        Self {
            id: webhook.id,
            room_id: webhook.room_id,
            name: webhook.name,
            description: webhook.description,
            enabled: webhook.enabled,
            created_by: webhook.created_by,
            created_at: webhook.created_at,
            last_used_at: webhook.last_used_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateChannelWebhookRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateChannelWebhookResponse {
    pub webhook: ChannelWebhookSummary,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateChannelWebhookRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostChannelWebhookRequest {
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryExport {
    pub exported_at: DateTime<Utc>,
    pub room_id: Option<Uuid>,
    pub messages: Vec<RoomMessage>,
    pub files: Vec<FileDiscoveryRecord>,
    pub recordings: Vec<CallRecording>,
}

#[derive(Debug, Clone)]
pub struct DiscoverySearchQuery {
    pub q: Option<String>,
    pub user_uri: Option<String>,
    pub room_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EDiscoveryCase {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: EDiscoveryCaseStatus,
    pub custodians: Vec<String>,
    pub query: EDiscoveryCaseQuery,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_exported_at: Option<DateTime<Utc>>,
    pub last_exported_by: Option<String>,
    pub last_export_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EDiscoveryCaseStatus {
    Open,
    OnHold,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EDiscoveryCaseQuery {
    pub q: Option<String>,
    pub user_uri: Option<String>,
    pub room_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEDiscoveryCaseRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub custodians: Vec<String>,
    #[serde(default)]
    pub query: EDiscoveryCaseQuery,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEDiscoveryCaseRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<EDiscoveryCaseStatus>,
    pub custodians: Option<Vec<String>>,
    pub query: Option<EDiscoveryCaseQuery>,
}

impl EDiscoveryCase {
    fn to_discovery_query(&self) -> DiscoverySearchQuery {
        let user_uri = if self.query.user_uri.is_some() {
            self.query.user_uri.clone()
        } else if self.custodians.len() == 1 {
            self.custodians.first().cloned()
        } else {
            None
        };
        let q = if self.custodians.len() > 1 && self.query.q.is_none() {
            Some(self.custodians.join(" "))
        } else {
            self.query.q.clone()
        };
        DiscoverySearchQuery {
            q,
            user_uri,
            room_id: self.query.room_id,
            from: self.query.from,
            to: self.query.to,
            limit: self.query.limit,
        }
    }
}

fn normalized_case_custodians(custodians: Vec<String>) -> Vec<String> {
    let mut custodians: Vec<_> = custodians
        .into_iter()
        .map(|custodian| custodian.trim().to_string())
        .filter(|custodian| !custodian.is_empty())
        .collect();
    custodians.sort();
    custodians.dedup();
    custodians
}

fn normalized_case_query(mut query: EDiscoveryCaseQuery) -> EDiscoveryCaseQuery {
    query.q = query
        .q
        .map(|value| value.trim().chars().take(240).collect())
        .filter(|value: &String| !value.is_empty());
    query.user_uri = query
        .user_uri
        .map(|value| value.trim().chars().take(160).collect())
        .filter(|value: &String| !value.is_empty());
    query.limit = Some(query.limit.unwrap_or(250).clamp(1, 1000));
    query
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionEnforcementResult {
    pub evaluated_at: DateTime<Utc>,
    pub dry_run: bool,
    pub matched_messages: usize,
    pub deleted_messages: usize,
    pub skipped_legal_hold_policies: Vec<Uuid>,
    pub policy_results: Vec<RetentionPolicyResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicyResult {
    pub policy_id: Uuid,
    pub room_id: Option<Uuid>,
    pub retain_days: Option<i64>,
    pub matched_messages: usize,
    pub deleted_messages: usize,
    pub matched_files: usize,
    pub deleted_files: usize,
    pub matched_recordings: usize,
    pub deleted_recordings: usize,
    pub legal_hold: bool,
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
pub struct RoomCallEnded {
    pub room_id: Uuid,
    pub conference_id: Uuid,
    pub call_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSearchResult {
    pub kind: String,
    pub id: Uuid,
    pub title: String,
    pub subtitle: String,
    pub room_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub conference_id: Option<Uuid>,
    pub call_uri: Option<String>,
    pub updated_at: DateTime<Utc>,
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
    #[serde(default)]
    pub mentions: Vec<MessageMention>,
    #[serde(default)]
    pub mentioned_user_uris: Vec<String>,
    #[serde(default = "default_message_priority")]
    pub priority: String,
    #[serde(default)]
    pub saved_by: Vec<String>,
    /// When set, the message is scheduled for future delivery.
    #[serde(default)]
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Whether a scheduled message has been delivered.
    #[serde(default = "default_true")]
    pub delivered: bool,
    /// Delivery status: pending, sent, delivered, failed.
    #[serde(default = "default_delivery_status")]
    pub delivery_status: String,
    /// Optional adaptive card payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_payload: Option<AdaptiveCard>,
    /// Thread this message belongs to (if it's a threaded reply).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<Uuid>,
}

fn default_message_priority() -> String {
    "normal".to_string()
}

fn default_delivery_status() -> String {
    "sent".to_string()
}

fn normalize_message_priority(value: Option<&str>) -> String {
    match value
        .unwrap_or("normal")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" => "high".to_string(),
        "urgent" => "urgent".to_string(),
        _ => "normal".to_string(),
    }
}

fn sip_domain(uri: &str) -> Option<String> {
    let bare = uri.trim().trim_start_matches("sip:");
    bare.split('@')
        .nth(1)
        .map(|domain| {
            domain
                .split(';')
                .next()
                .unwrap_or(domain)
                .to_ascii_lowercase()
        })
        .filter(|domain| !domain.is_empty())
}

fn is_external_call_target(caller: &str, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() {
        return false;
    }
    let Some(caller_domain) = sip_domain(caller) else {
        return !target.starts_with("sip:");
    };
    sip_domain(target).is_none_or(|target_domain| target_domain != caller_domain)
}

fn normalized_policy_domains(domains: Vec<String>) -> Vec<String> {
    let mut domains: Vec<String> = domains
        .into_iter()
        .map(|domain| domain.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect();
    domains.sort();
    domains.dedup();
    domains
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageMention {
    pub kind: String,
    pub token: String,
    pub user_sip_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReaction {
    pub emoji: String,
    pub user_uri: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReactionRecord {
    pub message_id: Uuid,
    pub reaction: MessageReaction,
}

impl MessageReactionRecord {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.message_id, self.reaction.user_uri, self.reaction.emoji
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReactionToggle {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub emoji: String,
    pub user_uri: String,
    pub added: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendRoomMessageRequest {
    pub body: String,
    pub reply_to: Option<Uuid>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub card_payload: Option<AdaptiveCard>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleRoomMessageRequest {
    pub body: String,
    pub scheduled_at: DateTime<Utc>,
    pub reply_to: Option<Uuid>,
    #[serde(default)]
    pub priority: Option<String>,
}

// ─── Message Threads ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageThread {
    pub id: Uuid,
    pub room_id: Uuid,
    pub root_message_id: Uuid,
    pub reply_count: i32,
    pub last_reply_at: DateTime<Utc>,
    pub participants: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadReplyRequest {
    pub body: String,
    #[serde(default)]
    pub priority: Option<String>,
}

// ─── Rate Limit Config ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub default_rpm: u32,
    pub auth_rpm: u32,
    pub file_upload_rpm: u32,
    pub message_send_rpm: u32,
    pub sse_connections: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            default_rpm: 100,
            auth_rpm: 10,
            file_upload_rpm: 20,
            message_send_rpm: 60,
            sse_connections: 5,
        }
    }
}

/// Identifies the endpoint group for per-group rate limiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointGroup {
    Default,
    Auth,
    FileUpload,
    MessageSend,
    SseConnect,
}

// ─── Tags ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub members: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTagRequest {
    pub name: Option<String>,
    pub members: Option<Vec<String>>,
}

// ─── Notification Preferences ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreference {
    pub room_id: Uuid,
    pub user_uri: String,
    pub notification_level: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNotificationPreferenceRequest {
    pub notification_level: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMessageState {
    pub message_id: Uuid,
    pub reactions: Vec<MessageReaction>,
    pub reads: Vec<MessageRead>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSearchResult {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub snippet: String,
    pub source: String,
    pub url: Option<String>,
    pub room_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub conference_id: Option<Uuid>,
    pub user_uri: Option<String>,
    pub file_id: Option<Uuid>,
    pub app_id: Option<Uuid>,
    pub score: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCopilotQueryRequest {
    pub question: String,
    #[serde(default)]
    pub context_query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotCitation {
    pub index: usize,
    pub result: UnifiedSearchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotAnswer {
    pub question: String,
    pub generated_at: DateTime<Utc>,
    pub provider_configured: bool,
    pub grounded: bool,
    pub answer: String,
    pub citations: Vec<CopilotCitation>,
    pub suggested_prompts: Vec<String>,
    pub governance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderStatus {
    pub kind: String,
    pub configured: bool,
    pub integration_ids: Vec<String>,
    pub endpoint_url: Option<String>,
    pub admin_url: Option<String>,
    pub api_key_configured: bool,
    pub supported_protocols: Vec<String>,
    pub open_source_options: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAiProviderRequest {
    pub enabled: Option<bool>,
    pub endpoint_url: Option<String>,
    pub admin_url: Option<String>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderDispatch {
    pub kind: String,
    pub provider_configured: bool,
    pub endpoint_url: Option<String>,
    pub status: String,
    pub payload: serde_json::Value,
    pub warnings: Vec<String>,
    pub governance: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmChatRequest {
    pub messages: Vec<LlmChatMessage>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SttTranscriptionRequest {
    #[serde(default)]
    pub recording_id: Option<Uuid>,
    #[serde(default)]
    pub file_id: Option<Uuid>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub diarization: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtsSynthesisRequest {
    pub text: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

fn grounded_copilot_answer(
    question: &str,
    provider_configured: bool,
    citations: &[CopilotCitation],
) -> String {
    if citations.is_empty() {
        return if provider_configured {
            "I could not find visible workspace content that answers this. Try a more specific project, channel, meeting, file, or person name.".to_string()
        } else {
            "I could not find visible workspace content that answers this. The Copilot provider is not configured, so I cannot use external reasoning or retrieval beyond governed local search.".to_string()
        };
    }

    let mut by_kind: HashMap<String, usize> = HashMap::new();
    for citation in citations {
        *by_kind.entry(citation.result.kind.clone()).or_default() += 1;
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let scope = kinds
        .into_iter()
        .map(|(kind, count)| format!("{count} {kind}"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines = Vec::new();
    if provider_configured {
        lines.push(format!(
            "Based on visible workspace content for \"{question}\", I found {scope}."
        ));
    } else {
        lines.push(format!(
            "Based on visible workspace content for \"{question}\", I found {scope}. The Copilot provider is not configured, so this is a local grounded answer."
        ));
    }
    for citation in citations.iter().take(5) {
        lines.push(format!(
            "[{}] {}: {}",
            citation.index,
            citation.result.title,
            concise_snippet(&citation.result.snippet, 180)
        ));
    }
    lines.join("\n")
}

fn concise_snippet(value: &str, max_chars: usize) -> String {
    let trimmed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= max_chars {
        trimmed
    } else {
        let mut snippet: String = trimmed.chars().take(max_chars).collect();
        snippet.push_str("...");
        snippet
    }
}

fn copilot_suggested_prompts(results: &[UnifiedSearchResult]) -> Vec<String> {
    let mut prompts = Vec::new();
    if results
        .iter()
        .any(|result| result.kind == "meeting" || result.kind == "recording")
    {
        prompts.push("Summarize the latest meeting context and open follow-ups.".to_string());
    }
    if results.iter().any(|result| result.kind == "file") {
        prompts.push("Show the files that support this answer.".to_string());
    }
    if results
        .iter()
        .any(|result| result.kind == "message" || result.kind == "channel")
    {
        prompts.push("Find the decision trail in chat.".to_string());
    }
    if prompts.is_empty() {
        prompts.push("Search for related people, meetings, and files.".to_string());
    }
    prompts.truncate(3);
    prompts
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
    pub locked: bool,
    #[serde(default)]
    pub active: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub spotlight_participant_id: Option<Uuid>,
    #[serde(default)]
    pub green_room_enabled: bool,
    #[serde(default)]
    pub chat_room_id: Option<Uuid>,
    #[serde(default)]
    pub registration_enabled: bool,
    #[serde(default)]
    pub max_registrations: Option<i32>,
    #[serde(default)]
    pub registration_fields: Option<serde_json::Value>,
    /// LiveKit room name (set when LiveKit is configured).
    #[serde(default)]
    pub livekit_room: Option<String>,
    /// LiveKit egress ID when recording is active via LiveKit.
    #[serde(default)]
    pub livekit_egress_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConferenceRequest {
    pub title: String,
    pub mode: ConferenceMode,
    #[serde(default)]
    pub registration_enabled: Option<bool>,
    #[serde(default)]
    pub max_registrations: Option<i32>,
    #[serde(default)]
    pub registration_fields: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinConferenceRequest {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub role: Option<ParticipantRole>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinConferenceError {
    NotFound,
    Locked,
    CapacityReached,
}

/// Extended response from `join_conference` that includes optional LiveKit
/// media credentials alongside the conference state.
#[derive(Debug, Clone, Serialize)]
pub struct JoinConferenceResponse {
    #[serde(flatten)]
    pub conference: Conference,
    /// LiveKit server URL the frontend should connect to (only present when
    /// LiveKit is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_url: Option<String>,
    /// Signed LiveKit access token for this participant (only present when
    /// LiveKit is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConferenceParticipantRequest {
    pub role: Option<ParticipantRole>,
    pub muted: Option<bool>,
    pub removed: Option<bool>,
    pub removal_reason: Option<String>,
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
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub removed: bool,
    #[serde(default)]
    pub removed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub removed_by: Option<String>,
    #[serde(default)]
    pub removal_reason: Option<String>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceLeaveReason {
    Left,
    Removed,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceAttendanceRecord {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub user_id: Uuid,
    pub sip_uri: String,
    pub role: ParticipantRole,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<i64>,
    pub leave_reason: Option<AttendanceLeaveReason>,
    pub removed_by: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMediaSettings {
    pub user_uri: String,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain: bool,
    pub background_mode: String,
    pub background_image_url: Option<String>,
    pub noise_suppression_configured: bool,
    pub virtual_backgrounds_configured: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMeetingMediaSettingsRequest {
    pub echo_cancellation: Option<bool>,
    pub noise_suppression: Option<bool>,
    pub auto_gain: Option<bool>,
    pub background_mode: Option<String>,
    pub background_image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceLayoutState {
    pub conference_id: Uuid,
    pub mode: String,
    pub max_visible: usize,
    pub together_scene: Option<String>,
    pub stage_participant_ids: Vec<Uuid>,
    pub sfu_layout_configured: bool,
    #[serde(default)]
    pub gallery_capacity: usize,
    #[serde(default = "default_true")]
    pub together_scene_supported: bool,
    #[serde(default)]
    pub layout_blockers: Vec<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConferenceLayoutRequest {
    pub mode: Option<String>,
    pub max_visible: Option<usize>,
    pub together_scene: Option<String>,
    pub stage_participant_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamTargetKind {
    Rtmp,
    Ndi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamStatus {
    Pending,
    Live,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingStreamSession {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub target_kind: StreamTargetKind,
    pub name: String,
    pub destination: String,
    pub status: StreamStatus,
    pub started_by: String,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub health: String,
    pub gateway_configured: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMeetingStreamRequest {
    pub target_kind: StreamTargetKind,
    pub name: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TownHallConfig {
    pub conference_id: Uuid,
    pub enabled: bool,
    pub max_viewers: usize,
    pub registration_required: bool,
    pub presenter_only_video: bool,
    pub attendee_mic_disabled: bool,
    pub qna_moderation_required: bool,
    pub overflow_url: Option<String>,
    pub broadcast_provider_configured: bool,
    #[serde(default)]
    pub broadcast_capacity: usize,
    #[serde(default)]
    pub attendee_mode: String,
    #[serde(default)]
    pub broadcast_ready: bool,
    #[serde(default)]
    pub broadcast_blockers: Vec<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTownHallConfigRequest {
    pub enabled: Option<bool>,
    pub max_viewers: Option<usize>,
    pub registration_required: Option<bool>,
    pub presenter_only_video: Option<bool>,
    pub attendee_mic_disabled: Option<bool>,
    pub qna_moderation_required: Option<bool>,
    pub overflow_url: Option<String>,
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

// ── Meeting lobby ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LobbyParticipantState {
    Waiting,
    Admitted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyParticipant {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub display_name: String,
    pub state: LobbyParticipantState,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceLobby {
    pub conference_id: Uuid,
    pub enabled: bool,
    pub participants: Vec<LobbyParticipant>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LobbyAdmitRequest {
    pub user_id: Uuid,
    pub admit: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LobbySettingsRequest {
    pub enabled: bool,
}

// ── Raise hand ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandRaise {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub raised_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaiseHandRequest {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub raised: bool,
}

// ── Meeting polls & Q&A ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollStatus {
    Draft,
    Active,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    pub id: Uuid,
    pub text: String,
    pub votes: Vec<String>, // SIP URIs of voters
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingPoll {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub question: String,
    pub options: Vec<PollOption>,
    pub status: PollStatus,
    pub anonymous: bool,
    pub multi_select: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePollRequest {
    pub question: String,
    pub options: Vec<String>,
    #[serde(default)]
    pub anonymous: bool,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CastVoteRequest {
    pub option_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaQuestion {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub text: String,
    pub asked_by: String,
    pub upvotes: Vec<String>,
    pub answered: bool,
    pub answer: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AskQuestionRequest {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnswerQuestionRequest {
    pub answer: String,
}

// ── Breakout rooms ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakoutStatus {
    Pending,
    Active,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakoutRoom {
    pub id: Uuid,
    pub name: String,
    pub participants: Vec<String>, // SIP URIs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakoutSession {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub rooms: Vec<BreakoutRoom>,
    pub status: BreakoutStatus,
    pub duration_secs: Option<u64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBreakoutRequest {
    pub rooms: Vec<CreateBreakoutRoomInput>,
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBreakoutRoomInput {
    pub name: String,
    pub participants: Vec<String>,
}

// ── PowerPoint Live / presentation sessions ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationSlide {
    pub index: usize,
    pub title: String,
    pub notes: Option<String>,
    pub render_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationSession {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub title: String,
    pub source_file_id: Option<Uuid>,
    pub presenter_uri: String,
    pub slides: Vec<PresentationSlide>,
    pub current_slide: usize,
    pub attendee_navigation_enabled: bool,
    pub renderer_configured: bool,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePresentationSessionRequest {
    pub title: String,
    pub source_file_id: Option<Uuid>,
    pub slides: Vec<CreatePresentationSlideRequest>,
    #[serde(default)]
    pub attendee_navigation_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePresentationSlideRequest {
    pub title: String,
    pub notes: Option<String>,
    pub render_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePresentationSessionRequest {
    pub current_slide: Option<usize>,
    pub attendee_navigation_enabled: Option<bool>,
    pub presenter_uri: Option<String>,
}

// ── Live captions / transcription ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub speaker_uri: String,
    pub speaker_name: String,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    pub is_final: bool,
    #[serde(default = "default_language")]
    pub language: Option<String>,
}

fn default_language() -> Option<String> {
    Some("en".to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostTranscriptRequest {
    pub speaker_uri: String,
    pub speaker_name: String,
    pub text: String,
    #[serde(default = "default_true")]
    pub is_final: bool,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptExport {
    pub conference_id: Uuid,
    pub title: String,
    pub segments: Vec<TranscriptSegment>,
    pub exported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingAssistantReport {
    pub conference_id: Uuid,
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub transcript_segments: usize,
    pub ai_provider_configured: bool,
    pub summary: String,
    pub key_topics: Vec<String>,
    pub action_items: Vec<MeetingActionItem>,
    pub decisions: Vec<String>,
    pub risks: Vec<String>,
    pub open_questions: Vec<String>,
    pub speaker_stats: Vec<MeetingSpeakerStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingActionItem {
    pub owner: Option<String>,
    pub text: String,
    pub source_segment_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSpeakerStat {
    pub speaker_uri: String,
    pub speaker_name: String,
    pub segments: usize,
    pub words: usize,
}

// ── Call quality metrics (CQD) ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CallQualityRating {
    #[default]
    Good,
    Warning,
    Poor,
}

struct CallQualityDiagnostics {
    rating: CallQualityRating,
    issues: Vec<String>,
    recommended_action: Option<String>,
}

fn call_quality_diagnostics(
    mos_score: f64,
    jitter_ms: f64,
    packet_loss_pct: f64,
    round_trip_ms: f64,
) -> CallQualityDiagnostics {
    let mut rating = if mos_score < 3.0 {
        CallQualityRating::Poor
    } else if mos_score < 3.8 {
        CallQualityRating::Warning
    } else {
        CallQualityRating::Good
    };
    let mut issues = Vec::new();

    if jitter_ms > 50.0 {
        rating = CallQualityRating::Poor;
        issues.push("high_jitter".to_string());
    } else if jitter_ms > 30.0 && rating == CallQualityRating::Good {
        rating = CallQualityRating::Warning;
        issues.push("elevated_jitter".to_string());
    } else if jitter_ms > 30.0 {
        issues.push("elevated_jitter".to_string());
    }

    if packet_loss_pct > 5.0 {
        rating = CallQualityRating::Poor;
        issues.push("high_packet_loss".to_string());
    } else if packet_loss_pct > 2.0 && rating == CallQualityRating::Good {
        rating = CallQualityRating::Warning;
        issues.push("elevated_packet_loss".to_string());
    } else if packet_loss_pct > 2.0 {
        issues.push("elevated_packet_loss".to_string());
    }

    if round_trip_ms > 300.0 {
        rating = CallQualityRating::Poor;
        issues.push("high_round_trip_time".to_string());
    } else if round_trip_ms > 150.0 && rating == CallQualityRating::Good {
        rating = CallQualityRating::Warning;
        issues.push("elevated_round_trip_time".to_string());
    } else if round_trip_ms > 150.0 {
        issues.push("elevated_round_trip_time".to_string());
    }

    if mos_score < 3.0 {
        issues.push("low_mos".to_string());
    } else if mos_score < 3.8 {
        issues.push("degraded_mos".to_string());
    }

    let recommended_action = match rating {
        CallQualityRating::Good => None,
        CallQualityRating::Warning => Some(
            "Review endpoint network stability, Wi-Fi signal, and competing bandwidth usage."
                .to_string(),
        ),
        CallQualityRating::Poor => Some(
            "Escalate to network diagnostics; check packet loss, latency path, codec, and device health."
                .to_string(),
        ),
    };

    CallQualityDiagnostics {
        rating,
        issues,
        recommended_action,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallQualityReport {
    pub id: Uuid,
    pub call_id: Uuid,
    pub user_sip_uri: String,
    pub codec: String,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub round_trip_ms: f64,
    pub mos_score: f64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    #[serde(default)]
    pub rating: CallQualityRating,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub recommended_action: Option<String>,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostCallQualityRequest {
    pub call_id: Uuid,
    pub codec: String,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub round_trip_ms: f64,
    pub mos_score: f64,
    #[serde(default)]
    pub bytes_sent: u64,
    #[serde(default)]
    pub bytes_received: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CallQualityQuery {
    pub user_sip_uri: Option<String>,
    pub call_id: Option<Uuid>,
    pub rating: Option<CallQualityRating>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallQualitySummary {
    pub total_reports: usize,
    pub avg_mos: f64,
    pub avg_jitter_ms: f64,
    pub avg_packet_loss_pct: f64,
    pub avg_round_trip_ms: f64,
    pub poor_quality_calls: usize, // MOS < 3.0
    pub warning_quality_calls: usize,
    pub worst_mos: f64,
}

// ── DLP (Data Loss Prevention) ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlpAction {
    Block,
    Warn,
    Audit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpPolicy {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub pattern: String, // regex pattern
    pub action: DlpAction,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDlpPolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub pattern: String,
    pub action: DlpAction,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateDlpPolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub pattern: Option<String>,
    pub action: Option<DlpAction>,
    pub enabled: Option<bool>,
}

fn validate_dlp_pattern(pattern: &str) -> Result<(), String> {
    regex::Regex::new(pattern)
        .map(|_| ())
        .map_err(|err| format!("invalid DLP pattern: {err}"))
}

fn dlp_content_snippet(content: &str) -> String {
    const MAX_CHARS: usize = 80;
    let mut snippet: String = content.chars().take(MAX_CHARS).collect();
    if content.chars().count() > MAX_CHARS {
        snippet.push_str("...");
    }
    snippet
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpViolation {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub policy_name: String,
    pub user_uri: String,
    pub action_taken: DlpAction,
    pub content_snippet: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct DlpViolationQuery {
    pub policy: Option<String>,
    pub user_uri: Option<String>,
    pub action: Option<DlpAction>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DlpScanResult {
    pub allowed: bool,
    pub violations: Vec<DlpViolation>,
}

// ── Information Barriers ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationBarrier {
    pub id: Uuid,
    pub name: String,
    pub segment1_name: String,
    pub segment1_users: Vec<String>,
    pub segment2_name: String,
    pub segment2_users: Vec<String>,
    pub block_chat: bool,
    pub block_call: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInformationBarrierRequest {
    pub name: String,
    pub segment1_name: String,
    #[serde(default)]
    pub segment1_users: Vec<String>,
    pub segment2_name: String,
    #[serde(default)]
    pub segment2_users: Vec<String>,
    #[serde(default = "default_true")]
    pub block_chat: bool,
    #[serde(default = "default_true")]
    pub block_call: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateInformationBarrierRequest {
    pub name: Option<String>,
    pub segment1_name: Option<String>,
    pub segment1_users: Option<Vec<String>>,
    pub segment2_name: Option<String>,
    pub segment2_users: Option<Vec<String>>,
    pub block_chat: Option<bool>,
    pub block_call: Option<bool>,
    pub enabled: Option<bool>,
}

/// Result of an information barrier check.
#[derive(Debug, Clone, Serialize)]
pub struct BarrierCheckResult {
    pub blocked: bool,
    pub barrier_id: Option<Uuid>,
    pub barrier_name: Option<String>,
}

// ── Sensitivity Labels ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityLabel {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub priority: i32,
    pub encrypt_content: bool,
    pub restrict_sharing: bool,
    pub watermark: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSensitivityLabelRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_label_color")]
    pub color: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub encrypt_content: bool,
    #[serde(default)]
    pub restrict_sharing: bool,
    #[serde(default)]
    pub watermark: bool,
}

fn csv_escape_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn default_label_color() -> String {
    "#6b7280".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSensitivityLabelRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub priority: Option<i32>,
    pub encrypt_content: Option<bool>,
    pub restrict_sharing: Option<bool>,
    pub watermark: Option<bool>,
}

// ── Custom RBAC Roles ─────────────────────────────────────────────

/// Well-known permission constants for custom roles.
pub mod permissions {
    pub const MANAGE_USERS: &str = "manage_users";
    pub const MANAGE_CHANNELS: &str = "manage_channels";
    pub const MANAGE_POLICIES: &str = "manage_policies";
    pub const VIEW_AUDIT: &str = "view_audit";
    pub const MANAGE_CALLS: &str = "manage_calls";
    pub const MANAGE_MEETINGS: &str = "manage_meetings";
    pub const MANAGE_FILES: &str = "manage_files";
    pub const MANAGE_EXTENSIONS: &str = "manage_extensions";
    pub const MANAGE_QUEUES: &str = "manage_queues";
    pub const MANAGE_DLP: &str = "manage_dlp";
    pub const MANAGE_BARRIERS: &str = "manage_barriers";
    pub const MANAGE_LABELS: &str = "manage_labels";
    pub const MANAGE_ROLES: &str = "manage_roles";
    pub const MANAGE_PACKAGES: &str = "manage_packages";

    pub fn all() -> Vec<&'static str> {
        vec![
            MANAGE_USERS,
            MANAGE_CHANNELS,
            MANAGE_POLICIES,
            VIEW_AUDIT,
            MANAGE_CALLS,
            MANAGE_MEETINGS,
            MANAGE_FILES,
            MANAGE_EXTENSIONS,
            MANAGE_QUEUES,
            MANAGE_DLP,
            MANAGE_BARRIERS,
            MANAGE_LABELS,
            MANAGE_ROLES,
            MANAGE_PACKAGES,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRole {
    pub id: Uuid,
    pub name: String,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCustomRoleRequest {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCustomRoleRequest {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

// ── Policy Packages ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPackage {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub policies: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePolicyPackageRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "empty_json_object")]
    pub policies: serde_json::Value,
}

fn empty_json_object() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePolicyPackageRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub policies: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignPolicyPackageRequest {
    pub user_ids: Vec<Uuid>,
}

// ── Bulk User Operations ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BulkImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

// ── Usage Analytics ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UsageAnalytics {
    pub total_users: usize,
    pub active_users: usize,
    pub total_messages: usize,
    pub total_calls: usize,
    pub total_meetings: usize,
    pub total_files: usize,
    pub total_storage_bytes: u64,
    pub online_users: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityPostureReport {
    pub score: u32,
    pub max_score: u32,
    pub posture: String,
    pub generated_at: DateTime<Utc>,
    pub controls: Vec<SecurityPostureControl>,
    pub recommendations: Vec<SecurityPostureRecommendation>,
    pub counts: SecurityPostureCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityPostureControl {
    pub id: String,
    pub category: String,
    pub title: String,
    pub status: String,
    pub score: u32,
    pub max_score: u32,
    pub summary: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityPostureRecommendation {
    pub control_id: String,
    pub priority: String,
    pub title: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityPostureCounts {
    pub active_users: usize,
    pub mfa_enabled_users: usize,
    pub enabled_sso_providers: usize,
    pub enabled_conditional_access_policies: usize,
    pub enabled_dlp_policies: usize,
    pub retention_policies: usize,
    pub legal_hold_policies: usize,
    pub enabled_information_barriers: usize,
    pub sensitivity_labels: usize,
    pub encryption_keys: usize,
    pub enabled_data_residency_regions: usize,
    pub audit_events: usize,
    pub pending_compliance_reviews: usize,
}

fn push_security_control(
    controls: &mut Vec<SecurityPostureControl>,
    id: &str,
    category: &str,
    title: &str,
    passed: bool,
    score: u32,
    max_score: u32,
    summary: String,
    remediation: &str,
) {
    controls.push(SecurityPostureControl {
        id: id.to_string(),
        category: category.to_string(),
        title: title.to_string(),
        status: if passed {
            "pass"
        } else if score > 0 {
            "warning"
        } else {
            "fail"
        }
        .to_string(),
        score,
        max_score,
        summary,
        remediation: remediation.to_string(),
    });
}

fn push_validation_check(
    checks: &mut Vec<EnterpriseValidationCheck>,
    id: &str,
    area: &str,
    passed: bool,
    summary: &str,
    evidence: Vec<String>,
    blockers: Vec<String>,
) {
    checks.push(EnterpriseValidationCheck {
        id: id.to_string(),
        area: area.to_string(),
        status: if passed { "pass" } else { "fail" }.to_string(),
        summary: summary.to_string(),
        evidence,
        blockers,
    });
}

// ── Meeting templates ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub default_lobby: bool,
    pub default_mute_on_join: bool,
    pub default_allow_reactions: bool,
    pub default_recording: bool,
    pub max_participants: Option<i32>,
    pub allowed_roles: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

impl StoredObject for MeetingTemplate {
    fn collection() -> &'static str {
        "meeting_templates"
    }
    fn key(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMeetingTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_lobby: bool,
    #[serde(default)]
    pub default_mute_on_join: bool,
    #[serde(default = "default_true")]
    pub default_allow_reactions: bool,
    #[serde(default)]
    pub default_recording: bool,
    pub max_participants: Option<i32>,
    #[serde(default)]
    pub allowed_roles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMeetingTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub default_lobby: Option<bool>,
    pub default_mute_on_join: Option<bool>,
    pub default_allow_reactions: Option<bool>,
    pub default_recording: Option<bool>,
    pub max_participants: Option<Option<i32>>,
    pub allowed_roles: Option<Vec<String>>,
}

// ── Spotlight ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct SetSpotlightRequest {
    pub participant_id: Option<Uuid>,
}

// ── Live meeting reactions ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingReaction {
    pub user_id: String,
    pub user_name: String,
    pub emoji: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMeetingReactionRequest {
    pub emoji: String,
}

// ── Green room ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreenRoomParticipant {
    pub user_id: Uuid,
    pub sip_uri: String,
    pub ready: bool,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreenRoomState {
    pub conference_id: Uuid,
    pub enabled: bool,
    pub participants: Vec<GreenRoomParticipant>,
}

// ── Out-of-office ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutOfOfficeSettings {
    pub message: Option<String>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetOutOfOfficeRequest {
    pub message: Option<String>,
    pub until: Option<DateTime<Utc>>,
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
    #[serde(default = "default_dlp_status")]
    pub dlp_status: String,
    #[serde(default)]
    pub dlp_violation_count: usize,
    #[serde(default)]
    pub legal_hold: bool,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_by: Option<String>,
    #[serde(default)]
    pub folder_id: Option<Uuid>,
    #[serde(default)]
    pub locked_by: Option<String>,
    #[serde(default)]
    pub locked_at: Option<DateTime<Utc>>,
}

fn default_dlp_status() -> String {
    "clean".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudStorageStatus {
    pub provider_configured: bool,
    pub provider_name: String,
    pub open_source_options: Vec<String>,
    pub endpoint_url: Option<String>,
    pub admin_url: Option<String>,
    pub sync_mode: String,
    pub local_file_count: usize,
    pub total_storage_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiscoveryRecord {
    pub id: Uuid,
    pub owner: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    pub dlp_status: String,
    pub dlp_violation_count: usize,
    pub legal_hold: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGovernanceDecision {
    pub allowed: bool,
    pub dlp_status: String,
    pub dlp_violation_count: usize,
    pub legal_hold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MalwareQuarantineStatus {
    Quarantined,
    Released,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalwareQuarantineItem {
    pub id: Uuid,
    pub owner: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub sha256: String,
    pub reason: String,
    pub status: MalwareQuarantineStatus,
    pub detected_at: DateTime<Utc>,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewMalwareQuarantineRequest {
    pub status: MalwareQuarantineStatus,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasbAccessDecision {
    pub allowed: bool,
    pub enforced: bool,
    pub reason: String,
}

// ─── File Versioning ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersion {
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub uploader: String,
    pub size: i64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    pub storage_path: String,
}

// ─── Folder Structure ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub room_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<Uuid>,
}

// ─── Approvals Workflow ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub requestor: String,
    pub approvers: Vec<String>,
    pub status: String,
    pub responses: serde_json::Value,
    pub room_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateApprovalRequest {
    pub title: String,
    pub description: Option<String>,
    pub approvers: Vec<String>,
    pub room_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalResponseInput {
    pub decision: String, // "approve" or "reject"
    pub comment: Option<String>,
}

// ─── Recording Policies ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingPolicy {
    pub id: Uuid,
    pub name: String,
    pub trigger: String,
    pub target_ids: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRecordingPolicyRequest {
    pub name: String,
    pub trigger: String,
    pub target_ids: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

// ─── Hold Music ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldMusic {
    pub id: Uuid,
    pub name: String,
    pub file_path: String,
    pub queue_id: Option<Uuid>,
    pub is_default: bool,
    pub uploaded_by: String,
    pub created_at: DateTime<Utc>,
}

// ─── Personal Call Groups ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalCallGroup {
    pub id: Uuid,
    pub user_id: String,
    pub name: String,
    pub numbers: Vec<String>,
    pub ring_duration: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePersonalCallGroupRequest {
    pub name: String,
    pub numbers: Vec<String>,
    pub ring_duration: Option<i32>,
    pub enabled: Option<bool>,
}

// ─── OAuth API Clients ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiClient {
    pub id: Uuid,
    pub name: String,
    pub client_id: String,
    pub client_secret_hash: String,
    pub scopes: Vec<String>,
    pub redirect_uris: Vec<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiClientRequest {
    pub name: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateApiClientResponse {
    pub client: ApiClient,
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub client_id: Uuid,
    pub user_uri: Option<String>,
    pub scopes: Vec<String>,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenRequest {
    pub grant_type: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: String,
}

// ─── Bots ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    pub id: Uuid,
    pub name: String,
    pub webhook_url: String,
    pub events: Vec<String>,
    pub owner_uri: String,
    pub api_token: String,
    #[serde(default)]
    pub allowed_rooms: Vec<Uuid>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

// ── Conditional Access Policies ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalAccessConditions {
    #[serde(default)]
    pub ip_ranges: Vec<String>,
    #[serde(default)]
    pub device_types: Vec<String>,
    #[serde(default)]
    pub user_groups: Vec<String>,
    #[serde(default)]
    pub time_windows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalAccessActions {
    #[serde(default)]
    pub allow: bool,
    #[serde(default)]
    pub block: bool,
    #[serde(default)]
    pub require_mfa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalAccessPolicy {
    pub id: Uuid,
    pub name: String,
    pub conditions: ConditionalAccessConditions,
    pub actions: ConditionalAccessActions,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub webhook_url: String,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBotRequest {
    pub name: Option<String>,
    pub webhook_url: Option<String>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotMessageRequest {
    pub room_id: Uuid,
    pub body: String,
}

// ─── Calendar Integration ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarIntegration {
    pub id: Uuid,
    pub user_uri: String,
    pub provider: String,
    pub access_token_enc: String,
    pub refresh_token_enc: Option<String>,
    pub calendar_id: Option<String>,
    pub enabled: bool,
    pub last_sync: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCalendarIntegrationRequest {
    pub provider: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub calendar_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub source: String,
}

// ─── Contact Sync ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactSyncConfig {
    pub id: Uuid,
    pub user_uri: String,
    pub provider: String,
    pub access_token_enc: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateContactSyncRequest {
    pub provider: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedContact {
    pub id: Uuid,
    pub user_uri: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub source: String,
    pub external_id: Option<String>,
    pub synced_at: DateTime<Utc>,
}

// ─── Outbound Connectors ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: Uuid,
    pub name: String,
    pub connector_type: String,
    pub webhook_url: String,
    pub events: Vec<String>,
    pub auth_header: Option<String>,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateConnectorRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub connector_type: String,
    pub webhook_url: String,
    #[serde(default)]
    pub events: Vec<String>,
    pub auth_header: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConnectorRequest {
    pub name: Option<String>,
    pub webhook_url: Option<String>,
    pub events: Option<Vec<String>>,
    pub auth_header: Option<String>,
    pub enabled: Option<bool>,
}

// ─── SSO / OIDC providers ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoProvider {
    pub id: Uuid,
    pub name: String,
    pub provider_type: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret_enc: String,
    pub issuer_url: String,
    pub redirect_uri: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Pending SSO login state stored server-side for CSRF and nonce verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoPendingState {
    pub provider_id: Uuid,
    pub nonce: String,
    pub created_at: DateTime<Utc>,
}

/// Cached OIDC discovery document from `.well-known/openid-configuration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

/// JWT claims expected in an OIDC ID token.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcIdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: OidcAudience,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// OIDC `aud` claim can be a single string or an array.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OidcAudience {
    Single(String),
    Multiple(Vec<String>),
}

impl OidcAudience {
    pub fn contains(&self, client_id: &str) -> bool {
        match self {
            OidcAudience::Single(s) => s == client_id,
            OidcAudience::Multiple(v) => v.iter().any(|s| s == client_id),
        }
    }
}

// ─── Line Delegation (Boss-Secretary) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDelegation {
    pub id: Uuid,
    pub owner_uri: String,
    pub delegate_uri: String,
    pub can_answer: bool,
    pub can_make: bool,
    pub can_view_history: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLineDelegationRequest {
    pub delegate_uri: String,
    pub can_answer: Option<bool>,
    pub can_make: Option<bool>,
    pub can_view_history: Option<bool>,
}

// ─── Common Area Phones ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonAreaPhone {
    pub id: Uuid,
    pub name: String,
    pub extension: String,
    pub location: String,
    pub features: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSsoProviderRequest {
    pub name: String,
    #[serde(default = "default_oidc")]
    pub provider_type: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    pub issuer_url: String,
    pub redirect_uri: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_oidc() -> String {
    "oidc".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSsoProviderRequest {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub enabled: Option<bool>,
}

// ─── Encryption Config (BYOK) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub id: Uuid,
    pub key_id: String,
    pub key_source: String,
    pub wrapped_key_enc: String,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotateEncryptionKeyRequest {
    #[serde(default)]
    pub customer_key_base64: Option<String>,
}

// ─── Admin Elevations (PAM) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminElevation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
    pub granted_by: String,
    pub granted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAdminElevationRequest {
    pub user_id: Uuid,
    pub reason: String,
    pub duration_minutes: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCommonAreaPhoneRequest {
    pub name: String,
    pub extension: String,
    pub location: Option<String>,
    pub features: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ─── Meeting Rooms ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRoom {
    pub id: Uuid,
    pub name: String,
    pub location: String,
    pub capacity: i32,
    pub equipment: Vec<String>,
    pub bookable: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMeetingRoomRequest {
    pub name: String,
    pub location: Option<String>,
    pub capacity: Option<i32>,
    pub equipment: Option<Vec<String>>,
    pub bookable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomBooking {
    pub id: Uuid,
    pub room_id: Uuid,
    pub meeting_id: Option<Uuid>,
    pub booked_by: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRoomBookingRequest {
    pub meeting_id: Option<Uuid>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

// ─── Provisioned Devices ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionedDevice {
    pub id: Uuid,
    pub mac_address: String,
    pub model: String,
    pub assigned_user: Option<String>,
    pub config_template: String,
    pub provisioned_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProvisionedDeviceRequest {
    pub mac_address: String,
    pub model: Option<String>,
    pub assigned_user: Option<String>,
    pub config_template: Option<String>,
}

// ─── Hot Desking ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotdeskSession {
    pub id: Uuid,
    pub device_id: Uuid,
    pub user_uri: String,
    pub logged_in_at: DateTime<Utc>,
    pub logged_out_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HotdeskLoginRequest {
    pub device_id: Uuid,
    pub user_uri: String,
}

// ─── Custom Emojis ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEmoji {
    pub id: Uuid,
    pub team_id: Uuid,
    pub shortcode: String,
    pub image_url: String,
    pub uploaded_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCustomEmojiRequest {
    pub shortcode: String,
    pub image_url: String,
}

// ─── Wiki Pages ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPage {
    pub id: Uuid,
    pub team_id: Uuid,
    pub title: String,
    pub body: String,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWikiPageRequest {
    pub title: String,
    pub body: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWikiPageRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub parent_id: Option<Uuid>,
}

// ─── Task Boards ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBoard {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskBoardRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub board_id: Uuid,
    pub title: String,
    pub description: String,
    pub assignee: Option<String>,
    pub status: String,
    pub priority: String,
    pub due_date: Option<DateTime<Utc>>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub assignee: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub assignee: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
}

// ─── Inline Translation ───

#[derive(Debug, Clone, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub target_language: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub source_language: Option<String>,
    pub target_language: String,
}

// ─── Adaptive Cards ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveCard {
    #[serde(default = "default_adaptive_card_type")]
    pub card_type: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub image_url: Option<String>,
    #[serde(default)]
    pub actions: Vec<AdaptiveCardAction>,
}

fn default_adaptive_card_type() -> String {
    "AdaptiveCard".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveCardAction {
    pub action_type: String,
    pub title: String,
    pub url: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateConditionalAccessPolicyRequest {
    pub name: String,
    pub conditions: ConditionalAccessConditions,
    pub actions: ConditionalAccessActions,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConditionalAccessPolicyRequest {
    pub name: Option<String>,
    pub conditions: Option<ConditionalAccessConditions>,
    pub actions: Option<ConditionalAccessActions>,
    pub enabled: Option<bool>,
}

impl AppState {
    pub fn list_conditional_access_policies(&self) -> Vec<ConditionalAccessPolicy> {
        let mut policies = self.conditional_access_policies.values();
        policies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        policies
    }

    pub fn create_conditional_access_policy(
        &self,
        req: CreateConditionalAccessPolicyRequest,
    ) -> ConditionalAccessPolicy {
        let policy = ConditionalAccessPolicy {
            id: Uuid::new_v4(),
            name: req.name,
            conditions: req.conditions,
            actions: req.actions,
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.conditional_access_policies
            .insert(policy.id, policy.clone());
        policy
    }

    pub fn update_conditional_access_policy(
        &self,
        id: Uuid,
        req: UpdateConditionalAccessPolicyRequest,
    ) -> Option<ConditionalAccessPolicy> {
        let mut policy = self.conditional_access_policies.get(&id)?;
        if let Some(name) = req.name {
            policy.name = name;
        }
        if let Some(conditions) = req.conditions {
            policy.conditions = conditions;
        }
        if let Some(actions) = req.actions {
            policy.actions = actions;
        }
        if let Some(enabled) = req.enabled {
            policy.enabled = enabled;
        }
        self.conditional_access_policies.insert(id, policy.clone());
        Some(policy)
    }

    pub fn delete_conditional_access_policy(&self, id: Uuid) -> bool {
        self.conditional_access_policies.remove(&id).is_some()
    }

    /// Evaluate conditional access policies against a login request context.
    pub fn evaluate_conditional_access(
        &self,
        ip_address: &str,
        device_type: &str,
        user_groups: &[String],
    ) -> ConditionalAccessActions {
        let policies = self.list_conditional_access_policies();
        let mut result = ConditionalAccessActions {
            allow: true,
            block: false,
            require_mfa: false,
        };

        for policy in policies.iter().filter(|p| p.enabled) {
            let ip_match = policy.conditions.ip_ranges.is_empty()
                || policy
                    .conditions
                    .ip_ranges
                    .iter()
                    .any(|r| ip_address.starts_with(r));
            let device_match = policy.conditions.device_types.is_empty()
                || policy
                    .conditions
                    .device_types
                    .contains(&device_type.to_string());
            let group_match = policy.conditions.user_groups.is_empty()
                || policy
                    .conditions
                    .user_groups
                    .iter()
                    .any(|g| user_groups.contains(g));

            if ip_match && device_match && group_match {
                if policy.actions.block {
                    result.block = true;
                    result.allow = false;
                }
                if policy.actions.require_mfa {
                    result.require_mfa = true;
                }
                if !policy.actions.allow && !policy.actions.block {
                    result.allow = false;
                }
            }
        }
        result
    }

    // ─── Webinar Registrations ───

    pub fn register_webinar(
        &self,
        conference_id: Uuid,
        req: RegisterWebinarRequest,
    ) -> Option<WebinarRegistration> {
        let conf = self.conferences.get(&conference_id)?;
        // Check max_registrations
        if let Some(max) = conf.max_registrations {
            let current = self
                .webinar_registrations
                .values()
                .iter()
                .filter(|r| r.conference_id == conference_id)
                .count();
            if current >= max as usize {
                let reg = WebinarRegistration {
                    id: Uuid::new_v4(),
                    conference_id,
                    name: req.name,
                    email: req.email,
                    status: "waitlisted".to_string(),
                    registered_at: Utc::now(),
                    custom_fields: req.custom_fields.unwrap_or_default(),
                };
                self.webinar_registrations.insert(reg.id, reg.clone());
                return Some(reg);
            }
        }
        let reg = WebinarRegistration {
            id: Uuid::new_v4(),
            conference_id,
            name: req.name,
            email: req.email,
            status: "registered".to_string(),
            registered_at: Utc::now(),
            custom_fields: req.custom_fields.unwrap_or_default(),
        };
        self.webinar_registrations.insert(reg.id, reg.clone());
        Some(reg)
    }

    pub fn list_webinar_registrations(&self, conference_id: Uuid) -> Vec<WebinarRegistration> {
        self.webinar_registrations
            .values()
            .into_iter()
            .filter(|r| r.conference_id == conference_id)
            .collect()
    }

    pub fn update_webinar_registration(
        &self,
        _conference_id: Uuid,
        reg_id: Uuid,
        req: UpdateRegistrationRequest,
    ) -> Option<WebinarRegistration> {
        let mut reg = self.webinar_registrations.get(&reg_id)?;
        if let Some(status) = req.status {
            reg.status = status;
        }
        self.webinar_registrations.insert(reg_id, reg.clone());
        Some(reg)
    }

    // ─── CNAM Lookup ───

    pub fn cnam_lookup(&self, number: &str) -> CnamLookupResult {
        if let Some(entry) = self.cnam_cache.get(&number.to_string()) {
            if entry.expires_at.map(|e| e > Utc::now()).unwrap_or(true) {
                return CnamLookupResult {
                    phone_number: number.to_string(),
                    caller_name: Some(entry.caller_name),
                    source: Some(entry.source),
                    cached: true,
                };
            }
        }
        // Placeholder for external API lookup
        CnamLookupResult {
            phone_number: number.to_string(),
            caller_name: None,
            source: None,
            cached: false,
        }
    }

    pub fn cnam_enrich_caller_id(&self, number: &str) -> Option<String> {
        let result = self.cnam_lookup(number);
        result.caller_name
    }

    pub fn set_cnam_providers(&self, providers: Vec<CnamProviderConfig>) {
        let mut lock = self.cnam_providers.write().expect("cnam_providers lock");
        *lock = providers;
    }

    pub fn list_cnam_providers(&self) -> Vec<CnamProviderConfig> {
        self.cnam_providers
            .read()
            .expect("cnam_providers lock")
            .clone()
    }

    // ─── SIP Gateways ───

    pub fn create_sip_gateway(&self, req: CreateSipGatewayRequest) -> SipGateway {
        let gw = SipGateway {
            id: Uuid::new_v4(),
            name: req.name,
            host: req.host,
            port: req.port.unwrap_or(5060),
            transport: req.transport.unwrap_or_else(|| "tls".to_string()),
            username: req.username.and_then(non_empty_string),
            password_enc: req
                .password
                .and_then(non_empty_string)
                .map(|password| self.encrypt_field(&password)),
            prefix: req.prefix.unwrap_or_default(),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.sip_gateways.insert(gw.id, gw.clone());
        gw
    }

    pub fn list_sip_gateways(&self) -> Vec<SipGateway> {
        let mut gws = self.sip_gateways.values();
        gws.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        gws
    }

    pub fn pstn_operator_connect_status(&self) -> PstnOperatorConnectStatus {
        let provider_available = self.enterprise_capability_available("pstn_sbc_operator_connect");
        let gateways = self.list_sip_gateways();
        let enabled_gateways: Vec<_> = gateways.iter().filter(|gateway| gateway.enabled).collect();
        let location_routes = self.list_location_routing_rules();
        let enabled_location_route_count =
            location_routes.iter().filter(|rule| rule.enabled).count();
        let e164_prefix_route_count = enabled_gateways
            .iter()
            .filter(|gateway| gateway.prefix.starts_with('+'))
            .count();
        let tls_gateway_count = enabled_gateways
            .iter()
            .filter(|gateway| gateway.transport.eq_ignore_ascii_case("tls"))
            .count();
        let authenticated_gateway_count = enabled_gateways
            .iter()
            .filter(|gateway| {
                gateway
                    .username
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
                    && gateway.password_enc.is_some()
            })
            .count();
        let emergency_route_ready = provider_available
            && enabled_gateways.iter().any(|gateway| {
                gateway.prefix.is_empty() || gateway.prefix == "911" || gateway.prefix == "+"
            });
        let routable =
            provider_available && !enabled_gateways.is_empty() && e164_prefix_route_count > 0;
        let mut blockers = Vec::new();
        if !provider_available {
            blockers.push("pstn_provider_not_configured".to_string());
        }
        if enabled_gateways.is_empty() {
            blockers.push("no_enabled_sip_gateway".to_string());
        }
        if e164_prefix_route_count == 0 {
            blockers.push("no_e164_prefix_route".to_string());
        }
        if tls_gateway_count == 0 {
            blockers.push("no_tls_gateway".to_string());
        }
        if !emergency_route_ready {
            blockers.push("emergency_route_not_ready".to_string());
        }

        PstnOperatorConnectStatus {
            provider_available,
            routable,
            gateway_count: gateways.len(),
            enabled_gateway_count: enabled_gateways.len(),
            tls_gateway_count,
            authenticated_gateway_count,
            e164_prefix_route_count,
            enabled_location_route_count,
            emergency_route_ready,
            blockers,
        }
    }

    pub fn update_sip_gateway(&self, id: Uuid, req: UpdateSipGatewayRequest) -> Option<SipGateway> {
        let mut gw = self.sip_gateways.get(&id)?;
        if let Some(name) = req.name {
            gw.name = name;
        }
        if let Some(host) = req.host {
            gw.host = host;
        }
        if let Some(port) = req.port {
            gw.port = port;
        }
        if let Some(transport) = req.transport {
            gw.transport = transport;
        }
        if let Some(username) = req.username {
            gw.username = non_empty_string(username);
        }
        if let Some(password) = req.password {
            gw.password_enc =
                non_empty_string(password).map(|password| self.encrypt_field(&password));
        }
        if let Some(prefix) = req.prefix {
            gw.prefix = prefix;
        }
        if let Some(enabled) = req.enabled {
            gw.enabled = enabled;
        }
        self.sip_gateways.insert(id, gw.clone());
        Some(gw)
    }

    pub fn delete_sip_gateway(&self, id: Uuid) -> bool {
        self.sip_gateways.remove(&id).is_some()
    }

    /// Find a gateway matching a dialed number by prefix (longest prefix wins).
    pub fn resolve_gateway(&self, dialed_number: &str) -> Option<SipGateway> {
        let mut best: Option<SipGateway> = None;
        let mut best_len = 0;
        for gw in self.sip_gateways.values() {
            if gw.enabled && dialed_number.starts_with(&gw.prefix) && gw.prefix.len() >= best_len {
                best_len = gw.prefix.len();
                best = Some(gw);
            }
        }
        best
    }

    // ─── Location Routing Rules ───

    pub fn create_location_routing_rule(
        &self,
        req: CreateLocationRoutingRuleRequest,
    ) -> LocationRoutingRule {
        let rule = LocationRoutingRule {
            id: Uuid::new_v4(),
            name: req.name,
            location_pattern: req.location_pattern,
            gateway_id: req.gateway_id,
            priority: req.priority.unwrap_or(0),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.location_routing_rules.insert(rule.id, rule.clone());
        rule
    }

    pub fn list_location_routing_rules(&self) -> Vec<LocationRoutingRule> {
        let mut rules = self.location_routing_rules.values();
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));
        rules
    }

    pub fn update_location_routing_rule(
        &self,
        id: Uuid,
        req: UpdateLocationRoutingRuleRequest,
    ) -> Option<LocationRoutingRule> {
        let mut rule = self.location_routing_rules.get(&id)?;
        if let Some(name) = req.name {
            rule.name = name;
        }
        if let Some(pattern) = req.location_pattern {
            rule.location_pattern = pattern;
        }
        if let Some(gw) = req.gateway_id {
            rule.gateway_id = gw;
        }
        if let Some(p) = req.priority {
            rule.priority = p;
        }
        if let Some(e) = req.enabled {
            rule.enabled = e;
        }
        self.location_routing_rules.insert(id, rule.clone());
        Some(rule)
    }

    pub fn delete_location_routing_rule(&self, id: Uuid) -> bool {
        self.location_routing_rules.remove(&id).is_some()
    }

    /// Evaluate location routing rules to find the best gateway for a location.
    pub fn resolve_location_route(&self, location: &str) -> Option<SipGateway> {
        let rules = self.list_location_routing_rules();
        for rule in rules.iter().filter(|r| r.enabled) {
            if location.contains(&rule.location_pattern) {
                return self.sip_gateways.get(&rule.gateway_id);
            }
        }
        None
    }

    pub fn create_emergency_location(
        &self,
        req: CreateEmergencyLocationRequest,
    ) -> EmergencyLocation {
        let location = EmergencyLocation {
            id: Uuid::new_v4(),
            name: req.name,
            address_line1: req.address_line1,
            address_line2: req.address_line2,
            city: req.city,
            region: req.region,
            postal_code: req.postal_code,
            country: req.country.unwrap_or_else(|| "US".to_string()),
            elin: req.elin,
            callback_number: req.callback_number,
            provider_location_id: req.provider_location_id,
            validated: req.validated.unwrap_or(false),
            created_at: Utc::now(),
        };
        self.emergency_locations
            .insert(location.id, location.clone());
        self.persist(&location);
        location
    }

    pub fn list_emergency_locations(&self) -> Vec<EmergencyLocation> {
        let mut locations = self.emergency_locations.values();
        locations.sort_by(|a, b| a.name.cmp(&b.name));
        locations
    }

    pub fn update_emergency_location(
        &self,
        id: Uuid,
        req: UpdateEmergencyLocationRequest,
    ) -> Option<EmergencyLocation> {
        let mut location = self.emergency_locations.get(&id)?;
        if let Some(value) = req.name {
            location.name = value;
        }
        if let Some(value) = req.address_line1 {
            location.address_line1 = value;
        }
        if let Some(value) = req.address_line2 {
            location.address_line2 = non_empty_string(value);
        }
        if let Some(value) = req.city {
            location.city = value;
        }
        if let Some(value) = req.region {
            location.region = value;
        }
        if let Some(value) = req.postal_code {
            location.postal_code = value;
        }
        if let Some(value) = req.country {
            location.country = value;
        }
        if let Some(value) = req.elin {
            location.elin = non_empty_string(value);
        }
        if let Some(value) = req.callback_number {
            location.callback_number = non_empty_string(value);
        }
        if let Some(value) = req.provider_location_id {
            location.provider_location_id = non_empty_string(value);
        }
        if let Some(value) = req.validated {
            location.validated = value;
        }
        self.emergency_locations.insert(id, location.clone());
        self.persist(&location);
        Some(location)
    }

    pub fn delete_emergency_location(&self, id: Uuid) -> bool {
        if self
            .emergency_assignments
            .values()
            .into_iter()
            .any(|assignment| assignment.location_id == id)
        {
            return false;
        }
        let deleted = self.emergency_locations.remove(&id).is_some();
        if deleted {
            self.delete_persisted(EmergencyLocation::collection(), id.to_string());
        }
        deleted
    }

    pub fn assign_emergency_location(
        &self,
        req: AssignEmergencyLocationRequest,
        updated_by: &str,
    ) -> Option<EmergencyCallingAssignment> {
        let normalized_user = normalize_emergency_user_uri(&req.user_uri)?;
        self.emergency_locations.get(&req.location_id)?;
        let mut emergency_numbers = req
            .emergency_numbers
            .unwrap_or_else(|| vec!["911".to_string(), "112".to_string(), "933".to_string()]);
        emergency_numbers = emergency_numbers
            .into_iter()
            .map(|number| normalize_dialed_number(&number))
            .filter(|number| !number.is_empty())
            .collect();
        emergency_numbers.sort();
        emergency_numbers.dedup();
        let assignment = EmergencyCallingAssignment {
            user_uri: normalized_user,
            location_id: req.location_id,
            emergency_numbers,
            updated_by: updated_by.to_string(),
            updated_at: Utc::now(),
        };
        self.emergency_assignments
            .insert(assignment.user_uri.clone(), assignment.clone());
        self.persist(&assignment);
        Some(assignment)
    }

    pub fn list_emergency_assignments(&self) -> Vec<EmergencyCallingAssignment> {
        let mut assignments = self.emergency_assignments.values();
        assignments.sort_by(|a, b| a.user_uri.cmp(&b.user_uri));
        assignments
    }

    pub fn remove_emergency_assignment(&self, user_uri: &str) -> bool {
        let Some(normalized_user) = normalize_emergency_user_uri(user_uri) else {
            return false;
        };
        let removed = self
            .emergency_assignments
            .remove(&normalized_user)
            .is_some();
        if removed {
            self.delete_persisted(
                EmergencyCallingAssignment::collection(),
                normalized_user.to_string(),
            );
        }
        removed
    }

    pub fn emergency_call_plan(&self, caller_uri: &str, dialed_number: &str) -> EmergencyCallPlan {
        let normalized_caller =
            normalize_emergency_user_uri(caller_uri).unwrap_or_else(|| caller_uri.to_string());
        let normalized_number = normalize_dialed_number(dialed_number);
        let assignment = self.emergency_assignments.get(&normalized_caller);
        let emergency = assignment.as_ref().is_some_and(|assignment| {
            assignment
                .emergency_numbers
                .iter()
                .any(|number| normalize_dialed_number(number) == normalized_number)
        }) || matches!(normalized_number.as_str(), "911" | "112" | "933");
        let e911_provider_available = self.enterprise_capability_available("e911");
        let pstn_provider_available =
            self.enterprise_capability_available("pstn_sbc_operator_connect");
        let location = assignment
            .as_ref()
            .and_then(|assignment| self.emergency_locations.get(&assignment.location_id));
        let gateway = location.as_ref().and_then(|location| {
            self.resolve_location_route(&format!(
                "{} {} {} {} {}",
                location.name,
                location.city,
                location.region,
                location.postal_code,
                location.country
            ))
            .or_else(|| self.resolve_gateway(&normalized_number))
        });
        let (allowed, reason) = if !emergency {
            (true, "not_emergency_number".to_string())
        } else if assignment.is_none() {
            (false, "missing_emergency_location_assignment".to_string())
        } else if location.as_ref().is_none_or(|location| !location.validated) {
            (false, "emergency_location_not_validated".to_string())
        } else if !e911_provider_available {
            (false, "e911_provider_not_configured".to_string())
        } else if !pstn_provider_available {
            (false, "pstn_provider_not_configured".to_string())
        } else if gateway.is_none() {
            (false, "no_emergency_gateway_route".to_string())
        } else {
            (true, "routable".to_string())
        };
        EmergencyCallPlan {
            caller_uri: normalized_caller,
            dialed_number: normalized_number,
            emergency,
            allowed,
            reason,
            location,
            gateway,
            e911_provider_available,
            pstn_provider_available,
        }
    }

    pub fn enterprise_capability_available(&self, id: &str) -> bool {
        self.enterprise_capability_report()
            .capabilities
            .into_iter()
            .any(|capability| capability.id == id && capability.status == "available")
    }

    // ─── Caption Language ───

    pub fn set_caption_language(&self, conference_id: Uuid, _language: &str) -> bool {
        self.conferences.get(&conference_id).is_some()
        // Language preference is stored in the transcript segments themselves
    }

    pub fn get_transcript_in_language(
        &self,
        conference_id: Uuid,
        language: Option<&str>,
    ) -> Vec<TranscriptSegment> {
        let segments = self.get_transcript(conference_id);
        match language {
            Some(lang) => segments
                .into_iter()
                .filter(|s| s.language.as_deref().unwrap_or("en") == lang)
                .collect(),
            None => segments,
        }
    }
}

// ─── Webinar Registrations ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebinarRegistration {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub name: String,
    pub email: String,
    pub status: String,
    pub registered_at: DateTime<Utc>,
    #[serde(default)]
    pub custom_fields: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterWebinarRequest {
    pub name: String,
    pub email: String,
    pub custom_fields: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRegistrationRequest {
    pub status: Option<String>,
}

// ─── Channel Tabs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTab {
    pub id: Uuid,
    pub room_id: Uuid,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub position: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateChannelTabRequest {
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateChannelTabRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub position: Option<i32>,
}

// ─── Message Extensions ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageExtension {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub description: String,
    pub handler_url: String,
    pub icon: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessageExtensionRequest {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub handler_url: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMessageExtensionRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub description: Option<String>,
    pub handler_url: Option<String>,
    pub icon: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokeMessageExtensionRequest {
    pub input: String,
}

// ─── App Catalog ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCatalogEntry {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon_url: Option<String>,
    pub manifest_url: Option<String>,
    pub installed: bool,
    pub installed_by: Option<String>,
    pub installed_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAppCatalogEntryRequest {
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon_url: Option<String>,
    pub manifest_url: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAppCatalogEntryRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon_url: Option<String>,
    pub manifest_url: Option<String>,
    pub version: Option<String>,
}

// ─── Guest Users ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub invited_by: String,
    pub team_id: Uuid,
    #[serde(default)]
    pub permissions: serde_json::Value,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InviteGuestRequest {
    pub email: String,
    pub display_name: String,
    pub permissions: Option<serde_json::Value>,
    pub expires_hours: Option<i64>,
}

// ─── CNAM Cache ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CnamEntry {
    pub id: Uuid,
    pub phone_number: String,
    pub caller_name: String,
    pub source: String,
    pub cached_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CnamLookupResult {
    pub phone_number: String,
    pub caller_name: Option<String>,
    pub source: Option<String>,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CnamProviderConfig {
    pub name: String,
    pub api_url: String,
    pub api_key_enc: Option<String>,
    pub enabled: bool,
}

// ─── Caption Language ───

#[derive(Debug, Clone, Deserialize)]
pub struct CaptionLanguageRequest {
    pub language: String,
}

// ─── SIP Gateways ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipGateway {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub transport: String,
    pub username: Option<String>,
    #[serde(default, skip_serializing)]
    pub password_enc: Option<String>,
    pub prefix: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstnOperatorConnectStatus {
    pub provider_available: bool,
    pub routable: bool,
    pub gateway_count: usize,
    pub enabled_gateway_count: usize,
    pub tls_gateway_count: usize,
    pub authenticated_gateway_count: usize,
    pub e164_prefix_route_count: usize,
    pub enabled_location_route_count: usize,
    pub emergency_route_ready: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSipGatewayRequest {
    pub name: String,
    pub host: String,
    pub port: Option<i32>,
    pub transport: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub prefix: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSipGatewayRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub transport: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub prefix: Option<String>,
    pub enabled: Option<bool>,
}

// ─── Location Routing Rules ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationRoutingRule {
    pub id: Uuid,
    pub name: String,
    pub location_pattern: String,
    pub gateway_id: Uuid,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLocationRoutingRuleRequest {
    pub name: String,
    pub location_pattern: String,
    pub gateway_id: Uuid,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLocationRoutingRuleRequest {
    pub name: Option<String>,
    pub location_pattern: Option<String>,
    pub gateway_id: Option<Uuid>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
}

// ─── Emergency Calling / E911 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyLocation {
    pub id: Uuid,
    pub name: String,
    pub address_line1: String,
    pub address_line2: Option<String>,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
    pub elin: Option<String>,
    pub callback_number: Option<String>,
    pub provider_location_id: Option<String>,
    pub validated: bool,
    pub created_at: DateTime<Utc>,
}

impl StoredObject for EmergencyLocation {
    fn collection() -> &'static str {
        "emergency_locations"
    }

    fn key(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEmergencyLocationRequest {
    pub name: String,
    pub address_line1: String,
    pub address_line2: Option<String>,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: Option<String>,
    pub elin: Option<String>,
    pub callback_number: Option<String>,
    pub provider_location_id: Option<String>,
    pub validated: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEmergencyLocationRequest {
    pub name: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub elin: Option<String>,
    pub callback_number: Option<String>,
    pub provider_location_id: Option<String>,
    pub validated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyCallingAssignment {
    pub user_uri: String,
    pub location_id: Uuid,
    pub emergency_numbers: Vec<String>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

impl StoredObject for EmergencyCallingAssignment {
    fn collection() -> &'static str {
        "emergency_calling_assignments"
    }

    fn key(&self) -> String {
        self.user_uri.clone()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignEmergencyLocationRequest {
    pub user_uri: String,
    pub location_id: Uuid,
    pub emergency_numbers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyCallPlan {
    pub caller_uri: String,
    pub dialed_number: String,
    pub emergency: bool,
    pub allowed: bool,
    pub reason: String,
    pub location: Option<EmergencyLocation>,
    pub gateway: Option<SipGateway>,
    pub e911_provider_available: bool,
    pub pstn_provider_available: bool,
}

// ─── Screen Share Annotations ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: Uuid,
    pub conference_id: Uuid,
    #[serde(rename = "type")]
    pub annotation_type: String, // draw, text, highlight
    pub data: AnnotationData,
    pub author_uri: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationData {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAnnotationRequest {
    #[serde(rename = "type")]
    pub annotation_type: String,
    pub data: AnnotationData,
}

impl AppState {
    pub fn add_annotation(
        &self,
        conference_id: Uuid,
        author_uri: &str,
        req: CreateAnnotationRequest,
    ) -> Annotation {
        let annotation = Annotation {
            id: Uuid::new_v4(),
            conference_id,
            annotation_type: req.annotation_type,
            data: req.data,
            author_uri: author_uri.to_string(),
            created_at: Utc::now(),
        };
        let mut annotations = self
            .conference_annotations
            .get(&conference_id)
            .unwrap_or_default();
        annotations.push(annotation.clone());
        self.conference_annotations
            .insert(conference_id, annotations);
        annotation
    }

    pub fn list_annotations(&self, conference_id: Uuid) -> Vec<Annotation> {
        self.conference_annotations
            .get(&conference_id)
            .unwrap_or_default()
    }

    pub fn clear_annotations(&self, conference_id: Uuid) {
        self.conference_annotations.remove(&conference_id);
    }
}

// ─── Whiteboards ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whiteboard {
    pub id: Uuid,
    pub conference_id: Uuid,
    pub name: String,
    pub elements: Vec<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWhiteboardRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddWhiteboardElementRequest {
    pub element: serde_json::Value,
}

impl AppState {
    pub fn get_or_create_whiteboard(
        &self,
        conference_id: Uuid,
        name: Option<String>,
    ) -> Whiteboard {
        if let Some(wb) = self.whiteboards.get(&conference_id) {
            return wb;
        }
        let wb = Whiteboard {
            id: Uuid::new_v4(),
            conference_id,
            name: name.unwrap_or_else(|| "Whiteboard".to_string()),
            elements: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.whiteboards.insert(conference_id, wb.clone());
        wb
    }

    pub fn get_whiteboard(&self, conference_id: Uuid) -> Option<Whiteboard> {
        self.whiteboards.get(&conference_id)
    }

    pub fn add_whiteboard_element(
        &self,
        conference_id: Uuid,
        element: serde_json::Value,
    ) -> Option<Whiteboard> {
        let mut wb = self.whiteboards.get(&conference_id)?;
        wb.elements.push(element);
        wb.updated_at = Utc::now();
        self.whiteboards.insert(conference_id, wb.clone());
        Some(wb)
    }
}

// ─── Scheduling Panels ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingPanel {
    pub id: Uuid,
    pub name: String,
    pub meeting_room_id: Uuid,
    pub device_identifier: String,
    pub display_mode: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

// ─── Bandwidth Policies ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthPolicy {
    pub id: Uuid,
    pub name: String,
    pub max_concurrent_calls: i32,
    pub max_bandwidth_kbps: i32,
    pub location_pattern: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSchedulingPanelRequest {
    pub name: String,
    pub meeting_room_id: Uuid,
    pub device_identifier: String,
    pub display_mode: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBandwidthPolicyRequest {
    pub name: String,
    pub max_concurrent_calls: Option<i32>,
    pub max_bandwidth_kbps: Option<i32>,
    pub location_pattern: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSchedulingPanelRequest {
    pub name: Option<String>,
    pub meeting_room_id: Option<Uuid>,
    pub display_mode: Option<String>,
    pub enabled: Option<bool>,
}

impl AppState {
    pub fn list_scheduling_panels(&self) -> Vec<SchedulingPanel> {
        let mut panels = self.scheduling_panels.values();
        panels.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        panels
    }

    pub fn create_scheduling_panel(&self, req: CreateSchedulingPanelRequest) -> SchedulingPanel {
        let panel = SchedulingPanel {
            id: Uuid::new_v4(),
            name: req.name,
            meeting_room_id: req.meeting_room_id,
            device_identifier: req.device_identifier,
            display_mode: req.display_mode.unwrap_or_else(|| "schedule".to_string()),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.scheduling_panels.insert(panel.id, panel.clone());
        panel
    }

    pub fn update_scheduling_panel(
        &self,
        id: Uuid,
        req: UpdateSchedulingPanelRequest,
    ) -> Option<SchedulingPanel> {
        let mut panel = self.scheduling_panels.get(&id)?;
        if let Some(name) = req.name {
            panel.name = name;
        }
        if let Some(room_id) = req.meeting_room_id {
            panel.meeting_room_id = room_id;
        }
        if let Some(mode) = req.display_mode {
            panel.display_mode = mode;
        }
        if let Some(enabled) = req.enabled {
            panel.enabled = enabled;
        }
        self.scheduling_panels.insert(id, panel.clone());
        Some(panel)
    }

    pub fn delete_scheduling_panel(&self, id: Uuid) -> bool {
        self.scheduling_panels.remove(&id).is_some()
    }

    pub fn get_panel_schedule(
        &self,
        device_identifier: &str,
    ) -> Option<(SchedulingPanel, Vec<RoomBooking>)> {
        let panel = self
            .scheduling_panels
            .values()
            .into_iter()
            .find(|p| p.device_identifier == device_identifier && p.enabled)?;
        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            .unwrap_or_else(Utc::now);
        let today_end = today_start + Duration::hours(24);
        let bookings: Vec<RoomBooking> = self
            .room_bookings
            .values()
            .into_iter()
            .filter(|b| {
                b.room_id == panel.meeting_room_id
                    && b.start_time < today_end
                    && b.end_time > today_start
            })
            .collect();
        Some((panel, bookings))
    }
}

// ─── Automation Rules (Workflow Builder) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: Uuid,
    pub name: String,
    pub trigger_event: String, // message_received, call_completed, meeting_started, user_joined
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAutomationRuleRequest {
    pub name: String,
    pub trigger_event: String,
    #[serde(default = "default_json_array")]
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAutomationRuleRequest {
    pub name: Option<String>,
    pub trigger_event: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn default_json_array() -> serde_json::Value {
    serde_json::Value::Array(Vec::new())
}

impl AppState {
    pub fn list_automation_rules(&self) -> Vec<AutomationRule> {
        let mut rules = self.automation_rules.values();
        rules.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        rules
    }

    pub fn create_automation_rule(
        &self,
        created_by: &str,
        req: CreateAutomationRuleRequest,
    ) -> AutomationRule {
        let rule = AutomationRule {
            id: Uuid::new_v4(),
            name: req.name,
            trigger_event: req.trigger_event,
            conditions: req.conditions,
            actions: req.actions,
            enabled: req.enabled.unwrap_or(true),
            created_by: created_by.to_string(),
            created_at: Utc::now(),
        };
        self.automation_rules.insert(rule.id, rule.clone());
        rule
    }

    pub fn update_automation_rule(
        &self,
        id: Uuid,
        req: UpdateAutomationRuleRequest,
    ) -> Option<AutomationRule> {
        let mut rule = self.automation_rules.get(&id)?;
        if let Some(name) = req.name {
            rule.name = name;
        }
        if let Some(trigger) = req.trigger_event {
            rule.trigger_event = trigger;
        }
        if let Some(conditions) = req.conditions {
            rule.conditions = conditions;
        }
        if let Some(actions) = req.actions {
            rule.actions = actions;
        }
        if let Some(enabled) = req.enabled {
            rule.enabled = enabled;
        }
        self.automation_rules.insert(id, rule.clone());
        Some(rule)
    }

    pub fn delete_automation_rule(&self, id: Uuid) -> bool {
        self.automation_rules.remove(&id).is_some()
    }

    pub fn evaluate_automation_rules(&self, event_type: &str, context: &serde_json::Value) {
        let rules = self.automation_rules.values();
        for rule in rules
            .iter()
            .filter(|r| r.enabled && r.trigger_event == event_type)
        {
            // Evaluate conditions - simple match for now
            let conditions_match = match rule.conditions.as_array() {
                Some(conditions) if conditions.is_empty() => true,
                Some(conditions) => conditions.iter().all(|cond| {
                    let field = cond.get("field").and_then(|v| v.as_str()).unwrap_or("");
                    let expected = cond.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    context.get(field).and_then(|v| v.as_str()).unwrap_or("") == expected
                }),
                None => true,
            };
            if conditions_match {
                // Execute actions via SSE broadcast
                if let Some(actions) = rule.actions.as_array() {
                    for action in actions {
                        let _ = self.sse_tx.send(SseEvent {
                            event_type: "automation_action".to_string(),
                            payload: serde_json::json!({
                                "rule_id": rule.id,
                                "action": action,
                                "context": context,
                            }),
                        });
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBandwidthPolicyRequest {
    pub name: Option<String>,
    pub max_concurrent_calls: Option<i32>,
    pub max_bandwidth_kbps: Option<i32>,
    pub location_pattern: Option<String>,
    pub enabled: Option<bool>,
}

// ─── Signage Displays ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignageDisplay {
    pub id: Uuid,
    pub name: String,
    pub location: String,
    pub content_url: String,
    pub schedule: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSignageDisplayRequest {
    pub name: String,
    pub location: Option<String>,
    pub content_url: Option<String>,
    pub schedule: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSignageDisplayRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub content_url: Option<String>,
    pub schedule: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseIntegration {
    pub id: String,
    pub category: String,
    pub name: String,
    pub description: String,
    pub integration_kind: String,
    pub default_provider: String,
    pub open_source_option: String,
    pub required_dependency: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub admin_url: Option<String>,
    #[serde(default)]
    pub api_key_configured: bool,
    #[serde(default, skip_serializing)]
    pub api_key_enc: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl StoredObject for EnterpriseIntegration {
    fn collection() -> &'static str {
        "enterprise_integrations"
    }

    fn key(&self) -> String {
        self.id.clone()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEnterpriseIntegrationRequest {
    pub enabled: Option<bool>,
    pub endpoint_url: Option<String>,
    pub admin_url: Option<String>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseCapabilityStatus {
    pub id: String,
    pub category: String,
    pub name: String,
    pub status: String,
    pub enabled: bool,
    pub configured: bool,
    pub blocking_dependency: Option<String>,
    pub default_provider: String,
    pub open_source_option: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseCapabilityReport {
    pub total: usize,
    pub available: usize,
    pub configured: usize,
    pub blocked: usize,
    pub capabilities: Vec<EnterpriseCapabilityStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseParityBlocker {
    pub id: String,
    pub category: String,
    pub name: String,
    pub status: String,
    pub required_dependency: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseParityReadinessReport {
    pub ready: bool,
    pub score: u8,
    pub available: usize,
    pub total: usize,
    pub critical_blockers: Vec<EnterpriseParityBlocker>,
    pub warnings: Vec<String>,
    pub consensus: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseIntegrationHealthCheck {
    pub id: String,
    pub category: String,
    pub name: String,
    pub status: String,
    pub checked_at: DateTime<Utc>,
    pub checks: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseIntegrationHealthReport {
    pub healthy: usize,
    pub warning: usize,
    pub blocked: usize,
    pub checked_at: DateTime<Utc>,
    pub integrations: Vec<EnterpriseIntegrationHealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseProviderProbe {
    pub id: String,
    pub category: String,
    pub name: String,
    pub adapter: String,
    pub target: Option<String>,
    pub status: String,
    pub latency_ms: Option<u128>,
    pub checked_at: DateTime<Utc>,
    pub evidence: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseProviderProbeReport {
    pub checked_at: DateTime<Utc>,
    pub reachable: usize,
    pub warning: usize,
    pub blocked: usize,
    pub probes: Vec<EnterpriseProviderProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseValidationCheck {
    pub id: String,
    pub area: String,
    pub status: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseValidationReport {
    pub generated_at: DateTime<Utc>,
    pub ready: bool,
    pub score: u8,
    pub passed: usize,
    pub warning: usize,
    pub failed: usize,
    pub checks: Vec<EnterpriseValidationCheck>,
    pub consensus: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseDeploymentPlanItem {
    pub id: String,
    pub category: String,
    pub name: String,
    pub priority: String,
    pub status: String,
    pub required_dependency: String,
    pub open_source_option: String,
    pub default_provider: String,
    pub endpoint_required: bool,
    pub credentials_required: bool,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseDeploymentPlan {
    pub generated_at: DateTime<Utc>,
    pub ready_to_deploy: bool,
    pub total: usize,
    pub completed: usize,
    pub remaining: usize,
    pub items: Vec<EnterpriseDeploymentPlanItem>,
    pub summary: Vec<String>,
}

fn enterprise_integration_defaults() -> Vec<EnterpriseIntegration> {
    let now = Utc::now();
    [
        (
            "speech_ivr",
            "ML/AI",
            "Speech IVR",
            "Speech-to-text and natural-language routing for IVR flows.",
            "ai_service",
            "Vosk or Whisper endpoint",
            "Vosk, whisper.cpp, Rasa",
            "ASR/NLU service reachable from the server",
        ),
        (
            "noise_suppression",
            "ML/AI",
            "Noise suppression",
            "Client or media-server noise suppression for calls and meetings.",
            "local_or_media_runtime",
            "WebRTC audio processing",
            "RNNoise, WebRTC NS",
            "Desktop/mobile audio runtime or SFU DSP support",
        ),
        (
            "auto_transcription",
            "ML/AI",
            "Auto-transcription",
            "Automatic meeting and call transcript generation.",
            "ai_service",
            "Whisper-compatible endpoint",
            "whisper.cpp, faster-whisper",
            "ASR service with recording or live audio access",
        ),
        (
            "text_to_speech",
            "ML/AI",
            "Text-to-speech",
            "Speech synthesis for IVR prompts, accessibility, and assistant readouts.",
            "ai_service",
            "Piper-compatible TTS endpoint",
            "Piper, Coqui TTS, Mimic3",
            "TTS synthesis service reachable from the server",
        ),
        (
            "meeting_assistant",
            "ML/AI",
            "AI meeting assistant",
            "Summaries, action items, and meeting Q&A over transcripts.",
            "ai_service",
            "OpenAI-compatible LLM endpoint",
            "Ollama, vLLM, LocalAI",
            "LLM service and transcript source",
        ),
        (
            "copilot",
            "ML/AI",
            "Copilot-style assistant",
            "Cross-workspace assistant for chat, meeting, file, and admin context.",
            "ai_service",
            "OpenAI-compatible LLM/RAG service",
            "Ollama, vLLM, Open WebUI pipelines",
            "LLM plus governed search/RAG index",
        ),
        (
            "virtual_backgrounds",
            "Media pipeline",
            "Virtual backgrounds",
            "Background blur/replacement for video calls.",
            "client_media_runtime",
            "WebRTC insertable streams",
            "MediaPipe, TensorFlow Lite",
            "Client video segmentation support",
        ),
        (
            "together_gallery",
            "Media pipeline",
            "Together mode and gallery",
            "Large gallery layout and composited meeting scenes.",
            "sfu_layout_service",
            "LiveKit/Jitsi layout service",
            "LiveKit, Jitsi, mediasoup",
            "SFU layout/compositor service",
        ),
        (
            "ndi_rtmp_streaming",
            "Media pipeline",
            "NDI/RTMP streaming",
            "Broadcast meeting output to NDI, RTMP, or recording pipelines.",
            "media_gateway",
            "RTMP gateway",
            "SRS, FFmpeg, Janus",
            "Streaming gateway reachable from media server",
        ),
        (
            "powerpoint_live",
            "Media pipeline",
            "PowerPoint Live",
            "Server-rendered slide sharing with presenter controls.",
            "document_render_service",
            "Collabora/LibreOffice renderer",
            "Collabora Online, LibreOffice headless",
            "Document conversion/rendering service",
        ),
        (
            "e911",
            "External services",
            "E911",
            "Emergency address, dispatchable location, and emergency call routing.",
            "carrier_service",
            "Emergency calling provider",
            "none",
            "Certified E911 provider or carrier contract",
        ),
        (
            "pstn_sbc_operator_connect",
            "External services",
            "PSTN/SBC/Operator Connect",
            "Carrier trunking, SBC routing, and operator connectivity.",
            "carrier_service",
            "SIP trunk or SBC provider",
            "OpenSIPS, Kamailio, FreeSWITCH",
            "Carrier/SBC trunk and numbering plan",
        ),
        (
            "cloud_storage",
            "External services",
            "Cloud storage",
            "SharePoint/OneDrive/GDrive-equivalent file backend.",
            "storage_service",
            "WebDAV/S3-compatible storage",
            "Nextcloud, ownCloud, MinIO",
            "External storage endpoint and credentials",
        ),
        (
            "advanced_threat_protection",
            "Security infra",
            "Advanced threat protection",
            "Malware scanning and attachment detonation workflow.",
            "security_service",
            "Malware scanning service",
            "ClamAV, YARA, Cuckoo",
            "Scanning daemon or sandbox endpoint",
        ),
        (
            "casb",
            "Security infra",
            "CASB",
            "Cloud app policy enforcement, access decisions, and activity controls.",
            "security_policy_service",
            "Policy decision service",
            "OPA, Wazuh, OpenSearch Security Analytics",
            "Policy engine and event stream",
        ),
        (
            "mobile_app",
            "Platform rewrites",
            "Mobile app",
            "Native mobile packaging and device capability surface.",
            "client_platform",
            "Capacitor/Tauri mobile build",
            "Capacitor, React Native",
            "Mobile build pipeline and app signing",
        ),
        (
            "web_client",
            "Platform rewrites",
            "Web client",
            "Browser deployment with web-safe calling and auth flows.",
            "client_platform",
            "Hosted web bundle",
            "Vite static hosting, Nginx",
            "TLS web hosting and browser media compatibility",
        ),
        (
            "popout_multi_window",
            "Platform rewrites",
            "Pop-out multi-window",
            "Separate windows for calls, chats, and meetings.",
            "desktop_runtime",
            "Tauri multi-window runtime",
            "Tauri windows",
            "Desktop shell support",
        ),
        (
            "town_hall_broadcast",
            "Scalability",
            "Town hall broadcast",
            "10,000+ viewer broadcast using SFU/CDN fanout.",
            "broadcast_service",
            "Broadcast SFU/CDN",
            "LiveKit egress, Janus, SRS, CDN",
            "SFU, egress, and CDN capacity",
        ),
    ]
    .into_iter()
    .map(
        |(
            id,
            category,
            name,
            description,
            integration_kind,
            default_provider,
            open_source_option,
            required_dependency,
        )| EnterpriseIntegration {
            id: id.to_string(),
            category: category.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            integration_kind: integration_kind.to_string(),
            default_provider: default_provider.to_string(),
            open_source_option: open_source_option.to_string(),
            required_dependency: required_dependency.to_string(),
            enabled: false,
            endpoint_url: None,
            admin_url: None,
            api_key_configured: false,
            api_key_enc: None,
            notes: String::new(),
            updated_by: None,
            updated_at: now,
        },
    )
    .collect()
}

fn enterprise_capability_status(integration: EnterpriseIntegration) -> EnterpriseCapabilityStatus {
    let configured = integration.endpoint_url.is_some()
        || integration.admin_url.is_some()
        || integration.api_key_configured
        || integration.api_key_enc.is_some()
        || matches!(
            integration.integration_kind.as_str(),
            "client_media_runtime"
                | "client_platform"
                | "desktop_runtime"
                | "local_or_media_runtime"
        ) && integration.enabled;
    let status = if integration.enabled && configured {
        "available"
    } else if integration.enabled {
        "needs_configuration"
    } else {
        "blocked"
    };
    EnterpriseCapabilityStatus {
        id: integration.id,
        category: integration.category,
        name: integration.name,
        status: status.to_string(),
        enabled: integration.enabled,
        configured,
        blocking_dependency: (status != "available").then_some(integration.required_dependency),
        default_provider: integration.default_provider,
        open_source_option: integration.open_source_option,
    }
}

fn enterprise_parity_recommendation(capability: &EnterpriseCapabilityStatus) -> String {
    if !capability.enabled {
        return format!(
            "Enable {} after selecting an open-source or carrier-backed provider such as {}.",
            capability.name, capability.open_source_option
        );
    }
    format!(
        "Finish {} configuration by adding {}, endpoint/admin URL, and credentials where required.",
        capability.name,
        capability
            .blocking_dependency
            .as_deref()
            .unwrap_or("provider details")
    )
}

fn integration_uses_local_runtime(integration: &EnterpriseIntegration) -> bool {
    matches!(
        integration.integration_kind.as_str(),
        "client_media_runtime" | "client_platform" | "desktop_runtime" | "local_or_media_runtime"
    )
}

fn integration_endpoint_has_supported_scheme(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    [
        "http://",
        "https://",
        "tcp://",
        "udp://",
        "grpc://",
        "grpcs://",
        "rtmp://",
        "rtmps://",
        "s3://",
        "webdav://",
        "webdavs://",
    ]
    .iter()
    .any(|scheme| lower.starts_with(scheme))
}

fn enterprise_integration_health(
    integration: EnterpriseIntegration,
) -> EnterpriseIntegrationHealthCheck {
    let checked_at = Utc::now();
    let mut checks = Vec::new();
    let mut blockers = Vec::new();
    if integration.enabled {
        checks.push("enabled".to_string());
    } else {
        blockers.push("integration_disabled".to_string());
    }

    if integration_uses_local_runtime(&integration) {
        checks.push("local_runtime_capability".to_string());
    } else if integration.endpoint_url.is_some()
        || integration.admin_url.is_some()
        || integration.api_key_configured
        || integration.api_key_enc.is_some()
    {
        checks.push("provider_configuration_present".to_string());
    } else {
        blockers.push("provider_configuration_missing".to_string());
    }

    if let Some(endpoint_url) = integration.endpoint_url.as_deref() {
        if integration_endpoint_has_supported_scheme(endpoint_url) {
            checks.push("endpoint_scheme_supported".to_string());
        } else {
            blockers.push("endpoint_scheme_unsupported".to_string());
        }
    }
    if let Some(admin_url) = integration.admin_url.as_deref() {
        let lower = admin_url.trim().to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            checks.push("admin_url_scheme_supported".to_string());
        } else {
            blockers.push("admin_url_scheme_unsupported".to_string());
        }
    }

    let status = if !integration.enabled
        || blockers
            .iter()
            .any(|blocker| blocker.ends_with("_missing") || blocker == "integration_disabled")
    {
        "blocked"
    } else if blockers.is_empty() {
        "healthy"
    } else {
        "warning"
    };

    EnterpriseIntegrationHealthCheck {
        id: integration.id,
        category: integration.category,
        name: integration.name,
        status: status.to_string(),
        checked_at,
        checks,
        blockers,
    }
}

fn enterprise_probe_adapter(integration: &EnterpriseIntegration) -> &'static str {
    match integration.integration_kind.as_str() {
        "ai_service" => "http_ai_provider",
        "storage_service" => "storage_provider",
        "security_service" => "security_scanner",
        "security_policy_service" => "policy_engine",
        "media_gateway" | "broadcast_service" | "sfu_layout_service" => "media_gateway",
        "document_render_service" => "document_renderer",
        "carrier_service" => "carrier_or_sbc",
        "client_media_runtime"
        | "client_platform"
        | "desktop_runtime"
        | "local_or_media_runtime" => "local_runtime",
        _ => "generic_provider",
    }
}

fn endpoint_target(integration: &EnterpriseIntegration) -> Option<String> {
    integration
        .endpoint_url
        .clone()
        .or_else(|| integration.admin_url.clone())
}

fn parse_tcp_endpoint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let without_scheme = trimmed
        .strip_prefix("tcp://")
        .or_else(|| trimmed.strip_prefix("udp://"))
        .unwrap_or(trimmed);
    if without_scheme.contains(':') {
        Some(without_scheme.trim_end_matches('/').to_string())
    } else {
        None
    }
}

async fn probe_http_target(target: &str) -> (String, Option<u128>, Vec<String>, Vec<String>) {
    let started = std::time::Instant::now();
    let mut evidence = Vec::new();
    let mut blockers = Vec::new();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            blockers.push(format!("http_client_error:{err}"));
            return ("blocked".to_string(), None, evidence, blockers);
        }
    };
    match client.get(target).send().await {
        Ok(response) => {
            let latency_ms = started.elapsed().as_millis();
            let status = response.status();
            evidence.push(format!("http_status:{}", status.as_u16()));
            evidence.push(format!("latency_ms:{latency_ms}"));
            if status.is_success()
                || status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                (
                    "reachable".to_string(),
                    Some(latency_ms),
                    evidence,
                    blockers,
                )
            } else if status.is_server_error() {
                blockers.push(format!("provider_server_error:{}", status.as_u16()));
                ("blocked".to_string(), Some(latency_ms), evidence, blockers)
            } else {
                blockers.push(format!("provider_unexpected_status:{}", status.as_u16()));
                ("warning".to_string(), Some(latency_ms), evidence, blockers)
            }
        }
        Err(err) => {
            blockers.push(format!("http_probe_failed:{err}"));
            ("blocked".to_string(), None, evidence, blockers)
        }
    }
}

async fn probe_tcp_target(target: &str) -> (String, Option<u128>, Vec<String>, Vec<String>) {
    let mut evidence = Vec::new();
    let mut blockers = Vec::new();
    let Some(address) = parse_tcp_endpoint(target) else {
        blockers.push("tcp_endpoint_missing_host_port".to_string());
        return ("blocked".to_string(), None, evidence, blockers);
    };
    let started = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&address),
    )
    .await
    {
        Ok(Ok(_stream)) => {
            let latency_ms = started.elapsed().as_millis();
            evidence.push(format!("tcp_connect:{address}"));
            evidence.push(format!("latency_ms:{latency_ms}"));
            (
                "reachable".to_string(),
                Some(latency_ms),
                evidence,
                blockers,
            )
        }
        Ok(Err(err)) => {
            blockers.push(format!("tcp_connect_failed:{err}"));
            ("blocked".to_string(), None, evidence, blockers)
        }
        Err(_) => {
            blockers.push("tcp_probe_timeout".to_string());
            ("blocked".to_string(), None, evidence, blockers)
        }
    }
}

async fn probe_enterprise_integration(
    integration: EnterpriseIntegration,
) -> EnterpriseProviderProbe {
    let checked_at = Utc::now();
    let adapter = enterprise_probe_adapter(&integration).to_string();
    let target = endpoint_target(&integration);
    let mut evidence = Vec::new();
    let mut blockers = Vec::new();

    if !integration.enabled {
        blockers.push("integration_disabled".to_string());
        return EnterpriseProviderProbe {
            id: integration.id,
            category: integration.category,
            name: integration.name,
            adapter,
            target,
            status: "blocked".to_string(),
            latency_ms: None,
            checked_at,
            evidence,
            blockers,
        };
    }

    if integration_uses_local_runtime(&integration) {
        evidence.push("local_runtime_declared".to_string());
        return EnterpriseProviderProbe {
            id: integration.id,
            category: integration.category,
            name: integration.name,
            adapter,
            target,
            status: "warning".to_string(),
            latency_ms: None,
            checked_at,
            evidence,
            blockers: vec!["local_runtime_requires_client_certification".to_string()],
        };
    }

    let Some(target_value) = target.clone() else {
        blockers.push("provider_endpoint_missing".to_string());
        return EnterpriseProviderProbe {
            id: integration.id,
            category: integration.category,
            name: integration.name,
            adapter,
            target,
            status: "blocked".to_string(),
            latency_ms: None,
            checked_at,
            evidence,
            blockers,
        };
    };

    let lower = target_value.trim().to_ascii_lowercase();
    let (status, latency_ms, probe_evidence, probe_blockers) = if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("webdav://")
        || lower.starts_with("webdavs://")
        || lower.starts_with("grpc://")
        || lower.starts_with("grpcs://")
    {
        let http_target = target_value
            .replacen("webdav://", "http://", 1)
            .replacen("webdavs://", "https://", 1)
            .replacen("grpc://", "http://", 1)
            .replacen("grpcs://", "https://", 1);
        probe_http_target(&http_target).await
    } else if lower.starts_with("tcp://") || lower.starts_with("udp://") {
        probe_tcp_target(&target_value).await
    } else if lower.starts_with("rtmp://")
        || lower.starts_with("rtmps://")
        || lower.starts_with("s3://")
    {
        evidence.push("provider_specific_scheme_detected".to_string());
        blockers.push("provider_specific_adapter_required".to_string());
        ("warning".to_string(), None, Vec::new(), Vec::new())
    } else {
        blockers.push("endpoint_scheme_unsupported".to_string());
        ("blocked".to_string(), None, Vec::new(), Vec::new())
    };
    evidence.extend(probe_evidence);
    blockers.extend(probe_blockers);

    EnterpriseProviderProbe {
        id: integration.id,
        category: integration.category,
        name: integration.name,
        adapter,
        target,
        status,
        latency_ms,
        checked_at,
        evidence,
        blockers,
    }
}

fn enterprise_deployment_priority(integration: &EnterpriseIntegration) -> String {
    match integration.id.as_str() {
        "e911"
        | "pstn_sbc_operator_connect"
        | "advanced_threat_protection"
        | "casb"
        | "cloud_storage" => "critical".to_string(),
        "auto_transcription"
        | "copilot"
        | "meeting_assistant"
        | "town_hall_broadcast"
        | "together_gallery"
        | "ndi_rtmp_streaming" => "high".to_string(),
        _ => "standard".to_string(),
    }
}

fn deployment_priority_rank(priority: &str) -> u8 {
    match priority {
        "critical" => 0,
        "high" => 1,
        _ => 2,
    }
}

fn enterprise_deployment_action(
    integration: &EnterpriseIntegration,
    capability: &EnterpriseCapabilityStatus,
) -> String {
    if capability.status == "available" {
        return "Validate end-to-end workflow and monitor provider health.".to_string();
    }
    if !integration.enabled {
        return format!(
            "Install or select {}, then enable this integration.",
            integration.open_source_option
        );
    }
    if integration_uses_local_runtime(integration) {
        return "Complete client/runtime packaging validation and enable tenant rollout."
            .to_string();
    }
    format!(
        "Deploy {}, configure endpoint/admin URL, and add required credentials.",
        integration.open_source_option
    )
}

fn ai_provider_integration_ids(kind: &str) -> Option<Vec<&'static str>> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "llm" => Some(vec!["copilot", "meeting_assistant"]),
        "stt" => Some(vec!["auto_transcription", "speech_ivr"]),
        "tts" => Some(vec!["text_to_speech"]),
        _ => None,
    }
}

fn ai_provider_protocols(kind: &str) -> Vec<String> {
    match kind {
        "llm" => vec![
            "openai_chat_completions".to_string(),
            "ollama_generate".to_string(),
            "vllm_openai_compatible".to_string(),
        ],
        "stt" => vec![
            "whisper_transcriptions".to_string(),
            "vosk_streaming".to_string(),
            "faster_whisper_batch".to_string(),
        ],
        "tts" => vec![
            "piper_http".to_string(),
            "coqui_tts".to_string(),
            "mimic3_http".to_string(),
        ],
        _ => Vec::new(),
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_dialed_number(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '+')
        .collect()
}

fn normalize_media_background_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "blur" => "blur".to_string(),
        "image" | "custom" => "image".to_string(),
        _ => "none".to_string(),
    }
}

fn normalize_speech_utterance(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            part.trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn ivr_option_matches_speech(option: &IvrOption, normalized_utterance: &str) -> bool {
    if normalized_utterance.is_empty() {
        return false;
    }
    let mut phrases = vec![option.label.clone(), option.digit.clone()];
    phrases.extend(option.speech_phrases.clone());
    phrases.into_iter().any(|phrase| {
        let normalized_phrase = normalize_speech_utterance(&phrase);
        !normalized_phrase.is_empty()
            && (normalized_utterance == normalized_phrase
                || normalized_utterance.contains(&normalized_phrase)
                || normalized_phrase.contains(normalized_utterance))
    })
}

fn normalize_conference_layout_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "speaker" | "stage" => "speaker".to_string(),
        "together" => "together".to_string(),
        _ => "gallery".to_string(),
    }
}

fn supported_together_scene(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "auditorium" | "conference" | "classroom"
    )
}

fn apply_layout_readiness(
    layout: &mut ConferenceLayoutState,
    participants: &[ConferenceParticipant],
) {
    let active_participants = participants
        .iter()
        .filter(|participant| !participant.removed)
        .count();
    let large_gallery_capacity = if layout.sfu_layout_configured { 49 } else { 9 };
    layout.gallery_capacity = large_gallery_capacity;
    layout.max_visible = layout.max_visible.clamp(1, large_gallery_capacity);
    layout.together_scene_supported = layout
        .together_scene
        .as_deref()
        .map(supported_together_scene)
        .unwrap_or(true);
    let mut blockers = Vec::new();
    if layout.mode == "together" && !layout.sfu_layout_configured {
        blockers.push("sfu_layout_service_not_configured".to_string());
    }
    if layout.mode == "together" && active_participants < 2 {
        blockers.push("together_mode_requires_two_participants".to_string());
    }
    if !layout.together_scene_supported {
        blockers.push("unsupported_together_scene".to_string());
    }
    if active_participants > large_gallery_capacity {
        blockers.push("participant_count_exceeds_gallery_capacity".to_string());
    }
    layout.layout_blockers = blockers;
}

fn valid_stream_destination(kind: &StreamTargetKind, destination: &str) -> bool {
    match kind {
        StreamTargetKind::Rtmp => {
            destination.starts_with("rtmp://") || destination.starts_with("rtmps://")
        }
        StreamTargetKind::Ndi => {
            let trimmed = destination.trim();
            !trimmed.is_empty() && !trimmed.contains("://")
        }
    }
}

fn apply_town_hall_readiness(config: &mut TownHallConfig) {
    config.broadcast_capacity = if config.broadcast_provider_configured {
        config.max_viewers
    } else {
        config.max_viewers.min(1000)
    };
    config.attendee_mode = if config.broadcast_provider_configured {
        "broadcast".to_string()
    } else {
        "interactive".to_string()
    };
    let mut blockers = Vec::new();
    if config.enabled && !config.broadcast_provider_configured && config.max_viewers > 1000 {
        blockers.push("broadcast_provider_required_for_large_town_hall".to_string());
    }
    if config.enabled
        && config.overflow_url.is_none()
        && config.max_viewers > config.broadcast_capacity
    {
        blockers.push("overflow_url_required_when_capacity_exceeds_local_fanout".to_string());
    }
    config.broadcast_ready = config.enabled
        && config.broadcast_provider_configured
        && config.broadcast_capacity >= config.max_viewers
        && blockers.is_empty();
    config.broadcast_blockers = blockers;
}

fn redact_stream_destination(destination: &str) -> String {
    if let Some((scheme, rest)) = destination.split_once("://") {
        if let Some(at) = rest.find('@') {
            return format!("{scheme}://***@{}", &rest[at + 1..]);
        }
    }
    destination.to_string()
}

fn meeting_summary(segments: &[TranscriptSegment]) -> String {
    if segments.is_empty() {
        return "No finalized transcript is available yet.".to_string();
    }
    segments
        .iter()
        .take(3)
        .map(|segment| segment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(600)
        .collect()
}

fn meeting_key_topics(segments: &[TranscriptSegment]) -> Vec<String> {
    let stop_words: HashSet<&str> = [
        "about", "after", "again", "also", "and", "are", "because", "but", "can", "for", "from",
        "have", "into", "our", "that", "the", "this", "to", "with", "will", "you", "your", "we",
    ]
    .into_iter()
    .collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for word in segments
        .iter()
        .flat_map(|segment| segment.text.split_whitespace())
    {
        let normalized = word
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
            .to_ascii_lowercase();
        if normalized.len() < 4 || stop_words.contains(normalized.as_str()) {
            continue;
        }
        *counts.entry(normalized).or_default() += 1;
    }
    let mut topics: Vec<_> = counts.into_iter().collect();
    topics.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    topics.into_iter().take(8).map(|(topic, _)| topic).collect()
}

fn meeting_action_items(segments: &[TranscriptSegment]) -> Vec<MeetingActionItem> {
    let markers = [
        "action item",
        "follow up",
        "follow-up",
        "todo",
        "to do",
        "need to",
        "needs to",
        "please",
        "assign",
    ];
    segments
        .iter()
        .filter(|segment| text_has_any(&segment.text, &markers))
        .take(12)
        .map(|segment| MeetingActionItem {
            owner: Some(segment.speaker_name.clone()).filter(|name| !name.trim().is_empty()),
            text: segment.text.clone(),
            source_segment_id: segment.id,
        })
        .collect()
}

fn meeting_lines_matching(segments: &[TranscriptSegment], markers: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    for segment in segments {
        if text_has_any(&segment.text, markers) && !lines.contains(&segment.text) {
            lines.push(segment.text.clone());
        }
        if lines.len() >= 12 {
            break;
        }
    }
    lines
}

fn meeting_open_questions(segments: &[TranscriptSegment]) -> Vec<String> {
    let mut questions = Vec::new();
    for segment in segments {
        if segment.text.contains('?') && !questions.contains(&segment.text) {
            questions.push(segment.text.clone());
        }
        if questions.len() >= 12 {
            break;
        }
    }
    questions
}

fn meeting_speaker_stats(segments: &[TranscriptSegment]) -> Vec<MeetingSpeakerStat> {
    let mut stats: HashMap<String, MeetingSpeakerStat> = HashMap::new();
    for segment in segments {
        let stat = stats
            .entry(segment.speaker_uri.clone())
            .or_insert_with(|| MeetingSpeakerStat {
                speaker_uri: segment.speaker_uri.clone(),
                speaker_name: segment.speaker_name.clone(),
                segments: 0,
                words: 0,
            });
        stat.segments += 1;
        stat.words += segment.text.split_whitespace().count();
        if stat.speaker_name.is_empty() && !segment.speaker_name.is_empty() {
            stat.speaker_name = segment.speaker_name.clone();
        }
    }
    let mut stats: Vec<_> = stats.into_values().collect();
    stats.sort_by(|left, right| {
        right
            .words
            .cmp(&left.words)
            .then_with(|| left.speaker_uri.cmp(&right.speaker_uri))
    });
    stats
}

fn text_has_any(text: &str, markers: &[&str]) -> bool {
    let text = text.to_ascii_lowercase();
    markers.iter().any(|marker| text.contains(marker))
}

// ─── Google Calendar Sync ───

/// Refresh a Google OAuth2 access token using the refresh token.
async fn refresh_google_access_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<(String, Option<i64>), String> {
    let client_id =
        std::env::var("PALE_GOOGLE_CLIENT_ID").map_err(|_| "PALE_GOOGLE_CLIENT_ID not set")?;
    let client_secret = std::env::var("PALE_GOOGLE_CLIENT_SECRET")
        .map_err(|_| "PALE_GOOGLE_CLIENT_SECRET not set")?;
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
        ])
        .send()
        .await
        .map_err(|e| format!("token refresh request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("token refresh failed ({status}): {body}"));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("token refresh parse error: {e}"))?;
    let access_token = body["access_token"]
        .as_str()
        .ok_or("missing access_token in refresh response")?
        .to_string();
    let expires_in = body["expires_in"].as_i64();
    Ok((access_token, expires_in))
}

/// Fetch events from Google Calendar API and convert to CalendarEvent.
/// Returns (events, Option<refreshed_access_token>)
async fn sync_google_calendar(
    client: &reqwest::Client,
    integration: &CalendarIntegration,
) -> Result<(Vec<CalendarEvent>, Option<String>), String> {
    let mut access_token = integration.access_token_enc.clone();
    let mut refreshed_token = None;

    // Try to refresh the token if we have a refresh token
    if let Some(ref refresh_token) = integration.refresh_token_enc {
        match refresh_google_access_token(client, refresh_token).await {
            Ok((new_token, _)) => {
                refreshed_token = Some(new_token.clone());
                access_token = new_token;
            }
            Err(e) => {
                log::warn!("Google token refresh failed, trying existing token: {e}");
            }
        }
    }

    let calendar_id = integration.calendar_id.as_deref().unwrap_or("primary");
    let now = Utc::now();
    let time_min = (now - Duration::days(7)).to_rfc3339();
    let time_max = (now + Duration::days(30)).to_rfc3339();
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(calendar_id)
    );
    let resp = client
        .get(&url)
        .bearer_auth(&access_token)
        .query(&[
            ("timeMin", time_min.as_str()),
            ("timeMax", time_max.as_str()),
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
            ("maxResults", "250"),
        ])
        .send()
        .await
        .map_err(|e| format!("Google Calendar API request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Google Calendar API error ({status}): {body}"));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Google Calendar API parse error: {e}"))?;
    let items = body["items"].as_array().cloned().unwrap_or_default();
    let mut events = Vec::new();
    for item in &items {
        let id = match item["id"].as_str() {
            Some(id) => format!("google:{id}"),
            None => continue,
        };
        let title = item["summary"].as_str().unwrap_or("(No title)").to_string();
        // Google returns dateTime for timed events, date for all-day events
        let start_str = item["start"]["dateTime"]
            .as_str()
            .or_else(|| item["start"]["date"].as_str());
        let end_str = item["end"]["dateTime"]
            .as_str()
            .or_else(|| item["end"]["date"].as_str());
        let (Some(start_str), Some(end_str)) = (start_str, end_str) else {
            continue;
        };
        let start = DateTime::parse_from_rfc3339(start_str)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(start_str, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
            });
        let end = DateTime::parse_from_rfc3339(end_str)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(end_str, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
            });
        if let (Ok(start), Ok(end)) = (start, end) {
            events.push(CalendarEvent {
                id,
                title,
                start,
                end,
                source: "google".to_string(),
            });
        }
    }
    Ok((events, refreshed_token))
}

// ─── CalDAV Sync ───

/// Fetch events from a CalDAV server via REPORT request and parse VEVENT blocks.
async fn sync_caldav_calendar(
    client: &reqwest::Client,
    integration: &CalendarIntegration,
) -> Result<Vec<CalendarEvent>, String> {
    let calendar_url = integration
        .calendar_id
        .as_deref()
        .ok_or("CalDAV integration missing calendar URL (calendar_id)")?;

    let now = Utc::now();
    let time_min = (now - Duration::days(7)).format("%Y%m%dT%H%M%SZ");
    let time_max = (now + Duration::days(30)).format("%Y%m%dT%H%M%SZ");

    // Use a calendar-query REPORT to fetch events in the time range
    let report_body = format!(
        r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{time_min}" end="{time_max}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#
    );

    let resp = client
        .request(
            reqwest::Method::from_bytes(b"REPORT").unwrap(),
            calendar_url,
        )
        .header("Content-Type", "application/xml; charset=utf-8")
        .header("Depth", "1")
        .basic_auth(
            &integration.access_token_enc,
            integration.refresh_token_enc.as_deref(),
        )
        .body(report_body)
        .send()
        .await
        .map_err(|e| format!("CalDAV REPORT request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("CalDAV REPORT error ({status}): {body}"));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("CalDAV response read error: {e}"))?;
    Ok(parse_ics_vevents(&body, "caldav"))
}

/// Simple VEVENT parser: extract events from iCalendar data embedded in CalDAV responses.
fn parse_ics_vevents(text: &str, source: &str) -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    // Split on VEVENT blocks — works for both raw ICS and XML-wrapped calendar-data
    for block in text.split("BEGIN:VEVENT") {
        if !block.contains("END:VEVENT") {
            continue;
        }
        let vevent = &block[..block.find("END:VEVENT").unwrap_or(block.len())];
        let uid = ics_prop(vevent, "UID").unwrap_or_default();
        let summary = ics_prop(vevent, "SUMMARY").unwrap_or_else(|| "(No title)".to_string());
        let dtstart = ics_prop(vevent, "DTSTART");
        let dtend = ics_prop(vevent, "DTEND");
        if let (Some(start), Some(end)) = (
            dtstart.and_then(|s| parse_ics_datetime(&s)),
            dtend.and_then(|s| parse_ics_datetime(&s)),
        ) {
            events.push(CalendarEvent {
                id: format!("{source}:{uid}"),
                title: summary,
                start,
                end,
                source: source.to_string(),
            });
        }
    }
    events
}

fn ics_prop(vevent: &str, prop: &str) -> Option<String> {
    for line in vevent.lines() {
        // Handle properties with parameters like DTSTART;TZID=...:value
        let line = line.trim();
        if line.starts_with(prop) {
            if let Some(idx) = line.find(':') {
                return Some(line[idx + 1..].trim().to_string());
            }
        }
    }
    None
}

fn parse_ics_datetime(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    // Try 20060102T150405Z format
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ") {
        return Some(dt.and_utc());
    }
    // Try without trailing Z
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        return Some(dt.and_utc());
    }
    // All-day: 20060102
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

// ─── ClamAV Integration ───

/// Scan file content via ClamAV clamd TCP protocol (INSTREAM command).
/// Returns Ok(true) if malware was found, Ok(false) if clean, Err on communication failure.
async fn clamav_scan(host: &str, body: &[u8]) -> Result<bool, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt as TokioAsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream =
        tokio::time::timeout(std::time::Duration::from_secs(10), TcpStream::connect(host))
            .await
            .map_err(|_| format!("ClamAV connect to {host} timed out"))?
            .map_err(|e| format!("ClamAV connect to {host} failed: {e}"))?;

    // Send zINSTREAM\0
    stream
        .write_all(b"zINSTREAM\0")
        .await
        .map_err(|e| format!("ClamAV write command failed: {e}"))?;

    // Send data in chunks (max 2 MiB per chunk per clamd protocol)
    const CHUNK_SIZE: usize = 2 * 1024 * 1024;
    for chunk in body.chunks(CHUNK_SIZE) {
        let len = (chunk.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .await
            .map_err(|e| format!("ClamAV write chunk len failed: {e}"))?;
        stream
            .write_all(chunk)
            .await
            .map_err(|e| format!("ClamAV write chunk data failed: {e}"))?;
    }

    // Send zero-length terminator
    stream
        .write_all(&0u32.to_be_bytes())
        .await
        .map_err(|e| format!("ClamAV write terminator failed: {e}"))?;

    // Read response (with timeout)
    let mut response = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        stream.read_to_end(&mut response),
    )
    .await
    .map_err(|_| "ClamAV read response timed out".to_string())?
    .map_err(|e| format!("ClamAV read response failed: {e}"))?;

    let response_str = String::from_utf8_lossy(&response);
    let response_str = response_str.trim().trim_end_matches('\0');

    if response_str.contains("FOUND") {
        log::warn!("ClamAV detected malware: {response_str}");
        Ok(true)
    } else if response_str.contains("OK") {
        Ok(false)
    } else {
        Err(format!("ClamAV unexpected response: {response_str}"))
    }
}

fn malware_signature_detected(filename: &str, content_type: &str, body: &[u8]) -> bool {
    // Local EICAR / test pattern check (always runs as a baseline)
    const EICAR: &[u8] = b"X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";
    if body.windows(EICAR.len()).any(|window| window == EICAR) {
        return true;
    }
    let lower_name = filename.to_ascii_lowercase();
    if lower_name.ends_with(".eicar") || lower_name.contains("eicar") {
        return true;
    }
    if is_textual_content(content_type) {
        let text = String::from_utf8_lossy(body).to_ascii_lowercase();
        if text.contains("malware-test-signature") || text.contains("virus-test-signature") {
            return true;
        }
    }
    false
}

/// Async malware scan: tries ClamAV first if configured, falls back to local pattern matching.
async fn malware_scan_async(filename: &str, content_type: &str, body: &[u8]) -> bool {
    // Try ClamAV if configured
    if let Ok(host) = std::env::var("PALE_CLAMAV_HOST") {
        match clamav_scan(&host, body).await {
            Ok(found) => return found,
            Err(e) => {
                log::error!("ClamAV scan failed, falling back to local patterns: {e}");
            }
        }
    }
    // Fallback to local pattern matching
    malware_signature_detected(filename, content_type, body)
}

// ─── AI Service Adapters ───

/// Call an LLM provider (Ollama or OpenAI-compatible) and return the response.
async fn llm_call_provider(
    endpoint_url: &str,
    model: &str,
    temperature: f32,
    max_tokens: usize,
    messages: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let provider = std::env::var("PALE_LLM_PROVIDER").unwrap_or_default();
    let api_key = std::env::var("PALE_LLM_API_KEY").ok();

    let (url, body) = if provider == "ollama" {
        // Ollama native API
        let url = format!("{}/api/chat", endpoint_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": temperature,
                "num_predict": max_tokens,
            }
        });
        (url, body)
    } else {
        // OpenAI-compatible API
        let url = format!("{}/v1/chat/completions", endpoint_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": false,
        });
        (url, body)
    };

    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("LLM request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM provider error ({status}): {body}"));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("LLM response parse error: {e}"))
}

/// Call an STT provider (Whisper-compatible) with audio data.
async fn stt_call_provider(
    endpoint_url: &str,
    audio_data: Vec<u8>,
    language: &str,
    filename: &str,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let provider = std::env::var("PALE_STT_PROVIDER").unwrap_or_default();
    let api_key = std::env::var("PALE_STT_API_KEY").ok();

    let url = if provider == "whisper" {
        // Local Whisper API (e.g., faster-whisper-server)
        format!("{}/asr", endpoint_url.trim_end_matches('/'))
    } else {
        // OpenAI-compatible API
        format!(
            "{}/v1/audio/transcriptions",
            endpoint_url.trim_end_matches('/')
        )
    };

    let file_part = reqwest::multipart::Part::bytes(audio_data)
        .file_name(filename.to_string())
        .mime_str("audio/wav")
        .map_err(|e| format!("multipart mime error: {e}"))?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("language", language.to_string());

    if provider != "whisper" {
        form = form.text("model", "whisper-1");
    }

    let mut req = client.post(&url).multipart(form);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("STT request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("STT provider error ({status}): {body}"));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("STT response parse error: {e}"))
}

/// Call a TTS provider (Piper or OpenAI-compatible) with text, return audio bytes.
async fn tts_call_provider(
    endpoint_url: &str,
    text: &str,
    voice: &str,
    format: &str,
) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let provider = std::env::var("PALE_TTS_PROVIDER").unwrap_or_default();
    let api_key = std::env::var("PALE_TTS_API_KEY").ok();

    let (url, body) = if provider == "piper" {
        // Piper TTS — simple POST text, get audio back
        let url = format!("{}/api/tts", endpoint_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "text": text,
            "speaker": voice,
            "output_format": format,
        });
        (url, body)
    } else {
        // OpenAI-compatible TTS
        let url = format!("{}/v1/audio/speech", endpoint_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "input": text,
            "voice": voice,
            "model": "tts-1",
            "response_format": format,
        });
        (url, body)
    };

    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("TTS request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("TTS provider error ({status}): {body}"));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("TTS response read error: {e}"))
}

// ─── AppState methods for new features ───

impl AppState {
    pub fn list_enterprise_integrations(&self) -> Vec<EnterpriseIntegration> {
        let mut integrations = enterprise_integration_defaults();
        for mut configured in self.enterprise_integrations.values() {
            configured.api_key_configured =
                configured.api_key_configured || configured.api_key_enc.is_some();
            if let Some(default) = integrations
                .iter_mut()
                .find(|item| item.id == configured.id)
            {
                *default = configured;
            } else {
                integrations.push(configured);
            }
        }
        integrations.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.name.cmp(&b.name))
        });
        integrations
    }

    pub fn update_enterprise_integration(
        &self,
        id: &str,
        req: UpdateEnterpriseIntegrationRequest,
        updated_by: &str,
    ) -> Option<EnterpriseIntegration> {
        let mut integration = self
            .enterprise_integrations
            .get(&id.to_string())
            .or_else(|| {
                enterprise_integration_defaults()
                    .into_iter()
                    .find(|integration| integration.id == id)
            })?;
        if let Some(enabled) = req.enabled {
            integration.enabled = enabled;
        }
        if let Some(endpoint_url) = req.endpoint_url {
            integration.endpoint_url = non_empty_string(endpoint_url);
        }
        if let Some(admin_url) = req.admin_url {
            integration.admin_url = non_empty_string(admin_url);
        }
        if req.clear_api_key.unwrap_or(false) {
            integration.api_key_enc = None;
            integration.api_key_configured = false;
        } else if let Some(api_key) = req.api_key.and_then(non_empty_string) {
            integration.api_key_enc = Some(self.encrypt_field(&api_key));
            integration.api_key_configured = true;
        }
        if let Some(notes) = req.notes {
            integration.notes = notes;
        }
        integration.updated_by = Some(updated_by.to_string());
        integration.updated_at = Utc::now();
        self.enterprise_integrations
            .insert(integration.id.clone(), integration.clone());
        self.persist(&integration);
        Some(integration)
    }

    pub fn enterprise_capability_report(&self) -> EnterpriseCapabilityReport {
        let capabilities: Vec<_> = self
            .list_enterprise_integrations()
            .into_iter()
            .map(enterprise_capability_status)
            .collect();
        let total = capabilities.len();
        let available = capabilities
            .iter()
            .filter(|capability| capability.status == "available")
            .count();
        let configured = capabilities
            .iter()
            .filter(|capability| capability.configured)
            .count();
        EnterpriseCapabilityReport {
            total,
            available,
            configured,
            blocked: total.saturating_sub(available),
            capabilities,
        }
    }

    pub fn enterprise_parity_readiness_report(&self) -> EnterpriseParityReadinessReport {
        let capability_report = self.enterprise_capability_report();
        let critical_blockers: Vec<_> = capability_report
            .capabilities
            .iter()
            .filter(|capability| capability.status != "available")
            .map(|capability| EnterpriseParityBlocker {
                id: capability.id.clone(),
                category: capability.category.clone(),
                name: capability.name.clone(),
                status: capability.status.clone(),
                required_dependency: capability
                    .blocking_dependency
                    .clone()
                    .unwrap_or_else(|| "provider configuration".to_string()),
                recommendation: enterprise_parity_recommendation(capability),
            })
            .collect();
        let warnings: Vec<String> = capability_report
            .capabilities
            .iter()
            .filter(|capability| capability.status == "needs_configuration")
            .map(|capability| {
                format!(
                    "{} is enabled but not usable until {} is configured.",
                    capability.name,
                    capability
                        .blocking_dependency
                        .as_deref()
                        .unwrap_or("its provider")
                )
            })
            .collect();
        let total = capability_report.total.max(1);
        let score = ((capability_report.available * 100) / total).min(100) as u8;
        let ready =
            capability_report.available == capability_report.total && critical_blockers.is_empty();
        let mut next_actions = critical_blockers
            .iter()
            .take(8)
            .map(|blocker| blocker.recommendation.clone())
            .collect::<Vec<_>>();
        if next_actions.is_empty() {
            next_actions.push("Run an end-to-end tenant validation across meetings, calling, files, compliance, and admin workflows.".to_string());
        }
        EnterpriseParityReadinessReport {
            ready,
            score,
            available: capability_report.available,
            total: capability_report.total,
            critical_blockers,
            warnings,
            consensus: vec![
                "Configured means enabled plus provider details, not just a checked toggle.".to_string(),
                "External Microsoft 365 foundation equivalents should remain separate open-source systems and be connected through integrations.".to_string(),
                "Teams Enterprise parity is not declared ready while any critical external capability is blocked or only partially configured.".to_string(),
            ],
            next_actions,
        }
    }

    pub fn enterprise_integration_health_report(&self) -> EnterpriseIntegrationHealthReport {
        let checked_at = Utc::now();
        let mut integrations: Vec<_> = self
            .list_enterprise_integrations()
            .into_iter()
            .map(enterprise_integration_health)
            .collect();
        integrations.sort_by(|left, right| {
            left.category
                .cmp(&right.category)
                .then_with(|| left.name.cmp(&right.name))
        });
        let healthy = integrations
            .iter()
            .filter(|integration| integration.status == "healthy")
            .count();
        let warning = integrations
            .iter()
            .filter(|integration| integration.status == "warning")
            .count();
        let blocked = integrations
            .iter()
            .filter(|integration| integration.status == "blocked")
            .count();
        EnterpriseIntegrationHealthReport {
            healthy,
            warning,
            blocked,
            checked_at,
            integrations,
        }
    }

    pub async fn enterprise_provider_probe_report(&self) -> EnterpriseProviderProbeReport {
        let checked_at = Utc::now();
        let handles: Vec<_> = self
            .list_enterprise_integrations()
            .into_iter()
            .map(|integration| tokio::spawn(probe_enterprise_integration(integration)))
            .collect();
        let mut probes = Vec::new();
        for handle in handles {
            if let Ok(probe) = handle.await {
                probes.push(probe);
            }
        }
        probes.sort_by(|left, right| {
            left.category
                .cmp(&right.category)
                .then_with(|| left.name.cmp(&right.name))
        });
        let reachable = probes
            .iter()
            .filter(|probe| probe.status == "reachable")
            .count();
        let warning = probes
            .iter()
            .filter(|probe| probe.status == "warning")
            .count();
        let blocked = probes
            .iter()
            .filter(|probe| probe.status == "blocked")
            .count();
        EnterpriseProviderProbeReport {
            checked_at,
            reachable,
            warning,
            blocked,
            probes,
        }
    }

    pub async fn enterprise_validation_report(&self) -> EnterpriseValidationReport {
        let generated_at = Utc::now();
        let capability_report = self.enterprise_capability_report();
        let security = self.security_posture_report();
        let probes = self.enterprise_provider_probe_report().await;
        let capability_by_id: HashMap<_, _> = capability_report
            .capabilities
            .iter()
            .map(|capability| (capability.id.as_str(), capability))
            .collect();
        let probe_by_id: HashMap<_, _> = probes
            .probes
            .iter()
            .map(|probe| (probe.id.as_str(), probe))
            .collect();

        let mut checks = Vec::new();
        push_validation_check(
            &mut checks,
            "core.calling_pbx",
            "Calling",
            true,
            "Core SIP registrar, PBX routing, queues, IVR, voicemail, and call center APIs are present.",
            vec![
                format!("users:{}", self.users.len()),
                format!("sip_accounts:{}", self.sip_accounts.len()),
                format!("routing_rules:{}", self.routing_rules.len()),
            ],
            Vec::new(),
        );
        push_validation_check(
            &mut checks,
            "core.meetings",
            "Meetings",
            true,
            "Meeting, conference, recording, template, poll, reaction, transcription, and town hall models are present.",
            vec![
                format!("conferences:{}", self.conferences.len()),
                format!("scheduled_meetings:{}", self.scheduled_meetings.len()),
            ],
            Vec::new(),
        );
        push_validation_check(
            &mut checks,
            "core.files",
            "Files",
            true,
            "File APIs, governance metadata, DLP hooks, and storage-provider readiness are present.",
            vec![format!("files:{}", self.files.len())],
            Vec::new(),
        );
        push_validation_check(
            &mut checks,
            "compliance.security_posture",
            "Compliance",
            security.score * 100 >= security.max_score * 60,
            "Security score, recommendations, and compliance controls are available.",
            vec![
                format!("security_score:{}/{}", security.score, security.max_score),
                format!("posture:{}", security.posture),
            ],
            if security.score * 100 >= security.max_score * 60 {
                Vec::new()
            } else {
                vec!["security_posture_needs_attention".to_string()]
            },
        );

        for (id, area, summary) in [
            (
                "advanced_threat_protection",
                "Security",
                "Malware/ATP scanning provider must be configured and reachable.",
            ),
            (
                "casb",
                "Security",
                "CASB or policy-engine provider must be configured and reachable.",
            ),
            (
                "cloud_storage",
                "Files",
                "External storage backend must be configured and reachable.",
            ),
            (
                "e911",
                "Calling",
                "Certified E911 provider must be configured and reachable.",
            ),
            (
                "pstn_sbc_operator_connect",
                "Calling",
                "PSTN/SBC provider must be configured and reachable.",
            ),
            (
                "auto_transcription",
                "AI",
                "STT provider must be configured and reachable.",
            ),
            (
                "text_to_speech",
                "AI",
                "TTS provider must be configured and reachable.",
            ),
            (
                "meeting_assistant",
                "AI",
                "LLM meeting assistant provider must be configured and reachable.",
            ),
            (
                "copilot",
                "AI",
                "LLM workspace assistant provider must be configured and reachable.",
            ),
            (
                "town_hall_broadcast",
                "Scale",
                "Broadcast/SFU provider must be configured and reachable for large town halls.",
            ),
        ] {
            let capability = capability_by_id.get(id).copied();
            let probe = probe_by_id.get(id).copied();
            let mut evidence = Vec::new();
            let mut blockers = Vec::new();
            if let Some(capability) = capability {
                evidence.push(format!("capability_status:{}", capability.status));
                if capability.status != "available" {
                    blockers.push(format!("capability_not_available:{}", capability.status));
                }
            } else {
                blockers.push("capability_missing".to_string());
            }
            if let Some(probe) = probe {
                evidence.push(format!("probe_status:{}", probe.status));
                evidence.extend(probe.evidence.clone());
                if probe.status != "reachable" {
                    blockers.extend(probe.blockers.clone());
                    if probe.status == "warning" {
                        blockers
                            .push("provider_requires_deeper_adapter_or_certification".to_string());
                    }
                }
            } else {
                blockers.push("provider_probe_missing".to_string());
            }
            let passed = blockers.is_empty();
            checks.push(EnterpriseValidationCheck {
                id: id.to_string(),
                area: area.to_string(),
                status: if passed { "pass" } else { "fail" }.to_string(),
                summary: summary.to_string(),
                evidence,
                blockers,
            });
        }

        for (id, area, summary) in [
            (
                "mobile_app",
                "Client Runtime",
                "Android/mobile packaging and device capability path must be validated.",
            ),
            (
                "web_client",
                "Client Runtime",
                "Browser deployment and browser media compatibility must be validated.",
            ),
            (
                "popout_multi_window",
                "Client Runtime",
                "Desktop multi-window lifecycle must be validated.",
            ),
            (
                "virtual_backgrounds",
                "Media",
                "Virtual background runtime must be certified on target clients.",
            ),
            (
                "together_gallery",
                "Media",
                "Gallery/together layout runtime must be certified.",
            ),
            (
                "ndi_rtmp_streaming",
                "Media",
                "Streaming gateway must be configured and reachable.",
            ),
            (
                "powerpoint_live",
                "Media",
                "Presentation renderer must be configured and reachable.",
            ),
        ] {
            let capability = capability_by_id.get(id).copied();
            let status = capability
                .map(|capability| capability.status.as_str())
                .unwrap_or("missing");
            let passed = status == "available";
            checks.push(EnterpriseValidationCheck {
                id: id.to_string(),
                area: area.to_string(),
                status: if passed { "pass" } else { "fail" }.to_string(),
                summary: summary.to_string(),
                evidence: vec![format!("capability_status:{status}")],
                blockers: if passed {
                    Vec::new()
                } else {
                    vec![format!("runtime_not_available:{status}")]
                },
            });
        }

        let passed = checks.iter().filter(|check| check.status == "pass").count();
        let warning = checks
            .iter()
            .filter(|check| check.status == "warning")
            .count();
        let failed = checks.iter().filter(|check| check.status == "fail").count();
        let total = checks.len().max(1);
        let score = ((passed * 100) / total).min(100) as u8;
        let ready = failed == 0 && warning == 0;
        let next_actions = checks
            .iter()
            .filter(|check| check.status != "pass")
            .take(10)
            .map(|check| format!("{}: {}", check.area, check.summary))
            .collect();
        EnterpriseValidationReport {
            generated_at,
            ready,
            score,
            passed,
            warning,
            failed,
            checks,
            consensus: vec![
                "A configured integration must be enabled and backed by provider details.".to_string(),
                "Network-reachable providers are stronger evidence than readiness toggles.".to_string(),
                "Provider-specific schemes such as S3, RTMP, and carrier services still require deeper adapters or certification before enterprise parity is declared.".to_string(),
            ],
            next_actions,
        }
    }

    pub fn enterprise_deployment_plan(&self) -> EnterpriseDeploymentPlan {
        let generated_at = Utc::now();
        let mut items: Vec<_> = self
            .list_enterprise_integrations()
            .into_iter()
            .map(|integration| {
                let capability = enterprise_capability_status(integration.clone());
                let endpoint_required = !integration_uses_local_runtime(&integration);
                EnterpriseDeploymentPlanItem {
                    id: integration.id.clone(),
                    category: integration.category.clone(),
                    name: integration.name.clone(),
                    priority: enterprise_deployment_priority(&integration),
                    status: capability.status.clone(),
                    required_dependency: integration.required_dependency.clone(),
                    open_source_option: integration.open_source_option.clone(),
                    default_provider: integration.default_provider.clone(),
                    endpoint_required,
                    credentials_required: endpoint_required,
                    action: enterprise_deployment_action(&integration, &capability),
                }
            })
            .collect();
        items.sort_by(|left, right| {
            deployment_priority_rank(&left.priority)
                .cmp(&deployment_priority_rank(&right.priority))
                .then_with(|| left.category.cmp(&right.category))
                .then_with(|| left.name.cmp(&right.name))
        });
        let total = items.len();
        let completed = items
            .iter()
            .filter(|item| item.status == "available")
            .count();
        let remaining = total.saturating_sub(completed);
        EnterpriseDeploymentPlan {
            generated_at,
            ready_to_deploy: remaining == 0,
            total,
            completed,
            remaining,
            items,
            summary: vec![
                "Install Microsoft 365 foundation equivalents as separate open-source services, then connect them here.".to_string(),
                "Critical items cover calling, emergency services, storage, malware protection, and CASB enforcement.".to_string(),
                "Do not mark parity complete until every required dependency is available and tenant workflows are validated.".to_string(),
            ],
        }
    }

    pub fn ai_provider_statuses(&self) -> Vec<AiProviderStatus> {
        ["llm", "stt", "tts"]
            .into_iter()
            .filter_map(|kind| self.ai_provider_status(kind))
            .collect()
    }

    pub fn ai_provider_status(&self, kind: &str) -> Option<AiProviderStatus> {
        let kind = kind.trim().to_ascii_lowercase();
        let integration_ids = ai_provider_integration_ids(&kind)?;
        let integrations = self.list_enterprise_integrations();
        let matched: Vec<_> = integration_ids
            .iter()
            .filter_map(|id| {
                integrations
                    .iter()
                    .find(|integration| integration.id == *id)
            })
            .collect();
        let configured = matched.iter().any(|integration| {
            enterprise_capability_status((*integration).clone()).status == "available"
        });
        let primary = matched
            .iter()
            .find(|integration| {
                enterprise_capability_status((**integration).clone()).status == "available"
            })
            .or_else(|| matched.first());
        let mut warnings = Vec::new();
        if !configured {
            warnings.push(format!("{kind}_provider_not_configured"));
        }
        Some(AiProviderStatus {
            kind: kind.clone(),
            configured,
            integration_ids: integration_ids.into_iter().map(str::to_string).collect(),
            endpoint_url: primary.and_then(|integration| integration.endpoint_url.clone()),
            admin_url: primary.and_then(|integration| integration.admin_url.clone()),
            api_key_configured: matched.iter().any(|integration| {
                integration.api_key_configured || integration.api_key_enc.is_some()
            }),
            supported_protocols: ai_provider_protocols(&kind),
            open_source_options: matched
                .into_iter()
                .map(|integration| integration.open_source_option.clone())
                .collect(),
            warnings,
        })
    }

    pub fn update_ai_provider(
        &self,
        kind: &str,
        input: UpdateAiProviderRequest,
        updated_by: &str,
    ) -> Option<AiProviderStatus> {
        let kind = kind.trim().to_ascii_lowercase();
        let ids = ai_provider_integration_ids(&kind)?;
        for id in ids {
            self.update_enterprise_integration(
                id,
                UpdateEnterpriseIntegrationRequest {
                    enabled: input.enabled,
                    endpoint_url: input.endpoint_url.clone(),
                    admin_url: input.admin_url.clone(),
                    api_key: input.api_key.clone(),
                    clear_api_key: input.clear_api_key,
                    notes: input.notes.clone(),
                },
                updated_by,
            )?;
        }
        self.ai_provider_status(&kind)
    }

    pub async fn llm_chat_dispatch(
        &self,
        principal: &str,
        input: LlmChatRequest,
    ) -> Result<AiProviderDispatch, String> {
        if input.messages.is_empty() {
            return Err("messages are required".to_string());
        }
        let status = self
            .ai_provider_status("llm")
            .ok_or_else(|| "unknown provider kind".to_string())?;
        let messages: Vec<_> = input
            .messages
            .into_iter()
            .map(|message| {
                serde_json::json!({
                    "role": message.role,
                    "content": message.content,
                })
            })
            .collect();
        let model = input.model.unwrap_or_else(|| "tenant-default".to_string());
        let temperature = input.temperature.unwrap_or(0.2);
        let max_tokens = input.max_tokens.unwrap_or(1024);

        // Actually call the LLM provider if PALE_LLM_API_URL is set
        let llm_api_url = std::env::var("PALE_LLM_API_URL").ok();

        let (dispatch_status, payload) = if let Some(ref url) = llm_api_url {
            match llm_call_provider(url, &model, temperature, max_tokens, &messages).await {
                Ok(response) => (
                    "completed".to_string(),
                    serde_json::json!({
                        "response": response,
                        "model": model,
                        "requested_by": principal,
                    }),
                ),
                Err(e) => (
                    format!("provider_error: {e}"),
                    serde_json::json!({
                        "model": model,
                        "messages": messages,
                        "requested_by": principal,
                    }),
                ),
            }
        } else {
            (
                if status.configured {
                    "ready_for_external_dispatch".to_string()
                } else {
                    "blocked_provider_not_configured".to_string()
                },
                serde_json::json!({
                    "protocols": status.supported_protocols,
                    "model": model,
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                    "messages": messages,
                    "requested_by": principal,
                }),
            )
        };

        Ok(AiProviderDispatch {
            kind: "llm".to_string(),
            provider_configured: llm_api_url.is_some() || status.configured,
            endpoint_url: llm_api_url.or_else(|| status.endpoint_url.clone()),
            status: dispatch_status,
            payload,
            warnings: status.warnings,
            governance: vec![
                "caller must provide already-authorized context".to_string(),
                "local server does not fabricate LLM output when provider is absent".to_string(),
                "provider credentials remain server-side".to_string(),
            ],
        })
    }

    pub async fn stt_transcription_dispatch(
        &self,
        principal: &str,
        input: SttTranscriptionRequest,
    ) -> Result<AiProviderDispatch, String> {
        if input.recording_id.is_none() && input.file_id.is_none() {
            return Err("recording_id or file_id is required".to_string());
        }
        // Validate references exist
        if let Some(recording_id) = input.recording_id {
            self.recording(recording_id)
                .ok_or_else(|| "recording not found".to_string())?;
        }
        if let Some(file_id) = input.file_id {
            self.files
                .get(&file_id)
                .ok_or_else(|| "file not found".to_string())?;
        }

        let status = self
            .ai_provider_status("stt")
            .ok_or_else(|| "unknown provider kind".to_string())?;
        let language = input.language.clone().unwrap_or_else(|| "en".to_string());

        let stt_api_url = std::env::var("PALE_STT_API_URL").ok();

        let (dispatch_status, payload) = if let Some(ref url) = stt_api_url {
            // Read audio data only when we actually have a provider to call
            let (audio_data, audio_filename) = if let Some(recording_id) = input.recording_id {
                let rec = self.recording(recording_id).unwrap();
                let fid = rec
                    .file_id
                    .ok_or_else(|| "recording has no associated file".to_string())?;
                let path = self.file_path(fid);
                let data = tokio::fs::read(&path)
                    .await
                    .map_err(|e| format!("failed to read recording file: {e}"))?;
                (data, format!("recording_{recording_id}.wav"))
            } else {
                let file_id = input.file_id.unwrap();
                let file_record = self.files.get(&file_id).unwrap();
                let path = self.file_path(file_id);
                let data = tokio::fs::read(&path)
                    .await
                    .map_err(|e| format!("failed to read file: {e}"))?;
                (data, file_record.filename.clone())
            };

            match stt_call_provider(url, audio_data, &language, &audio_filename).await {
                Ok(response) => (
                    "completed".to_string(),
                    serde_json::json!({
                        "transcription": response,
                        "language": language,
                        "requested_by": principal,
                    }),
                ),
                Err(e) => (
                    format!("provider_error: {e}"),
                    serde_json::json!({
                        "recording_id": input.recording_id,
                        "file_id": input.file_id,
                        "language": language,
                        "requested_by": principal,
                    }),
                ),
            }
        } else {
            (
                if status.configured {
                    "ready_for_external_dispatch".to_string()
                } else {
                    "blocked_provider_not_configured".to_string()
                },
                serde_json::json!({
                    "protocols": status.supported_protocols,
                    "recording_id": input.recording_id,
                    "file_id": input.file_id,
                    "language": language,
                    "diarization": input.diarization.unwrap_or(true),
                    "requested_by": principal,
                }),
            )
        };

        Ok(AiProviderDispatch {
            kind: "stt".to_string(),
            provider_configured: stt_api_url.is_some() || status.configured,
            endpoint_url: stt_api_url.or_else(|| status.endpoint_url.clone()),
            status: dispatch_status,
            payload,
            warnings: status.warnings,
            governance: vec![
                "transcript text must be posted back through transcription completion APIs"
                    .to_string(),
                "recording and file references are validated before dispatch".to_string(),
                "local server does not fabricate transcript segments".to_string(),
            ],
        })
    }

    pub async fn tts_synthesis_dispatch(
        &self,
        principal: &str,
        input: TtsSynthesisRequest,
    ) -> Result<AiProviderDispatch, String> {
        let text = input.text.trim().to_string();
        if text.is_empty() {
            return Err("text is required".to_string());
        }
        let status = self
            .ai_provider_status("tts")
            .ok_or_else(|| "unknown provider kind".to_string())?;
        let voice = input.voice.unwrap_or_else(|| "tenant-default".to_string());
        let format = input.format.unwrap_or_else(|| "wav".to_string());

        let tts_api_url = std::env::var("PALE_TTS_API_URL").ok();

        let (dispatch_status, payload) = if let Some(ref url) = tts_api_url {
            match tts_call_provider(url, &text, &voice, &format).await {
                Ok(audio_bytes) => {
                    use base64::Engine;
                    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);
                    (
                        "completed".to_string(),
                        serde_json::json!({
                            "audio_bytes_len": audio_bytes.len(),
                            "audio_base64": audio_b64,
                            "format": format,
                            "voice": voice,
                            "requested_by": principal,
                        }),
                    )
                }
                Err(e) => (
                    format!("provider_error: {e}"),
                    serde_json::json!({
                        "text": text,
                        "voice": voice,
                        "format": format,
                        "requested_by": principal,
                    }),
                ),
            }
        } else {
            (
                if status.configured {
                    "ready_for_external_dispatch".to_string()
                } else {
                    "blocked_provider_not_configured".to_string()
                },
                serde_json::json!({
                    "protocols": status.supported_protocols,
                    "text": text,
                    "voice": voice,
                    "language": input.language.unwrap_or_else(|| "en".to_string()),
                    "format": format,
                    "requested_by": principal,
                }),
            )
        };

        Ok(AiProviderDispatch {
            kind: "tts".to_string(),
            provider_configured: tts_api_url.is_some() || status.configured,
            endpoint_url: tts_api_url.or_else(|| status.endpoint_url.clone()),
            status: dispatch_status,
            payload,
            warnings: status.warnings,
            governance: vec![
                "audio bytes must come from the configured TTS provider".to_string(),
                "local server does not synthesize placeholder audio".to_string(),
                "provider credentials remain server-side".to_string(),
            ],
        })
    }

    // Channel Tabs
    pub fn list_channel_tabs(&self, room_id: Uuid) -> Vec<ChannelTab> {
        let mut tabs: Vec<ChannelTab> = self
            .channel_tabs
            .values()
            .into_iter()
            .filter(|t| t.room_id == room_id)
            .collect();
        tabs.sort_by_key(|t| t.position);
        tabs
    }

    pub fn create_channel_tab(
        &self,
        room_id: Uuid,
        req: CreateChannelTabRequest,
        created_by: &str,
    ) -> ChannelTab {
        let tab = ChannelTab {
            id: Uuid::new_v4(),
            room_id,
            name: req.name,
            url: req.url,
            icon: req.icon,
            created_by: created_by.to_string(),
            created_at: Utc::now(),
            position: req.position.unwrap_or(0),
        };
        self.channel_tabs.insert(tab.id, tab.clone());
        tab
    }

    pub fn update_channel_tab(&self, id: Uuid, req: UpdateChannelTabRequest) -> Option<ChannelTab> {
        let mut tab = self.channel_tabs.get(&id)?;
        if let Some(name) = req.name {
            tab.name = name;
        }
        if let Some(url) = req.url {
            tab.url = url;
        }
        if let Some(icon) = req.icon {
            tab.icon = Some(icon);
        }
        if let Some(position) = req.position {
            tab.position = position;
        }
        self.channel_tabs.insert(id, tab.clone());
        Some(tab)
    }

    pub fn delete_channel_tab(&self, id: Uuid) -> bool {
        self.channel_tabs.remove(&id).is_some()
    }

    // Message Extensions
    pub fn list_message_extensions(&self) -> Vec<MessageExtension> {
        let mut exts = self.message_extensions.values();
        exts.sort_by(|a, b| a.name.cmp(&b.name));
        exts
    }

    pub fn create_message_extension(&self, req: CreateMessageExtensionRequest) -> MessageExtension {
        let ext = MessageExtension {
            id: Uuid::new_v4(),
            name: req.name,
            command: req.command,
            description: req.description.unwrap_or_default(),
            handler_url: req.handler_url,
            icon: req.icon,
            enabled: true,
            created_at: Utc::now(),
        };
        self.message_extensions.insert(ext.id, ext.clone());
        ext
    }

    pub fn update_message_extension(
        &self,
        id: Uuid,
        req: UpdateMessageExtensionRequest,
    ) -> Option<MessageExtension> {
        let mut ext = self.message_extensions.get(&id)?;
        if let Some(name) = req.name {
            ext.name = name;
        }
        if let Some(command) = req.command {
            ext.command = command;
        }
        if let Some(description) = req.description {
            ext.description = description;
        }
        if let Some(handler_url) = req.handler_url {
            ext.handler_url = handler_url;
        }
        if let Some(icon) = req.icon {
            ext.icon = Some(icon);
        }
        if let Some(enabled) = req.enabled {
            ext.enabled = enabled;
        }
        self.message_extensions.insert(id, ext.clone());
        Some(ext)
    }

    pub fn delete_message_extension(&self, id: Uuid) -> bool {
        self.message_extensions.remove(&id).is_some()
    }

    pub fn get_message_extension_by_command(&self, command: &str) -> Option<MessageExtension> {
        self.message_extensions
            .values()
            .into_iter()
            .find(|e| e.command == command && e.enabled)
    }

    // App Catalog
    pub fn list_app_catalog(&self) -> Vec<AppCatalogEntry> {
        let mut apps = self.app_catalog.values();
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        apps
    }

    pub fn create_app_catalog_entry(&self, req: CreateAppCatalogEntryRequest) -> AppCatalogEntry {
        let entry = AppCatalogEntry {
            id: Uuid::new_v4(),
            name: req.name,
            description: req.description.unwrap_or_default(),
            category: req.category.unwrap_or_else(|| "other".to_string()),
            icon_url: req.icon_url,
            manifest_url: req.manifest_url,
            installed: false,
            installed_by: None,
            installed_at: None,
            version: req.version,
            created_at: Utc::now(),
        };
        self.app_catalog.insert(entry.id, entry.clone());
        entry
    }

    pub fn update_app_catalog_entry(
        &self,
        id: Uuid,
        req: UpdateAppCatalogEntryRequest,
    ) -> Option<AppCatalogEntry> {
        let mut entry = self.app_catalog.get(&id)?;
        if let Some(name) = req.name {
            entry.name = name;
        }
        if let Some(description) = req.description {
            entry.description = description;
        }
        if let Some(category) = req.category {
            entry.category = category;
        }
        if let Some(icon_url) = req.icon_url {
            entry.icon_url = Some(icon_url);
        }
        if let Some(manifest_url) = req.manifest_url {
            entry.manifest_url = Some(manifest_url);
        }
        if let Some(version) = req.version {
            entry.version = Some(version);
        }
        self.app_catalog.insert(id, entry.clone());
        Some(entry)
    }

    pub fn install_app(&self, id: Uuid, installed_by: &str) -> Option<AppCatalogEntry> {
        let mut entry = self.app_catalog.get(&id)?;
        entry.installed = true;
        entry.installed_by = Some(installed_by.to_string());
        entry.installed_at = Some(Utc::now());
        self.app_catalog.insert(id, entry.clone());
        Some(entry)
    }

    pub fn uninstall_app(&self, id: Uuid) -> Option<AppCatalogEntry> {
        let mut entry = self.app_catalog.get(&id)?;
        entry.installed = false;
        entry.installed_by = None;
        entry.installed_at = None;
        self.app_catalog.insert(id, entry.clone());
        Some(entry)
    }

    pub fn delete_app_catalog_entry(&self, id: Uuid) -> bool {
        self.app_catalog.remove(&id).is_some()
    }

    // Guest Users
    pub fn list_guest_users(&self, team_id: Uuid) -> Vec<GuestUser> {
        let mut guests: Vec<GuestUser> = self
            .guest_users
            .values()
            .into_iter()
            .filter(|g| g.team_id == team_id)
            .collect();
        guests.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        guests
    }

    pub fn invite_guest(
        &self,
        team_id: Uuid,
        req: InviteGuestRequest,
        invited_by: &str,
    ) -> GuestUser {
        let token = Uuid::new_v4().to_string();
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        let expires_at = Utc::now() + Duration::hours(req.expires_hours.unwrap_or(72));
        let guest = GuestUser {
            id: Uuid::new_v4(),
            email: req.email,
            display_name: req.display_name,
            invited_by: invited_by.to_string(),
            team_id,
            permissions: req.permissions.unwrap_or_default(),
            token_hash,
            expires_at,
            created_at: Utc::now(),
        };
        self.guest_users.insert(guest.id, guest.clone());
        guest
    }

    pub fn delete_guest(&self, id: Uuid) -> bool {
        self.guest_users.remove(&id).is_some()
    }

    // Bandwidth Policies
    pub fn list_bandwidth_policies(&self) -> Vec<BandwidthPolicy> {
        let mut policies = self.bandwidth_policies.values();
        policies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        policies
    }

    pub fn create_bandwidth_policy(&self, req: CreateBandwidthPolicyRequest) -> BandwidthPolicy {
        let policy = BandwidthPolicy {
            id: Uuid::new_v4(),
            name: req.name,
            max_concurrent_calls: req.max_concurrent_calls.unwrap_or(0),
            max_bandwidth_kbps: req.max_bandwidth_kbps.unwrap_or(0),
            location_pattern: req.location_pattern.unwrap_or_else(|| "*".to_string()),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.bandwidth_policies.insert(policy.id, policy.clone());
        policy
    }

    pub fn update_bandwidth_policy(
        &self,
        id: Uuid,
        req: UpdateBandwidthPolicyRequest,
    ) -> Option<BandwidthPolicy> {
        let mut policy = self.bandwidth_policies.get(&id)?;
        if let Some(name) = req.name {
            policy.name = name;
        }
        if let Some(max_concurrent_calls) = req.max_concurrent_calls {
            policy.max_concurrent_calls = max_concurrent_calls;
        }
        if let Some(max_bandwidth_kbps) = req.max_bandwidth_kbps {
            policy.max_bandwidth_kbps = max_bandwidth_kbps;
        }
        if let Some(location_pattern) = req.location_pattern {
            policy.location_pattern = location_pattern;
        }
        if let Some(enabled) = req.enabled {
            policy.enabled = enabled;
        }
        self.bandwidth_policies.insert(id, policy.clone());
        Some(policy)
    }

    pub fn delete_bandwidth_policy(&self, id: Uuid) -> bool {
        self.bandwidth_policies.remove(&id).is_some()
    }

    /// Check if a new call can be admitted based on bandwidth policies.
    pub fn check_call_admission(&self, location: &str) -> bool {
        let active_call_count = self.calls.len();
        for policy in self.bandwidth_policies.values() {
            if !policy.enabled {
                continue;
            }
            if policy.location_pattern == "*" || location.contains(&policy.location_pattern) {
                if policy.max_concurrent_calls > 0
                    && active_call_count >= policy.max_concurrent_calls as usize
                {
                    return false;
                }
            }
        }
        true
    }

    // Signage Displays
    pub fn list_signage_displays(&self) -> Vec<SignageDisplay> {
        let mut displays = self.signage_displays.values();
        displays.sort_by(|a, b| a.name.cmp(&b.name));
        displays
    }

    pub fn create_signage_display(&self, req: CreateSignageDisplayRequest) -> SignageDisplay {
        let display = SignageDisplay {
            id: Uuid::new_v4(),
            name: req.name,
            location: req.location.unwrap_or_default(),
            content_url: req.content_url.unwrap_or_default(),
            schedule: req.schedule.unwrap_or(serde_json::json!({})),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.signage_displays.insert(display.id, display.clone());
        display
    }

    pub fn update_signage_display(
        &self,
        id: Uuid,
        req: UpdateSignageDisplayRequest,
    ) -> Option<SignageDisplay> {
        let mut display = self.signage_displays.get(&id)?;
        if let Some(name) = req.name {
            display.name = name;
        }
        if let Some(location) = req.location {
            display.location = location;
        }
        if let Some(content_url) = req.content_url {
            display.content_url = content_url;
        }
        if let Some(schedule) = req.schedule {
            display.schedule = schedule;
        }
        if let Some(enabled) = req.enabled {
            display.enabled = enabled;
        }
        self.signage_displays.insert(id, display.clone());
        Some(display)
    }

    pub fn delete_signage_display(&self, id: Uuid) -> bool {
        self.signage_displays.remove(&id).is_some()
    }

    pub fn get_signage_content(&self, id: Uuid) -> Option<String> {
        let display = self.signage_displays.get(&id)?;
        if !display.enabled {
            return None;
        }
        Some(display.content_url.clone())
    }
}

fn is_textual_content(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("csv")
        || content_type.contains("javascript")
        || content_type.contains("typescript")
        || content_type.contains("x-www-form-urlencoded")
}

fn meeting_to_ics(meeting: &ScheduledMeeting) -> String {
    let status = if meeting.status == MeetingStatus::Cancelled {
        "CANCELLED"
    } else {
        "CONFIRMED"
    };
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//Palephone//Meetings//EN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "METHOD:PUBLISH".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}@palephone", meeting.id),
        format!(
            "DTSTAMP:{}",
            ics_timestamp(meeting.updated_at.unwrap_or(meeting.created_at))
        ),
        format!("DTSTART:{}", ics_timestamp(meeting.starts_at)),
        format!("DTEND:{}", ics_timestamp(meeting.ends_at)),
        format!("SUMMARY:{}", ics_escape(&meeting.title)),
        format!("DESCRIPTION:{}", ics_escape(&meeting.description)),
        format!("ORGANIZER:MAILTO:{}", ics_escape(&meeting.organizer_uri)),
        format!("STATUS:{status}"),
    ];
    if let Some(recurrence) = &meeting.recurrence {
        lines.push(format!("RRULE:{}", recurrence_to_rrule(recurrence)));
    }
    for participant in &meeting.participants {
        lines.push(format!("ATTENDEE:MAILTO:{}", ics_escape(participant)));
    }
    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    lines.join("\r\n")
}

fn ics_timestamp(value: DateTime<Utc>) -> String {
    value.format("%Y%m%dT%H%M%SZ").to_string()
}

fn ics_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn recurrence_to_rrule(recurrence: &MeetingRecurrence) -> String {
    let frequency = match recurrence.frequency {
        MeetingRecurrenceFrequency::Daily => "DAILY",
        MeetingRecurrenceFrequency::Weekly => "WEEKLY",
        MeetingRecurrenceFrequency::Monthly => "MONTHLY",
    };
    let mut rule = format!("FREQ={frequency};INTERVAL={}", recurrence.interval.max(1));
    if let Some(until) = recurrence.until {
        rule.push_str(&format!(";UNTIL={}", ics_timestamp(until)));
    }
    rule
}

fn normalize_meeting_recurrence(
    recurrence: Option<MeetingRecurrence>,
    starts_at: DateTime<Utc>,
) -> Result<Option<MeetingRecurrence>, String> {
    let Some(mut recurrence) = recurrence else {
        return Ok(None);
    };
    recurrence.interval = recurrence.interval.max(1);
    if recurrence.until.is_some_and(|until| until <= starts_at) {
        return Err("recurrence end must be after meeting start".to_string());
    }
    Ok(Some(recurrence))
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

    #[serde(default = "default_true")]
    pub allow_private_calls: bool,
    #[serde(default = "default_true")]
    pub allow_external_calls: bool,
    #[serde(default = "default_true")]
    pub allow_call_forwarding: bool,
    #[serde(default = "default_true")]
    pub allow_call_recording: bool,

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
    pub number: String,    // SIP URI or phone number
    pub ring_timeout: i32, // seconds to ring before trying next
    pub label: String,     // "Office", "Mobile", "Home"
}

impl Default for UserCallSettings {
    fn default() -> Self {
        Self {
            user_sip_uri: String::new(),
            allow_private_calls: true,
            allow_external_calls: true,
            allow_call_forwarding: true,
            allow_call_recording: true,
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
    #[serde(default)]
    pub speech_enabled: bool,
    #[serde(default = "default_speech_language")]
    pub speech_language: String,
    #[serde(default)]
    pub speech_provider_configured: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

fn default_speech_language() -> String {
    "en-US".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvrOption {
    pub digit: String,
    pub label: String,
    pub destination: String,
    pub destination_type: String, // user, ring_group, ivr, voicemail, external
    #[serde(default)]
    pub speech_phrases: Vec<String>,
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
    pub speech_enabled: Option<bool>,
    pub speech_language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveIvrSpeechRequest {
    pub utterance: String,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvrSpeechResolution {
    pub ivr_id: Uuid,
    pub utterance: String,
    pub language: Option<String>,
    pub provider_configured: bool,
    pub matched: bool,
    pub reason: String,
    pub option: Option<IvrOption>,
    pub route: Option<ResolvedRoute>,
}

// ─── Call Route Resolution ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRoute {
    pub destination_type: String, // user, ring_group, ivr
    pub destination: String,      // SIP URI or ID
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
    pub role: String, // agent, supervisor, qa, admin
    pub display_name: String,
    pub queues: Vec<Uuid>,
    pub skills: Vec<String>,
    pub max_concurrent: i32,
    pub auto_answer: bool,
    pub state: String, // available, on_call, wrap_up, break, training, meeting, offline
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
    pub mode: String, // listen, whisper, barge
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

fn normalize_emergency_user_uri(value: &str) -> Option<String> {
    normalize_sip_uri(value).or_else(|| normalize_sip_uri(&format!("sip:{}", value.trim())))
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

fn collaboration_matches(values: &[String], term: &str) -> bool {
    let tokens: Vec<&str> = term.split_whitespace().collect();
    values.iter().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains(term)
            || (!tokens.is_empty() && tokens.iter().all(|token| value.contains(token)))
    })
}

fn score_match(value: &str, term: &str) -> i32 {
    let value = value.to_ascii_lowercase();
    if value == term {
        100
    } else if value.starts_with(term) {
        80
    } else if value.contains(term) {
        40
    } else {
        0
    }
}

fn room_visible_to(state: &AppState, room_id: Uuid, principal: &str) -> bool {
    state.room(room_id).is_some_and(|room| {
        room.members
            .iter()
            .any(|member| member.user_sip_uri == principal)
    })
}

fn sip_message_visible_to(message: &SipMessage, principal: &str) -> bool {
    message.from_uri == principal || message.to_uri == principal
}

fn recording_visible_to(recording: &CallRecording, principal: &str) -> bool {
    recording.caller_uri == principal
        || recording.callee_uri == principal
        || recording.recorded_by == principal
}

fn nats_subject_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

async fn publish_nats_message(
    url: &str,
    subject: &str,
    payload: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address = nats_tcp_address(url)?;
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream.write_all(b"CONNECT {\"verbose\":false}\r\n").await?;
    stream
        .write_all(format!("PUB {} {}\r\n", subject, payload.len()).as_bytes())
        .await?;
    stream.write_all(payload).await?;
    stream.write_all(b"\r\n").await?;
    stream.write_all(b"PING\r\n").await?;
    stream.shutdown().await?;
    Ok(())
}

fn nats_tcp_address(url: &str) -> Result<String, String> {
    let address = url.strip_prefix("nats://").unwrap_or(url);
    if address.is_empty() {
        return Err("empty NATS URL".to_string());
    }
    Ok(if address.contains(':') {
        address.to_string()
    } else {
        format!("{}:4222", address)
    })
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
                let _ = state.transition_agent_state(
                    &agent_uri,
                    "available",
                    Some("wrap_up_expired".to_string()),
                );
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
        if condition.negate {
            !matched
        } else {
            matched
        }
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

impl PersistedMapObject for MalwareQuarantineItem {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for EnterpriseIntegration {
    type Key = String;

    fn map_key(&self) -> Self::Key {
        self.id.clone()
    }
}

impl PersistedMapObject for EmergencyLocation {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for EmergencyCallingAssignment {
    type Key = String;

    fn map_key(&self) -> Self::Key {
        self.user_uri.clone()
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

impl PersistedMapObject for Team {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for ScheduledMeeting {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for RetentionPolicy {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for EDiscoveryCase {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for DlpPolicy {
    type Key = Uuid;

    fn map_key(&self) -> Self::Key {
        self.id
    }
}

impl PersistedMapObject for ChannelWebhook {
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

// ─── MFA / TOTP Types ───

#[derive(Debug, Clone, Serialize)]
pub struct MfaSetupResponse {
    pub provisioning_uri: String,
    pub secret_base32: String,
    pub backup_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MfaStatusResponse {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MfaVerifyRequest {
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MfaValidateRequest {
    pub code: String,
}

/// Response from user login when MFA is required — contains a temporary token
/// that must be exchanged via POST /v1/mfa/validate.
#[derive(Debug, Clone, Serialize)]
pub struct MfaPendingResponse {
    pub mfa_required: bool,
    pub mfa_token: String,
}

// ─── Session Management Types ───

#[derive(Debug, Clone, Serialize)]
pub struct UserSessionInfo {
    pub id: String,
    pub device_name: String,
    pub device_type: String,
    pub ip_address: String,
    pub created_at: String,
    pub last_active: String,
    pub current: bool,
}

// ─── Certificate Auth Types ───

/// Extract identity from a client certificate's CN or SAN.
pub fn extract_cert_identity(cn: &str) -> Option<String> {
    // Map CN to SIP URI: "300" -> "sip:300@<domain>" or pass through if already a URI
    if cn.starts_with("sip:") || cn.starts_with("sips:") {
        Some(cn.to_string())
    } else if cn.contains('@') {
        Some(format!("sip:{}", cn))
    } else {
        // Just the user part — caller must resolve domain
        Some(cn.to_string())
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
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let user_id = Uuid::new_v4();
        let join = JoinConferenceRequest {
            user_id,
            sip_uri: "sip:alice@example.com".to_string(),
            role: None,
        };

        state
            .join_conference(conference.id, join.clone(), false)
            .unwrap();
        let updated = state.join_conference(conference.id, join, false).unwrap();

        assert_eq!(updated.participants.len(), 1);
        assert_eq!(updated.participants[0].role, ParticipantRole::Member);
    }

    #[test]
    fn locked_conference_blocks_new_participants_but_allows_reconnect_and_override() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-locked-conference-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Security Review".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let existing_id = Uuid::new_v4();
        let join = JoinConferenceRequest {
            user_id: existing_id,
            sip_uri: "sip:alice@example.com".to_string(),
            role: Some(ParticipantRole::Member),
        };
        state
            .join_conference(conference.id, join.clone(), false)
            .unwrap();

        let locked = state.set_conference_locked(conference.id, true).unwrap();
        assert!(locked.locked);
        assert_eq!(
            state
                .join_conference(conference.id, join, false)
                .unwrap()
                .participants
                .len(),
            1
        );
        let rejected = state.join_conference(
            conference.id,
            JoinConferenceRequest {
                user_id: Uuid::new_v4(),
                sip_uri: "sip:bob@example.com".to_string(),
                role: Some(ParticipantRole::Member),
            },
            false,
        );
        assert!(matches!(rejected, Err(JoinConferenceError::Locked)));

        let admitted = state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:moderator-guest@example.com".to_string(),
                    role: Some(ParticipantRole::Member),
                },
                true,
            )
            .unwrap();
        assert_eq!(admitted.participants.len(), 2);
    }

    #[test]
    fn call_quality_reports_are_diagnosed_summarized_and_persisted() {
        let data_dir = std::env::temp_dir().join(format!("pale-cqd-{}", Uuid::new_v4()));
        let token = "012345678901234567890123".to_string();
        let admin_hash = sha256_hex("admin-password".as_bytes());
        let storage_key = "cqd-storage-key".to_string();

        let state = AppState::persistent(
            data_dir.clone(),
            token.clone(),
            "admin".to_string(),
            admin_hash.clone(),
            storage_key.clone(),
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let good = state.post_call_quality(
            "sip:alice@example.com",
            PostCallQualityRequest {
                call_id: Uuid::new_v4(),
                codec: "opus".to_string(),
                jitter_ms: 8.0,
                packet_loss_pct: 0.1,
                round_trip_ms: 60.0,
                mos_score: 4.4,
                bytes_sent: 1200,
                bytes_received: 1400,
            },
        );
        let poor = state.post_call_quality(
            "sip:bob@example.com",
            PostCallQualityRequest {
                call_id: Uuid::new_v4(),
                codec: "opus".to_string(),
                jitter_ms: 75.0,
                packet_loss_pct: 7.5,
                round_trip_ms: 340.0,
                mos_score: 2.6,
                bytes_sent: 800,
                bytes_received: 600,
            },
        );

        assert_eq!(good.rating, CallQualityRating::Good);
        assert!(good.issues.is_empty());
        assert_eq!(poor.rating, CallQualityRating::Poor);
        assert!(poor.issues.contains(&"high_jitter".to_string()));
        assert!(poor.issues.contains(&"high_packet_loss".to_string()));
        assert!(poor.recommended_action.is_some());
        let summary = state.call_quality_summary();
        assert_eq!(summary.total_reports, 2);
        assert_eq!(summary.poor_quality_calls, 1);
        assert_eq!(summary.warning_quality_calls, 0);
        assert_eq!(summary.worst_mos, 2.6);
        let poor_results = state.search_call_quality(CallQualityQuery {
            rating: Some(CallQualityRating::Poor),
            limit: Some(1),
            ..CallQualityQuery::default()
        });
        assert_eq!(poor_results.len(), 1);
        assert_eq!(poor_results[0].call_id, poor.call_id);
        let user_results = state.search_call_quality(CallQualityQuery {
            user_sip_uri: Some("bob@example.com".to_string()),
            ..CallQualityQuery::default()
        });
        assert_eq!(user_results.len(), 1);
        assert_eq!(user_results[0].rating, CallQualityRating::Poor);
        drop(state);

        let reloaded = AppState::persistent(
            data_dir,
            token,
            "admin".to_string(),
            admin_hash,
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let reports = reloaded.list_call_quality();
        assert_eq!(reports.len(), 2);
        assert!(reports
            .iter()
            .any(|report| report.rating == CallQualityRating::Poor));
    }

    #[test]
    fn conference_participant_moderation_requires_host_or_admin() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-conference-moderation-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Moderated".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let host_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: host_id,
                    sip_uri: "sip:host@example.com".to_string(),
                    role: Some(ParticipantRole::Host),
                },
                false,
            )
            .unwrap();
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: member_id,
                    sip_uri: "sip:member@example.com".to_string(),
                    role: Some(ParticipantRole::Member),
                },
                false,
            )
            .unwrap();

        let initial_attendance = state.conference_attendance(conference.id);
        assert_eq!(initial_attendance.len(), 2);
        assert_eq!(
            initial_attendance
                .iter()
                .filter(|record| record.left_at.is_none())
                .count(),
            2
        );

        assert!(!state.can_moderate_conference(conference.id, "sip:member@example.com", false));
        assert!(state.can_moderate_conference(conference.id, "sip:host@example.com", false));

        let updated = state
            .update_conference_participant(
                conference.id,
                member_id,
                UpdateConferenceParticipantRequest {
                    role: Some(ParticipantRole::Moderator),
                    muted: Some(true),
                    removed: Some(true),
                    removal_reason: Some("policy".to_string()),
                },
                "sip:host@example.com",
            )
            .unwrap();
        let participant = updated
            .participants
            .iter()
            .find(|participant| participant.user_id == member_id)
            .unwrap();
        assert_eq!(participant.role, ParticipantRole::Moderator);
        assert!(participant.muted);
        assert!(participant.removed);
        assert_eq!(
            participant.removed_by.as_deref(),
            Some("sip:host@example.com")
        );
        let removed_attendance = state.conference_attendance(conference.id);
        let removed_record = removed_attendance
            .iter()
            .find(|record| record.user_id == member_id)
            .unwrap();
        assert!(removed_record.left_at.is_some());
        assert_eq!(
            removed_record.leave_reason,
            Some(AttendanceLeaveReason::Removed)
        );
        assert_eq!(
            removed_record.removed_by.as_deref(),
            Some("sip:host@example.com")
        );

        let restored = state
            .update_conference_participant(
                conference.id,
                member_id,
                UpdateConferenceParticipantRequest {
                    role: None,
                    muted: Some(false),
                    removed: Some(false),
                    removal_reason: None,
                },
                "sip:host@example.com",
            )
            .unwrap();
        let participant = restored
            .participants
            .iter()
            .find(|participant| participant.user_id == member_id)
            .unwrap();
        assert!(!participant.removed);
        assert!(!participant.muted);
        assert!(participant.removed_at.is_none());
        let restored_attendance = state.conference_attendance(conference.id);
        let member_records: Vec<_> = restored_attendance
            .iter()
            .filter(|record| record.user_id == member_id)
            .collect();
        assert_eq!(member_records.len(), 2);
        assert!(member_records.iter().any(|record| record.left_at.is_none()));

        state.deactivate_conference(conference.id).unwrap();
        let ended_attendance = state.conference_attendance(conference.id);
        assert!(ended_attendance
            .iter()
            .all(|record| record.left_at.is_some()));
        assert!(ended_attendance.iter().any(|record| {
            record.user_id == member_id && record.leave_reason == Some(AttendanceLeaveReason::Ended)
        }));
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

        assert_eq!(
            state.principal_for_bearer(&session.token),
            Some("admin".to_string())
        );
        assert!(matches!(
            state.authenticate_admin("admin", "wrong", "test"),
            Err(AuthError::Unauthorized)
        ));
        assert!(state
            .audit_events()
            .iter()
            .any(|event| event.action == "admin.login.succeeded"));
    }

    #[test]
    fn audit_events_are_searchable_by_admin_filters() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-audit-search-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        state.record_audit_event(
            "admin",
            "user.created",
            Some("sip:alice@example.com".to_string()),
        );
        state.record_audit_event(
            "sip:bob@example.com",
            "message.deleted",
            Some("room-1".to_string()),
        );
        state.record_audit_event("admin", "audit.exported", Some("records=2".to_string()));

        let admin_user_events = state.search_audit_events(AdminAuditQuery {
            principal: Some("ADMIN".to_string()),
            action: Some("user".to_string()),
            ..AdminAuditQuery::default()
        });
        assert_eq!(admin_user_events.len(), 1);
        assert_eq!(admin_user_events[0].action, "user.created");

        let target_events = state.search_audit_events(AdminAuditQuery {
            target: Some("ROOM".to_string()),
            ..AdminAuditQuery::default()
        });
        assert_eq!(target_events.len(), 1);
        assert_eq!(target_events[0].principal, "sip:bob@example.com");

        let limited = state.search_audit_events(AdminAuditQuery {
            limit: Some(2),
            ..AdminAuditQuery::default()
        });
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].action, "audit.exported");
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

        let user = state
            .create_user(CreateUserRequest {
                display_name: "Alice".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: Some("test123".to_string()),
                role: None,
            })
            .unwrap();
        let login = state
            .authenticate_user("sip:alice@example.com", "test123", "127.0.0.1", "desktop")
            .unwrap();
        assert_eq!(
            state.principal_for_bearer(&login.token),
            Some("sip:alice@example.com".to_string())
        );
        assert_eq!(state.delete_user(user.id).unwrap().display_name, "Alice");
        assert_eq!(state.principal_for_bearer(&login.token), None);
        assert!(state.users().is_empty());
        let inactive = state
            .all_users()
            .into_iter()
            .find(|candidate| candidate.id == user.id)
            .unwrap();
        assert!(!inactive.active);
        assert!(inactive.deactivated_at.is_some());
        assert!(matches!(
            state.authenticate_user("sip:alice@example.com", "test123", "127.0.0.1", "desktop"),
            Err(AuthError::Unauthorized)
        ));
        let active = state.set_user_active(user.id, true, "admin").unwrap();
        assert!(active.active);
        assert!(state
            .authenticate_user("sip:alice@example.com", "test123", "127.0.0.1", "desktop")
            .is_ok());

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
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let second = state.create_room(
            "sip:bob@example.com",
            CreateRoomRequest {
                name: "Alice".to_string(),
                description: None,
                members: vec!["sip:alice@example.com".to_string()],
                is_direct: Some(true),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
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
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );

        assert!(!room.is_direct);
        assert_eq!(room.members.len(), 2);
        assert!(room
            .members
            .iter()
            .any(|member| member.user_sip_uri == "sip:alice@example.com"));
        assert!(room
            .members
            .iter()
            .any(|member| member.user_sip_uri == "sip:bob@example.com"));

        let target = state
            .join_room_call(room.id, "sip:alice@example.com", RoomCallMode::Video)
            .expect("room call target");
        let conference = state.conference_by_uri(&target.call_uri).unwrap();
        assert!(target.call_uri.starts_with("sip:conf-"));
        assert_eq!(conference.mode, ConferenceMode::Video);
        assert!(conference.active);
        assert_eq!(conference.participants[0].sip_uri, "sip:alice@example.com");

        let ended = state.end_room_call(room.id).expect("room call ended");
        assert_eq!(ended.conference_id, target.conference_id);
        assert_eq!(ended.call_uri, target.call_uri);
        let ended_conference = state.conference_by_uri(&target.call_uri).unwrap();
        assert!(!ended_conference.active);
        let updated_room = state.room(room.id).unwrap();
        assert!(updated_room.call_uri.is_none());
        assert!(updated_room.conference_id.is_none());
    }

    #[test]
    fn collaboration_search_discovers_visible_business_containers() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-collaboration-search-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let team = state.create_team(
            "sip:alice@example.com",
            CreateTeamRequest {
                name: "Revenue Ops".to_string(),
                description: Some("Pipeline planning".to_string()),
                members: vec![
                    "sip:bob@example.com".to_string(),
                    "sip:mallory@example.com".to_string(),
                ],
            },
        );
        let channel = state
            .create_team_channel(
                "sip:alice@example.com",
                team.id,
                CreateRoomRequest {
                    name: "Forecast".to_string(),
                    description: Some("Quarterly forecast room".to_string()),
                    members: vec![],
                    is_direct: Some(false),
                    team_id: None,
                    channel_name: Some("forecast".to_string()),
                    channel_type: None,
                    channel_owners: Vec::new(),
                    posting_policy: None,
                },
            )
            .expect("team channel");
        let private_room = state.create_room(
            "sip:carol@example.com",
            CreateRoomRequest {
                name: "Revenue Escalations".to_string(),
                description: Some("Not visible to Alice".to_string()),
                members: vec!["sip:dave@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let meeting = state
            .create_scheduled_meeting(
                "sip:alice@example.com",
                CreateScheduledMeetingRequest {
                    title: "Forecast Review".to_string(),
                    description: Some("Revenue forecast sync".to_string()),
                    room_id: Some(channel.id),
                    participants: vec!["sip:bob@example.com".to_string()],
                    starts_at: Utc::now() + Duration::hours(1),
                    ends_at: Utc::now() + Duration::hours(2),
                    mode: Some(RoomCallMode::Video),
                    recurrence: None,
                },
            )
            .expect("meeting");
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Revenue Standup".to_string(),
            mode: ConferenceMode::Audio,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:alice@example.com".to_string(),
                    role: Some(ParticipantRole::Member),
                },
                false,
            )
            .unwrap();

        let results = state.search_collaboration("sip:alice@example.com", "revenue", 10);
        let kinds: Vec<_> = results.iter().map(|result| result.kind.as_str()).collect();
        assert!(kinds.contains(&"team"));
        assert!(kinds.contains(&"channel"));
        assert!(kinds.contains(&"meeting"));
        assert!(kinds.contains(&"conference"));
        assert!(results.iter().any(|result| result.id == team.id));
        assert!(results.iter().any(|result| result.id == meeting.id));
        assert!(!results.iter().any(|result| result.id == private_room.id));

        let limited = state.search_collaboration("sip:alice@example.com", "revenue", 2);
        assert_eq!(limited.len(), 2);
        assert!(state
            .search_collaboration("sip:alice@example.com", "   ", 10)
            .is_empty());
    }

    #[test]
    fn unified_search_finds_enterprise_content_for_authorized_user() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-unified-search-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state
            .create_user(CreateUserRequest {
                display_name: "Alice Revenue Search".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: None,
                role: Some("user".to_string()),
            })
            .unwrap();

        let team = state.create_team(
            "sip:alice@example.com",
            CreateTeamRequest {
                name: "Revenue Search Team".to_string(),
                description: Some("Revenue planning".to_string()),
                members: vec!["sip:bob@example.com".to_string()],
            },
        );
        let channel = state
            .create_team_channel(
                "sip:alice@example.com",
                team.id,
                CreateRoomRequest {
                    name: "Revenue Search".to_string(),
                    description: Some("Revenue channel".to_string()),
                    members: vec!["sip:bob@example.com".to_string()],
                    is_direct: Some(false),
                    team_id: None,
                    channel_name: Some("revenue-search".to_string()),
                    channel_type: Some("standard".to_string()),
                    channel_owners: Vec::new(),
                    posting_policy: None,
                },
            )
            .unwrap();
        state
            .send_room_message(
                channel.id,
                "sip:alice@example.com",
                "Revenue search kickoff",
                None,
                None,
            )
            .unwrap();
        let sip_message = state.store_sip_message(StoreSipMessage {
            call_id: None,
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            content_type: "text/plain".to_string(),
            body: "Revenue search direct follow-up".to_string(),
        });
        let meeting = state
            .create_scheduled_meeting(
                "sip:alice@example.com",
                CreateScheduledMeetingRequest {
                    title: "Revenue Search Review".to_string(),
                    description: Some("Revenue search meeting".to_string()),
                    room_id: Some(channel.id),
                    participants: vec!["sip:bob@example.com".to_string()],
                    starts_at: Utc::now() + Duration::hours(1),
                    ends_at: Utc::now() + Duration::hours(2),
                    mode: Some(RoomCallMode::Video),
                    recurrence: None,
                },
            )
            .unwrap();
        let file = FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "revenue-search-plan.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            size: 42,
            sha256: "revenue-search-hash".to_string(),
            created_at: Utc::now(),
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        };
        state.put_file_record(file.clone());
        state
            .store_recording(CallRecording {
                id: Uuid::new_v4(),
                call_id: Some("revenue-search-call".to_string()),
                caller_uri: "sip:alice@example.com".to_string(),
                callee_uri: "sip:bob@example.com".to_string(),
                duration_secs: 120,
                file_id: Some(file.id),
                recorded_by: "sip:alice@example.com".to_string(),
                created_at: Utc::now(),
                conference_id: meeting.conference_id,
                transcript_segment_count: 0,
                legal_hold: false,
                deleted_at: None,
                deleted_by: None,
            })
            .unwrap();
        let app = state.create_app_catalog_entry(CreateAppCatalogEntryRequest {
            name: "Revenue Search Bot".to_string(),
            description: Some("Revenue workflow assistant".to_string()),
            category: Some("productivity".to_string()),
            icon_url: None,
            manifest_url: None,
            version: Some("1.0.0".to_string()),
        });
        state.install_app(app.id, "sip:alice@example.com").unwrap();

        let results = state.unified_search("sip:alice@example.com", "revenue search", 20);
        let kinds: Vec<_> = results.iter().map(|result| result.kind.as_str()).collect();
        assert!(kinds.contains(&"message"));
        assert!(kinds.contains(&"channel"));
        assert!(kinds.contains(&"team"));
        assert!(kinds.contains(&"user"));
        assert!(kinds.contains(&"meeting"));
        assert!(kinds.contains(&"file"));
        assert!(kinds.contains(&"recording"));
        assert!(kinds.contains(&"app"));
        assert!(results
            .iter()
            .any(|result| result.id == channel.id.to_string()));
        assert!(results
            .iter()
            .any(|result| result.id == sip_message.id.to_string()));
        assert!(results
            .iter()
            .any(|result| result.id == file.id.to_string()));
        assert!(state
            .unified_search("sip:alice@example.com", "   ", 20)
            .is_empty());
    }

    #[test]
    fn unified_search_does_not_leak_private_content_to_outsiders() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-unified-search-privacy-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Secret Revenue Search".to_string(),
                description: Some("Private room".to_string()),
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        state
            .send_room_message(
                room.id,
                "sip:alice@example.com",
                "Secret revenue search details",
                None,
                None,
            )
            .unwrap();
        let room_message_id = state.room_messages(room.id)[0].id;
        let sip_message = state.store_sip_message(StoreSipMessage {
            call_id: None,
            from_uri: "sip:alice@example.com".to_string(),
            to_uri: "sip:bob@example.com".to_string(),
            content_type: "text/plain".to_string(),
            body: "Secret revenue search direct message".to_string(),
        });
        let file = FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "secret-revenue-search.xlsx".to_string(),
            content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                .to_string(),
            size: 64,
            sha256: "secret-revenue-search-hash".to_string(),
            created_at: Utc::now(),
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        };
        state.put_file_record(file);
        state
            .store_recording(CallRecording {
                id: Uuid::new_v4(),
                call_id: Some("secret-revenue-search-call".to_string()),
                caller_uri: "sip:alice@example.com".to_string(),
                callee_uri: "sip:bob@example.com".to_string(),
                duration_secs: 90,
                file_id: None,
                recorded_by: "sip:alice@example.com".to_string(),
                created_at: Utc::now(),
                conference_id: None,
                transcript_segment_count: 0,
                legal_hold: false,
                deleted_at: None,
                deleted_by: None,
            })
            .unwrap();

        let outsider = state.unified_search("sip:mallory@example.com", "secret revenue search", 20);
        assert!(outsider.is_empty());

        let admin = state.unified_search("admin", "secret revenue search", 20);
        assert!(admin.iter().any(|result| result.kind == "file"));
        assert!(admin.iter().any(|result| result.kind == "recording"));
        assert!(admin
            .iter()
            .any(|result| result.id == sip_message.id.to_string()));
        assert!(!admin
            .iter()
            .any(|result| result.id == room_message_id.to_string()));
        assert!(!admin.iter().any(|result| result.kind == "room"));
    }

    #[test]
    fn copilot_answers_are_grounded_and_do_not_bypass_search_visibility() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-copilot-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state
            .update_enterprise_integration(
                "copilot",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://localhost:11434".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("local ollama".to_string()),
                },
                "admin",
            )
            .unwrap();
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Board Revenue Plan".to_string(),
                description: Some("Private board planning".to_string()),
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        state
            .send_room_message(
                room.id,
                "sip:alice@example.com",
                "Board revenue plan says expand enterprise calling.",
                None,
                None,
            )
            .unwrap();

        let alice = state
            .copilot_query(
                "sip:alice@example.com",
                CreateCopilotQueryRequest {
                    question: "What is in the board revenue plan?".to_string(),
                    context_query: Some("board revenue plan".to_string()),
                    limit: Some(6),
                },
            )
            .unwrap();
        assert!(alice.provider_configured);
        assert!(alice.grounded);
        assert!(alice.answer.contains("Board Revenue Plan"));
        assert!(alice
            .citations
            .iter()
            .any(|citation| citation.result.room_id == Some(room.id)));
        assert!(alice
            .governance
            .iter()
            .any(|entry| entry.contains("visible to the caller")));

        let outsider = state
            .copilot_query(
                "sip:mallory@example.com",
                CreateCopilotQueryRequest {
                    question: "What is in the board revenue plan?".to_string(),
                    context_query: Some("board revenue plan".to_string()),
                    limit: Some(6),
                },
            )
            .unwrap();
        assert!(outsider.provider_configured);
        assert!(!outsider.grounded);
        assert!(outsider.citations.is_empty());
        assert!(!outsider.answer.contains("expand enterprise calling"));
    }

    #[tokio::test]
    async fn ai_provider_apis_configure_llm_stt_and_tts_dispatch_contracts() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-ai-providers-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let initial = state.ai_provider_statuses();
        assert_eq!(initial.len(), 3);
        assert!(initial.iter().all(|status| !status.configured));
        assert!(initial.iter().any(|status| status.kind == "tts"));

        let llm = state
            .update_ai_provider(
                "llm",
                UpdateAiProviderRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://ollama.local:11434".to_string()),
                    admin_url: None,
                    api_key: Some("llm-token".to_string()),
                    clear_api_key: None,
                    notes: Some("Ollama/vLLM compatible endpoint".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(llm.configured);
        assert_eq!(llm.integration_ids, vec!["copilot", "meeting_assistant"]);

        let stt = state
            .update_ai_provider(
                "stt",
                UpdateAiProviderRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://whisper.local/v1/audio/transcriptions".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("Whisper-compatible STT".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(stt.configured);

        let tts = state
            .update_ai_provider(
                "tts",
                UpdateAiProviderRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://piper.local/synthesize".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("Piper TTS".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(tts.configured);
        assert_eq!(tts.integration_ids, vec!["text_to_speech"]);

        let llm_dispatch = state
            .llm_chat_dispatch(
                "sip:alice@example.com",
                LlmChatRequest {
                    model: Some("llama3.1".to_string()),
                    temperature: Some(0.1),
                    max_tokens: Some(256),
                    messages: vec![LlmChatMessage {
                        role: "user".to_string(),
                        content: "Summarize the latest meeting".to_string(),
                    }],
                },
            )
            .await
            .unwrap();
        assert_eq!(llm_dispatch.status, "ready_for_external_dispatch");
        assert_eq!(llm_dispatch.payload["model"], "llama3.1");
        assert!(llm_dispatch
            .governance
            .iter()
            .any(|line| line.contains("does not fabricate LLM output")));

        let file = FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "speech.wav".to_string(),
            content_type: "audio/wav".to_string(),
            size: 128,
            sha256: "speech-sha".to_string(),
            created_at: Utc::now(),
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        };
        state.put_file_record(file.clone());
        let stt_dispatch = state
            .stt_transcription_dispatch(
                "sip:alice@example.com",
                SttTranscriptionRequest {
                    recording_id: None,
                    file_id: Some(file.id),
                    language: Some("en-US".to_string()),
                    diarization: Some(false),
                },
            )
            .await
            .unwrap();
        assert_eq!(stt_dispatch.status, "ready_for_external_dispatch");
        assert_eq!(stt_dispatch.payload["file_id"], file.id.to_string());
        assert_eq!(stt_dispatch.payload["diarization"], false);

        let tts_dispatch = state
            .tts_synthesis_dispatch(
                "sip:alice@example.com",
                TtsSynthesisRequest {
                    text: "Welcome to the conference".to_string(),
                    voice: Some("alloy".to_string()),
                    language: Some("en-US".to_string()),
                    format: Some("mp3".to_string()),
                },
            )
            .await
            .unwrap();
        assert_eq!(tts_dispatch.status, "ready_for_external_dispatch");
        assert_eq!(tts_dispatch.payload["voice"], "alloy");
        assert!(tts_dispatch
            .governance
            .iter()
            .any(|line| line.contains("does not synthesize placeholder audio")));
    }

    #[test]
    fn enterprise_capability_registry_tracks_external_parity_dependencies() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-enterprise-integrations-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let initial = state.enterprise_capability_report();
        assert_eq!(initial.total, 19);
        assert_eq!(initial.available, 0);
        assert_eq!(initial.blocked, 19);
        assert!(initial
            .capabilities
            .iter()
            .any(|capability| capability.id == "e911" && capability.blocking_dependency.is_some()));

        state
            .update_enterprise_integration(
                "e911",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: None,
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: None,
                },
                "admin",
            )
            .unwrap();
        let needs_config = state.enterprise_capability_report();
        let e911 = needs_config
            .capabilities
            .iter()
            .find(|capability| capability.id == "e911")
            .unwrap();
        assert_eq!(e911.status, "needs_configuration");
        assert_eq!(needs_config.available, 0);

        state
            .update_enterprise_integration(
                "advanced_threat_protection",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("tcp://clamav.local:3310".to_string()),
                    admin_url: Some("https://security.local".to_string()),
                    api_key: Some("scanner-token".to_string()),
                    clear_api_key: None,
                    notes: Some("ClamAV daemon".to_string()),
                },
                "admin",
            )
            .unwrap();
        let report = state.enterprise_capability_report();
        let atp = report
            .capabilities
            .iter()
            .find(|capability| capability.id == "advanced_threat_protection")
            .unwrap();
        assert_eq!(atp.status, "available");
        assert_eq!(report.available, 1);
        assert_eq!(report.configured, 1);
        assert!(state
            .list_enterprise_integrations()
            .iter()
            .any(|integration| integration.id == "advanced_threat_protection"
                && integration.api_key_enc.is_some()));
    }

    #[test]
    fn enterprise_parity_readiness_requires_every_external_capability() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-enterprise-readiness-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let initial = state.enterprise_parity_readiness_report();
        assert!(!initial.ready);
        assert_eq!(initial.score, 0);
        assert_eq!(initial.critical_blockers.len(), 19);
        assert!(initial
            .consensus
            .iter()
            .any(|line| line.contains("not just a checked toggle")));

        state
            .update_enterprise_integration(
                "e911",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: None,
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: None,
                },
                "admin",
            )
            .unwrap();
        let partial = state.enterprise_parity_readiness_report();
        assert!(!partial.ready);
        assert_eq!(partial.score, 0);
        assert!(partial
            .warnings
            .iter()
            .any(|warning| warning.contains("E911 is enabled but not usable")));

        for integration in enterprise_integration_defaults() {
            let needs_endpoint = !matches!(
                integration.integration_kind.as_str(),
                "client_media_runtime"
                    | "client_platform"
                    | "desktop_runtime"
                    | "local_or_media_runtime"
            );
            state
                .update_enterprise_integration(
                    &integration.id,
                    UpdateEnterpriseIntegrationRequest {
                        enabled: Some(true),
                        endpoint_url: needs_endpoint
                            .then(|| format!("https://{}.internal", integration.id)),
                        admin_url: None,
                        api_key: None,
                        clear_api_key: None,
                        notes: Some("readiness test".to_string()),
                    },
                    "admin",
                )
                .unwrap();
        }
        let ready = state.enterprise_parity_readiness_report();
        assert!(ready.ready);
        assert_eq!(ready.score, 100);
        assert!(ready.critical_blockers.is_empty());
        assert_eq!(ready.next_actions.len(), 1);
        assert!(ready.next_actions[0].contains("end-to-end tenant validation"));
    }

    #[test]
    fn enterprise_integration_health_flags_invalid_or_missing_provider_configuration() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-enterprise-health-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let initial = state.enterprise_integration_health_report();
        assert_eq!(initial.healthy, 0);
        assert_eq!(initial.blocked, 19);

        state
            .update_enterprise_integration(
                "e911",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("ftp://e911.invalid".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: None,
                },
                "admin",
            )
            .unwrap();
        let invalid = state.enterprise_integration_health_report();
        let e911 = invalid
            .integrations
            .iter()
            .find(|integration| integration.id == "e911")
            .unwrap();
        assert_eq!(e911.status, "warning");
        assert!(e911
            .blockers
            .contains(&"endpoint_scheme_unsupported".to_string()));

        state
            .update_enterprise_integration(
                "virtual_backgrounds",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: None,
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: None,
                },
                "admin",
            )
            .unwrap();
        let local_runtime = state.enterprise_integration_health_report();
        let backgrounds = local_runtime
            .integrations
            .iter()
            .find(|integration| integration.id == "virtual_backgrounds")
            .unwrap();
        assert_eq!(backgrounds.status, "healthy");
        assert!(backgrounds
            .checks
            .contains(&"local_runtime_capability".to_string()));
    }

    #[tokio::test]
    async fn enterprise_provider_probe_checks_real_tcp_reachability() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-enterprise-provider-probe-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        state
            .update_enterprise_integration(
                "advanced_threat_protection",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some(format!("tcp://{address}")),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("test ClamAV endpoint".to_string()),
                },
                "admin",
            )
            .unwrap();

        let report = state.enterprise_provider_probe_report().await;
        let atp = report
            .probes
            .iter()
            .find(|probe| probe.id == "advanced_threat_protection")
            .unwrap();
        assert_eq!(atp.status, "reachable");
        assert_eq!(atp.adapter, "security_scanner");
        assert!(atp.latency_ms.is_some());
        assert!(atp
            .evidence
            .iter()
            .any(|line| line.starts_with("tcp_connect:")));
        accept_task.await.unwrap();
    }

    #[tokio::test]
    async fn enterprise_validation_report_refuses_readiness_without_reachable_providers() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-enterprise-validation-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state
            .update_enterprise_integration(
                "cloud_storage",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("s3://tenant-files".to_string()),
                    admin_url: None,
                    api_key: Some("storage-secret".to_string()),
                    clear_api_key: None,
                    notes: Some("MinIO requires provider-specific probe".to_string()),
                },
                "admin",
            )
            .unwrap();
        state
            .update_enterprise_integration(
                "virtual_backgrounds",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: None,
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("client runtime declared".to_string()),
                },
                "admin",
            )
            .unwrap();

        let report = state.enterprise_validation_report().await;
        assert!(!report.ready);
        assert!(report.failed > 0);
        assert!(report
            .consensus
            .iter()
            .any(|line| line.contains("Network-reachable providers")));
        let storage = report
            .checks
            .iter()
            .find(|check| check.id == "cloud_storage")
            .unwrap();
        assert_eq!(storage.status, "fail");
        assert!(storage
            .blockers
            .contains(&"provider_specific_adapter_required".to_string()));
        assert!(storage
            .blockers
            .contains(&"provider_requires_deeper_adapter_or_certification".to_string()));
        let background = report
            .checks
            .iter()
            .find(|check| check.id == "virtual_backgrounds")
            .unwrap();
        assert_eq!(background.status, "pass");
    }

    #[test]
    fn enterprise_deployment_plan_prioritizes_external_foundation_work() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-enterprise-deployment-plan-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let initial = state.enterprise_deployment_plan();
        assert!(!initial.ready_to_deploy);
        assert_eq!(initial.total, 19);
        assert_eq!(initial.completed, 0);
        assert_eq!(initial.remaining, 19);
        assert_eq!(initial.items[0].priority, "critical");
        assert!(initial
            .summary
            .iter()
            .any(|line| line.contains("separate open-source services")));
        let e911 = initial.items.iter().find(|item| item.id == "e911").unwrap();
        assert!(e911.endpoint_required);
        assert!(e911.credentials_required);
        assert!(e911.action.contains("Install or select"));

        state
            .update_enterprise_integration(
                "advanced_threat_protection",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("tcp://clamav.local:3310".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("ClamAV".to_string()),
                },
                "admin",
            )
            .unwrap();
        let updated = state.enterprise_deployment_plan();
        assert_eq!(updated.completed, 1);
        assert_eq!(updated.remaining, 18);
        let atp = updated
            .items
            .iter()
            .find(|item| item.id == "advanced_threat_protection")
            .unwrap();
        assert_eq!(atp.status, "available");
        assert!(atp.action.contains("Validate end-to-end workflow"));
    }

    #[test]
    fn cloud_storage_status_tracks_external_open_source_backend_readiness() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-cloud-storage-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state.put_file_record(FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "plan.docx".to_string(),
            content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_string(),
            size: 1024,
            sha256: "cloud-storage-plan".to_string(),
            created_at: Utc::now(),
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        });

        let local_only = state.cloud_storage_status();
        assert!(!local_only.provider_configured);
        assert_eq!(local_only.sync_mode, "local_server_storage");
        assert_eq!(local_only.local_file_count, 1);
        assert_eq!(local_only.total_storage_bytes, 1024);
        assert!(local_only
            .open_source_options
            .iter()
            .any(|option| option == "Nextcloud"));
        assert!(!local_only.warnings.is_empty());

        state
            .update_enterprise_integration(
                "cloud_storage",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("https://files.example.com/remote.php/dav".to_string()),
                    admin_url: Some("https://files.example.com/settings/admin".to_string()),
                    api_key: Some("secret".to_string()),
                    clear_api_key: None,
                    notes: Some("Nextcloud WebDAV".to_string()),
                },
                "admin",
            )
            .unwrap();
        let external = state.cloud_storage_status();
        assert!(external.provider_configured);
        assert_eq!(external.sync_mode, "external_provider_ready");
        assert_eq!(
            external.endpoint_url.as_deref(),
            Some("https://files.example.com/remote.php/dav")
        );
        assert!(external.warnings.is_empty());
    }

    #[test]
    fn speech_ivr_requires_provider_and_matches_only_configured_phrases() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-speech-ivr-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let ivr = state
            .create_ivr(CreateIvrRequest {
                name: "Main Speech IVR".to_string(),
                extension: "sip:main@example.com".to_string(),
                greeting_text: Some("Say sales or support.".to_string()),
                greeting_file_id: None,
                timeout_secs: Some(10),
                max_retries: Some(2),
                invalid_destination: Some("sip:operator@example.com".to_string()),
                timeout_destination: Some("sip:operator@example.com".to_string()),
                speech_enabled: Some(true),
                speech_language: Some("en-US".to_string()),
                options: vec![
                    IvrOption {
                        digit: "1".to_string(),
                        label: "Sales".to_string(),
                        destination: "sip:sales@example.com".to_string(),
                        destination_type: "ring_group".to_string(),
                        speech_phrases: vec!["sales".to_string(), "talk to sales".to_string()],
                    },
                    IvrOption {
                        digit: "2".to_string(),
                        label: "Support".to_string(),
                        destination: "sip:support@example.com".to_string(),
                        destination_type: "ring_group".to_string(),
                        speech_phrases: vec!["support".to_string(), "technical help".to_string()],
                    },
                ],
            })
            .unwrap();

        let blocked = state
            .resolve_ivr_speech(
                ivr.id,
                ResolveIvrSpeechRequest {
                    utterance: "support please".to_string(),
                    language: None,
                },
            )
            .unwrap();
        assert!(!blocked.provider_configured);
        assert!(!blocked.matched);
        assert_eq!(blocked.reason, "speech_ivr_provider_not_configured");

        state
            .update_enterprise_integration(
                "speech_ivr",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://localhost:9000/asr".to_string()),
                    admin_url: None,
                    api_key: Some("speech-token".to_string()),
                    clear_api_key: None,
                    notes: Some("Vosk/Rasa speech IVR".to_string()),
                },
                "admin",
            )
            .unwrap();

        let matched = state
            .resolve_ivr_speech(
                ivr.id,
                ResolveIvrSpeechRequest {
                    utterance: "I need technical help".to_string(),
                    language: None,
                },
            )
            .unwrap();
        assert!(matched.provider_configured);
        assert!(matched.matched);
        assert_eq!(
            matched
                .route
                .as_ref()
                .map(|route| route.destination.as_str()),
            Some("sip:support@example.com")
        );

        let unknown = state
            .resolve_ivr_speech(
                ivr.id,
                ResolveIvrSpeechRequest {
                    utterance: "billing".to_string(),
                    language: None,
                },
            )
            .unwrap();
        assert!(!unknown.matched);
        assert_eq!(unknown.reason, "no_configured_phrase_matched");
        assert!(unknown.route.is_none());
    }

    #[test]
    fn emergency_call_plan_fails_closed_until_e911_dependencies_are_ready() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-e911-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );

        let unassigned = state.emergency_call_plan("sip:alice@example.com", "911");
        assert!(unassigned.emergency);
        assert!(!unassigned.allowed);
        assert_eq!(unassigned.reason, "missing_emergency_location_assignment");

        state.create_sip_gateway(CreateSipGatewayRequest {
            name: "Emergency SBC".to_string(),
            host: "sbc.example.com".to_string(),
            port: Some(5061),
            transport: Some("tls".to_string()),
            username: None,
            password: None,
            prefix: Some(String::new()),
            enabled: Some(true),
        });
        let location = state.create_emergency_location(CreateEmergencyLocationRequest {
            name: "HQ".to_string(),
            address_line1: "1 Main St".to_string(),
            address_line2: None,
            city: "New York".to_string(),
            region: "NY".to_string(),
            postal_code: "10001".to_string(),
            country: Some("US".to_string()),
            elin: Some("+12125550100".to_string()),
            callback_number: Some("+12125550101".to_string()),
            provider_location_id: Some("loc-hq".to_string()),
            validated: Some(false),
        });
        let assignment = state
            .assign_emergency_location(
                AssignEmergencyLocationRequest {
                    user_uri: "alice@example.com".to_string(),
                    location_id: location.id,
                    emergency_numbers: Some(vec!["9-1-1".to_string(), "911".to_string()]),
                },
                "admin",
            )
            .unwrap();
        assert_eq!(assignment.user_uri, "sip:alice@example.com");
        assert_eq!(assignment.emergency_numbers, vec!["911".to_string()]);

        let unvalidated = state.emergency_call_plan("sip:alice@example.com", "911");
        assert!(!unvalidated.allowed);
        assert_eq!(unvalidated.reason, "emergency_location_not_validated");

        state
            .update_emergency_location(
                location.id,
                UpdateEmergencyLocationRequest {
                    name: None,
                    address_line1: None,
                    address_line2: None,
                    city: None,
                    region: None,
                    postal_code: None,
                    country: None,
                    elin: None,
                    callback_number: None,
                    provider_location_id: None,
                    validated: Some(true),
                },
            )
            .unwrap();
        let missing_e911 = state.emergency_call_plan("sip:alice@example.com", "911");
        assert!(!missing_e911.allowed);
        assert_eq!(missing_e911.reason, "e911_provider_not_configured");

        state
            .update_enterprise_integration(
                "e911",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("https://e911.example.com".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("Certified E911 provider".to_string()),
                },
                "admin",
            )
            .unwrap();
        let missing_pstn = state.emergency_call_plan("sip:alice@example.com", "911");
        assert!(!missing_pstn.allowed);
        assert_eq!(missing_pstn.reason, "pstn_provider_not_configured");

        state
            .update_enterprise_integration(
                "pstn_sbc_operator_connect",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("sip:sbc.example.com".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("SBC trunk".to_string()),
                },
                "admin",
            )
            .unwrap();
        let routable = state.emergency_call_plan("sip:alice@example.com", "911");
        assert!(routable.allowed);
        assert_eq!(routable.reason, "routable");
        assert!(routable.gateway.is_some());
        assert!(routable.e911_provider_available);
        assert!(routable.pstn_provider_available);

        let non_emergency = state.emergency_call_plan("sip:alice@example.com", "5551212");
        assert!(!non_emergency.emergency);
        assert!(non_emergency.allowed);
        assert_eq!(non_emergency.reason, "not_emergency_number");
    }

    #[test]
    fn pstn_operator_connect_status_requires_provider_routes_and_redacts_gateway_secrets() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-pstn-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let initial = state.pstn_operator_connect_status();
        assert!(!initial.provider_available);
        assert!(!initial.routable);
        assert!(initial
            .blockers
            .contains(&"pstn_provider_not_configured".to_string()));
        assert!(initial
            .blockers
            .contains(&"no_enabled_sip_gateway".to_string()));

        let gateway = state.create_sip_gateway(CreateSipGatewayRequest {
            name: "Carrier SBC".to_string(),
            host: "sbc.carrier.example".to_string(),
            port: Some(5061),
            transport: Some("tls".to_string()),
            username: Some("trunk-user".to_string()),
            password: Some("trunk-secret".to_string()),
            prefix: Some("+".to_string()),
            enabled: Some(true),
        });
        assert_ne!(gateway.password_enc.as_deref(), Some("trunk-secret"));
        assert!(gateway.password_enc.is_some());
        let serialized = serde_json::to_value(&gateway).unwrap();
        assert!(serialized.get("password_enc").is_none());

        let missing_provider = state.pstn_operator_connect_status();
        assert!(!missing_provider.routable);
        assert_eq!(missing_provider.tls_gateway_count, 1);
        assert_eq!(missing_provider.authenticated_gateway_count, 1);
        assert_eq!(missing_provider.e164_prefix_route_count, 1);

        state
            .update_enterprise_integration(
                "pstn_sbc_operator_connect",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("sip:sbc.carrier.example".to_string()),
                    admin_url: Some("https://carrier.example/admin".to_string()),
                    api_key: Some("operator-connect-token".to_string()),
                    clear_api_key: None,
                    notes: Some("Carrier trunk".to_string()),
                },
                "admin",
            )
            .unwrap();
        let ready = state.pstn_operator_connect_status();
        assert!(ready.provider_available);
        assert!(ready.routable);
        assert!(ready.emergency_route_ready);
        assert!(ready.blockers.is_empty());
    }

    #[test]
    fn presentation_sessions_track_powerpoint_live_controls_and_renderer_readiness() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-powerpoint-live-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Roadmap".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:presenter@example.com".to_string(),
                    role: Some(ParticipantRole::Host),
                },
                false,
            )
            .unwrap();

        assert!(state
            .create_presentation_session(
                conference.id,
                "presenter@example.com",
                CreatePresentationSessionRequest {
                    title: "   ".to_string(),
                    source_file_id: None,
                    slides: vec![],
                    attendee_navigation_enabled: false,
                },
            )
            .is_none());

        let session = state
            .create_presentation_session(
                conference.id,
                "presenter@example.com",
                CreatePresentationSessionRequest {
                    title: "   ".to_string(),
                    source_file_id: None,
                    slides: vec![
                        CreatePresentationSlideRequest {
                            title: "Strategy".to_string(),
                            notes: Some("Open with customer asks".to_string()),
                            render_url: None,
                        },
                        CreatePresentationSlideRequest {
                            title: "Execution".to_string(),
                            notes: None,
                            render_url: Some("https://renderer.local/slides/2.png".to_string()),
                        },
                    ],
                    attendee_navigation_enabled: false,
                },
            )
            .unwrap();
        assert_eq!(session.title, "Presentation");
        assert_eq!(session.presenter_uri, "sip:presenter@example.com");
        assert_eq!(session.current_slide, 0);
        assert!(!session.renderer_configured);

        let advanced = state
            .update_presentation_session(
                session.id,
                UpdatePresentationSessionRequest {
                    current_slide: Some(99),
                    attendee_navigation_enabled: Some(true),
                    presenter_uri: None,
                },
            )
            .unwrap();
        assert_eq!(advanced.current_slide, 1);
        assert!(advanced.attendee_navigation_enabled);

        state
            .update_enterprise_integration(
                "powerpoint_live",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://collabora.local".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("Collabora renderer".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(
            state
                .presentation_session(session.id)
                .unwrap()
                .renderer_configured
        );

        let ended = state.end_presentation_session(session.id).unwrap();
        assert!(ended.ended_at.is_some());
        let unchanged = state
            .update_presentation_session(
                session.id,
                UpdatePresentationSessionRequest {
                    current_slide: Some(0),
                    attendee_navigation_enabled: None,
                    presenter_uri: None,
                },
            )
            .unwrap();
        assert_eq!(unchanged.current_slide, 1);
    }

    #[test]
    fn meeting_media_settings_and_layout_report_runtime_readiness() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-media-effects-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Design Review".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let host_id = Uuid::new_v4();
        let guest_id = Uuid::new_v4();
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: host_id,
                    sip_uri: "sip:host@example.com".to_string(),
                    role: Some(ParticipantRole::Host),
                },
                false,
            )
            .unwrap();
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: guest_id,
                    sip_uri: "sip:guest@example.com".to_string(),
                    role: None,
                },
                false,
            )
            .unwrap();

        let media = state.update_meeting_media_settings(
            "host@example.com",
            UpdateMeetingMediaSettingsRequest {
                echo_cancellation: Some(false),
                noise_suppression: Some(true),
                auto_gain: Some(true),
                background_mode: Some("custom".to_string()),
                background_image_url: Some("https://cdn.example/bg.png".to_string()),
            },
        );
        assert_eq!(media.user_uri, "sip:host@example.com");
        assert_eq!(media.background_mode, "image");
        assert_eq!(
            media.background_image_url.as_deref(),
            Some("https://cdn.example/bg.png")
        );
        assert!(!media.noise_suppression_configured);
        assert!(!media.virtual_backgrounds_configured);

        let layout = state
            .update_conference_layout_state(
                conference.id,
                "sip:host@example.com",
                UpdateConferenceLayoutRequest {
                    mode: Some("together".to_string()),
                    max_visible: Some(500),
                    together_scene: Some("auditorium".to_string()),
                    stage_participant_ids: Some(vec![host_id, Uuid::new_v4(), guest_id]),
                },
            )
            .unwrap();
        assert_eq!(layout.mode, "together");
        assert_eq!(layout.max_visible, 9);
        assert_eq!(layout.gallery_capacity, 9);
        assert_eq!(layout.stage_participant_ids, vec![host_id, guest_id]);
        assert!(!layout.sfu_layout_configured);
        assert!(layout
            .layout_blockers
            .contains(&"sfu_layout_service_not_configured".to_string()));

        for id in [
            "noise_suppression",
            "virtual_backgrounds",
            "together_gallery",
        ] {
            state
                .update_enterprise_integration(
                    id,
                    UpdateEnterpriseIntegrationRequest {
                        enabled: Some(true),
                        endpoint_url: Some(format!("local://{id}")),
                        admin_url: None,
                        api_key: None,
                        clear_api_key: None,
                        notes: None,
                    },
                    "admin",
                )
                .unwrap();
        }
        let ready_media = state.meeting_media_settings("sip:host@example.com");
        assert!(ready_media.noise_suppression_configured);
        assert!(ready_media.virtual_backgrounds_configured);
        assert!(
            state
                .conference_layout_state(conference.id)
                .unwrap()
                .sfu_layout_configured
        );
        let ready_layout = state.conference_layout_state(conference.id).unwrap();
        assert_eq!(ready_layout.gallery_capacity, 49);
        assert!(ready_layout.layout_blockers.is_empty());
    }

    #[test]
    fn stream_sessions_require_gateway_validate_targets_and_redact_credentials() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-streaming-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Town hall".to_string(),
            mode: ConferenceMode::Webinar,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });

        let missing_gateway = state
            .start_stream_session(
                conference.id,
                "sip:host@example.com",
                CreateMeetingStreamRequest {
                    target_kind: StreamTargetKind::Rtmp,
                    name: "YouTube".to_string(),
                    destination: "rtmps://user:secret@live.example/app/key".to_string(),
                },
            )
            .unwrap_err();
        assert_eq!(missing_gateway, "streaming_gateway_not_configured");

        state
            .update_enterprise_integration(
                "ndi_rtmp_streaming",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("rtmp://egress.local/live".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("SRS gateway".to_string()),
                },
                "admin",
            )
            .unwrap();

        let invalid = state
            .start_stream_session(
                conference.id,
                "sip:host@example.com",
                CreateMeetingStreamRequest {
                    target_kind: StreamTargetKind::Rtmp,
                    name: "Bad".to_string(),
                    destination: "https://not-rtmp.example".to_string(),
                },
            )
            .unwrap_err();
        assert_eq!(invalid, "invalid_stream_destination");

        let session = state
            .start_stream_session(
                conference.id,
                "sip:host@example.com",
                CreateMeetingStreamRequest {
                    target_kind: StreamTargetKind::Rtmp,
                    name: "".to_string(),
                    destination: "rtmps://user:secret@live.example/app/key".to_string(),
                },
            )
            .unwrap();
        assert_eq!(session.name, "Meeting stream");
        assert_eq!(session.status, StreamStatus::Live);
        assert_eq!(session.destination, "rtmps://***@live.example/app/key");
        assert!(session.gateway_configured);

        let ndi = state
            .start_stream_session(
                conference.id,
                "sip:host@example.com",
                CreateMeetingStreamRequest {
                    target_kind: StreamTargetKind::Ndi,
                    name: "NDI Program".to_string(),
                    destination: "Pale Program Output".to_string(),
                },
            )
            .unwrap();
        assert_eq!(ndi.target_kind, StreamTargetKind::Ndi);

        let stopped = state.stop_stream_session(session.id).unwrap();
        assert_eq!(stopped.status, StreamStatus::Stopped);
        assert!(stopped.stopped_at.is_some());
    }

    #[test]
    fn town_hall_config_enforces_webinar_capacity_and_provider_readiness() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-town-hall-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let meeting = state.create_conference(CreateConferenceRequest {
            title: "Daily standup".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        assert_eq!(
            state
                .update_town_hall_config(
                    meeting.id,
                    "admin",
                    UpdateTownHallConfigRequest {
                        enabled: Some(true),
                        max_viewers: None,
                        registration_required: None,
                        presenter_only_video: None,
                        attendee_mic_disabled: None,
                        qna_moderation_required: None,
                        overflow_url: None,
                    },
                )
                .unwrap_err(),
            "town_hall_requires_webinar"
        );

        let webinar = state.create_conference(CreateConferenceRequest {
            title: "All hands".to_string(),
            mode: ConferenceMode::Webinar,
            registration_enabled: Some(true),
            max_registrations: None,
            registration_fields: None,
        });
        let config = state
            .update_town_hall_config(
                webinar.id,
                "admin",
                UpdateTownHallConfigRequest {
                    enabled: Some(true),
                    max_viewers: Some(1_000_000),
                    registration_required: Some(true),
                    presenter_only_video: Some(true),
                    attendee_mic_disabled: Some(true),
                    qna_moderation_required: Some(true),
                    overflow_url: Some("https://cdn.example/overflow".to_string()),
                },
            )
            .unwrap();
        assert_eq!(config.max_viewers, 100000);
        assert!(!config.broadcast_provider_configured);
        assert_eq!(config.broadcast_capacity, 1000);
        assert_eq!(config.attendee_mode, "interactive");
        assert!(!config.broadcast_ready);
        assert!(config
            .broadcast_blockers
            .contains(&"broadcast_provider_required_for_large_town_hall".to_string()));

        state
            .update_town_hall_config(
                webinar.id,
                "admin",
                UpdateTownHallConfigRequest {
                    enabled: Some(true),
                    max_viewers: Some(1),
                    registration_required: None,
                    presenter_only_video: None,
                    attendee_mic_disabled: None,
                    qna_moderation_required: None,
                    overflow_url: None,
                },
            )
            .unwrap();
        state
            .join_conference(
                webinar.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:first@example.com".to_string(),
                    role: None,
                },
                false,
            )
            .unwrap();
        let rejected = state
            .join_conference(
                webinar.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:second@example.com".to_string(),
                    role: None,
                },
                false,
            )
            .unwrap_err();
        assert_eq!(rejected, JoinConferenceError::CapacityReached);

        state
            .join_conference(
                webinar.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:presenter@example.com".to_string(),
                    role: Some(ParticipantRole::Host),
                },
                true,
            )
            .unwrap();

        state
            .update_enterprise_integration(
                "town_hall_broadcast",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("https://broadcast.example".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("CDN fanout".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(
            state
                .town_hall_config(webinar.id)
                .unwrap()
                .broadcast_provider_configured
        );
        let broadcast_ready = state.town_hall_config(webinar.id).unwrap();
        assert!(broadcast_ready.broadcast_ready);
        assert_eq!(broadcast_ready.attendee_mode, "broadcast");
        assert_eq!(
            broadcast_ready.broadcast_capacity,
            broadcast_ready.max_viewers
        );
        assert!(broadcast_ready.broadcast_blockers.is_empty());
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
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let target = state
            .start_room_call(room.id, RoomCallMode::Audio)
            .expect("room call target");
        let message = state
            .send_room_message(room.id, "sip:alice@example.com", "hello", None, None)
            .unwrap();
        let edited = state
            .edit_room_message(message.id, "sip:alice@example.com", "hello team")
            .unwrap();
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
    fn room_message_reads_are_idempotent_and_persistent() {
        let data_dir = std::env::temp_dir().join(format!("pale-read-receipts-{}", Uuid::new_v4()));
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
                name: "Receipts".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let message = state
            .send_room_message(room.id, "sip:alice@example.com", "please read", None, None)
            .unwrap();
        let first = state
            .mark_room_message_read(message.id, "sip:bob@example.com")
            .expect("read receipt");
        let second = state
            .mark_room_message_read(message.id, "sip:bob@example.com")
            .expect("read receipt update");

        assert_eq!(first.message_id, message.id);
        assert_eq!(second.reader_uri, "sip:bob@example.com");
        assert_eq!(state.message_reads(message.id).len(), 1);
        assert!(state
            .mark_room_message_read(Uuid::new_v4(), "sip:bob@example.com")
            .is_none());
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
        let reads = reloaded.message_reads(message.id);
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].reader_uri, "sip:bob@example.com");

        reloaded.delete_room_message(message.id).unwrap();
        assert!(reloaded.message_reads(message.id).is_empty());
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
        assert!(reloaded_after_delete.message_reads(message.id).is_empty());
        drop(reloaded_after_delete);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn room_message_reactions_toggle_and_persist() {
        let data_dir =
            std::env::temp_dir().join(format!("pale-message-reactions-{}", Uuid::new_v4()));
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
                name: "Reactions".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let message = state
            .send_room_message(room.id, "sip:alice@example.com", "react here", None, None)
            .unwrap();
        let added = state
            .toggle_message_reaction(message.id, "sip:bob@example.com", "👍")
            .expect("reaction added");
        assert!(added.added);
        assert_eq!(state.message_reactions(message.id).len(), 1);
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
        let reactions = reloaded.message_reactions(message.id);
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].emoji, "👍");
        assert_eq!(reactions[0].user_uri, "sip:bob@example.com");
        let removed = reloaded
            .toggle_message_reaction(message.id, "sip:bob@example.com", "👍")
            .expect("reaction removed");
        assert!(!removed.added);
        assert!(reloaded.message_reactions(message.id).is_empty());
        drop(reloaded);

        let reloaded_after_remove = AppState::persistent(
            data_dir.clone(),
            "012345678901234567890123".to_string(),
            "admin".to_string(),
            sha256_hex("admin-password".as_bytes()),
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        assert!(reloaded_after_remove
            .message_reactions(message.id)
            .is_empty());
        drop(reloaded_after_remove);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn room_messages_support_priority_and_saved_state() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-message-priority-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Priority".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let msg = state
            .send_room_message(
                room.id,
                "sip:alice@example.com",
                "Please review now",
                None,
                Some("urgent".to_string()),
            )
            .unwrap();
        assert_eq!(msg.priority, "urgent");

        let saved = state
            .set_message_saved(msg.id, "sip:bob@example.com", true)
            .unwrap();
        assert_eq!(saved.saved_by, vec!["sip:bob@example.com".to_string()]);
        let unsaved = state
            .set_message_saved(msg.id, "sip:bob@example.com", false)
            .unwrap();
        assert!(unsaved.saved_by.is_empty());
    }

    #[test]
    fn channel_webhooks_post_with_token_and_respect_lifecycle() {
        let data_dir =
            std::env::temp_dir().join(format!("pale-channel-webhooks-{}", Uuid::new_v4()));
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
                name: "Deployments".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: Some("deployments".to_string()),
                channel_type: None,
                channel_owners: vec!["sip:alice@example.com".to_string()],
                posting_policy: Some("owners".to_string()),
            },
        );
        assert!(state
            .send_room_message(room.id, "sip:bob@example.com", "blocked", None, None)
            .is_err());
        let created = state
            .create_channel_webhook(
                room.id,
                "sip:alice@example.com",
                CreateChannelWebhookRequest {
                    name: "Build pipeline".to_string(),
                    description: Some("CI deployment notices".to_string()),
                },
            )
            .unwrap();
        assert!(created.token.starts_with("wh_"));
        assert_eq!(state.list_channel_webhooks(room.id).len(), 1);

        let posted = state
            .post_channel_webhook(
                &created.token,
                PostChannelWebhookRequest {
                    title: Some("Deploy".to_string()),
                    text: "Production deploy succeeded".to_string(),
                },
            )
            .unwrap();
        assert_eq!(posted.room_id, room.id);
        assert!(posted.sender_uri.starts_with("sip:webhook-"));
        assert!(posted.body.contains("Production deploy succeeded"));
        assert!(state.list_channel_webhooks(room.id)[0]
            .last_used_at
            .is_some());
        drop(state);

        let reloaded = AppState::persistent(
            data_dir.clone(),
            "012345678901234567890123".to_string(),
            "admin".to_string(),
            sha256_hex("admin-password".as_bytes()),
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        assert!(reloaded.list_channel_webhooks(room.id)[0]
            .last_used_at
            .is_some());
        let disabled = reloaded
            .set_channel_webhook_enabled(room.id, created.webhook.id, false)
            .unwrap();
        assert!(!disabled.enabled);
        assert!(reloaded
            .post_channel_webhook(
                &created.token,
                PostChannelWebhookRequest {
                    title: None,
                    text: "should fail".to_string(),
                },
            )
            .is_err());
        reloaded
            .delete_channel_webhook(room.id, created.webhook.id)
            .unwrap();
        assert!(reloaded.list_channel_webhooks(room.id).is_empty());
        assert!(reloaded
            .post_channel_webhook(
                &created.token,
                PostChannelWebhookRequest {
                    title: None,
                    text: "still fails".to_string(),
                },
            )
            .is_err());
        drop(reloaded);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn teams_channels_meetings_and_governance_are_persistent_business_objects() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-business-collab-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let team = state.create_team(
            "sip:alice@example.com",
            CreateTeamRequest {
                name: "Engineering".to_string(),
                description: Some("Product engineering".to_string()),
                members: vec!["sip:bob@example.com".to_string()],
            },
        );
        assert_eq!(state.list_teams_for_user("sip:bob@example.com").len(), 1);

        let channel = state
            .create_team_channel(
                "sip:alice@example.com",
                team.id,
                CreateRoomRequest {
                    name: "General".to_string(),
                    description: None,
                    members: Vec::new(),
                    is_direct: Some(false),
                    team_id: Some(team.id),
                    channel_name: Some("General".to_string()),
                    channel_type: None,
                    channel_owners: Vec::new(),
                    posting_policy: None,
                },
            )
            .expect("team channel");
        assert_eq!(channel.team_id, Some(team.id));
        assert!(channel
            .members
            .iter()
            .any(|member| member.user_sip_uri == "sip:bob@example.com"));

        let meeting = state
            .create_scheduled_meeting(
                "sip:alice@example.com",
                CreateScheduledMeetingRequest {
                    title: "Planning".to_string(),
                    description: None,
                    room_id: Some(channel.id),
                    participants: vec!["sip:bob@example.com".to_string()],
                    starts_at: Utc::now(),
                    ends_at: Utc::now() + Duration::minutes(30),
                    mode: Some(RoomCallMode::Video),
                    recurrence: None,
                },
            )
            .expect("scheduled meeting");
        assert_eq!(state.list_meetings_for_user("sip:bob@example.com").len(), 1);
        let target = state
            .start_scheduled_meeting(meeting.id, "sip:bob@example.com")
            .expect("meeting call");
        assert_eq!(target.room_id, channel.id);

        let policy = state.upsert_retention_policy(
            "admin",
            UpsertRetentionPolicyRequest {
                id: None,
                name: "Legal hold".to_string(),
                scope: "room".to_string(),
                room_id: Some(channel.id),
                retain_days: None,
                legal_hold: Some(true),
                export_enabled: Some(true),
            },
        );
        assert!(policy.legal_hold);
        assert_eq!(state.retention_policies().len(), 1);
    }

    #[test]
    fn private_moderated_channels_restrict_membership_and_posting() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-channel-governance-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let team = state.create_team(
            "sip:owner@example.com",
            CreateTeamRequest {
                name: "Launch".to_string(),
                description: None,
                members: vec![
                    "sip:member@example.com".to_string(),
                    "sip:outsider@example.com".to_string(),
                ],
            },
        );
        let channel = state
            .create_team_channel(
                "sip:owner@example.com",
                team.id,
                CreateRoomRequest {
                    name: "Leadership".to_string(),
                    description: None,
                    members: vec!["sip:member@example.com".to_string()],
                    is_direct: None,
                    team_id: None,
                    channel_name: Some("Leadership".to_string()),
                    channel_type: Some("private".to_string()),
                    channel_owners: vec!["sip:owner@example.com".to_string()],
                    posting_policy: Some("owners".to_string()),
                },
            )
            .unwrap();

        assert_eq!(channel.channel_type, "private");
        assert_eq!(channel.posting_policy, "owners");
        assert!(channel
            .members
            .iter()
            .any(|member| member.user_sip_uri == "sip:member@example.com"));
        assert!(!channel
            .members
            .iter()
            .any(|member| member.user_sip_uri == "sip:outsider@example.com"));
        assert!(state
            .send_room_message(channel.id, "sip:member@example.com", "hello", None, None)
            .is_err());
        assert!(state
            .send_room_message(
                channel.id,
                "sip:owner@example.com",
                "approved update",
                None,
                None
            )
            .is_ok());
    }

    #[test]
    fn room_messages_resolve_structured_user_and_channel_mentions() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-mention-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state
            .create_user(CreateUserRequest {
                display_name: "Alice Smith".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: Some("alice-password".to_string()),
                role: None,
            })
            .unwrap();
        state
            .create_user(CreateUserRequest {
                display_name: "Bob Jones".to_string(),
                sip_uri: "sip:bob@example.com".to_string(),
                matrix_user_id: None,
                password: Some("bob-password".to_string()),
                role: None,
            })
            .unwrap();
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Project".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );

        let msg = state
            .send_room_message(
                room.id,
                "sip:alice@example.com",
                "Can @Bob Jones check this? @channel",
                None,
                None,
            )
            .unwrap();
        assert!(msg.mentions.iter().any(|mention| mention.kind == "user"
            && mention.user_sip_uri.as_deref() == Some("sip:bob@example.com")));
        assert!(msg.mentions.iter().any(|mention| mention.kind == "channel"));
        assert_eq!(
            msg.mentioned_user_uris,
            vec![
                "sip:alice@example.com".to_string(),
                "sip:bob@example.com".to_string()
            ]
        );

        let edited = state
            .edit_room_message(msg.id, "sip:alice@example.com", "@alice can own this")
            .expect("edited message");
        assert_eq!(
            edited.mentioned_user_uris,
            vec!["sip:alice@example.com".to_string()]
        );
    }

    #[test]
    fn collaboration_policy_controls_broad_mentions() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-mention-policy-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:owner@example.com",
            CreateRoomRequest {
                name: "Ops".to_string(),
                description: None,
                members: vec!["sip:member@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );

        let blocked = state.send_room_message(
            room.id,
            "sip:member@example.com",
            "@channel please review",
            None,
            None,
        );
        assert!(blocked.is_err());

        let updated = state.update_collaboration_policy(
            "admin",
            UpdateCollaborationPolicyRequest {
                structured_mentions_enabled: None,
                broad_mentions_enabled: None,
                broad_mentions_allowed_roles: Some(vec!["admin".to_string(), "member".to_string()]),
                broad_mentions_per_minute: Some(1),
                external_access_enabled: None,
                allowed_external_domains: None,
                urgent_messages_enabled: None,
                meeting_recording_enabled: None,
            },
        );
        assert_eq!(updated.broad_mentions_per_minute, 1);

        assert!(state
            .send_room_message(
                room.id,
                "sip:member@example.com",
                "@channel first",
                None,
                None
            )
            .is_ok());
        let rate_limited = state.send_room_message(
            room.id,
            "sip:member@example.com",
            "@channel second",
            None,
            None,
        );
        assert!(rate_limited.is_err());
    }

    #[test]
    fn collaboration_policy_controls_external_access_and_urgent_messages() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-collaboration-policy-depth-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:owner@example.com",
            CreateRoomRequest {
                name: "Policy".to_string(),
                description: None,
                members: vec!["sip:member@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        state.update_collaboration_policy(
            "admin",
            UpdateCollaborationPolicyRequest {
                structured_mentions_enabled: None,
                broad_mentions_enabled: None,
                broad_mentions_allowed_roles: None,
                broad_mentions_per_minute: None,
                external_access_enabled: Some(true),
                allowed_external_domains: Some(vec!["partner.example".to_string()]),
                urgent_messages_enabled: Some(false),
                meeting_recording_enabled: Some(false),
            },
        );

        assert!(state
            .authorize_external_participants(
                "sip:owner@example.com",
                &["sip:guest@blocked.example".to_string()]
            )
            .is_err());
        assert!(state
            .authorize_external_participants(
                "sip:owner@example.com",
                &["sip:guest@partner.example".to_string()]
            )
            .is_ok());
        assert!(state
            .store_recording(CallRecording {
                id: Uuid::new_v4(),
                call_id: Some("policy-call".to_string()),
                caller_uri: "sip:owner@example.com".to_string(),
                callee_uri: "sip:member@example.com".to_string(),
                duration_secs: 90,
                file_id: None,
                recorded_by: "sip:owner@example.com".to_string(),
                created_at: Utc::now(),
                conference_id: None,
                transcript_segment_count: 0,
                legal_hold: false,
                deleted_at: None,
                deleted_by: None,
            })
            .is_err());
        assert!(state
            .send_room_message(
                room.id,
                "sip:owner@example.com",
                "urgent update",
                None,
                Some("urgent".to_string()),
            )
            .is_err());
    }

    #[test]
    fn user_call_policy_controls_private_external_forwarding_and_recording() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-user-call-policy-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let mut settings = state.get_user_call_settings("sip:alice@example.com");
        settings.allow_private_calls = false;
        settings.allow_external_calls = false;
        settings.allow_call_forwarding = false;
        settings.allow_call_recording = false;
        settings.forward_always = Some("sip:bob@example.com".to_string());
        settings.dnd_enabled = true;
        settings.dnd_forward_to = Some("sip:bob@example.com".to_string());
        state.set_user_call_settings(settings);

        assert!(state
            .create_call(CreateCallRequest {
                conference_id: None,
                caller: "sip:alice@example.com".to_string(),
                callees: vec!["sip:bob@example.com".to_string()],
                media: vec![MediaKind::Audio],
            })
            .is_err());
        assert!(state
            .create_call(CreateCallRequest {
                conference_id: Some(Uuid::new_v4()),
                caller: "sip:alice@example.com".to_string(),
                callees: vec!["sip:bob@example.com".to_string()],
                media: vec![MediaKind::Audio],
            })
            .is_ok());
        assert!(state
            .create_call(CreateCallRequest {
                conference_id: Some(Uuid::new_v4()),
                caller: "sip:alice@example.com".to_string(),
                callees: vec!["sip:carrier@external.example".to_string()],
                media: vec![MediaKind::Audio],
            })
            .is_err());
        assert_eq!(
            state.resolve_call_forwarding("sip:alice@example.com", "always"),
            None
        );
        assert_eq!(state.check_dnd("sip:alice@example.com"), (true, None));
        assert!(state
            .store_recording(CallRecording {
                id: Uuid::new_v4(),
                call_id: Some("blocked-recording".to_string()),
                caller_uri: "sip:alice@example.com".to_string(),
                callee_uri: "sip:bob@example.com".to_string(),
                duration_secs: 30,
                file_id: None,
                recorded_by: "sip:alice@example.com".to_string(),
                created_at: Utc::now(),
                conference_id: None,
                transcript_segment_count: 0,
                legal_hold: false,
                deleted_at: None,
                deleted_by: None,
            })
            .is_err());
    }

    #[test]
    fn retention_enforcement_previews_applies_and_skips_legal_hold() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-retention-enforce-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:owner@example.com",
            CreateRoomRequest {
                name: "Records".to_string(),
                description: None,
                members: vec!["sip:member@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let old = state
            .send_room_message(room.id, "sip:owner@example.com", "old", None, None)
            .unwrap();
        let fresh = state
            .send_room_message(room.id, "sip:owner@example.com", "fresh", None, None)
            .unwrap();
        state
            .set_room_message_created_at_for_test(old.id, Utc::now() - Duration::days(30))
            .unwrap();

        let hold = state.upsert_retention_policy(
            "admin",
            UpsertRetentionPolicyRequest {
                id: None,
                name: "Hold".to_string(),
                scope: "room".to_string(),
                room_id: Some(room.id),
                retain_days: Some(1),
                legal_hold: Some(true),
                export_enabled: Some(true),
            },
        );
        let preview_hold = state.enforce_retention(true);
        assert_eq!(preview_hold.matched_messages, 0);
        assert_eq!(preview_hold.skipped_legal_hold_policies, vec![hold.id]);
        assert!(state.room_message(old.id).is_some());

        let updated_hold = state.upsert_retention_policy(
            "compliance-admin",
            UpsertRetentionPolicyRequest {
                id: Some(hold.id),
                name: "One day".to_string(),
                scope: "room".to_string(),
                room_id: Some(room.id),
                retain_days: Some(1),
                legal_hold: Some(false),
                export_enabled: Some(true),
            },
        );
        assert_eq!(updated_hold.created_by, "admin");
        let preview = state.enforce_retention(true);
        assert_eq!(preview.matched_messages, 1);
        assert_eq!(preview.deleted_messages, 0);
        assert!(state.room_message(old.id).is_some());

        let applied = state.enforce_retention(false);
        assert_eq!(applied.deleted_messages, 1);
        assert!(state.room_message(old.id).is_none());
        assert!(state.room_message(fresh.id).is_some());
        assert!(state.delete_retention_policy(hold.id));
        assert!(state.retention_policies().is_empty());
    }

    #[test]
    fn discovery_search_filters_messages_by_keyword_user_room_and_date() {
        let state = AppState::new(
            PathBuf::from(format!(
                "/tmp/pale-discovery-search-test-{}",
                Uuid::new_v4()
            )),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:owner@example.com",
            CreateRoomRequest {
                name: "Discovery".to_string(),
                description: None,
                members: vec!["sip:member@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        let target = state
            .send_room_message(
                room.id,
                "sip:owner@example.com",
                "quarterly acquisition review",
                None,
                None,
            )
            .unwrap();
        let target_created_at = Utc::now();
        state
            .set_room_message_created_at_for_test(target.id, target_created_at)
            .unwrap();
        state
            .send_room_message(
                room.id,
                "sip:member@example.com",
                "routine operations",
                None,
                None,
            )
            .unwrap();

        let searched = state.discovery_search(DiscoverySearchQuery {
            q: Some("acquisition".to_string()),
            user_uri: Some("owner@example.com".to_string()),
            room_id: Some(room.id),
            from: Some(target_created_at - Duration::minutes(5)),
            to: Some(target_created_at + Duration::minutes(5)),
            limit: Some(10),
        });
        assert_eq!(searched.messages.len(), 1);
        assert_eq!(searched.messages[0].id, target.id);
        assert!(searched.files.is_empty());

        let missed = state.discovery_search(DiscoverySearchQuery {
            q: Some("acquisition".to_string()),
            user_uri: Some("member@example.com".to_string()),
            room_id: Some(room.id),
            from: None,
            to: None,
            limit: Some(10),
        });
        assert!(missed.messages.is_empty());
    }

    #[test]
    fn ediscovery_cases_save_queries_and_export_custodian_scoped_results() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-ediscovery-case-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let room = state.create_room(
            "sip:alice@example.com",
            CreateRoomRequest {
                name: "Investigation".to_string(),
                description: None,
                members: vec!["sip:bob@example.com".to_string()],
                is_direct: Some(false),
                team_id: None,
                channel_name: None,
                channel_type: None,
                channel_owners: Vec::new(),
                posting_policy: None,
            },
        );
        state
            .send_room_message(
                room.id,
                "sip:alice@example.com",
                "Project mercury decision",
                None,
                None,
            )
            .unwrap();
        state
            .send_room_message(
                room.id,
                "sip:bob@example.com",
                "Project mercury follow up",
                None,
                None,
            )
            .unwrap();
        state
            .send_room_message(
                room.id,
                "sip:mallory@example.com",
                "Project mercury outsider",
                None,
                None,
            )
            .unwrap();

        let case = state
            .create_ediscovery_case(
                "admin",
                CreateEDiscoveryCaseRequest {
                    name: "Mercury inquiry".to_string(),
                    description: "Custodian scoped search".to_string(),
                    custodians: vec![
                        "sip:alice@example.com".to_string(),
                        "sip:bob@example.com".to_string(),
                    ],
                    query: EDiscoveryCaseQuery {
                        q: Some("mercury".to_string()),
                        room_id: Some(room.id),
                        limit: Some(100),
                        ..EDiscoveryCaseQuery::default()
                    },
                },
            )
            .unwrap();
        let export = state.export_ediscovery_case(case.id, "admin").unwrap();
        assert_eq!(export.messages.len(), 2);
        assert!(export
            .messages
            .iter()
            .all(|message| message.sender_uri != "sip:mallory@example.com"));

        let updated = state.list_ediscovery_cases().remove(0);
        assert_eq!(updated.id, case.id);
        assert_eq!(updated.last_export_count, 2);
        assert_eq!(updated.last_exported_by.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn dlp_blocks_file_upload_content() {
        let data_dir = std::env::temp_dir().join(format!("pale-file-dlp-{}", Uuid::new_v4()));
        let token = "012345678901234567890123".to_string();
        let admin_hash = sha256_hex("admin-password".as_bytes());
        let storage_key = "dlp-storage-key".to_string();
        let state = AppState::persistent(
            data_dir.clone(),
            token.clone(),
            "admin".to_string(),
            admin_hash.clone(),
            storage_key.clone(),
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let policy = state
            .create_dlp_policy(
                "admin",
                CreateDlpPolicyRequest {
                    name: "Secrets".to_string(),
                    description: None,
                    pattern: "SECRET-[0-9]+".to_string(),
                    action: DlpAction::Block,
                    enabled: true,
                },
            )
            .unwrap();
        assert!(state
            .create_dlp_policy(
                "admin",
                CreateDlpPolicyRequest {
                    name: "Invalid".to_string(),
                    description: None,
                    pattern: "(".to_string(),
                    action: DlpAction::Block,
                    enabled: true,
                },
            )
            .is_err());
        let preview = state.preview_content_dlp("admin", "customer SECRET-000");
        assert!(!preview.allowed);
        assert_eq!(preview.violations.len(), 1);
        assert!(state.list_dlp_violations().is_empty());
        let unicode_preview =
            state.preview_content_dlp("admin", &format!("{} SECRET-001", "é".repeat(90)));
        assert_eq!(unicode_preview.violations.len(), 1);
        assert!(unicode_preview.violations[0]
            .content_snippet
            .ends_with("..."));

        let decision = state
            .file_governance_for_upload(
                "sip:alice@example.com",
                "notes.txt",
                "text/plain",
                b"customer SECRET-123",
            )
            .await;

        assert!(!decision.allowed);
        assert_eq!(decision.dlp_status, "blocked");
        assert_eq!(decision.dlp_violation_count, 1);
        assert_eq!(state.list_dlp_violations().len(), 1);
        let filtered = state.search_dlp_violations(DlpViolationQuery {
            policy: Some("secret".to_string()),
            user_uri: Some("alice@example.com".to_string()),
            action: Some(DlpAction::Block),
            limit: Some(10),
            ..DlpViolationQuery::default()
        });
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].policy_name, "Secrets");

        let disabled = state
            .update_dlp_policy(
                policy.id,
                UpdateDlpPolicyRequest {
                    name: Some("Sensitive tokens".to_string()),
                    description: Some("Blocks internal secret tokens".to_string()),
                    pattern: None,
                    action: None,
                    enabled: Some(false),
                },
            )
            .unwrap()
            .unwrap();
        assert!(state
            .update_dlp_policy(
                policy.id,
                UpdateDlpPolicyRequest {
                    name: None,
                    description: None,
                    pattern: Some("[".to_string()),
                    action: None,
                    enabled: None,
                },
            )
            .is_err());
        assert_eq!(disabled.name, "Sensitive tokens");
        assert!(!disabled.enabled);
        let allowed = state
            .file_governance_for_upload(
                "sip:alice@example.com",
                "notes-2.txt",
                "text/plain",
                b"customer SECRET-456",
            )
            .await;
        assert!(allowed.allowed);
        drop(state);

        let reloaded = AppState::persistent(
            data_dir,
            token,
            "admin".to_string(),
            admin_hash,
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let policies = reloaded.list_dlp_policies();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "Sensitive tokens");
        assert!(!policies[0].enabled);
        assert_eq!(reloaded.list_dlp_violations().len(), 1);
    }

    #[tokio::test]
    async fn advanced_threat_protection_blocks_known_malware_when_configured() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-atp-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let eicar = b"X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

        let before = state
            .file_governance_for_upload("sip:alice@example.com", "eicar.txt", "text/plain", eicar)
            .await;
        assert!(before.allowed);
        assert_eq!(before.dlp_status, "clean");
        assert!(!state.advanced_threat_protection_available());

        state
            .update_enterprise_integration(
                "advanced_threat_protection",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("tcp://clamav.local:3310".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("ClamAV".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(state.advanced_threat_protection_available());

        let after = state
            .file_governance_for_upload("sip:alice@example.com", "eicar.txt", "text/plain", eicar)
            .await;
        assert!(!after.allowed);
        assert_eq!(after.dlp_status, "malware_blocked");
        assert_eq!(after.dlp_violation_count, 1);
    }

    #[test]
    fn malware_quarantine_records_review_and_persists() {
        let data_dir = std::env::temp_dir().join(format!("pale-atp-quarantine-{}", Uuid::new_v4()));
        let token = "012345678901234567890123".to_string();
        let admin_hash = sha256_hex("admin-password".as_bytes());
        let storage_key = "atp-quarantine-storage-key".to_string();
        let state = AppState::persistent(
            data_dir.clone(),
            token.clone(),
            "admin".to_string(),
            admin_hash.clone(),
            storage_key.clone(),
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();

        let item = state.quarantine_malware_upload(
            "sip:alice@example.com",
            "eicar.txt",
            "text/plain",
            68,
            "malware-sha",
            "malware_signature_detected",
        );
        assert_eq!(item.status, MalwareQuarantineStatus::Quarantined);
        assert_eq!(state.list_malware_quarantine().len(), 1);

        let reviewed = state
            .review_malware_quarantine(
                item.id,
                "admin",
                ReviewMalwareQuarantineRequest {
                    status: MalwareQuarantineStatus::Deleted,
                    notes: Some("confirmed malware".to_string()),
                },
            )
            .unwrap();
        assert_eq!(reviewed.status, MalwareQuarantineStatus::Deleted);
        assert_eq!(reviewed.reviewed_by.as_deref(), Some("admin"));
        assert_eq!(reviewed.review_notes.as_deref(), Some("confirmed malware"));
        assert!(reviewed.reviewed_at.is_some());
        drop(state);

        let reloaded = AppState::persistent(
            data_dir,
            token,
            "admin".to_string(),
            admin_hash,
            storage_key,
            DEFAULT_MAX_UPLOAD_BYTES,
            MediaConfig::default(),
        )
        .unwrap();
        let items = reloaded.list_malware_quarantine();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, item.id);
        assert_eq!(items[0].status, MalwareQuarantineStatus::Deleted);
        assert_eq!(items[0].review_notes.as_deref(), Some("confirmed malware"));
    }

    #[test]
    fn casb_file_access_enforces_blocked_security_classifications_when_configured() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-casb-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let blocked_file = FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "blocked.txt".to_string(),
            content_type: "text/plain".to_string(),
            size: 12,
            sha256: "blocked".to_string(),
            created_at: Utc::now(),
            dlp_status: "blocked".to_string(),
            dlp_violation_count: 1,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        };
        let clean_file = FileRecord {
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            filename: "clean.txt".to_string(),
            id: Uuid::new_v4(),
            sha256: "clean".to_string(),
            ..blocked_file.clone()
        };

        let before =
            state.casb_file_access_decision("sip:alice@example.com", "download", &blocked_file);
        assert!(before.allowed);
        assert!(!before.enforced);
        assert_eq!(before.reason, "casb_not_configured");

        state
            .update_enterprise_integration(
                "casb",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://opa.local/v1/data/pale/casb".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("OPA policy decision service".to_string()),
                },
                "admin",
            )
            .unwrap();
        assert!(state.casb_available());

        let blocked =
            state.casb_file_access_decision("sip:alice@example.com", "download", &blocked_file);
        assert!(!blocked.allowed);
        assert!(blocked.enforced);
        assert_eq!(blocked.reason, "file_blocked");

        let clean =
            state.casb_file_access_decision("sip:alice@example.com", "download", &clean_file);
        assert!(clean.allowed);
        assert!(clean.enforced);
        assert_eq!(clean.reason, "allowed");
    }

    #[test]
    fn security_posture_report_scores_configured_enterprise_controls() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-security-posture-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        state
            .create_user(CreateUserRequest {
                display_name: "Alice".to_string(),
                sip_uri: "sip:alice@example.com".to_string(),
                matrix_user_id: None,
                password: Some("alice-password".to_string()),
                role: None,
            })
            .unwrap();
        let initial = state.security_posture_report();
        assert_eq!(initial.posture, "needs_attention");
        assert!(initial
            .recommendations
            .iter()
            .any(|rec| rec.control_id == "protection.dlp"));

        state
            .create_dlp_policy(
                "admin",
                CreateDlpPolicyRequest {
                    name: "Secrets".to_string(),
                    description: None,
                    pattern: "SECRET-[0-9]+".to_string(),
                    action: DlpAction::Block,
                    enabled: true,
                },
            )
            .unwrap();
        state.upsert_retention_policy(
            "admin",
            UpsertRetentionPolicyRequest {
                id: None,
                name: "Global retention".to_string(),
                scope: "global".to_string(),
                room_id: None,
                retain_days: Some(365),
                legal_hold: Some(true),
                export_enabled: Some(true),
            },
        );
        state.create_conditional_access_policy(CreateConditionalAccessPolicyRequest {
            name: "Require MFA".to_string(),
            conditions: ConditionalAccessConditions {
                ip_ranges: Vec::new(),
                device_types: Vec::new(),
                user_groups: Vec::new(),
                time_windows: Vec::new(),
            },
            actions: ConditionalAccessActions {
                allow: true,
                block: false,
                require_mfa: true,
            },
            enabled: Some(true),
        });
        state.create_barrier(CreateInformationBarrierRequest {
            name: "Regulated wall".to_string(),
            segment1_name: "Trading".to_string(),
            segment1_users: vec!["sip:alice@example.com".to_string()],
            segment2_name: "Research".to_string(),
            segment2_users: vec!["sip:bob@example.com".to_string()],
            block_chat: true,
            block_call: true,
            enabled: true,
        });
        state.create_label(CreateSensitivityLabelRequest {
            name: "Confidential".to_string(),
            description: "Restrict sensitive content".to_string(),
            color: "#7c3aed".to_string(),
            priority: 100,
            encrypt_content: true,
            restrict_sharing: true,
            watermark: false,
        });
        state.rotate_encryption_key(RotateEncryptionKeyRequest {
            customer_key_base64: None,
        });

        let improved = state.security_posture_report();
        assert!(improved.score > initial.score);
        assert_eq!(improved.counts.enabled_dlp_policies, 1);
        assert_eq!(improved.counts.retention_policies, 1);
        assert_eq!(improved.counts.legal_hold_policies, 1);
        assert_eq!(improved.counts.enabled_conditional_access_policies, 1);
        assert_eq!(improved.counts.enabled_information_barriers, 1);
        assert_eq!(improved.counts.sensitivity_labels, 1);
        assert_eq!(improved.counts.encryption_keys, 1);
        assert!(improved
            .controls
            .iter()
            .any(|control| control.id == "identity.mfa" && control.status == "pass"));
    }

    #[test]
    fn retention_enforcement_covers_files_and_discovery_exports_them() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-file-retention-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let file = FileRecord {
            id: Uuid::new_v4(),
            owner: "sip:alice@example.com".to_string(),
            filename: "old-plan.txt".to_string(),
            content_type: "text/plain".to_string(),
            size: 12,
            sha256: "abc123".to_string(),
            created_at: Utc::now() - Duration::days(30),
            dlp_status: "clean".to_string(),
            dlp_violation_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
            folder_id: None,
            locked_by: None,
            locked_at: None,
        };
        state.put_file_record(file.clone());
        state.upsert_retention_policy(
            "admin",
            UpsertRetentionPolicyRequest {
                id: None,
                name: "Files one day".to_string(),
                scope: "files".to_string(),
                room_id: None,
                retain_days: Some(1),
                legal_hold: Some(false),
                export_enabled: Some(true),
            },
        );

        let preview = state.enforce_retention(true);
        assert_eq!(preview.policy_results[0].matched_files, 1);
        assert!(state.file_record(file.id).unwrap().deleted_at.is_none());

        let applied = state.enforce_retention(false);
        assert_eq!(applied.policy_results[0].deleted_files, 1);
        assert!(state.file_records().is_empty());

        let export = state.discovery_export(None);
        assert_eq!(export.files.len(), 1);
        assert_eq!(export.files[0].id, file.id);
        assert_eq!(export.files[0].deleted_by.as_deref(), Some("retention"));
    }

    #[test]
    fn meeting_assistant_extracts_transcript_intelligence_without_leaking_visibility() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-meeting-assistant-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Launch Review".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        state
            .join_conference(
                conference.id,
                JoinConferenceRequest {
                    user_id: Uuid::new_v4(),
                    sip_uri: "sip:alice@example.com".to_string(),
                    role: Some(ParticipantRole::Host),
                },
                false,
            )
            .unwrap();
        assert!(state.conference_visible_to(conference.id, "sip:alice@example.com"));
        assert!(!state.conference_visible_to(conference.id, "sip:mallory@example.com"));

        state.post_transcript(
            conference.id,
            PostTranscriptRequest {
                speaker_uri: "sip:alice@example.com".to_string(),
                speaker_name: "Alice".to_string(),
                text: "We decided to launch Friday and need to update the runbook.".to_string(),
                is_final: true,
                language: None,
            },
        );
        state.post_transcript(
            conference.id,
            PostTranscriptRequest {
                speaker_uri: "sip:bob@example.com".to_string(),
                speaker_name: "Bob".to_string(),
                text: "Risk: billing migration is blocked. Can Clara follow up?".to_string(),
                is_final: true,
                language: None,
            },
        );

        let report = state.meeting_assistant_report(conference.id).unwrap();
        assert_eq!(report.transcript_segments, 2);
        assert!(!report.ai_provider_configured);
        assert!(report.summary.contains("launch Friday"));
        assert!(report
            .action_items
            .iter()
            .any(|item| item.text.contains("need to update")));
        assert!(report.decisions.iter().any(|line| line.contains("decided")));
        assert!(report.risks.iter().any(|line| line.contains("Risk")));
        assert!(report
            .open_questions
            .iter()
            .any(|line| line.contains("Can Clara")));
        assert_eq!(report.speaker_stats.len(), 2);

        state
            .update_enterprise_integration(
                "meeting_assistant",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://llm.local/v1/chat/completions".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: None,
                },
                "admin",
            )
            .unwrap();
        assert!(
            state
                .meeting_assistant_report(conference.id)
                .unwrap()
                .ai_provider_configured
        );
    }

    #[test]
    fn transcription_jobs_orchestrate_asr_without_faking_provider_output() {
        let state = AppState::new(
            PathBuf::from(format!("/tmp/pale-transcription-{}", Uuid::new_v4())),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference = state.create_conference(CreateConferenceRequest {
            title: "Planning".to_string(),
            mode: ConferenceMode::Video,
            registration_enabled: None,
            max_registrations: None,
            registration_fields: None,
        });
        let recording = state
            .store_recording(CallRecording {
                id: Uuid::new_v4(),
                call_id: Some("planning-call".to_string()),
                caller_uri: "sip:alice@example.com".to_string(),
                callee_uri: "sip:bob@example.com".to_string(),
                duration_secs: 600,
                file_id: Some(Uuid::new_v4()),
                recorded_by: "sip:alice@example.com".to_string(),
                created_at: Utc::now(),
                conference_id: Some(conference.id),
                transcript_segment_count: 0,
                legal_hold: false,
                deleted_at: None,
                deleted_by: None,
            })
            .unwrap();
        let blocked = state.transcription_jobs_for_recording(recording.id);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].status, TranscriptionJobStatus::Blocked);
        assert_eq!(
            blocked[0].error.as_deref(),
            Some("auto_transcription_provider_not_configured")
        );

        state
            .update_enterprise_integration(
                "auto_transcription",
                UpdateEnterpriseIntegrationRequest {
                    enabled: Some(true),
                    endpoint_url: Some("http://whisper.local".to_string()),
                    admin_url: None,
                    api_key: None,
                    clear_api_key: None,
                    notes: Some("Whisper endpoint".to_string()),
                },
                "admin",
            )
            .unwrap();
        let queued = state
            .queue_transcription_job(recording.id, "admin", Some("en".to_string()))
            .unwrap();
        assert_eq!(queued.status, TranscriptionJobStatus::Queued);
        assert!(queued.provider_configured);
        let processing = state.start_transcription_job(queued.id).unwrap();
        assert_eq!(processing.status, TranscriptionJobStatus::Processing);

        let completed = state
            .complete_transcription_job(
                queued.id,
                vec![
                    PostTranscriptRequest {
                        speaker_uri: "sip:alice@example.com".to_string(),
                        speaker_name: "Alice".to_string(),
                        text: "We decided to launch the rollout next week.".to_string(),
                        is_final: true,
                        language: None,
                    },
                    PostTranscriptRequest {
                        speaker_uri: "sip:bob@example.com".to_string(),
                        speaker_name: "Bob".to_string(),
                        text: "Action item: Bob will prepare the checklist.".to_string(),
                        is_final: true,
                        language: None,
                    },
                ],
            )
            .unwrap();
        assert_eq!(completed.status, TranscriptionJobStatus::Completed);
        assert_eq!(completed.transcript_segment_count, 2);
        assert_eq!(state.get_transcript(conference.id).len(), 2);
        assert_eq!(
            state
                .recording(recording.id)
                .unwrap()
                .transcript_segment_count,
            2
        );
    }

    #[test]
    fn retention_enforcement_covers_recordings_and_preserves_legal_hold() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-recording-retention-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let conference_id = Uuid::new_v4();
        state.post_transcript(
            conference_id,
            PostTranscriptRequest {
                speaker_uri: "sip:alice@example.com".to_string(),
                speaker_name: "Alice".to_string(),
                text: "We need to preserve this decision.".to_string(),
                is_final: true,
                language: None,
            },
        );
        let recording = CallRecording {
            id: Uuid::new_v4(),
            call_id: Some("call-1".to_string()),
            caller_uri: "sip:alice@example.com".to_string(),
            callee_uri: "sip:bob@example.com".to_string(),
            duration_secs: 600,
            file_id: None,
            recorded_by: "sip:alice@example.com".to_string(),
            created_at: Utc::now() - Duration::days(30),
            conference_id: Some(conference_id),
            transcript_segment_count: 0,
            legal_hold: false,
            deleted_at: None,
            deleted_by: None,
        };
        state.store_recording(recording.clone()).unwrap();
        assert_eq!(
            state.recordings_for_user("sip:alice@example.com")[0].transcript_segment_count,
            1
        );

        state.upsert_retention_policy(
            "admin",
            UpsertRetentionPolicyRequest {
                id: None,
                name: "Recording hold".to_string(),
                scope: "recordings".to_string(),
                room_id: None,
                retain_days: Some(1),
                legal_hold: Some(true),
                export_enabled: Some(true),
            },
        );
        let held = state
            .delete_recording(recording.id, "sip:admin@example.com")
            .unwrap();
        assert!(held.deleted_at.is_some());
        assert_eq!(held.deleted_by.as_deref(), Some("sip:admin@example.com"));
        assert!(state
            .recordings_for_user("sip:alice@example.com")
            .is_empty());
        assert_eq!(state.discovery_export(None).recordings.len(), 1);

        let preview = state.enforce_retention(true);
        assert_eq!(preview.policy_results[0].matched_recordings, 0);
        assert_eq!(preview.skipped_legal_hold_policies.len(), 1);
    }

    #[test]
    fn scheduled_meetings_can_be_updated_cancelled_and_exported_as_ics() {
        let state = AppState::new(
            PathBuf::from("/tmp/pale-meeting-lifecycle-test"),
            "012345678901234567890123".to_string(),
            sha256_hex("admin-password".as_bytes()),
        );
        let starts_at = Utc::now() + Duration::days(1);
        let meeting = state
            .create_scheduled_meeting(
                "sip:organizer@example.com",
                CreateScheduledMeetingRequest {
                    title: "Planning".to_string(),
                    description: None,
                    room_id: None,
                    participants: vec!["sip:alice@example.com".to_string()],
                    starts_at,
                    ends_at: starts_at + Duration::hours(1),
                    mode: Some(RoomCallMode::Video),
                    recurrence: Some(MeetingRecurrence {
                        frequency: MeetingRecurrenceFrequency::Weekly,
                        interval: 1,
                        until: Some(starts_at + Duration::days(28)),
                    }),
                },
            )
            .expect("meeting");

        let updated = state
            .update_scheduled_meeting(
                meeting.id,
                "sip:organizer@example.com",
                UpdateScheduledMeetingRequest {
                    title: Some("Planning Updated".to_string()),
                    description: Some("Agenda".to_string()),
                    participants: Some(vec!["sip:bob@example.com".to_string()]),
                    starts_at: None,
                    ends_at: None,
                    recurrence: None,
                },
            )
            .expect("updated meeting");
        assert_eq!(updated.title, "Planning Updated");
        assert!(updated
            .participants
            .contains(&"sip:bob@example.com".to_string()));

        let ics = state
            .meeting_ics(meeting.id, "sip:bob@example.com")
            .expect("ics");
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("RRULE:FREQ=WEEKLY;INTERVAL=1"));

        let cancelled = state
            .cancel_scheduled_meeting(meeting.id, "sip:organizer@example.com")
            .expect("cancelled meeting");
        assert_eq!(cancelled.status, MeetingStatus::Cancelled);
        assert!(state
            .start_scheduled_meeting(meeting.id, "sip:bob@example.com")
            .is_none());
    }

    #[test]
    fn nats_helpers_normalize_subjects_and_addresses() {
        assert_eq!(
            nats_subject_token("room.message/created"),
            "room_message_created"
        );
        assert_eq!(
            nats_tcp_address("nats://localhost").unwrap(),
            "localhost:4222"
        );
        assert_eq!(
            nats_tcp_address("nats://localhost:4223").unwrap(),
            "localhost:4223"
        );
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

// ─── Federation ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationPeer {
    pub id: Uuid,
    pub domain: String,
    pub server_url: String,
    pub shared_key_enc: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateFederationPeerRequest {
    pub domain: String,
    pub server_url: String,
    pub shared_key: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateFederationPeerRequest {
    pub server_url: Option<String>,
    pub shared_key: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedMessage {
    pub id: Uuid,
    pub from_domain: String,
    pub from_user: String,
    pub to_domain: String,
    pub to_user: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FederationSendRequest {
    pub to_domain: String,
    pub to_user: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FederationReceiveRequest {
    pub from_domain: String,
    pub from_user: String,
    pub to_user: String,
    pub body: String,
    pub shared_key: String,
}

// ─── Loop Components ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopComponent {
    pub id: Uuid,
    pub room_id: Uuid,
    pub component_type: String,
    pub data: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLoopComponentRequest {
    pub component_type: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopComponentRequest {
    pub data: serde_json::Value,
}

// ─── Compliance Reviews ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReview {
    pub id: Uuid,
    pub message_id: Uuid,
    pub policy_id: Option<Uuid>,
    pub category: String,
    pub severity: String,
    pub flagged_content: String,
    pub status: String,
    pub reviewer: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComplianceScanRequest {
    pub message_id: Uuid,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateComplianceReviewRequest {
    pub status: String,
}

// ─── Data Residency ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataResidencyConfig {
    pub id: Uuid,
    pub region: String,
    pub pg_connection_string_enc: String,
    pub file_storage_path: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDataResidencyConfigRequest {
    pub region: String,
    pub pg_connection_string: String,
    pub file_storage_path: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateDataResidencyConfigRequest {
    pub pg_connection_string: Option<String>,
    pub file_storage_path: Option<String>,
    pub enabled: Option<bool>,
}

impl AppState {
    // ─── Federation methods ───

    pub fn list_federation_peers(&self) -> Vec<FederationPeer> {
        let mut peers = self.federation_peers.values();
        peers.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        peers
    }

    pub fn create_federation_peer(&self, req: CreateFederationPeerRequest) -> FederationPeer {
        let peer = FederationPeer {
            id: Uuid::new_v4(),
            domain: req.domain,
            server_url: req.server_url,
            shared_key_enc: req.shared_key,
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.federation_peers.insert(peer.id, peer.clone());
        peer
    }

    pub fn update_federation_peer(
        &self,
        id: Uuid,
        req: UpdateFederationPeerRequest,
    ) -> Option<FederationPeer> {
        let mut peer = self.federation_peers.get(&id)?;
        if let Some(url) = req.server_url {
            peer.server_url = url;
        }
        if let Some(key) = req.shared_key {
            peer.shared_key_enc = key;
        }
        if let Some(enabled) = req.enabled {
            peer.enabled = enabled;
        }
        self.federation_peers.insert(id, peer.clone());
        Some(peer)
    }

    pub fn delete_federation_peer(&self, id: Uuid) -> bool {
        self.federation_peers.remove(&id).is_some()
    }

    pub fn get_federation_peer_by_domain(&self, domain: &str) -> Option<FederationPeer> {
        self.federation_peers
            .values()
            .into_iter()
            .find(|p| p.domain == domain)
    }

    pub fn store_federated_message(&self, msg: FederatedMessage) {
        self.federated_messages.write().expect("lock").push(msg);
    }

    pub fn list_federated_messages(&self) -> Vec<FederatedMessage> {
        self.federated_messages.read().expect("lock").clone()
    }

    pub fn list_federated_messages_for_user(&self, user: &str) -> Vec<FederatedMessage> {
        self.federated_messages
            .read()
            .expect("lock")
            .iter()
            .filter(|m| m.to_user == user || m.from_user == user)
            .cloned()
            .collect()
    }

    // ─── Loop Component methods ───

    pub fn list_loop_components(&self, room_id: Uuid) -> Vec<LoopComponent> {
        let mut components: Vec<_> = self
            .loop_components
            .values()
            .into_iter()
            .filter(|c| c.room_id == room_id)
            .collect();
        components.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        components
    }

    pub fn create_loop_component(
        &self,
        room_id: Uuid,
        created_by: &str,
        req: CreateLoopComponentRequest,
    ) -> LoopComponent {
        let now = Utc::now();
        let component = LoopComponent {
            id: Uuid::new_v4(),
            room_id,
            component_type: req.component_type,
            data: req.data.unwrap_or(serde_json::json!({})),
            created_by: created_by.to_string(),
            created_at: now,
            updated_at: now,
        };
        self.loop_components.insert(component.id, component.clone());
        component
    }

    pub fn update_loop_component(
        &self,
        id: Uuid,
        req: UpdateLoopComponentRequest,
    ) -> Option<LoopComponent> {
        let mut component = self.loop_components.get(&id)?;
        component.data = req.data;
        component.updated_at = Utc::now();
        self.loop_components.insert(id, component.clone());
        Some(component)
    }

    pub fn delete_loop_component(&self, id: Uuid) -> bool {
        self.loop_components.remove(&id).is_some()
    }

    // ─── Compliance methods ───

    pub fn list_compliance_reviews(&self) -> Vec<ComplianceReview> {
        let mut reviews = self.compliance_reviews.values();
        reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        reviews
    }

    pub fn scan_message_compliance(&self, req: ComplianceScanRequest) -> Vec<ComplianceReview> {
        let mut flagged = Vec::new();
        // Keyword patterns
        let keywords = ["confidential", "secret", "password", "ssn", "credit card"];
        let lower_body = req.body.to_lowercase();
        for keyword in &keywords {
            if lower_body.contains(keyword) {
                let review = ComplianceReview {
                    id: Uuid::new_v4(),
                    message_id: req.message_id,
                    policy_id: None,
                    category: "keyword".to_string(),
                    severity: "medium".to_string(),
                    flagged_content: keyword.to_string(),
                    status: "pending".to_string(),
                    reviewer: None,
                    reviewed_at: None,
                    created_at: Utc::now(),
                };
                self.compliance_reviews.insert(review.id, review.clone());
                flagged.push(review);
            }
        }
        // Basic toxicity heuristic
        let toxic_terms = ["hate", "kill", "threat", "attack", "bomb"];
        for term in &toxic_terms {
            if lower_body.contains(term) {
                let review = ComplianceReview {
                    id: Uuid::new_v4(),
                    message_id: req.message_id,
                    policy_id: None,
                    category: "toxicity".to_string(),
                    severity: "high".to_string(),
                    flagged_content: term.to_string(),
                    status: "pending".to_string(),
                    reviewer: None,
                    reviewed_at: None,
                    created_at: Utc::now(),
                };
                self.compliance_reviews.insert(review.id, review.clone());
                flagged.push(review);
            }
        }
        flagged
    }

    pub fn update_compliance_review(
        &self,
        id: Uuid,
        reviewer: &str,
        req: UpdateComplianceReviewRequest,
    ) -> Option<ComplianceReview> {
        let mut review = self.compliance_reviews.get(&id)?;
        review.status = req.status;
        review.reviewer = Some(reviewer.to_string());
        review.reviewed_at = Some(Utc::now());
        self.compliance_reviews.insert(id, review.clone());
        Some(review)
    }

    // ─── Data Residency methods ───

    pub fn list_data_residency_configs(&self) -> Vec<DataResidencyConfig> {
        let mut configs = self.data_residency_configs.values();
        configs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        configs
    }

    pub fn create_data_residency_config(
        &self,
        req: CreateDataResidencyConfigRequest,
    ) -> DataResidencyConfig {
        let config = DataResidencyConfig {
            id: Uuid::new_v4(),
            region: req.region,
            pg_connection_string_enc: req.pg_connection_string,
            file_storage_path: req.file_storage_path,
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };
        self.data_residency_configs
            .insert(config.id, config.clone());
        config
    }

    pub fn update_data_residency_config(
        &self,
        id: Uuid,
        req: UpdateDataResidencyConfigRequest,
    ) -> Option<DataResidencyConfig> {
        let mut config = self.data_residency_configs.get(&id)?;
        if let Some(conn) = req.pg_connection_string {
            config.pg_connection_string_enc = conn;
        }
        if let Some(path) = req.file_storage_path {
            config.file_storage_path = path;
        }
        if let Some(enabled) = req.enabled {
            config.enabled = enabled;
        }
        self.data_residency_configs.insert(id, config.clone());
        Some(config)
    }

    pub fn delete_data_residency_config(&self, id: Uuid) -> bool {
        self.data_residency_configs.remove(&id).is_some()
    }
}
