-- Old cluster data is wiped, not converted: chunks were never stored, so no
-- version of a past cluster can be rebuilt. Bootstrap re-syncs live txs.
--
-- Dropping transactions.cluster_id first (a catalog-only change) lets the
-- TRUNCATE skip CASCADE, which would empty transactions too. Truncating rather
-- than recreating clusters keeps its id sequence, so ids are never reused.
ALTER TABLE transactions DROP COLUMN cluster_id;
TRUNCATE clusters, cluster_deltas;
DROP TABLE cluster_deltas;

DROP INDEX clusters_active_txids_idx;
ALTER TABLE clusters
    DROP COLUMN txids,
    ALTER COLUMN total_weight DROP DEFAULT,
    ADD COLUMN version   INT         NOT NULL DEFAULT 1,
    ADD COLUMN closed_at TIMESTAMPTZ;

-- One row per chunk per version; a version is the full chunk list of the
-- cluster at one sync round. `live` marks the current version of an active
-- cluster, the only rows the by-txid lookup may match.
CREATE TABLE cluster_chunks (
    cluster_id BIGINT      NOT NULL REFERENCES clusters (id),
    version    INT         NOT NULL,
    -- 0 = mined first
    position   SMALLINT    NOT NULL,
    -- sats; chunkfee (modified fee), except on a chunk a block split: the
    -- base fees of its mined txs, from the block
    fee        BIGINT      NOT NULL,
    -- chunkweight (sigops-adjusted), except on a chunk a block split: the raw
    -- weights of its mined txs, from the block
    weight     BIGINT      NOT NULL,
    -- mining order within the chunk
    txids      TEXT[]      NOT NULL,
    live       BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (cluster_id, version, position)
);

CREATE INDEX cluster_chunks_live_txids_idx ON cluster_chunks USING GIN (txids)
    WHERE live;

ALTER TABLE transactions
    ADD COLUMN cluster_id BIGINT REFERENCES clusters (id) ON DELETE SET NULL;
CREATE INDEX idx_transactions_cluster_id ON transactions (cluster_id);
