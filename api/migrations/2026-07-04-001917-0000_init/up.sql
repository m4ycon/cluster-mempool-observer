-- Clusters of related mempool transactions.
CREATE TABLE clusters (
    id            BIGSERIAL   PRIMARY KEY,
    txids         TEXT[]      NOT NULL,
    total_fee     BIGINT      NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL,
    confirmed_at  TIMESTAMPTZ,
    total_vsize   BIGINT      NOT NULL DEFAULT 0
);

-- `find_active` seeds the in-memory snapshot at boot and is the one query that
-- reads every live cluster. Its two filters are exactly this index's predicate,
-- so the scan skips confirmed and closed rows, which only ever accumulate.
CREATE INDEX clusters_active_idx ON clusters (id)
    WHERE confirmed_at IS NULL AND txids <> '{}';

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

-- Transactions we have seen in the mempool or in a mined block.
--
-- `hollow` marks a row we know of but never got the body for; such a row also
-- carries vsize = 0, which no real transaction has.
--
-- `input_txids` holds the parent txids this transaction spends from. NULL means
-- we never managed to learn them, which is distinct from '{}' (a coinbase,
-- which spends nothing). Bootstrap rows carry only the unconfirmed parents
-- `getrawmempool verbose` reports as `depends`; the other write paths carry the
-- full vin set.
CREATE TABLE transactions (
    txid               TEXT        PRIMARY KEY,
    fee                BIGINT,
    vsize              BIGINT      NOT NULL,
    first_seen_at      TIMESTAMPTZ NOT NULL,
    confirmed_at       TIMESTAMPTZ,
    cluster_id         BIGINT      REFERENCES clusters(id) ON DELETE SET NULL,
    confirmed_at_block TEXT        REFERENCES blocks(hash) ON DELETE SET NULL,
    hollow             BOOLEAN     NOT NULL DEFAULT FALSE,
    input_txids        TEXT[]
);

CREATE INDEX idx_transactions_cluster_id ON transactions (cluster_id);
CREATE INDEX idx_transactions_confirmed_at_block ON transactions (confirmed_at_block);

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

-- One row per periodic sample of the api's in-memory mempool/cluster state.
CREATE TABLE mempool_snapshots (
    sampled_at         TIMESTAMPTZ PRIMARY KEY,
    cluster_count      INT         NOT NULL,
    clustered_tx_count INT         NOT NULL,
    mempool_tx_count   INT         NOT NULL,
    total_vsize        BIGINT      NOT NULL,
    total_fee          BIGINT      NOT NULL
);

-- Append-only log of process/node lifecycle facts, so mempool_deltas/cluster_deltas
-- history can be told apart from artifacts of the api restarting or the node going away.
CREATE TYPE system_event_kind AS ENUM (
    'server_started',
    'server_stopped',
    'bootstrap_started',
    'bootstrap_completed',
    'node_connected',
    'node_disconnected',
    'node_version_changed'
);

CREATE TABLE system_events (
    id         BIGSERIAL         PRIMARY KEY,
    kind       system_event_kind NOT NULL,
    details    JSONB             NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ       NOT NULL DEFAULT now()
);

CREATE INDEX system_events_created_at_idx ON system_events (created_at);
CREATE INDEX system_events_kind_created_at_idx ON system_events (kind, created_at);
