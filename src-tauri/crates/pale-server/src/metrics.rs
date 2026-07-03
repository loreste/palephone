use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::AppState;

/// Install the Prometheus metrics recorder and return the handle for the /metrics endpoint.
pub fn install_recorder() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// Axum middleware that records request metrics.
pub async fn request_metrics(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    counter!("http_requests_total", "method" => method.clone(), "path" => path.clone(), "status" => status.clone()).increment(1);
    histogram!("http_request_duration_seconds", "method" => method, "path" => path)
        .record(duration);

    response
}

/// Handler for GET /metrics — returns Prometheus text format.
pub async fn metrics_handler(State(handle): State<Arc<PrometheusHandle>>) -> impl IntoResponse {
    handle.render()
}

/// Record application-level gauges from AppState.
pub fn record_app_gauges(state: &AppState) {
    gauge!("pale_users_total").set(state.users().len() as f64);
    gauge!("pale_sip_accounts_total").set(state.sip_accounts().len() as f64);
    gauge!("pale_registrations_active").set(state.registrations().len() as f64);
    gauge!("pale_dialogs_active").set(state.sip_dialogs().len() as f64);
    gauge!("pale_subscriptions_active").set(state.sip_subscriptions().len() as f64);
    gauge!("pale_presence_online").set(
        state
            .all_presence()
            .iter()
            .filter(|p| p.status != crate::PresenceStatus::Offline)
            .count() as f64,
    );
    gauge!("pale_calls_total").set(state.calls().len() as f64);
    gauge!("pale_conferences_total").set(state.list_conferences().len() as f64);
    gauge!("pale_routing_rules_total").set(state.routing_rules().len() as f64);
    gauge!("pale_files_total").set(state.file_records().len() as f64);
}
