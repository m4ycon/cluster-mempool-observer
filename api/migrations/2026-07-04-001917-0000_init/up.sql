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
    cluster_id      BIGINT      REFERENCES clusters(id) ON DELETE SET NULL,
    left_mempool_at TIMESTAMPTZ
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
