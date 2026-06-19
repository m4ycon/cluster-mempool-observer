CREATE TABLE transactions (
    txid          TEXT        PRIMARY KEY,
    version       INTEGER     NOT NULL,
    lock_time     BIGINT      NOT NULL,
    vsize         BIGINT      NOT NULL,
    weight        BIGINT      NOT NULL,
    input_count   BIGINT      NOT NULL,
    input_txids   TEXT[]      NOT NULL,
    output_count  BIGINT      NOT NULL,
    confirmations BIGINT      NOT NULL,
    time          TIMESTAMPTZ
);

CREATE TABLE mempool_deltas (
    id          BIGSERIAL   PRIMARY KEY,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    added       TEXT[]      NOT NULL,
    removed     TEXT[]      NOT NULL
);
