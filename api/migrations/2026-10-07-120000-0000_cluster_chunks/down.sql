-- Restores the schema only: the cluster data is wiped again, and neither the
-- data the up migration wiped nor cluster_deltas history comes back.
ALTER TABLE transactions DROP COLUMN cluster_id;
DROP TABLE cluster_chunks;
TRUNCATE clusters;

ALTER TABLE clusters
    DROP COLUMN closed_at,
    DROP COLUMN version,
    ALTER COLUMN total_weight SET DEFAULT 0,
    ADD COLUMN txids TEXT[] NOT NULL;
CREATE INDEX clusters_active_txids_idx ON clusters USING GIN (txids)
    WHERE status = 'active';

CREATE TABLE cluster_deltas (
    id            BIGSERIAL   PRIMARY KEY,
    cluster_id    BIGINT      NOT NULL REFERENCES clusters(id),
    added_txids   TEXT[]      NOT NULL DEFAULT '{}',
    removed_txids TEXT[]      NOT NULL DEFAULT '{}',
    fee_delta     BIGINT      NOT NULL,
    weight_delta  BIGINT      NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cluster_deltas_cluster_id_id_idx ON cluster_deltas (cluster_id, id);

ALTER TABLE transactions
    ADD COLUMN cluster_id BIGINT REFERENCES clusters(id) ON DELETE SET NULL;
CREATE INDEX idx_transactions_cluster_id ON transactions (cluster_id);
