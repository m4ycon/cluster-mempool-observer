-- No query looks cluster_deltas up by txid, so these only slowed every delta insert.
DROP INDEX cluster_deltas_added_txids_idx;
DROP INDEX cluster_deltas_removed_txids_idx;
