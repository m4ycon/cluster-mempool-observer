use super::{NATIVE_RESOLUTION_SECS, RepoResult};
use crate::db::instrument::query;
use crate::db::models::{MempoolCounterSampleRow, NewMempoolCounterSampleRow};
use crate::db::pool::DbPool;
use crate::db::schema::mempool_counter_samples;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Timestamptz};
use diesel_async::RunQueryDsl;
use time::OffsetDateTime;

const REPO_LABEL: &str = "counter_sample";

#[derive(Clone)]
pub struct CounterSampleRepository {
    pool: DbPool,
}

impl CounterSampleRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, sample: &NewMempoolCounterSampleRow) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(mempool_counter_samples::table)
                .values(sample)
                .on_conflict(mempool_counter_samples::sampled_at)
                .do_nothing()
                .execute(conn)
                .await
        })
        .await
    }

    /// Points in `[from, to]` ascending, at `resolution_secs` granularity.
    pub async fn range(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        resolution_secs: i64,
    ) -> RepoResult<Vec<MempoolCounterSampleRow>> {
        if resolution_secs == NATIVE_RESOLUTION_SECS {
            self.range_native(from, to).await
        } else {
            self.range_bucketed(from, to, resolution_secs).await
        }
    }

    async fn range_native(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> RepoResult<Vec<MempoolCounterSampleRow>> {
        query(&self.pool, REPO_LABEL, "range_native", async |conn| {
            mempool_counter_samples::table
                .filter(mempool_counter_samples::sampled_at.ge(from))
                .filter(mempool_counter_samples::sampled_at.le(to))
                .order(mempool_counter_samples::sampled_at.asc())
                .select(MempoolCounterSampleRow::as_select())
                .load(conn)
                .await
        })
        .await
    }

    async fn range_bucketed(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        resolution_secs: i64,
    ) -> RepoResult<Vec<MempoolCounterSampleRow>> {
        // Counts must SUM across a bucket, not keep the last row like the gauge
        // table's DISTINCT ON read does -- an instantaneous reading and a count
        // of events are aggregated differently, which is the whole reason
        // counters got their own table.
        let sql = "
            WITH bucketed AS (
                SELECT
                    to_timestamp(floor(extract(epoch FROM sampled_at) / $1::float8) * $1::float8) AS bucket,
                    sampled_at, added_txs, confirmed_txs, evicted_txs
                FROM mempool_counter_samples
                WHERE sampled_at >= $2 AND sampled_at <= $3
            )
            SELECT
                max(sampled_at) AS sampled_at,
                $1::bigint AS period_secs,
                CASE WHEN bool_or(added_txs IS NULL OR confirmed_txs IS NULL OR evicted_txs IS NULL)
                     THEN NULL ELSE sum(added_txs)::bigint END AS added_txs,
                CASE WHEN bool_or(added_txs IS NULL OR confirmed_txs IS NULL OR evicted_txs IS NULL)
                     THEN NULL ELSE sum(confirmed_txs)::bigint END AS confirmed_txs,
                CASE WHEN bool_or(added_txs IS NULL OR confirmed_txs IS NULL OR evicted_txs IS NULL)
                     THEN NULL ELSE sum(evicted_txs)::bigint END AS evicted_txs
            FROM bucketed
            GROUP BY bucket
            ORDER BY bucket ASC
        ";

        query(&self.pool, REPO_LABEL, "range_bucketed", async |conn| {
            diesel::sql_query(sql)
                .bind::<BigInt, _>(resolution_secs)
                .bind::<Timestamptz, _>(from)
                .bind::<Timestamptz, _>(to)
                .load::<MempoolCounterSampleRow>(conn)
                .await
        })
        .await
    }

    /// The sampler's cursor: the end of the last window it already covered.
    pub async fn latest_sampled_at(&self) -> RepoResult<Option<OffsetDateTime>> {
        query(&self.pool, REPO_LABEL, "latest_sampled_at", async |conn| {
            mempool_counter_samples::table
                .order(mempool_counter_samples::sampled_at.desc())
                .select(mempool_counter_samples::sampled_at)
                .first(conn)
                .await
                .optional()
        })
        .await
    }
}
