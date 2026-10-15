import type { ClusterChunkRef, ClusterRef } from '../types/events';

/** Every member of the cluster, in mining order. */
export function clusterTxids(c: ClusterRef): string[] {
  return c.chunks.flatMap((chunk) => chunk.txids);
}

export function clusterTxCount(c: ClusterRef): number {
  return c.chunks.reduce((acc, chunk) => acc + chunk.txids.length, 0);
}

export function chunkIndexByTxid(
  chunks: ClusterChunkRef[],
): Map<string, number> {
  const map = new Map<string, number>();
  chunks.forEach((chunk, i) => {
    for (const txid of chunk.txids) map.set(txid, i);
  });
  return map;
}

export function chunkFeerate(chunk: ClusterChunkRef): number {
  return chunk.vsize > 0 ? chunk.fee / chunk.vsize : 0;
}
