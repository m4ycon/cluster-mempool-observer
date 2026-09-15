ALTER INDEX mempool_gauge_samples_pkey RENAME TO mempool_snapshots_pkey;
ALTER TABLE mempool_gauge_samples RENAME TO mempool_snapshots;
