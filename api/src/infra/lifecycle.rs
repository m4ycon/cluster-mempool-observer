use crate::db::models::SystemEventKind;
use crate::infra::config::ApiConfig;
use crate::services::system_event::SystemEventService;
use serde_json::{Value, json};
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;

/// Commit the running binary was built from. Baked in at compile time from the
/// `GIT_SHA` build arg; absent for a plain local `cargo build`.
const GIT_SHA: &str = match option_env!("GIT_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

/// `server_started` event details: which build came up, and where it served.
pub fn server_started_details(cfg: &ApiConfig) -> Value {
    json!({
        "git_sha": GIT_SHA,
        "bind": cfg.bind,
    })
}

/// The unix signal that triggered graceful shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    Term,
    Int,
}

impl ShutdownSignal {
    pub fn name(self) -> &'static str {
        match self {
            ShutdownSignal::Term => "SIGTERM",
            ShutdownSignal::Int => "SIGINT",
        }
    }
}

/// `server_stopped` event details: which signal triggered shutdown and how long the process ran.
pub fn server_stopped_details(signal: ShutdownSignal, uptime: Duration) -> Value {
    json!({
        "signal": signal.name(),
        "uptime_secs": uptime.as_secs(),
    })
}

/// Records `server_started`. Called once the listener is bound, never before: a
/// process that failed to bind never served, and the row would be a lie.
pub async fn record_server_started(service: &SystemEventService, cfg: &ApiConfig) {
    service
        .record(SystemEventKind::ServerStarted, server_started_details(cfg))
        .await;
}

/// Waits on `signal`, then records `server_stopped`. Takes the signal source as a
/// future so tests can drive shutdown without raising a real signal.
pub async fn record_server_stopped(
    service: &SystemEventService,
    start: Instant,
    signal: impl Future<Output = ShutdownSignal>,
) {
    let signal = signal.await;
    tracing::info!("received {}, shutting down", signal.name());
    service
        .record(
            SystemEventKind::ServerStopped,
            server_stopped_details(signal, start.elapsed()),
        )
        .await;
}

/// Notifies the tx backfill consumer to drain, then waits (bounded by
/// `timeout`) for it to finish.
pub async fn drain_tx_backfill_consumer(
    shutdown: &Notify,
    handle: oneshot::Receiver<JoinHandle<()>>,
    timeout: Duration,
) {
    shutdown.notify_one();
    let drain = async {
        let task = handle.await.ok()?;
        task.await.ok()
    };
    if tokio::time::timeout(timeout, drain).await.is_err() {
        tracing::warn!("tx_backfill: consumer drain timed out after {timeout:?}");
    }
}

/// Resolves on SIGTERM or SIGINT. Linux containers only, no Windows fallback.
pub async fn wait_for_shutdown_signal() -> ShutdownSignal {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => ShutdownSignal::Term,
        _ = sigint.recv() => ShutdownSignal::Int,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_started_details_carries_the_build_and_bind() {
        let cfg = ApiConfig {
            bind: "0.0.0.0:9999".to_string(),
            ..Default::default()
        };

        let details = server_started_details(&cfg);

        assert_eq!(details["git_sha"], GIT_SHA);
        assert_eq!(details["bind"], "0.0.0.0:9999");
    }

    #[test]
    fn shutdown_signal_names_match_the_signal() {
        assert_eq!(ShutdownSignal::Term.name(), "SIGTERM");
        assert_eq!(ShutdownSignal::Int.name(), "SIGINT");
    }

    #[test]
    fn server_stopped_details_carries_signal_and_uptime() {
        let details = server_stopped_details(ShutdownSignal::Int, Duration::from_secs(123));

        assert_eq!(details["signal"], "SIGINT");
        assert_eq!(details["uptime_secs"], 123);
    }
}
