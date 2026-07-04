-- Replace the batch (added[]/removed[]) mempool_deltas table with an
-- append-only per-txid event log tagged with a typed reason.
DROP TABLE mempool_deltas;

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
