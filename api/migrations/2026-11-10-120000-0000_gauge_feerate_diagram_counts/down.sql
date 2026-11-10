ALTER TABLE mempool_gauge_samples
    DROP COLUMN feerate_diagram_missing,
    DROP COLUMN feerate_diagram_extra,
    DROP COLUMN feerate_diagram_node_total,
    ADD COLUMN feerate_missing_percentage DOUBLE PRECISION,
    ADD COLUMN feerate_extra_percentage   DOUBLE PRECISION;
