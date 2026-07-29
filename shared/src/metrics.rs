use crate::env::{ConfigError, env_or, env_parse};
use metrics_exporter_prometheus::{BuildError, PrometheusBuilder, PrometheusHandle};
use std::future::Future;
use std::time::Instant;

const DEFAULT_METRICS_BIND: &str = "127.0.0.1:3334";

/// Latency buckets, in seconds, shared by every `*_seconds` histogram.
const LATENCY_BUCKETS_SECONDS: &[f64] = &[
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Whether to install the Prometheus recorder and serve the admin endpoint.
    pub enabled: bool,

    /// `host:port` the admin listener (serving `/metrics`) binds to.
    pub bind: String,
}

impl MetricsConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(MetricsConfig {
            enabled: env_parse("METRICS_ENABLED", true)?,
            bind: env_or("METRICS_BIND", DEFAULT_METRICS_BIND),
        })
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: DEFAULT_METRICS_BIND.to_string(),
        }
    }
}

/// Installs the process-wide Prometheus recorder.
pub fn init_metrics() -> Result<PrometheusHandle, BuildError> {
    PrometheusBuilder::new()
        .set_buckets(LATENCY_BUCKETS_SECONDS)?
        .install_recorder()
}

/// Times `f` and records the elapsed seconds on histogram `name`.
pub fn timed<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    metrics::histogram!(name).record(start.elapsed().as_secs_f64());
    out
}

/// [`timed`] with labels. Label values must come from a bounded set -- a stage
/// or operation name, never a txid, cluster id or block hash.
pub fn timed_with<T>(
    name: &'static str,
    labels: &[(&'static str, &'static str)],
    f: impl FnOnce() -> T,
) -> T {
    let start = Instant::now();
    let out = f();
    record_elapsed(name, labels, start);
    out
}

/// Async [`timed`]: measures wall-clock time from poll start to completion,
/// so time the future spends suspended counts.
pub async fn timed_async<T>(name: &'static str, f: impl Future<Output = T>) -> T {
    let start = Instant::now();
    let out = f.await;
    metrics::histogram!(name).record(start.elapsed().as_secs_f64());
    out
}

/// [`timed_async`] with labels. Same cardinality rule as [`timed_with`].
pub async fn timed_async_with<T>(
    name: &'static str,
    labels: &[(&'static str, &'static str)],
    f: impl Future<Output = T>,
) -> T {
    let start = Instant::now();
    let out = f.await;
    record_elapsed(name, labels, start);
    out
}

/// Records `start.elapsed()` on histogram `name`.
///
/// Escape hatch for call sites that cannot wrap their work in a closure --
/// where the timed region borrows across an `await`, or ends early on a
/// branch. Prefer [`timed`] / [`timed_async`] when the shape allows.
pub fn record_elapsed(name: &'static str, labels: &[(&'static str, &'static str)], start: Instant) {
    let labels: Vec<metrics::Label> = labels
        .iter()
        .map(|(k, v)| metrics::Label::new(*k, *v))
        .collect();
    metrics::histogram!(name, labels.iter()).record(start.elapsed().as_secs_f64());
}

#[cfg(test)]
mod metrics_config_tests {
    use super::*;

    #[test]
    fn from_env_defaults_when_unset() {
        for k in ["METRICS_ENABLED", "METRICS_BIND"] {
            unsafe { std::env::remove_var(k) };
        }
        let cfg = MetricsConfig::from_env().unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.bind, DEFAULT_METRICS_BIND);
    }

    #[test]
    fn from_env_reads_overrides() {
        unsafe {
            std::env::set_var("METRICS_ENABLED", "false");
            std::env::set_var("METRICS_BIND", "0.0.0.0:9100");
        }
        let cfg = MetricsConfig::from_env().unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bind, "0.0.0.0:9100");
    }

    #[test]
    fn from_env_rejects_unparseable_enabled() {
        unsafe { std::env::set_var("METRICS_ENABLED", "yes") };
        let err = MetricsConfig::from_env().unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { key, .. } if key == "METRICS_ENABLED"));
    }
}

#[cfg(test)]
mod timing_helper_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The helpers must return the wrapped value untouched and run the body
    /// exactly once -- recording is a side effect, never a filter.
    #[test]
    fn timed_returns_value_and_runs_body_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let out = timed("test_sync_seconds", || {
            c.fetch_add(1, Ordering::SeqCst);
            41 + 1
        });
        assert_eq!(out, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn timed_with_returns_value() {
        let out = timed_with("test_sync_labelled_seconds", &[("stage", "diff")], || "ok");
        assert_eq!(out, "ok");
    }

    #[tokio::test]
    async fn timed_async_returns_value_and_runs_body_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let out = timed_async("test_async_seconds", async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(7)
        })
        .await;
        assert_eq!(out, Ok(7));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn timed_async_with_returns_value() {
        let out = timed_async_with(
            "test_async_labelled_seconds",
            &[("repo", "cluster"), ("op", "find_active")],
            async { 3 },
        )
        .await;
        assert_eq!(out, 3);
    }
}
