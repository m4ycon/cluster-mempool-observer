import { describe, expect, it } from 'vitest';
import type { ClusterRef } from '../types/events';
import { clusterTxCount, clusterTxids } from './clusterChunks';

const CLUSTER: ClusterRef = {
  id: 1,
  chunks: [
    { txids: ['c', 'a'], fee: 900, vsize: 200 },
    { txids: ['b'], fee: 100, vsize: 100 },
  ],
  total_vsize: 300,
  total_fee: 1000,
  first_seen_at: '2026-01-01T00:00:00.000Z',
};

describe('clusterTxids', () => {
  it('flattens the chunks in mining order', () => {
    expect(clusterTxids(CLUSTER)).toEqual(['c', 'a', 'b']);
  });
});

describe('clusterTxCount', () => {
  it('counts the members of every chunk', () => {
    expect(clusterTxCount(CLUSTER)).toBe(3);
  });
});
