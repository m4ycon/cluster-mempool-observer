use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{Cluster, ClusterChunkRow, ClusterRow, ClusterStatus};
use crate::db::pool::DbPool;
use crate::db::schema::{cluster_chunks, clusters};
use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt, Text};
use diesel_async::{AsyncPgConnection, RunQueryDsl};

const REPO_LABEL: &str = "cluster";

/// Past this size the planner trades the GIN index for a per-row `&&` filter,
/// which the join form beats.
pub const ACTIVE_IDS_JOIN_MIN_TXIDS: usize = 64;

#[derive(QueryableByName)]
struct ClusterId {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

#[derive(Clone)]
pub struct ClusterRepository {
    pool: DbPool,
}

impl ClusterRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// The cluster whose current version holds `txid`: the active one, else the
    /// one that closed last. Closed clusters keep their last version, so several
    /// can match.
    pub async fn find_by_txid(&self, txid: &str) -> RepoResult<Option<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_by_txid", async |conn| {
            let id: Option<i64> = clusters::table
                .inner_join(
                    cluster_chunks::table.on(cluster_chunks::cluster_id
                        .eq(clusters::id)
                        .and(cluster_chunks::version.eq(clusters::version))),
                )
                .filter(cluster_chunks::txids.contains(vec![txid]))
                .select(clusters::id)
                .order((
                    cluster_chunks::live.desc(),
                    clusters::closed_at.desc().nulls_last(),
                    clusters::id.desc(),
                ))
                .first(conn)
                .await
                .optional()?;
            let Some(id) = id else {
                return Ok(None);
            };
            Ok(load_clusters(conn, ClusterFilter::Ids(&[id])).await?.pop())
        })
        .await
    }

    pub async fn find_by_ids(&self, ids: &[i64]) -> RepoResult<Vec<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_by_ids", async |conn| {
            load_clusters(conn, ClusterFilter::Ids(ids)).await
        })
        .await
    }

    pub async fn find_active(&self) -> RepoResult<Vec<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_active", async |conn| {
            load_clusters(conn, ClusterFilter::Active).await
        })
        .await
    }

    pub async fn count(&self) -> RepoResult<i64> {
        query(&self.pool, REPO_LABEL, "count", async |conn| {
            clusters::table.count().get_result(conn).await
        })
        .await
    }

    pub async fn find_active_ids_by_txids(&self, txids: &[String]) -> RepoResult<Vec<i64>> {
        if txids.is_empty() {
            return Ok(Vec::new());
        }
        query(
            &self.pool,
            REPO_LABEL,
            "find_active_ids_by_txids",
            async |conn| {
                if txids.len() < ACTIVE_IDS_JOIN_MIN_TXIDS {
                    return cluster_chunks::table
                        .filter(cluster_chunks::txids.overlaps_with(txids))
                        .filter(cluster_chunks::live)
                        .select(cluster_chunks::cluster_id)
                        .distinct()
                        .load(conn)
                        .await;
                }

                // Reaches the live chunks through the active clusters: no index
                // covers a plain `live` filter, which would read the whole history.
                let rows: Vec<ClusterId> = diesel::sql_query(
                    "SELECT DISTINCT c.id \
                       FROM clusters c \
                       JOIN cluster_chunks ch \
                         ON ch.cluster_id = c.id AND ch.version = c.version \
                      CROSS JOIN LATERAL unnest(ch.txids) AS m(txid) \
                       JOIN unnest($1) AS b(txid) ON b.txid = m.txid \
                      WHERE c.status = 'active'",
                )
                .bind::<Array<Text>, _>(txids)
                .load(conn)
                .await?;
                Ok(rows.into_iter().map(|row| row.id).collect())
            },
        )
        .await
    }
}

pub(super) enum ClusterFilter<'a> {
    Ids(&'a [i64]),
    Active,
}

/// Loads clusters with the chunks of their current version, in one statement so
/// the row and its chunks come from the same snapshot.
pub(super) async fn load_clusters(
    conn: &mut AsyncPgConnection,
    filter: ClusterFilter<'_>,
) -> QueryResult<Vec<Cluster>> {
    let mut statement = clusters::table
        .left_join(
            cluster_chunks::table.on(cluster_chunks::cluster_id
                .eq(clusters::id)
                .and(cluster_chunks::version.eq(clusters::version))),
        )
        .select((
            ClusterRow::as_select(),
            Option::<ClusterChunkRow>::as_select(),
        ))
        .order((clusters::id.asc(), cluster_chunks::position.asc()))
        .into_boxed();
    statement = match filter {
        ClusterFilter::Ids(ids) => statement.filter(clusters::id.eq_any(ids)),
        ClusterFilter::Active => statement.filter(clusters::status.eq(ClusterStatus::Active)),
    };
    let rows: Vec<(ClusterRow, Option<ClusterChunkRow>)> = statement.load(conn).await?;

    let mut out: Vec<Cluster> = Vec::new();
    for (row, chunk) in rows {
        if out.last().is_none_or(|last| last.id != row.id) {
            out.push(Cluster::from_row(row, Vec::new()));
        }
        if let (Some(chunk), Some(last)) = (chunk, out.last_mut()) {
            last.chunks.push(chunk.into());
        }
    }
    Ok(out)
}
