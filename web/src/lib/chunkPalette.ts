import type { ClusterChunkRef } from '../types/events';

// Off-theme on purpose: chunks are categories, not magnitudes, so the hues
// spread around the wheel instead of following the orange fee-rate tiers.
// Lightness stays mid-high so each one reads on the dark background.
export const CHUNK_PALETTE = [
  '#4ea1ff',
  '#ff6b6b',
  '#51cf66',
  '#fcc419',
  '#cc5de8',
  '#22b8cf',
  '#ff922b',
  '#f06595',
  '#94d82d',
  '#9775fa',
] as const;

/**
 * One palette colour per chunk, in chunk order. A chunk never takes the colour
 * of the chunk mined right before it, nor of any chunk holding one of its
 * parents; among what is left it takes the least recently used, so a cluster
 * with more chunks than colours repeats them as far apart as it can.
 *
 * Parents only ever sit in the same or an earlier chunk, so every chunk a
 * parent can point to is already coloured when its child's chunk is reached.
 */
export function chunkColors(
  chunks: ClusterChunkRef[],
  parentsOf: (txid: string) => readonly string[] | null | undefined,
): string[] {
  const chunkOf = new Map<string, number>();
  chunks.forEach((chunk, i) => {
    for (const txid of chunk.txids) chunkOf.set(txid, i);
  });

  const colors: string[] = [];
  const lastUse = new Map<string, number>(CHUNK_PALETTE.map((c) => [c, -1]));
  chunks.forEach((chunk, i) => {
    const banned = new Set<string>();
    if (i > 0) banned.add(colors[i - 1]);
    for (const txid of chunk.txids) {
      for (const parent of parentsOf(txid) ?? []) {
        const j = chunkOf.get(parent);
        if (j !== undefined && j < i) banned.add(colors[j]);
      }
    }
    const free = CHUNK_PALETTE.filter((c) => !banned.has(c));
    const pool = free.length > 0 ? free : CHUNK_PALETTE;
    const pick = pool.reduce((a, b) =>
      (lastUse.get(b) ?? -1) < (lastUse.get(a) ?? -1) ? b : a,
    );
    colors.push(pick);
    lastUse.set(pick, i);
  });
  return colors;
}
