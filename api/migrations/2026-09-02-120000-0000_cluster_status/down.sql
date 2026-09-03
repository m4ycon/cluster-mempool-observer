UPDATE clusters
   SET txids = '{}', total_fee = 0, total_vsize = 0
 WHERE status IN ('evicted', 'merged');

DROP INDEX clusters_active_txids_idx;
CREATE INDEX clusters_active_txids_idx ON clusters USING GIN (txids)
    WHERE confirmed_at IS NULL AND txids <> '{}';

DROP INDEX clusters_active_idx;
CREATE INDEX clusters_active_idx ON clusters (id)
    WHERE confirmed_at IS NULL AND txids <> '{}';

ALTER TABLE clusters DROP COLUMN status;
DROP TYPE cluster_status;
