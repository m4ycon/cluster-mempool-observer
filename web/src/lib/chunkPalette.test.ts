import { describe, expect, it } from 'vitest';
import type { ClusterChunkRef } from '../types/events';
import { CHUNK_PALETTE, chunkColors } from './chunkPalette';

function chunks(...txids: string[][]): ClusterChunkRef[] {
  return txids.map((ids) => ({ txids: ids, fee: 100, vsize: 100 }));
}

const noParents = () => [];

describe('chunkColors', () => {
  it('gives one colour per chunk, all from the palette', () => {
    const colors = chunkColors(chunks(['a'], ['b'], ['c']), noParents);
    expect(colors).toHaveLength(3);
    for (const c of colors) expect(CHUNK_PALETTE).toContain(c);
  });

  it('never repeats the colour of the chunk mined right before', () => {
    const many = chunks(...Array.from({ length: 40 }, (_, i) => [`t${i}`]));
    const colors = chunkColors(many, noParents);
    for (let i = 1; i < colors.length; i++) {
      expect(colors[i]).not.toBe(colors[i - 1]);
    }
  });

  it('avoids the colour of any chunk holding a parent, however far back', () => {
    // c0 is the parent of everything in the last chunk; ten chunks in between
    // would otherwise cycle the least-recently-used pick back to c0's colour.
    const ids = Array.from({ length: CHUNK_PALETTE.length }, (_, i) => [
      `t${i}`,
    ]);
    const all = chunks(...ids, ['child']);
    const parents = new Map([['child', ['t0']]]);
    const colors = chunkColors(all, (t) => parents.get(t));
    expect(colors[colors.length - 1]).not.toBe(colors[0]);
  });

  it('is deterministic for the same input', () => {
    const input = chunks(['a', 'b'], ['c'], ['d', 'e']);
    expect(chunkColors(input, noParents)).toEqual(
      chunkColors(input, noParents),
    );
  });

  it('ignores parents outside the cluster', () => {
    const colors = chunkColors(chunks(['a']), () => ['outside']);
    expect(colors).toEqual([CHUNK_PALETTE[0]]);
  });
});
