ALTER TABLE transactions ADD COLUMN left_mempool_at TIMESTAMPTZ;

UPDATE transactions SET left_mempool_at = confirmed_at WHERE confirmed_at IS NOT NULL;
