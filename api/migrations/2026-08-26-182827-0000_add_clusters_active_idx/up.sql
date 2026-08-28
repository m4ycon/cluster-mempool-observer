-- `find_active` seeds the in-memory snapshot at boot and is the one query that
-- reads every live cluster. Its two filters are exactly this index's predicate,
-- so the scan skips confirmed and closed rows, which only ever accumulate.
CREATE INDEX clusters_active_idx ON clusters (id)
    WHERE confirmed_at IS NULL AND txids <> '{}';
