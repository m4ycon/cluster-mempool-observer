CREATE INDEX cluster_deltas_added_txids_idx ON cluster_deltas USING GIN (added_txids);
CREATE INDEX cluster_deltas_removed_txids_idx ON cluster_deltas USING GIN (removed_txids);
