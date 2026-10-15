import { useMemo, useState } from 'react';
import { chunkColors } from '../lib/chunkPalette';
import type { ClusterRef, TransactionRef } from '../types/events';

export interface ChunkView {
  chunkOf: Map<string, number>;
  colors: string[];
  highlighted: number | null;
  setHighlighted: (chunk: number | null) => void;
}

/** Chunk state shared by a cluster's graph and its chunk strip. */
export function useChunkView(
  cluster: ClusterRef | undefined,
  txs: Map<string, TransactionRef>,
): ChunkView {
  const [highlighted, setHighlighted] = useState<number | null>(null);
  const chunks = cluster?.chunks;
  const chunkOf = useMemo(() => {
    const map = new Map<string, number>();
    chunks?.forEach((chunk, i) => {
      for (const txid of chunk.txids) map.set(txid, i);
    });
    return map;
  }, [chunks]);
  // Parents come from the fetched txs, so a colour can still move once while
  // they load; after that it only changes when the chunks do.
  const colors = useMemo(
    () => chunkColors(chunks ?? [], (txid) => txs.get(txid)?.input_txids),
    [chunks, txs],
  );
  return { chunkOf, colors, highlighted, setHighlighted };
}
