-- Clusters of related mempool transactions.
CREATE TABLE clusters (
    id            BIGSERIAL   PRIMARY KEY,
    txids         TEXT[]      NOT NULL,
    total_fee     BIGINT      NOT NULL,
    first_seen_at TIMESTAMPTZ,
    confirmed_at  TIMESTAMPTZ,
    total_vsize   BIGINT      NOT NULL DEFAULT 0
);

-- Transactions we have seen in the mempool or in a mined block.
CREATE TABLE transactions (
    txid            TEXT        PRIMARY KEY,
    fee             BIGINT,
    vsize           BIGINT      NOT NULL,
    first_seen_at   TIMESTAMPTZ NOT NULL,
    confirmed_at    TIMESTAMPTZ,
    cluster_id      BIGINT      REFERENCES clusters(id) ON DELETE SET NULL
);

CREATE INDEX idx_transactions_cluster_id ON transactions (cluster_id);

-- Mined blocks.
CREATE TABLE blocks (
    hash        TEXT             PRIMARY KEY,
    height      BIGINT           NOT NULL,
    mined_at    TIMESTAMPTZ      NOT NULL,
    tx_count    BIGINT           NOT NULL,
    total_bytes BIGINT           NOT NULL,
    total_fee   BIGINT           NOT NULL,
    difficulty  DOUBLE PRECISION NOT NULL
);

-- Append-only per-txid mempool delta log.
CREATE TYPE delta_reason AS ENUM (
    'add_mempool',
    'remove_confirmed',
    'remove_evicted'
);

CREATE TABLE mempool_deltas (
    id         BIGSERIAL    PRIMARY KEY,
    txid       TEXT         NOT NULL,
    reason     delta_reason NOT NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX mempool_deltas_created_at_idx ON mempool_deltas (created_at);

-- Append-only per-cluster membership/totals change log. One row per mutation
-- round; fee_delta/vsize_delta are the signed change caused by that round.
CREATE TABLE cluster_deltas (
    id            BIGSERIAL   PRIMARY KEY,
    cluster_id    BIGINT      NOT NULL REFERENCES clusters(id),
    added_txids   TEXT[]      NOT NULL DEFAULT '{}',
    removed_txids TEXT[]      NOT NULL DEFAULT '{}',
    fee_delta     BIGINT      NOT NULL,
    vsize_delta   BIGINT      NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX cluster_deltas_cluster_id_id_idx ON cluster_deltas (cluster_id, id);
CREATE INDEX cluster_deltas_added_txids_idx ON cluster_deltas USING GIN (added_txids);
CREATE INDEX cluster_deltas_removed_txids_idx ON cluster_deltas USING GIN (removed_txids);
