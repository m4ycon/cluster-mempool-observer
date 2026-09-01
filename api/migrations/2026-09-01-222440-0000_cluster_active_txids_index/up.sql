-- Backs a by-member-txid overlap (`&&`) lookup of live clusters. Same predicate
-- as `clusters_active_idx`, so confirmed and closed clusters -- which only ever
-- accumulate -- are excluded from the index.
CREATE INDEX clusters_active_txids_idx ON clusters USING GIN (txids)
    WHERE confirmed_at IS NULL AND txids <> '{}';
