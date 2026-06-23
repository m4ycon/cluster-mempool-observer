// @generated automatically by Diesel CLI.

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
        first_seen_at -> Nullable<Timestamptz>,
        confirmed_at -> Nullable<Timestamptz>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(mempool_deltas, transactions,);
