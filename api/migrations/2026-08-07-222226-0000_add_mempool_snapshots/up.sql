-- One row per periodic sample of the api's in-memory mempool/cluster state.
CREATE TABLE mempool_snapshots (
    sampled_at         TIMESTAMPTZ PRIMARY KEY,
    cluster_count      INT         NOT NULL,
    clustered_tx_count INT         NOT NULL,
    mempool_tx_count   INT         NOT NULL,
    total_vsize        BIGINT      NOT NULL,
    total_fee          BIGINT      NOT NULL
);
