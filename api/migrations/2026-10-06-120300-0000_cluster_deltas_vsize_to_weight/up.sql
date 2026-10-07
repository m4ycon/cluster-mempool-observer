ALTER TABLE cluster_deltas RENAME COLUMN vsize_delta TO weight_delta;
ALTER TABLE cluster_deltas ALTER COLUMN weight_delta TYPE BIGINT USING weight_delta * 4;
