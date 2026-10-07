ALTER TABLE cluster_deltas ALTER COLUMN weight_delta TYPE BIGINT USING
    CASE WHEN weight_delta >= 0 THEN (weight_delta + 3) / 4 ELSE -((-weight_delta + 3) / 4) END;
ALTER TABLE cluster_deltas RENAME COLUMN weight_delta TO vsize_delta;
