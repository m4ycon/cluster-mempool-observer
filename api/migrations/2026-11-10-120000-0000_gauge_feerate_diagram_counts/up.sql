ALTER TABLE mempool_gauge_samples
    DROP COLUMN feerate_missing_percentage,
    DROP COLUMN feerate_extra_percentage,
    ADD COLUMN feerate_diagram_missing    BIGINT,
    ADD COLUMN feerate_diagram_extra      BIGINT,
    ADD COLUMN feerate_diagram_node_total BIGINT;
