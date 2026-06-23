CREATE TABLE transactions (
    txid          TEXT        PRIMARY KEY,
    fee           BIGINT,
    vsize         BIGINT      NOT NULL,
    first_seen_at TIMESTAMPTZ,
    confirmed_at  TIMESTAMPTZ
);

CREATE TABLE mempool_deltas (
    id          BIGSERIAL   PRIMARY KEY,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    added       TEXT[]      NOT NULL,
    removed     TEXT[]      NOT NULL
);
