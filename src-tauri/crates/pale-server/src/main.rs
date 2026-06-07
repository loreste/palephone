use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use pale_server::{
    http, metrics,
    pjsip_runtime::{PjsipRuntime, PjsipRuntimeConfig, TlsConfig},
    sip, AppState, MediaConfig, ServerConfig, TurnConfig, TurnTransport,
};
use tracing_subscriber::EnvFilter;

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

    let config = config_from_env();
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

    // Rate limiting
    let rate_limit_rps: u32 = std::env::var("PALE_RATE_LIMIT_RPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    app_state.set_rate_limit_rps(rate_limit_rps);

    let state = Arc::new(app_state);
    tokio::fs::create_dir_all(state.files_dir()).await?;

    let sip_backend = sip_backend_from_env();
    let _pjsip_runtime = match sip_backend {
        SipBackend::Pjsip => {
            let tls = tls_config_from_env(config.sip_addr.port())?;
            let encrypted_by_default = tls.is_some();
            let runtime = PjsipRuntime::start(
                PjsipRuntimeConfig {
                    sip_addr: config.sip_addr,
                    enable_udp: env_bool("PALE_SIP_UDP", !encrypted_by_default),
                    enable_tcp: env_bool("PALE_SIP_TCP", false),
                    tls,
                    require_srtp: env_bool("PALE_SIP_SRTP", true),
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
            let sip_state = state.clone();
            let sip_addr = config.sip_addr;
            tokio::spawn(async move {
                if let Err(err) = sip::run_udp_server(sip_addr, sip_state).await {
                    log::error!("SIP parser server stopped: {}", err);
                }
            });
            log::info!("Pale SIP parser UDP server listening on {}", config.sip_addr);
            None
        }
    };

    // Spawn periodic gauge refresh (every 30s)
    let gauge_state = state.clone();
    tokio::spawn(async move {
        loop {
            metrics::record_app_gauges(&gauge_state);
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
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
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
        }
    });

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

fn config_from_env() -> ServerConfig {
    ServerConfig {
        http_addr: ServerConfig::read_addr("PALE_HTTP_ADDR", "127.0.0.1:8080"),
        sip_addr: ServerConfig::read_addr("PALE_SIP_ADDR", "0.0.0.0:5060"),
        data_dir: std::env::var("PALE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./pale-data")),
        http_token: required_secret("PALE_SERVER_TOKEN"),
        admin_username: std::env::var("PALE_ADMIN_USERNAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "admin".to_string()),
        admin_password_hash: pale_server::sha256_hex(
            required_secret("PALE_ADMIN_PASSWORD").as_bytes(),
        ),
        storage_key: required_secret("PALE_STORAGE_KEY"),
        max_upload_bytes: std::env::var("PALE_MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100 * 1024 * 1024),
        http_tls_cert: optional_env("PALE_HTTP_TLS_CERT").map(PathBuf::from),
        http_tls_key: optional_env("PALE_HTTP_TLS_KEY").map(PathBuf::from),
        media: media_config_from_env(),
    }
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

fn required_secret(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| value.len() >= 24)
        .unwrap_or_else(|| panic!("{} must be set to at least 24 characters", name))
}

fn tls_config_from_env(sip_port: u16) -> Result<Option<TlsConfig>, Box<dyn std::error::Error + Send + Sync>> {
    if !env_bool("PALE_SIP_TLS", true) {
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

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

enum SipBackend {
    Pjsip,
    UdpParser,
}

fn sip_backend_from_env() -> SipBackend {
    match std::env::var("PALE_SIP_BACKEND")
        .unwrap_or_else(|_| "pjsip".to_string())
        .to_lowercase()
        .as_str()
    {
        "udp-parser" | "parser" | "custom" => SipBackend::UdpParser,
        _ => SipBackend::Pjsip,
    }
}
