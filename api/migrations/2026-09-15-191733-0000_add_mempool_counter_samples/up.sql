CREATE TABLE mempool_counter_samples (
    -- The window's END, not its start
    sampled_at    TIMESTAMPTZ PRIMARY KEY,
    period_secs   BIGINT      NOT NULL,
    -- NULL means "this window was not counted" (e.g. the first sample)
    added_txs     BIGINT,
    confirmed_txs BIGINT,
    evicted_txs   BIGINT
);
