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
        version -> Int4,
        lock_time -> Int8,
        vsize -> Int8,
        weight -> Int8,
        input_count -> Int8,
        input_txids -> Array<Text>,
        output_count -> Int8,
        confirmations -> Int8,
        time -> Nullable<Timestamptz>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(mempool_deltas, transactions,);
