use axum::{Router, routing::get};
use axum_prometheus::{EndpointLabel, PrometheusMetricLayer, PrometheusMetricLayerBuilder};
use metrics_exporter_prometheus::PrometheusHandle;
use shared::metrics::{MetricsConfig, init_metrics};
use std::time::Duration;

/// How often histogram samples are drained into their aggregated form.
const UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Websocket routes, excluded from the HTTP layer.
///
/// These connections are long-lived, so their "request duration" is really the
/// client's session length -- it would swamp the histogram and say nothing
/// about server latency. Per-message handling is measured in the pipeline
/// instead. `axum_http_requests_pending` would likewise just count open
/// sockets.
const WEBSOCKET_ROUTES: &[&str] = &["/mempool/delta", "/mempool/stats", "/clusters/delta"];

/// Per-route request histograms, keyed by matched path rather than raw URI so
/// path params cannot inflate label cardinality.
pub fn http_layer() -> PrometheusMetricLayer<'static> {
    PrometheusMetricLayerBuilder::new()
        .with_ignore_patterns(WEBSOCKET_ROUTES)
        .with_endpoint_label_type(EndpointLabel::MatchedPathWithFallbackFn(collapse_unmatched))
        .build()
}

/// Label for a request that matched no route.
fn collapse_unmatched(_uri: &str) -> String {
    "unmatched".to_string()
}

/// Installs the process-wide recorder and serves `/metrics` on its own listener.
pub async fn serve(cfg: &MetricsConfig) {
    if !cfg.enabled {
        tracing::info!("metrics disabled");
        return;
    }

    let handle = match init_metrics() {
        Ok(handle) => handle,
        Err(e) => {
            tracing::error!("failed to install metrics recorder: {e}");
            return;
        }
    };

    let listener = match tokio::net::TcpListener::bind(&cfg.bind).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("failed to bind metrics listener on {}: {e}", cfg.bind);
            return;
        }
    };

    tokio::spawn(run_upkeep(handle.clone()));

    let router = Router::new().route(
        "/metrics",
        get(move || {
            let handle = handle.clone();
            async move { handle.render() }
        }),
    );

    tracing::info!("metrics listening on {}", cfg.bind);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!("metrics server error: {e}");
        }
    });
}

/// Nothing drains the recorder on its own once the crate's built-in exporter
/// is compiled out, so unaggregated samples pile up until this runs.
async fn run_upkeep(handle: PrometheusHandle) {
    let mut ticker = tokio::time::interval(UPKEEP_INTERVAL);
    loop {
        ticker.tick().await;
        handle.run_upkeep();
    }
}
