use std::future::Future;
use std::time::Duration;

/// How often `wait_for` re-checks its condition.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `condition` until it yields `Some`, or gives up after `timeout`.
pub async fn wait_for<T, F, Fut>(timeout: Duration, mut condition: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(value) = condition().await {
            return Some(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
