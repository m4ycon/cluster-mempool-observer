ALTER TABLE mempool_gauge_samples
    ADD COLUMN feerate_missing_percentage DOUBLE PRECISION,
    ADD COLUMN feerate_extra_percentage   DOUBLE PRECISION;
