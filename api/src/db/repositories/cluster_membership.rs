use super::{MEMPOOL_DELTA_INSERT_CHUNK_SIZE, RepoResult, TRANSACTION_INSERT_CHUNK_SIZE};
use crate::db::instrument::query;
use crate::db::models::{
    Cluster, ClusterChunkRow, ClusterRow, ClusterStatus, NewCluster, NewClusterChunk,
    NewClusterRow, NewTransaction,
};
use crate::db::pool::DbPool;
use crate::db::schema::{cluster_chunks, clusters, transactions};
use diesel::dsl::now;
use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use shared::models::ClusterChunk;
use std::collections::HashMap;
use time::OffsetDateTime;

pub struct ClusterVersionUpdate<'a> {
    pub cluster_id: i64,
    pub chunks: &'a [ClusterChunk],
}

const REPO_LABEL: &str = "cluster_membership";

/// Postgres caps a statement at 65535 bind params; `NewClusterChunk` has 7 columns.
const CLUSTER_CHUNK_INSERT_CHUNK_SIZE: usize = 9_000;

/// The two ways a cluster's active life can end without being mined.
#[derive(Clone, Copy)]
enum ClusterClosing {
    /// Every member of the cluster has left the mempool without being confirmed.
    Evicted,
    /// This cluster has been merged into another cluster.
    Merged,
}

impl From<ClusterClosing> for ClusterStatus {
    fn from(closing: ClusterClosing) -> Self {
        match closing {
            ClusterClosing::Evicted => ClusterStatus::Evicted,
            ClusterClosing::Merged => ClusterStatus::Merged,
        }
    }
}

#[derive(QueryableByName)]
struct AllocatedId {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

#[derive(Clone)]
pub struct ClusterMembershipRepository {
    pool: DbPool,
}

impl ClusterMembershipRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Inserts a brand-new cluster as its version 1 and links its member txs.
    pub async fn insert_with_members(&self, new: &NewCluster) -> RepoResult<Cluster> {
        query(
            &self.pool,
            REPO_LABEL,
            "insert_with_members",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                    let mut inserted = Self::insert_in(conn, std::slice::from_ref(new)).await?;
                    Ok(inserted.remove(0))
                })
                .await
            },
        )
        .await
    }

    /// Inserts a batch of brand-new clusters and links their member txs, one
    /// database transaction per chunk instead of one per cluster.
    pub async fn insert_many_with_members(&self, new: &[NewCluster]) -> RepoResult<Vec<Cluster>> {
        if new.is_empty() {
            return Ok(Vec::new());
        }

        query(
            &self.pool,
            REPO_LABEL,
            "insert_many_with_members",
            async |conn| {
                let mut inserted: Vec<Cluster> = Vec::with_capacity(new.len());
                for batch in new.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
                    let rows = conn
                        .transaction::<_, diesel::result::Error, _>(async |conn| {
                            Self::insert_in(conn, batch).await
                        })
                        .await?;
                    inserted.extend(rows);
                }
                Ok(inserted)
            },
        )
        .await
    }

    async fn insert_in(
        conn: &mut AsyncPgConnection,
        new: &[NewCluster],
    ) -> Result<Vec<Cluster>, diesel::result::Error> {
        // ids up front, so each chunk row knows its cluster without relying on
        // the order RETURNING hands rows back in
        let ids: Vec<i64> = diesel::sql_query(
            "SELECT nextval('clusters_id_seq') AS id FROM generate_series(1, $1)",
        )
        .bind::<BigInt, _>(new.len() as i64)
        .load::<AllocatedId>(conn)
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect();

        let cluster_rows: Vec<NewClusterRow> = ids
            .iter()
            .zip(new)
            .map(|(&id, cluster)| {
                let sums = totals(&cluster.chunks);
                NewClusterRow {
                    id,
                    total_weight: sums.weight,
                    total_fee: sums.fee,
                    first_seen_at: cluster.first_seen_at,
                }
            })
            .collect();
        let rows: Vec<ClusterRow> = diesel::insert_into(clusters::table)
            .values(&cluster_rows)
            .returning(ClusterRow::as_returning())
            .get_results(conn)
            .await?;

        let chunk_rows: Vec<NewClusterChunk> = ids
            .iter()
            .zip(new)
            .flat_map(|(&id, cluster)| chunk_rows(id, 1, &cluster.chunks, true))
            .collect();
        Self::insert_chunks(conn, &chunk_rows).await?;

        let members: Vec<String> = new
            .iter()
            .flat_map(|cluster| cluster.chunks.iter())
            .flat_map(|chunk| chunk.txids.iter().cloned())
            .collect();
        Self::insert_hollow_members(conn, &members).await?;

        diesel::sql_query(
            "UPDATE transactions t \
                SET cluster_id = ch.cluster_id \
               FROM (SELECT cluster_id, unnest(txids) AS txid \
                       FROM cluster_chunks WHERE cluster_id = ANY($1)) ch \
              WHERE t.txid = ch.txid",
        )
        .bind::<Array<BigInt>, _>(&ids)
        .execute(conn)
        .await?;

        let mut chunks_by_id: HashMap<i64, &[ClusterChunk]> = ids
            .iter()
            .zip(new)
            .map(|(&id, cluster)| (id, cluster.chunks.as_slice()))
            .collect();
        Ok(rows
            .into_iter()
            .map(|row| {
                let chunks = chunks_by_id
                    .remove(&row.id)
                    .expect("RETURNING hands back the ids allocated above")
                    .to_vec();
                Cluster::from_row(row, chunks)
            })
            .collect())
    }

    /// Records the cluster's chunks as a new version, detaches any tx still
    /// linked to the cluster that is no longer a member, and links the current
    /// members. A cluster that is no longer active, or whose chunks did not
    /// change, is returned as it is.
    pub async fn replace_chunks(&self, update: ClusterVersionUpdate<'_>) -> RepoResult<Cluster> {
        query(&self.pool, REPO_LABEL, "replace_chunks", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                Self::replace_chunks_in(conn, &update, true).await
            })
            .await
        })
        .await
    }

    async fn replace_chunks_in(
        conn: &mut AsyncPgConnection,
        update: &ClusterVersionUpdate<'_>,
        live: bool,
    ) -> Result<Cluster, diesel::result::Error> {
        let &ClusterVersionUpdate { cluster_id, chunks } = update;

        let old: ClusterRow = clusters::table
            .find(cluster_id)
            .select(ClusterRow::as_select())
            .for_update()
            .first(conn)
            .await?;
        let current: Vec<ClusterChunk> = cluster_chunks::table
            .filter(cluster_chunks::cluster_id.eq(cluster_id))
            .filter(cluster_chunks::version.eq(old.version))
            .order(cluster_chunks::position.asc())
            .select(ClusterChunkRow::as_select())
            .load(conn)
            .await?
            .into_iter()
            .map(ClusterChunk::from)
            .collect();
        if old.status != ClusterStatus::Active || current == chunks {
            return Ok(Cluster::from_row(old, current));
        }

        let version = old.version + 1;
        diesel::update(
            cluster_chunks::table
                .filter(cluster_chunks::cluster_id.eq(cluster_id))
                .filter(cluster_chunks::version.eq(old.version)),
        )
        .set(cluster_chunks::live.eq(false))
        .execute(conn)
        .await?;
        Self::insert_chunks(conn, &chunk_rows(cluster_id, version, chunks, live)).await?;

        let sums = totals(chunks);
        let row = diesel::update(clusters::table.find(cluster_id))
            .set((
                clusters::version.eq(version),
                clusters::total_weight.eq(sums.weight),
                clusters::total_fee.eq(sums.fee),
            ))
            .returning(ClusterRow::as_returning())
            .get_result(conn)
            .await?;
        let cluster = Cluster::from_row(row, chunks.to_vec());

        let members: Vec<String> = cluster.txids().cloned().collect();

        // detach txs that are no longer members of the cluster
        diesel::update(transactions::table)
            .filter(transactions::cluster_id.eq(cluster_id))
            .filter(transactions::txid.ne_all(&members))
            .set(transactions::cluster_id.eq(None::<i64>))
            .execute(conn)
            .await?;

        Self::insert_hollow_members(conn, &members).await?;

        // attach current members to the cluster
        diesel::update(transactions::table)
            .filter(transactions::txid.eq_any(&members))
            .filter(transactions::cluster_id.is_distinct_from(cluster_id))
            .set(transactions::cluster_id.eq(cluster_id))
            .execute(conn)
            .await?;

        Ok(cluster)
    }

    pub async fn mark_evicted(&self, ids: &[i64]) -> RepoResult<usize> {
        self.close_many(ids, ClusterClosing::Evicted).await
    }

    pub async fn mark_merged(&self, ids: &[i64]) -> RepoResult<usize> {
        self.close_many(ids, ClusterClosing::Merged).await
    }

    /// Ends the active life of clusters that merged away or lost their members.
    async fn close_many(&self, ids: &[i64], closing: ClusterClosing) -> RepoResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let status = ClusterStatus::from(closing);

        query(&self.pool, REPO_LABEL, "close_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                // id order, so two writers locking overlapping sets cannot deadlock
                let closing: Vec<i64> = clusters::table
                    .filter(clusters::id.eq_any(ids))
                    .filter(clusters::status.eq(ClusterStatus::Active))
                    .order(clusters::id.asc())
                    .select(clusters::id)
                    .for_update()
                    .load(conn)
                    .await?;
                if closing.is_empty() {
                    return Ok(0);
                }

                diesel::update(clusters::table.filter(clusters::id.eq_any(&closing)))
                    .set((clusters::status.eq(status), clusters::closed_at.eq(now)))
                    .execute(conn)
                    .await?;
                Self::retire_chunks(conn, &closing).await?;
                diesel::update(transactions::table)
                    .filter(transactions::cluster_id.eq_any(&closing))
                    .set(transactions::cluster_id.eq(None::<i64>))
                    .execute(conn)
                    .await?;

                Ok(closing.len())
            })
            .await
        })
        .await
    }

    /// Marks the still-active clusters among `ids` as confirmed, in one
    /// transaction: all of them or none. The rows keep their current version so
    /// confirmed member txs stay linked. Returns how many were newly confirmed.
    pub async fn confirm_many(
        &self,
        ids: &[i64],
        confirmed_at: OffsetDateTime,
    ) -> RepoResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        query(&self.pool, REPO_LABEL, "confirm_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                Self::confirm_many_in(conn, ids, confirmed_at).await
            })
            .await
        })
        .await
    }

    async fn confirm_many_in(
        conn: &mut AsyncPgConnection,
        ids: &[i64],
        confirmed_at: OffsetDateTime,
    ) -> Result<usize, diesel::result::Error> {
        // id order, so two writers locking overlapping sets cannot deadlock
        let pending: Vec<i64> = clusters::table
            .filter(clusters::id.eq_any(ids))
            .filter(clusters::status.eq(ClusterStatus::Active))
            .order(clusters::id.asc())
            .select(clusters::id)
            .for_update()
            .load(conn)
            .await?;
        if pending.is_empty() {
            return Ok(0);
        }

        diesel::update(clusters::table.filter(clusters::id.eq_any(&pending)))
            .set((
                clusters::confirmed_at.eq(confirmed_at),
                clusters::status.eq(ClusterStatus::Confirmed),
                clusters::closed_at.eq(now),
            ))
            .execute(conn)
            .await?;
        Self::retire_chunks(conn, &pending).await?;

        Ok(pending.len())
    }

    /// [`Self::replace_chunks`] on each cluster, then [`Self::confirm_many`]
    /// on all of them, in one transaction: all of them or none. The new
    /// versions are born closed, so their chunks are never live. Returns how
    /// many were newly confirmed.
    pub async fn trim_and_confirm_many(
        &self,
        updates: &[ClusterVersionUpdate<'_>],
        confirmed_at: OffsetDateTime,
    ) -> RepoResult<usize> {
        if updates.is_empty() {
            return Ok(0);
        }
        // id order, so two writers locking overlapping sets cannot deadlock
        let mut updates: Vec<&ClusterVersionUpdate<'_>> = updates.iter().collect();
        updates.sort_by_key(|update| update.cluster_id);
        let ids: Vec<i64> = updates.iter().map(|update| update.cluster_id).collect();

        query(
            &self.pool,
            REPO_LABEL,
            "trim_and_confirm_many",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                    for update in &updates {
                        Self::replace_chunks_in(conn, update, false).await?;
                    }
                    Self::confirm_many_in(conn, &ids, confirmed_at).await
                })
                .await
            },
        )
        .await
    }

    /// Takes the chunks of closing clusters out of the by-txid lookup (live=false).
    async fn retire_chunks(
        conn: &mut AsyncPgConnection,
        cluster_ids: &[i64],
    ) -> Result<(), diesel::result::Error> {
        diesel::update(
            cluster_chunks::table
                .filter(cluster_chunks::cluster_id.eq_any(cluster_ids))
                .filter(cluster_chunks::live),
        )
        .set(cluster_chunks::live.eq(false))
        .execute(conn)
        .await?;
        Ok(())
    }

    async fn insert_chunks(
        conn: &mut AsyncPgConnection,
        rows: &[NewClusterChunk],
    ) -> Result<(), diesel::result::Error> {
        for batch in rows.chunks(CLUSTER_CHUNK_INSERT_CHUNK_SIZE) {
            diesel::insert_into(cluster_chunks::table)
                .values(batch)
                .execute(conn)
                .await?;
        }
        Ok(())
    }

    async fn insert_hollow_members(
        conn: &mut AsyncPgConnection,
        txids: &[String],
    ) -> Result<(), diesel::result::Error> {
        if txids.is_empty() {
            return Ok(());
        }

        let rows: Vec<NewTransaction> = txids
            .iter()
            .map(|txid| NewTransaction::hollow(txid))
            .collect();
        let rows = NewTransaction::sorted_by_txid(&rows);
        for chunk in rows.chunks(TRANSACTION_INSERT_CHUNK_SIZE) {
            diesel::insert_into(transactions::table)
                .values(chunk.to_vec())
                .on_conflict(transactions::txid)
                .do_nothing()
                .execute(conn)
                .await?;
        }
        Ok(())
    }
}

struct ChunkSums {
    weight: i64,
    fee: i64,
}

fn totals(chunks: &[ClusterChunk]) -> ChunkSums {
    ChunkSums {
        weight: chunks.iter().map(|chunk| chunk.weight as i64).sum(),
        fee: chunks.iter().map(|chunk| chunk.fee_sats as i64).sum(),
    }
}

fn chunk_rows(
    cluster_id: i64,
    version: i32,
    chunks: &[ClusterChunk],
    live: bool,
) -> Vec<NewClusterChunk> {
    chunks
        .iter()
        .enumerate()
        .map(|(position, chunk)| NewClusterChunk {
            cluster_id,
            version,
            position: position as i16,
            fee: chunk.fee_sats as i64,
            weight: chunk.weight as i64,
            txids: chunk.txids.clone(),
            live,
        })
        .collect()
}
