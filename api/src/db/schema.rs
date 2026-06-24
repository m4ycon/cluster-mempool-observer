// @generated automatically by Diesel CLI.

diesel::table! {
    clusters (id) {
        id -> Int8,
        txids -> Array<Text>,
        total_fee -> Int8,
        first_seen_at -> Nullable<Timestamptz>,
        confirmed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    mempool_deltas (id) {
        id -> Int8,
        observed_at -> Timestamptz,
        added -> Array<Text>,
        removed -> Array<Text>,
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
    }
}

diesel::joinable!(transactions -> clusters (cluster_id));

diesel::allow_tables_to_appear_in_same_query!(clusters, mempool_deltas, transactions,);
