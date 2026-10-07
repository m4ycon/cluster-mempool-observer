ALTER TABLE transactions ALTER COLUMN weight TYPE BIGINT USING (weight + 3) / 4;
ALTER TABLE transactions RENAME COLUMN weight TO vsize;

UPDATE mempool_gauge_samples SET total_weight = (total_weight + 3) / 4;
ALTER TABLE mempool_gauge_samples RENAME COLUMN total_weight TO total_vsize;
