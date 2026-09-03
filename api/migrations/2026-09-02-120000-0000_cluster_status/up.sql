CREATE TYPE cluster_status AS ENUM ('active', 'confirmed', 'evicted', 'merged');

ALTER TABLE clusters ADD COLUMN status cluster_status NOT NULL DEFAULT 'active';

UPDATE clusters SET status = 'confirmed' WHERE confirmed_at IS NOT NULL;
UPDATE clusters SET status = 'evicted' WHERE confirmed_at IS NULL AND txids = '{}';


DROP INDEX clusters_active_idx;
CREATE INDEX clusters_active_idx ON clusters (id)
    WHERE status = 'active';

DROP INDEX clusters_active_txids_idx;
CREATE INDEX clusters_active_txids_idx ON clusters USING GIN (txids)
    WHERE status = 'active';
