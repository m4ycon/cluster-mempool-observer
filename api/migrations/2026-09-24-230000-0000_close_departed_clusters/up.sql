-- Closes the active clusters whose every member has already left the mempool,
-- writing what the live confirm/evict paths would have written at the time.

-- mempool_deltas has no txid index, so every member's last delta is found in
-- one pass here instead of once per cluster.
CREATE TEMP TABLE member_last_delta ON COMMIT DROP AS
SELECT DISTINCT ON (d.txid) d.txid, d.reason, d.created_at
  FROM mempool_deltas d
  JOIN (SELECT DISTINCT unnest(txids) AS txid
          FROM clusters
         WHERE status = 'active') m ON m.txid = d.txid
 ORDER BY d.txid, d.id DESC;

ALTER TABLE member_last_delta ADD PRIMARY KEY (txid);
-- autovacuum never analyzes temp tables
ANALYZE member_last_delta;

-- kept_txids is NULL when every member was evicted, dropped_txids when every
-- member was confirmed. A member with no delta row at all disqualifies the
-- cluster, same as one still in the mempool.
CREATE TEMP TABLE departed_clusters ON COMMIT DROP AS
SELECT c.id,
       array_agg(m.txid ORDER BY m.ord) FILTER (WHERE d.reason = 'remove_confirmed') AS kept_txids,
       array_agg(m.txid ORDER BY m.ord) FILTER (WHERE d.reason = 'remove_evicted') AS dropped_txids,
       -- the live path sums block fees, where a missing one simply adds nothing
       coalesce(sum(t.fee) FILTER (WHERE d.reason = 'remove_confirmed'), 0)::BIGINT AS kept_fee,
       coalesce(sum(t.vsize) FILTER (WHERE d.reason = 'remove_confirmed'), 0)::BIGINT AS kept_vsize,
       coalesce(
           max(t.confirmed_at) FILTER (WHERE d.reason = 'remove_confirmed'),
           max(d.created_at) FILTER (WHERE d.reason = 'remove_confirmed')
       ) AS confirmed_at
  FROM clusters c
 CROSS JOIN LATERAL unnest(c.txids) WITH ORDINALITY AS m(txid, ord)
  LEFT JOIN member_last_delta d ON d.txid = m.txid
  LEFT JOIN transactions t ON t.txid = m.txid
 WHERE c.status = 'active'
 GROUP BY c.id
HAVING bool_and(d.reason IS NOT NULL AND d.reason <> 'add_mempool');

ALTER TABLE departed_clusters ADD PRIMARY KEY (id);
ANALYZE departed_clusters;

-- Batching only bounds each statement: diesel runs this file as one
-- transaction, and COMMIT is rejected inside it.
DO $$
DECLARE
    batch BIGINT[];
BEGIN
    FOR batch IN
        SELECT array_agg(id)
          FROM (SELECT id, (row_number() OVER (ORDER BY id) - 1) / 500 AS n
                  FROM departed_clusters) numbered
         GROUP BY n
         ORDER BY n
    LOOP
        WITH closed AS (
            UPDATE clusters c
               SET status = 'evicted'
              FROM departed_clusters p
             WHERE p.id = c.id
               AND p.id = ANY (batch)
               AND p.kept_txids IS NULL
            RETURNING c.id, c.txids, c.total_fee, c.total_vsize
        ), unlinked AS (
            UPDATE transactions t
               SET cluster_id = NULL
              FROM closed
             WHERE t.cluster_id = closed.id
        )
        INSERT INTO cluster_deltas (cluster_id, removed_txids, fee_delta, vsize_delta)
        SELECT id, txids, -total_fee, -total_vsize
          FROM closed;

        WITH old AS (
            SELECT c.id, c.total_fee, c.total_vsize,
                   p.kept_txids, p.dropped_txids, p.kept_fee, p.kept_vsize
              FROM clusters c
              JOIN departed_clusters p ON p.id = c.id
             WHERE p.id = ANY (batch)
               AND p.kept_txids IS NOT NULL
               AND p.dropped_txids IS NOT NULL
        ), trimmed AS (
            UPDATE clusters c
               SET txids = old.kept_txids,
                   total_fee = old.kept_fee,
                   total_vsize = old.kept_vsize
              FROM old
             WHERE c.id = old.id
        ), unlinked AS (
            UPDATE transactions t
               SET cluster_id = NULL
              FROM old
             WHERE t.cluster_id = old.id
               AND t.txid <> ALL (old.kept_txids)
        )
        INSERT INTO cluster_deltas (cluster_id, removed_txids, fee_delta, vsize_delta)
        SELECT id, dropped_txids, kept_fee - total_fee, kept_vsize - total_vsize
          FROM old;

        -- a separate statement, so the closing delta sees the trimmed members
        -- and totals written just above
        WITH confirmed AS (
            UPDATE clusters c
               SET status = 'confirmed',
                   confirmed_at = p.confirmed_at
              FROM departed_clusters p
             WHERE p.id = c.id
               AND p.id = ANY (batch)
               AND p.kept_txids IS NOT NULL
            RETURNING c.id, c.txids, c.total_fee, c.total_vsize
        )
        INSERT INTO cluster_deltas (cluster_id, removed_txids, fee_delta, vsize_delta)
        SELECT id, txids, -total_fee, -total_vsize
          FROM confirmed;
    END LOOP;
END $$;

-- ON COMMIT DROP alone would let a second run in the same transaction collide
-- with these names.
DROP TABLE departed_clusters;
DROP TABLE member_last_delta;
