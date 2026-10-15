import { useMemo, useState } from 'react';
import { chunkColors } from '../lib/chunkPalette';
import { chunkIndexByTxid } from '../lib/clusterChunks';
import type {
  ClusterChunkRef,
  ClusterRef,
  TransactionRef,
} from '../types/events';

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
  const chunks = cluster?.chunks;
  const [hover, setHover] = useState<{
    chunks: ClusterChunkRef[];
    chunk: number;
  } | null>(null);
  const highlighted =
    hover !== null && hover.chunks === chunks ? hover.chunk : null;
  const setHighlighted = (chunk: number | null) =>
    setHover(chunk === null || !chunks ? null : { chunks, chunk });
  const chunkOf = useMemo(() => chunkIndexByTxid(chunks ?? []), [chunks]);
  // Parents come from the fetched txs, so a colour can still move once while
  // they load; after that it only changes when the chunks do.
  const colors = useMemo(
    () => chunkColors(chunks ?? [], (txid) => txs.get(txid)?.input_txids),
    [chunks, txs],
  );
  return { chunkOf, colors, highlighted, setHighlighted };
}
