use crate::db::models::{MempoolCounterSampleRow, NewMempoolCounterSampleRow, SystemEventKind};
use crate::db::repositories::RepoResult;
use crate::db::{CounterSampleRepository, MempoolDeltaRepository, SystemEventRepository};
use crate::error::ApiError;
use crate::infra::block_gate::MAX_HOLD;
use crate::services::resolution::{DEFAULT_RANGE, pick_resolution};
use shared::api::{CounterPoint, CounterSeries};
use std::time::Duration as StdDuration;
use time::{Duration, OffsetDateTime};
use tokio::time::MissedTickBehavior;

/// A row carries the time its entry was observed, but `BlockGate` can hold the
/// flush for up to `MAX_HOLD`. A tx seen at 12:00 and flushed at 12:03 lands
/// stamped 12:00, after the window covering it was counted and the cursor moved
/// past, so nothing ever counts it. Derived so it cannot fall behind that hold.
const SAFETY_MARGIN: Duration = Duration::seconds(MAX_HOLD.as_secs() as i64 + 60);

#[derive(Clone)]
pub struct CounterSampleService {
    counter_sample_repository: CounterSampleRepository,
    mempool_delta_repository: MempoolDeltaRepository,
    system_event_repository: SystemEventRepository,
}

impl CounterSampleService {
    pub fn new(
        counter_sample_repository: CounterSampleRepository,
        mempool_delta_repository: MempoolDeltaRepository,
        system_event_repository: SystemEventRepository,
    ) -> Self {
        Self {
            counter_sample_repository,
            mempool_delta_repository,
            system_event_repository,
        }
    }

    /// Samples forever, once per `interval`.
    pub async fn run(&self, interval: StdDuration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            self.sample(interval).await;
        }
    }

    /// Samples once, inserting a new row into `mempool_counter_samples`.
    pub async fn sample(&self, interval: StdDuration) {
        let now = OffsetDateTime::now_utc();
        let window_end = now - SAFETY_MARGIN;

        let window_start = match self.counter_sample_repository.latest_sampled_at().await {
            Ok(cursor) => cursor,
            Err(e) => {
                tracing::warn!("counter sample: failed to read cursor: {e}");
                return;
            }
        };

        let Some(window_start) = window_start else {
            // No prior cursor: this row only establishes one for the next tick,
            // since there is no known window start to count from.
            let row = NewMempoolCounterSampleRow {
                sampled_at: window_end,
                period_secs: interval.as_secs() as i64,
                added_txs: None,
                confirmed_txs: None,
                evicted_txs: None,
            };
            if let Err(e) = self.counter_sample_repository.insert(&row).await {
                tracing::warn!("counter sample: failed to insert first sample: {e}");
            }
            return;
        };

        // Clock skew, or a tick firing before the previous window closed.
        if window_end <= window_start {
            return;
        }

        if let Err(e) = self.sample_window(window_start, window_end).await {
            tracing::warn!("counter sample: failed to insert sample: {e}");
        }
    }

    /// All three series, aligned on one set of buckets so a client's single
    /// request cannot get them out of sync with each other.
    pub async fn series(
        &self,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<CounterSeries, ApiError> {
        let (resolution_secs, rows) = self.fetch_range(from, to).await?;
        Ok(CounterSeries {
            resolution_secs,
            points: rows
                .into_iter()
                .map(|row| CounterPoint {
                    sampled_at: row.sampled_at,
                    added_txs: row.added_txs,
                    confirmed_txs: row.confirmed_txs,
                    evicted_txs: row.evicted_txs,
                })
                .collect(),
        })
    }

    async fn fetch_range(
        &self,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<(i64, Vec<MempoolCounterSampleRow>), ApiError> {
        let to = to.unwrap_or_else(OffsetDateTime::now_utc);
        let from = from.unwrap_or(to - DEFAULT_RANGE);
        if from >= to {
            return Err(ApiError::BadRequest(
                "`from` must be earlier than `to`".to_string(),
            ));
        }

        let resolution_secs = pick_resolution(to - from);
        let rows = self
            .counter_sample_repository
            .range(from, to, resolution_secs)
            .await?;
        Ok((resolution_secs, rows))
    }

    /// Writes one row for `(window_start, window_end]`, skipping the count if a
    /// restart landed in the window. `window_end > window_start` is the
    /// caller's responsibility.
    async fn sample_window(
        &self,
        window_start: OffsetDateTime,
        window_end: OffsetDateTime,
    ) -> RepoResult<()> {
        let period_secs = (window_end - window_start).whole_seconds();

        let restarted = self
            .system_event_repository
            .exists_of_kind_in(SystemEventKind::ServerStarted, window_start, window_end)
            .await?;

        let row = if restarted {
            NewMempoolCounterSampleRow {
                sampled_at: window_end,
                period_secs,
                added_txs: None,
                confirmed_txs: None,
                evicted_txs: None,
            }
        } else {
            let (added_txs, confirmed_txs, evicted_txs) = self
                .mempool_delta_repository
                .count_deltas_in(window_start, window_end)
                .await?;
            NewMempoolCounterSampleRow {
                sampled_at: window_end,
                period_secs,
                added_txs: Some(added_txs),
                confirmed_txs: Some(confirmed_txs),
                evicted_txs: Some(evicted_txs),
            }
        };

        self.counter_sample_repository.insert(&row).await?;
        Ok(())
    }
}
