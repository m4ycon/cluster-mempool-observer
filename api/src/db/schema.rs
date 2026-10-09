// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "cluster_status"))]
    pub struct ClusterStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "delta_reason"))]
    pub struct DeltaReason;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "system_event_kind"))]
    pub struct SystemEventKind;
}

diesel::table! {
    blocks (hash) {
        hash -> Text,
        height -> Int8,
        mined_at -> Timestamptz,
        tx_count -> Int8,
        total_bytes -> Int8,
        total_fee -> Int8,
        difficulty -> Float8,
        created_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    cluster_chunks (cluster_id, version, position) {
        cluster_id -> Int8,
        version -> Int4,
        position -> Int2,
        fee -> Int8,
        weight -> Int8,
        txids -> Array<Text>,
        live -> Bool,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::ClusterStatus;

    clusters (id) {
        id -> Int8,
        total_fee -> Int8,
        first_seen_at -> Timestamptz,
        confirmed_at -> Nullable<Timestamptz>,
        total_weight -> Int8,
        status -> ClusterStatus,
        version -> Int4,
        closed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    mempool_counter_samples (sampled_at) {
        sampled_at -> Timestamptz,
        period_secs -> Int8,
        added_txs -> Nullable<Int8>,
        confirmed_txs -> Nullable<Int8>,
        evicted_txs -> Nullable<Int8>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::DeltaReason;

    mempool_deltas (id) {
        id -> Int8,
        txid -> Text,
        reason -> DeltaReason,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    mempool_gauge_samples (sampled_at) {
        sampled_at -> Timestamptz,
        cluster_count -> Int4,
        clustered_tx_count -> Int4,
        mempool_tx_count -> Int4,
        total_weight -> Int8,
        total_fee -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::SystemEventKind;

    system_events (id) {
        id -> Int8,
        kind -> SystemEventKind,
        details -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    transactions (txid) {
        txid -> Text,
        fee -> Nullable<Int8>,
        weight -> Int8,
        first_seen_at -> Timestamptz,
        confirmed_at -> Nullable<Timestamptz>,
        cluster_id -> Nullable<Int8>,
        confirmed_at_block -> Nullable<Text>,
        input_txids -> Nullable<Array<Text>>,
    }
}

diesel::joinable!(cluster_chunks -> clusters (cluster_id));
diesel::joinable!(transactions -> blocks (confirmed_at_block));
diesel::joinable!(transactions -> clusters (cluster_id));

diesel::allow_tables_to_appear_in_same_query!(
    blocks,
    cluster_chunks,
    clusters,
    mempool_counter_samples,
    mempool_deltas,
    mempool_gauge_samples,
    system_events,
    transactions,
);
