import { useEffect, useMemo, useRef, useState } from 'react';
import { httpClient } from '../lib/http';
import { ApiRoutes } from '../lib/routes';
import type { TransactionLookup, TransactionRef } from '../types/events';

const TXIDS_PER_REQUEST = 128;
const MAX_CACHED = 2000;

/** A cached row this long past its last fetch attempt is eligible for retry. */
export const STALE_RETRY_MS = 30_000;

export interface TransactionCache {
  txs: Map<string, TransactionRef>;
  missing: Set<string>;
  loading: boolean;
  error: Error | null;
}

interface Cache {
  txs: Map<string, TransactionRef>;
  missing: Set<string>;
}

const EMPTY_CACHE: Cache = { txs: new Map(), missing: new Set() };

function chunk<T>(items: T[], size: number): T[][] {
  const chunks: T[][] = [];
  for (let i = 0; i < items.length; i += size) {
    chunks.push(items.slice(i, i + size));
  }
  return chunks;
}

/** Drops cached entries outside `wanted` once the cache grows past MAX_CACHED. */
function evict(cache: Cache, wanted: Set<string>): Cache {
  if (cache.txs.size + cache.missing.size <= MAX_CACHED) return cache;
  return {
    txs: new Map([...cache.txs].filter(([txid]) => wanted.has(txid))),
    missing: new Set([...cache.missing].filter((txid) => wanted.has(txid))),
  };
}

/** A row missing its parents entirely, as opposed to `[]` (coinbase, complete). */
function isIncomplete(tx: TransactionRef): boolean {
  return tx.hollow || tx.input_txids === null;
}

/**
 * Incrementally fetches txids not already cached and merges results in; never replaces
 * the cache wholesale, since a row's fee/vsize/input_txids fill in over time.
 *
 * Complete rows are cached forever, but an incomplete row (hollow, or `input_txids:
 * null`) or a server-reported miss is retried every STALE_RETRY_MS -- the row that
 * arrived incomplete a moment ago is exactly the one a live backfill is about to fill in.
 * `key` (below) only reruns the effect on a genuine membership change, so retries are
 * driven by a `setInterval` inside that same effect instead, independent of rerenders.
 */
export function useTransactionCache(txids: string[]): TransactionCache {
  const [cache, setCache] = useState<Cache>(EMPTY_CACHE);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const cacheRef = useRef(cache);
  cacheRef.current = cache;

  // Last fetch-attempt time per txid; a ref so recording it never triggers a render.
  const lastAttemptRef = useRef(new Map<string, number>());
  const controllersRef = useRef(new Set<AbortController>());

  // Sorted, deduped, and joined to a string so the effect below only reruns
  // on a genuine membership change, not a new array identity with the same txids.
  const key = useMemo(() => [...new Set(txids)].sort().join(','), [txids]);

  useEffect(() => {
    const wanted = key === '' ? [] : key.split(',');
    const wantedSet = new Set(wanted);

    const attempt = () => {
      const current = cacheRef.current;
      const now = Date.now();
      const needed = wanted.filter((txid) => {
        const cachedTx = current.txs.get(txid);
        const isMissing = current.missing.has(txid);
        if (!cachedTx && !isMissing) return true;
        if (cachedTx && !isIncomplete(cachedTx)) return false;
        const lastAttempt = lastAttemptRef.current.get(txid);
        return lastAttempt === undefined || now - lastAttempt >= STALE_RETRY_MS;
      });

      if (needed.length === 0) {
        // Nothing to fetch right now; only clear loading if nothing else is in flight.
        if (controllersRef.current.size === 0) setLoading(false);
        return;
      }
      for (const txid of needed) lastAttemptRef.current.set(txid, now);

      const controller = new AbortController();
      controllersRef.current.add(controller);
      setLoading(true);

      Promise.all(
        chunk(needed, TXIDS_PER_REQUEST).map((batch) =>
          httpClient.get<TransactionLookup>(ApiRoutes.transactions(batch), {
            signal: controller.signal,
          }),
        ),
      )
        .then((lookups) => {
          if (controller.signal.aborted) return;
          setCache((prev) => {
            const nextTxs = new Map(prev.txs);
            const nextMissing = new Set(prev.missing);
            for (const lookup of lookups) {
              for (const tx of lookup.found) {
                nextTxs.set(tx.txid, tx);
                nextMissing.delete(tx.txid);
              }
              for (const txid of lookup.missing) nextMissing.add(txid);
            }
            return evict({ txs: nextTxs, missing: nextMissing }, wantedSet);
          });
          setError(null);
        })
        .catch((err: unknown) => {
          // controller.signal only aborts via the cleanup below (unmount/key change).
          // A timeout aborts httpClient's own combined signal instead, leaving this one
          // clear, so it falls through and correctly surfaces as an error.
          if (controller.signal.aborted) return;
          setError(err instanceof Error ? err : new Error(String(err)));
        })
        .finally(() => {
          controllersRef.current.delete(controller);
          if (!controller.signal.aborted && controllersRef.current.size === 0) {
            setLoading(false);
          }
        });
    };

    attempt();
    const interval = setInterval(attempt, STALE_RETRY_MS);

    return () => {
      clearInterval(interval);
      for (const controller of controllersRef.current) controller.abort();
      controllersRef.current.clear();
    };
  }, [key]);

  return { txs: cache.txs, missing: cache.missing, loading, error };
}
