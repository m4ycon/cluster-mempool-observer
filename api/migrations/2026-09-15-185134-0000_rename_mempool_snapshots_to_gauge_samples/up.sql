ALTER TABLE mempool_snapshots RENAME TO mempool_gauge_samples;
ALTER INDEX mempool_snapshots_pkey RENAME TO mempool_gauge_samples_pkey;
