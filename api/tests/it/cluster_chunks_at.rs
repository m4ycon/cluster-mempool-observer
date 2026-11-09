#![cfg(feature = "db_integration_tests")]

use api::db::models::{ChunkFeeWeightRow, NewCluster};
use api::db::schema::{cluster_chunks, clusters};
use api::db::{ClusterVersionUpdate, DbPool, Repos};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::models::ClusterChunk;
use testkit::fixtures::fixed_time;
use testkit::postgres::isolated_pool;
use time::{Duration, OffsetDateTime};

fn chunk(txids: &[&str], fee_sats: u64, weight: u64) -> ClusterChunk {
    ClusterChunk {
        txids: txids.iter().map(|txid| txid.to_string()).collect(),
        fee_sats,
        weight,
    }
}

fn row(fee: i64, weight: i64) -> ChunkFeeWeightRow {
    ChunkFeeWeightRow { fee, weight }
}

fn secs(n: i64) -> OffsetDateTime {
    fixed_time() + Duration::seconds(n)
}

async fn insert(repos: &Repos, chunks: Vec<ClusterChunk>) -> i64 {
    repos
        .cluster_membership
        .insert_with_members(&NewCluster {
            chunks,
            first_seen_at: secs(-3600),
        })
        .await
        .expect("insert")
        .id
}

async fn replace(repos: &Repos, cluster_id: i64, chunks: &[ClusterChunk]) {
    repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate { cluster_id, chunks })
        .await
        .expect("new version");
}

// Pinned, so the order does not hang on the local clock.
async fn set_version_created_at(pool: &DbPool, cluster_id: i64, version: i32, at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    diesel::update(
        cluster_chunks::table
            .filter(cluster_chunks::cluster_id.eq(cluster_id))
            .filter(cluster_chunks::version.eq(version)),
    )
    .set(cluster_chunks::created_at.eq(at))
    .execute(&mut conn)
    .await
    .expect("set created_at");
}

async fn set_closed_at(pool: &DbPool, cluster_id: i64, closed_at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    diesel::update(clusters::table.find(cluster_id))
        .set(clusters::closed_at.eq(closed_at))
        .execute(&mut conn)
        .await
        .expect("set closed_at");
}

async fn chunks_at(repos: &Repos, at: OffsetDateTime) -> Vec<ChunkFeeWeightRow> {
    let mut rows = repos
        .cluster
        .find_active_chunks_at(at)
        .await
        .expect("query");
    rows.sort_by_key(|row| (row.fee, row.weight));
    rows
}

#[tokio::test]
async fn a_version_written_after_the_instant_does_not_count() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let id = insert(
        &repos,
        vec![chunk(&["a"], 300, 400), chunk(&["b"], 100, 800)],
    )
    .await;
    replace(
        &repos,
        id,
        &[chunk(&["a"], 300, 400), chunk(&["b"], 900, 800)],
    )
    .await;
    set_version_created_at(&pool, id, 1, secs(-60)).await;
    set_version_created_at(&pool, id, 2, secs(10)).await;

    assert_eq!(
        chunks_at(&repos, secs(0)).await,
        vec![row(100, 800), row(300, 400)]
    );
    assert_eq!(
        chunks_at(&repos, secs(10)).await,
        vec![row(300, 400), row(900, 800)],
        "a version written exactly at the instant already holds"
    );
}

#[tokio::test]
async fn a_cluster_closed_by_the_instant_is_gone() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let before = insert(&repos, vec![chunk(&["a"], 100, 400)]).await;
    let exactly = insert(&repos, vec![chunk(&["b"], 200, 400)]).await;
    repos
        .cluster_membership
        .mark_evicted(&[before, exactly])
        .await
        .expect("close");
    for id in [before, exactly] {
        set_version_created_at(&pool, id, 1, secs(-60)).await;
    }
    set_closed_at(&pool, before, secs(-10)).await;
    set_closed_at(&pool, exactly, secs(0)).await;

    assert_eq!(chunks_at(&repos, secs(0)).await, vec![]);
}

#[tokio::test]
async fn a_cluster_closed_after_the_instant_counts_at_the_version_then_current() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let id = insert(&repos, vec![chunk(&["a"], 100, 400)]).await;
    replace(
        &repos,
        id,
        &[chunk(&["a"], 100, 400), chunk(&["b"], 200, 400)],
    )
    .await;
    replace(&repos, id, &[chunk(&["a"], 100, 400)]).await;
    repos
        .cluster_membership
        .confirm_many(&[id], secs(0))
        .await
        .expect("confirm");
    set_version_created_at(&pool, id, 1, secs(-60)).await;
    set_version_created_at(&pool, id, 2, secs(-30)).await;
    set_version_created_at(&pool, id, 3, secs(20)).await;
    set_closed_at(&pool, id, secs(20)).await;

    assert_eq!(
        chunks_at(&repos, secs(0)).await,
        vec![row(100, 400), row(200, 400)]
    );
}

#[tokio::test]
async fn a_merge_swaps_both_clusters_for_the_keepers_new_version() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let keeper = insert(&repos, vec![chunk(&["a"], 100, 400)]).await;
    let loser = insert(&repos, vec![chunk(&["b"], 200, 400)]).await;
    repos
        .cluster_membership
        .mark_merged(&[loser])
        .await
        .expect("merge");
    replace(&repos, keeper, &[chunk(&["a", "b"], 300, 800)]).await;
    set_version_created_at(&pool, keeper, 1, secs(-60)).await;
    set_version_created_at(&pool, loser, 1, secs(-60)).await;
    set_version_created_at(&pool, keeper, 2, secs(0)).await;
    set_closed_at(&pool, loser, secs(0)).await;

    let before = chunks_at(&repos, secs(-1)).await;
    let after = chunks_at(&repos, secs(0)).await;

    assert_eq!(before, vec![row(100, 400), row(200, 400)]);
    assert_eq!(after, vec![row(300, 800)]);
}
