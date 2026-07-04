// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "delta_reason"))]
    pub struct DeltaReason;
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
    }
}

diesel::table! {
    clusters (id) {
        id -> Int8,
        txids -> Array<Text>,
        total_fee -> Int8,
        first_seen_at -> Nullable<Timestamptz>,
        confirmed_at -> Nullable<Timestamptz>,
        total_vsize -> Int8,
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
    transactions (txid) {
        txid -> Text,
        fee -> Nullable<Int8>,
        vsize -> Int8,
        first_seen_at -> Timestamptz,
        confirmed_at -> Nullable<Timestamptz>,
        cluster_id -> Nullable<Int8>,
        left_mempool_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(transactions -> clusters (cluster_id));

diesel::allow_tables_to_appear_in_same_query!(blocks, clusters, mempool_deltas, transactions,);
