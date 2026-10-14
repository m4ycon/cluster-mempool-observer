import type { ClusterRef } from '../types/events';

/** Every member of the cluster, in mining order. */
export function clusterTxids(c: ClusterRef): string[] {
  return c.chunks.flatMap((chunk) => chunk.txids);
}

export function clusterTxCount(c: ClusterRef): number {
  return c.chunks.reduce((acc, chunk) => acc + chunk.txids.length, 0);
}
