-- Restore the original batch-format mempool_deltas table.
DROP TABLE mempool_deltas;
DROP TYPE delta_reason;

CREATE TABLE mempool_deltas (
    id          BIGSERIAL   PRIMARY KEY,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    added       TEXT[]      NOT NULL,
    removed     TEXT[]      NOT NULL
);
