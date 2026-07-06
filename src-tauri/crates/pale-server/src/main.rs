use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
#[cfg(feature = "native-pjsip")]
use pale_server::pjsip_runtime::{PjsipRuntime, PjsipRuntimeConfig, TlsConfig};
use pale_server::{
    cli, http, metrics, sip, AppState, MediaConfig, ServerConfig, TurnConfig, TurnTransport,
};
use tracing_subscriber::EnvFilter;

#[cfg(feature = "native-pjsip")]
type NativePjsipRuntime = PjsipRuntime;
#[cfg(not(feature = "native-pjsip"))]
type NativePjsipRuntime = ();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Structured logging: JSON in production, pretty in dev
    let json_logs = env_bool("PALE_LOG_JSON", false);
    if json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
            .init();
    }

    // Install Prometheus metrics recorder
    let prom_handle = Arc::new(metrics::install_recorder());

    let config = match config_from_env() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("pale-server: {message}");
            std::process::exit(1);
        }
    };
    let mut app_state = AppState::persistent(
        config.data_dir.clone(),
        config.http_token.clone(),
        config.admin_username.clone(),
        config.admin_password_hash.clone(),
        config.storage_key.clone(),
        config.max_upload_bytes,
        config.media.clone(),
    )?;

    // Connect to PostgreSQL if PALE_DATABASE_URL is set
    if let Some(database_url) = optional_env("PALE_DATABASE_URL") {
        let max_pg_connections: usize = std::env::var("PALE_PG_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        match pale_server::PgStore::connect(&database_url, max_pg_connections).await {
            Ok(pg) => {
                if let Err(e) = pg.run_migrations().await {
                    log::error!("PostgreSQL migration failed: {}", e);
                    return Err(e);
                }
                app_state.set_pg_store(pg);
                app_state.load_from_postgres().await;
                log::info!("PostgreSQL connected and migrations applied");
            }
            Err(e) => {
                log::error!("Failed to connect to PostgreSQL: {}", e);
                return Err(e);
            }
        }
    } else {
        log::info!("No PALE_DATABASE_URL set — using SQLite-only persistence");
    }

    // Configure LiveKit SFU if environment variables are set
    if let Some(lk_config) = pale_server::livekit::LiveKitConfig::from_env() {
        log::info!(
            "LiveKit configured: {} (key={})",
            lk_config.url,
            lk_config.api_key
        );
        app_state.set_livekit(lk_config);
    } else {
        log::info!("LiveKit not configured — conferences use signaling-only mode");
    }

    // Configure S3 storage backend if PALE_S3_BUCKET is set
    if let Some(s3_config) = pale_server::storage_backend::S3Config::from_env() {
        log::info!(
            "S3 storage configured: bucket={} region={} endpoint={}",
            s3_config.bucket,
            s3_config.region,
            s3_config.endpoint.as_deref().unwrap_or("default"),
        );
        let client = pale_server::storage_backend::StorageClient::s3(&s3_config).await;
        app_state.set_storage_client(client);
    } else {
        log::info!("S3 not configured — using local file storage");
        let client =
            pale_server::storage_backend::StorageClient::local(config.data_dir.join("files"));
        app_state.set_storage_client(client);
    }

    // Configure VAPID for Web Push notifications
    if let Some(vapid_config) = pale_server::web_push::VapidConfig::from_env() {
        log::info!(
            "Web Push VAPID configured (subject={})",
            vapid_config.subject,
        );
        app_state.set_vapid_config(vapid_config);
    } else {
        log::info!("Web Push not configured — set PALE_VAPID_PUBLIC_KEY and PALE_VAPID_PRIVATE_KEY to enable");
    }

    // CLI mode: if --cli flag is passed, run CLI commands and exit
    if std::env::args().any(|arg| arg == "--cli") {
        let cli_args: Vec<String> = std::env::args()
            .skip_while(|arg| arg != "--cli")
            .skip(1)
            .collect();
        let mut full_args = vec!["pale-server".to_string()];
        full_args.extend(cli_args);
        match cli::Cli::try_parse_from(&full_args) {
            Ok(parsed) => {
                cli::run_cli(parsed, &app_state);
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    // Rate limiting
    let rate_limit_rps: u32 = std::env::var("PALE_RATE_LIMIT_RPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    app_state.set_rate_limit_rps(rate_limit_rps);

    let sip_backend = sip_backend_from_env();

    // Use PALE_SIP_EXTERNAL_ADDR if set (public-facing SIP address for clients),
    // otherwise derive from HTTP bind address, replacing 0.0.0.0 with 127.0.0.1
    let sip_external = match std::env::var("PALE_SIP_EXTERNAL_ADDR") {
        Ok(addr) => addr,
        Err(_) => {
            let addr = config.sip_addr.to_string();
            if addr.starts_with("0.0.0.0") && matches!(sip_backend, SipBackend::UdpParser) {
                log::warn!(
                    "PALE_SIP_EXTERNAL_ADDR is not set — advertising the SIP registrar as 127.0.0.1, \
                     so only clients on this machine can register. Set PALE_SIP_EXTERNAL_ADDR to this \
                     server's public hostname or IP for remote clients."
                );
            }
            addr.replace("0.0.0.0", "127.0.0.1")
        }
    };
    let parser_tls = parser_tls_config_from_env(config.sip_addr.port())?;
    let parser_tls_external = parser_tls.as_ref().map(|tls| {
        optional_env("PALE_SIP_TLS_EXTERNAL_ADDR").unwrap_or_else(|| {
            let host = sip_external
                .rsplit_once(':')
                .map(|(host, _)| host)
                .unwrap_or(sip_external.as_str());
            format!("{host}:{}", tls.port)
        })
    });
    // Only advertise a registrar when the active backend implements REGISTER.
    // The pjsip backend cannot register clients, so login/provisioning
    // responses must not point clients at it.
    match sip_backend {
        SipBackend::UdpParser => {
            if let Some(external) = parser_tls_external.clone() {
                app_state.set_sip_registrar(external, "tls");
            } else if env_bool("PALE_SIP_TCP", true) {
                app_state.set_sip_registrar(sip_external.clone(), "tcp");
            } else {
                app_state.set_sip_registrar(sip_external.clone(), "udp");
            }
        }
        SipBackend::Disabled => log::warn!(
            "SIP backend disabled for this build; login responses will not advertise a SIP registrar"
        ),
        #[cfg(feature = "native-pjsip")]
        SipBackend::Pjsip => log::info!(
            "SIP backend 'pjsip' does not implement REGISTER; login responses will not advertise a SIP registrar"
        ),
    }

    let state = Arc::new(app_state);
    tokio::fs::create_dir_all(state.files_dir()).await?;
    let _pjsip_runtime: Option<NativePjsipRuntime> = match sip_backend {
        #[cfg(feature = "native-pjsip")]
        SipBackend::Pjsip => {
            let tls = tls_config_from_env(config.sip_addr.port())?;
            let encrypted_by_default = tls.is_some();
            let enable_udp = env_bool("PALE_SIP_UDP", !encrypted_by_default);
            let enable_tcp = env_bool("PALE_SIP_TCP", false);
            let require_srtp = env_bool("PALE_SIP_SRTP", true);
            log::info!(
                "Config: HTTP {} (TLS {}) | SIP {} [UDP {} / TCP {} / TLS {}] SRTP {} | registrar advertised as {} | storage: {} | TURN: {} | rate limit {} req/s",
                config.http_addr,
                onoff(config.http_tls_cert.is_some() && config.http_tls_key.is_some()),
                config.sip_addr,
                onoff(enable_udp),
                onoff(enable_tcp),
                tls.as_ref().map(|t| format!("on (port {})", t.port)).unwrap_or_else(|| "off".to_string()),
                onoff(require_srtp),
                "none (pjsip backend has no registrar)",
                if state.pg_store().is_some() {
                    "PostgreSQL".to_string()
                } else {
                    format!("SQLite at {} (set PALE_DATABASE_URL for PostgreSQL)", config.data_dir.display())
                },
                config.media.turn.as_ref().map(|t| t.server.as_str()).unwrap_or("none"),
                rate_limit_rps,
            );
            let runtime = PjsipRuntime::start(
                PjsipRuntimeConfig {
                    sip_addr: config.sip_addr,
                    enable_udp,
                    enable_tcp,
                    tls,
                    require_srtp,
                    media: config.media.clone(),
                },
                state.clone(),
            )
            .map_err(|err| format!("failed to start PJSIP SIP server: {err}"))?;
            log::info!("Pale PJSIP server listening on {}", config.sip_addr);
            Some(runtime)
        }
        SipBackend::UdpParser => {
            state.set_runtime_event_persistence(false);
            let enable_tcp = env_bool("PALE_SIP_TCP", true);
            let enable_udp = env_bool("PALE_SIP_UDP", false);
            let tls_label = parser_tls
                .as_ref()
                .map(|tls| format!("on (port {})", tls.port))
                .unwrap_or_else(|| "off".to_string());
            log::info!(
                "Config: SIP parser {} [UDP {} / TCP {} / TLS {}] | registrar advertised as {} over {}",
                config.sip_addr,
                onoff(enable_udp),
                onoff(enable_tcp),
                tls_label,
                parser_tls_external.as_deref().unwrap_or(&sip_external),
                if parser_tls_external.is_some() {
                    "tls"
                } else if enable_tcp {
                    "tcp"
                } else {
                    "udp"
                },
            );
            if let Some(tls) = parser_tls.clone() {
                let tls_addr = SocketAddr::new(config.sip_addr.ip(), tls.port);
                let listener = sip::bind_tcp_listener(tls_addr).await.map_err(|err| {
                    format!("failed to bind SIP TLS parser listener on {tls_addr}: {err}")
                })?;
                log::info!("Pale SIP parser TLS server listening on {}", tls_addr);
                let sip_state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) =
                        sip::serve_tls(listener, &tls.cert_file, &tls.privkey_file, sip_state).await
                    {
                        log::error!("SIP TLS parser server stopped: {}", err);
                        std::process::exit(1);
                    }
                });
            }
            if enable_tcp {
                let listener = sip::bind_tcp_listener(config.sip_addr)
                    .await
                    .map_err(|err| {
                        format!(
                            "failed to bind SIP TCP parser listener on {}: {err}",
                            config.sip_addr
                        )
                    })?;
                log::info!(
                    "Pale SIP parser TCP server listening on {}",
                    config.sip_addr
                );
                let sip_state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) = sip::serve_tcp(listener, sip_state).await {
                        log::error!("SIP TCP parser server stopped: {}", err);
                        std::process::exit(1);
                    }
                });
            }
            if enable_udp {
                // Bind synchronously at startup: if the SIP listener cannot
                // start, the process must exit nonzero instead of silently
                // serving HTTP with no SIP listener.
                let socket = match sip::bind_udp_socket(config.sip_addr).await {
                    Ok(socket) => socket,
                    Err(err) => {
                        log::error!(
                            "failed to start SIP parser UDP server on {}: {}",
                            config.sip_addr,
                            err
                        );
                        std::process::exit(1);
                    }
                };
                log::info!(
                    "Pale SIP parser UDP server listening on {}",
                    config.sip_addr
                );
                let sip_state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) = sip::serve_udp(socket, sip_state).await {
                        // The receive loop only exits on unrecoverable socket
                        // errors; treat that as a fatal outage rather than
                        // continuing to serve HTTP with no SIP listener.
                        log::error!("SIP parser UDP server stopped: {}", err);
                        std::process::exit(1);
                    }
                });
            }
            if parser_tls.is_none() && !enable_tcp && !enable_udp {
                log::error!("no SIP parser listener enabled; enable PALE_SIP_TLS, PALE_SIP_TCP, or PALE_SIP_UDP");
                std::process::exit(1);
            }
            None
        }
        SipBackend::Disabled => {
            log::warn!(
                "No SIP listener started because the configured SIP backend is unavailable in this build"
            );
            None
        }
    };

    // Spawn periodic gauge refresh (every 30s)
    let gauge_state = state.clone();
    tokio::spawn(async move {
        loop {
            metrics::record_app_gauges(&gauge_state);
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    // Spawn scheduled message delivery task (every 30s)
    let scheduled_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let delivered = scheduled_state.deliver_scheduled_messages();
            if !delivered.is_empty() {
                log::info!("Delivered {} scheduled message(s)", delivered.len());
            }
        }
    });

    // Spawn periodic database cleanup (every 24 hours, also runs on startup)
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        loop {
            // Run cleanup if PgStore is available
            if let Some(pg) = &cleanup_state.pg_store() {
                match pg.cleanup_expired().await {
                    Ok(()) => log::info!("Database cleanup completed"),
                    Err(e) => log::warn!("Database cleanup failed: {}", e),
                }
            }
            tokio::time::sleep(Duration::from_secs(86400)).await;
        }
    });

    if let Some(interval) = retention_enforcement_interval_from_env() {
        let retention_state = state.clone();
        let run_on_startup = env_bool("PALE_RETENTION_ENFORCEMENT_RUN_ON_STARTUP", false);
        log::info!(
            "Retention enforcement worker enabled every {} seconds{}",
            interval.as_secs(),
            if run_on_startup {
                " and on startup"
            } else {
                ""
            }
        );
        tokio::spawn(async move {
            if run_on_startup {
                run_retention_enforcement(&retention_state);
            }
            loop {
                tokio::time::sleep(interval).await;
                run_retention_enforcement(&retention_state);
            }
        });
    } else {
        log::info!(
            "Retention enforcement worker disabled; set PALE_RETENTION_ENFORCEMENT_INTERVAL_SECS to enable it"
        );
    }

    // Build app router with /metrics endpoint
    let metrics_router = axum::Router::new()
        .route("/metrics", axum::routing::get(metrics::metrics_handler))
        .with_state(prom_handle);
    let app = http::router(state).merge(metrics_router);

    if let (Some(cert), Some(key)) = (&config.http_tls_cert, &config.http_tls_key) {
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
        log::info!("Pale backend HTTPS API listening on {}", config.http_addr);
        axum_server::bind_rustls(config.http_addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        if !config.http_addr.ip().is_loopback() {
            log::warn!(
                "Pale backend HTTP API is bound to a non-loopback address without TLS; set PALE_HTTP_TLS_CERT and PALE_HTTP_TLS_KEY"
            );
        }
        let listener = tokio::net::TcpListener::bind(config.http_addr).await?;
        log::info!("Pale backend HTTP API listening on {}", config.http_addr);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn run_retention_enforcement(state: &AppState) {
    let result = state.enforce_retention(false);
    log::info!(
        "Retention enforcement completed: policies={}, matched_messages={}, deleted_messages={}",
        result.policy_results.len(),
        result.matched_messages,
        result.deleted_messages
    );
}

fn retention_enforcement_interval_from_env() -> Option<Duration> {
    retention_enforcement_interval(
        optional_env("PALE_RETENTION_ENFORCEMENT_INTERVAL_SECS").as_deref(),
    )
}

fn retention_enforcement_interval(value: Option<&str>) -> Option<Duration> {
    let seconds = value?.trim().parse::<u64>().ok()?;
    if seconds == 0 {
        None
    } else {
        Some(Duration::from_secs(seconds.max(60)))
    }
}

trait EnvAddr {
    fn read_addr(name: &str, default: &str) -> SocketAddr;
}

impl EnvAddr for ServerConfig {
    fn read_addr(name: &str, default: &str) -> SocketAddr {
        std::env::var(name)
            .unwrap_or_else(|_| default.to_string())
            .parse()
            .unwrap_or_else(|_| default.parse().expect("default address is valid"))
    }
}

fn config_from_env() -> Result<ServerConfig, String> {
    let mut errors = Vec::new();
    let http_token = checked_secret("PALE_SERVER_TOKEN", &mut errors);
    let admin_password = checked_secret("PALE_ADMIN_PASSWORD", &mut errors);
    let storage_key = checked_secret("PALE_STORAGE_KEY", &mut errors);
    if !errors.is_empty() {
        return Err(format!(
            "configuration errors:\n  - {}\n\n\
             Generate a strong secret with: openssl rand -base64 32\n\
             See .env.example for the full list of settings.",
            errors.join("\n  - ")
        ));
    }

    Ok(ServerConfig {
        http_addr: ServerConfig::read_addr("PALE_HTTP_ADDR", "127.0.0.1:8080"),
        sip_addr: ServerConfig::read_addr("PALE_SIP_ADDR", "0.0.0.0:5060"),
        data_dir: std::env::var("PALE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./pale-data")),
        http_token: http_token.expect("validated above"),
        admin_username: std::env::var("PALE_ADMIN_USERNAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "admin".to_string()),
        admin_password_hash: pale_server::hash_password(&admin_password.expect("validated above")),
        storage_key: storage_key.expect("validated above"),
        max_upload_bytes: std::env::var("PALE_MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100 * 1024 * 1024),
        http_tls_cert: optional_env("PALE_HTTP_TLS_CERT").map(PathBuf::from),
        http_tls_key: optional_env("PALE_HTTP_TLS_KEY").map(PathBuf::from),
        media: media_config_from_env(),
        ca_cert_path: optional_env("PALE_CA_CERT_PATH").map(PathBuf::from),
        verify_client_certs: env_bool("PALE_VERIFY_CLIENT_CERTS", false),
        livekit: pale_server::livekit::LiveKitConfig::from_env(),
    })
}

fn media_config_from_env() -> MediaConfig {
    let stun_servers = std::env::var("PALE_STUN_SERVERS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let turn = optional_env("PALE_TURN_SERVER").map(|server| TurnConfig {
        server,
        transport: turn_transport_from_env(),
        username: optional_env("PALE_TURN_USERNAME"),
        realm: optional_env("PALE_TURN_REALM"),
        password: optional_env("PALE_TURN_PASSWORD"),
    });

    MediaConfig {
        ice_enabled: env_bool("PALE_ICE", true),
        stun_servers,
        stun_ignore_failure: env_bool("PALE_STUN_IGNORE_FAILURE", true),
        turn,
    }
}

fn turn_transport_from_env() -> TurnTransport {
    match std::env::var("PALE_TURN_TRANSPORT")
        .unwrap_or_else(|_| "udp".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "tcp" => TurnTransport::Tcp,
        "tls" => TurnTransport::Tls,
        _ => TurnTransport::Udp,
    }
}

fn checked_secret(name: &str, errors: &mut Vec<String>) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if value.len() >= 24 => Some(value),
        Ok(_) => {
            errors.push(format!("{name} is set but shorter than 24 characters"));
            None
        }
        Err(_) => {
            errors.push(format!("{name} is not set"));
            None
        }
    }
}

#[cfg(feature = "native-pjsip")]
fn tls_config_from_env(
    sip_port: u16,
) -> Result<Option<TlsConfig>, Box<dyn std::error::Error + Send + Sync>> {
    // Explicit PALE_SIP_TLS=true/false wins. When unset, TLS is enabled
    // exactly when a cert and key are provided.
    let certs_present =
        optional_env("PALE_SIP_TLS_CERT").is_some() && optional_env("PALE_SIP_TLS_KEY").is_some();
    if !env_bool("PALE_SIP_TLS", certs_present) {
        if certs_present {
            log::warn!(
                "PALE_SIP_TLS_CERT/KEY are set but PALE_SIP_TLS=false; SIP TLS stays disabled"
            );
        } else {
            log::info!(
                "SIP TLS disabled — set PALE_SIP_TLS_CERT and PALE_SIP_TLS_KEY to enable it"
            );
        }
        return Ok(None);
    }

    let cert_file = required_env("PALE_SIP_TLS_CERT")?;
    let privkey_file = required_env("PALE_SIP_TLS_KEY")?;
    let port = std::env::var("PALE_SIP_TLS_PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()?
        .unwrap_or_else(|| sip_port.saturating_add(1));

    Ok(Some(TlsConfig {
        port,
        cert_file,
        privkey_file,
        ca_list_file: optional_env("PALE_SIP_TLS_CA_FILE"),
        ca_list_path: optional_env("PALE_SIP_TLS_CA_PATH"),
        password: optional_env("PALE_SIP_TLS_KEY_PASSWORD"),
        verify_client: env_bool("PALE_SIP_TLS_VERIFY_CLIENT", false),
        require_client_cert: env_bool("PALE_SIP_TLS_REQUIRE_CLIENT_CERT", false),
    }))
}

#[cfg(not(feature = "native-pjsip"))]
fn parser_tls_config_from_env(
    sip_port: u16,
) -> Result<Option<ParserTlsConfig>, Box<dyn std::error::Error + Send + Sync>> {
    let certs_present =
        optional_env("PALE_SIP_TLS_CERT").is_some() && optional_env("PALE_SIP_TLS_KEY").is_some();
    if !env_bool("PALE_SIP_TLS", certs_present) {
        if certs_present {
            log::warn!(
                "PALE_SIP_TLS_CERT/KEY are set but PALE_SIP_TLS=false; SIP TLS stays disabled"
            );
        } else {
            log::warn!(
                "SIP TLS disabled; new clients will fall back to TCP unless PALE_SIP_TLS_CERT and PALE_SIP_TLS_KEY are set"
            );
        }
        return Ok(None);
    }

    let cert_file = required_env("PALE_SIP_TLS_CERT")?;
    let privkey_file = required_env("PALE_SIP_TLS_KEY")?;
    let port = std::env::var("PALE_SIP_TLS_PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()?
        .unwrap_or_else(|| sip_port.saturating_add(1));

    Ok(Some(ParserTlsConfig {
        port,
        cert_file,
        privkey_file,
    }))
}

#[cfg(feature = "native-pjsip")]
fn parser_tls_config_from_env(
    _sip_port: u16,
) -> Result<Option<ParserTlsConfig>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(None)
}

#[derive(Clone)]
struct ParserTlsConfig {
    port: u16,
    cert_file: String,
    privkey_file: String,
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be set when PALE_SIP_TLS is enabled").into())
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn onoff(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

enum SipBackend {
    #[cfg(feature = "native-pjsip")]
    Pjsip,
    UdpParser,
    Disabled,
}

fn sip_backend_from_env() -> SipBackend {
    match std::env::var("PALE_SIP_BACKEND")
        .unwrap_or_else(|_| "udp-parser".to_string())
        .to_lowercase()
        .as_str()
    {
        "udp-parser" | "parser" | "custom" => SipBackend::UdpParser,
        #[cfg(feature = "native-pjsip")]
        _ => SipBackend::Pjsip,
        #[cfg(not(feature = "native-pjsip"))]
        other => {
            log::warn!(
                "PALE_SIP_BACKEND={} requested, but this pale-server build does not include native PJSIP; disabling SIP listener",
                other
            );
            SipBackend::Disabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_enforcement_interval_parses_disabled_values() {
        assert_eq!(retention_enforcement_interval(None), None);
        assert_eq!(retention_enforcement_interval(Some("")), None);
        assert_eq!(retention_enforcement_interval(Some("0")), None);
        assert_eq!(retention_enforcement_interval(Some("not-a-number")), None);
    }

    #[test]
    fn retention_enforcement_interval_clamps_and_accepts_seconds() {
        assert_eq!(
            retention_enforcement_interval(Some("30")),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            retention_enforcement_interval(Some("86400")),
            Some(Duration::from_secs(86400))
        );
    }
}
