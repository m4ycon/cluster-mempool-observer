use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle, PrometheusRecorder};
use shared::metrics::LATENCY_BUCKETS_SECONDS;
use std::future::Future;

/// Runs `f` against a recorder scoped to the calling thread and returns the
/// rendered scrape payload.
///
/// The global recorder can only be installed once per process, which would
/// force one metric assertion per test binary. A thread-local recorder lets
/// each test own its own, so metrics can be asserted individually.
pub fn capture<F: Future>(f: F) -> String {
    let (recorder, handle) = local_recorder();

    let guard = metrics::set_default_local_recorder(&recorder);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build test runtime")
        .block_on(f);
    drop(guard);

    handle.run_upkeep();
    handle.render()
}

/// A recorder plus its handle, for tests that are already inside a runtime and
/// so cannot use [`capture`], which builds one.
pub fn local_recorder() -> (PrometheusRecorder, PrometheusHandle) {
    let recorder = PrometheusBuilder::new()
        .set_buckets(LATENCY_BUCKETS_SECONDS)
        .expect("bucket ladder is non-empty")
        .build_recorder();
    let handle = recorder.handle();
    (recorder, handle)
}

/// Asserts `rendered` contains `series` as a whole line.
pub fn assert_series(rendered: &str, series: &str) {
    assert!(
        rendered.lines().any(|line| line == series),
        "expected series `{series}` in:\n{rendered}"
    );
}

/// Asserts no line in `rendered` starts with `prefix`.
pub fn assert_no_series(rendered: &str, prefix: &str) {
    assert!(
        !rendered.lines().any(|line| line.starts_with(prefix)),
        "expected no series starting `{prefix}` in:\n{rendered}"
    );
}
