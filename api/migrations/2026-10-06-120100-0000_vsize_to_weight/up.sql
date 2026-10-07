ALTER TABLE transactions RENAME COLUMN vsize TO weight;

ALTER TABLE transactions ALTER COLUMN weight TYPE BIGINT USING weight * 4;

ALTER TABLE mempool_gauge_samples RENAME COLUMN total_vsize TO total_weight;
UPDATE mempool_gauge_samples SET total_weight = total_weight * 4;
