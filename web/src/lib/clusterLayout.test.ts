import { describe, expect, it } from 'vitest';
import type { ClusterRef } from '../types/events';
import { packLayout, treemapLayout } from './clusterLayout';

const cluster = (id: number, vsize: number): ClusterRef => ({
  id,
  txids: [`tx${id}`],
  total_vsize: vsize,
  total_fee: vsize * 2,
  first_seen_at: '2026-01-01T00:00:00.000Z',
});

describe('packLayout / treemapLayout', () => {
  it('returns [] for an empty cluster list (no bogus root leaf)', () => {
    expect(packLayout([], 'vsize')).toEqual([]);
    expect(treemapLayout([], 'vsize')).toEqual([]);
  });

  it('emits one node per real cluster, each carrying a ClusterRef', () => {
    const clusters = [cluster(1, 500), cluster(2, 900), cluster(3, 100)];
    const packed = packLayout(clusters, 'vsize');
    const cells = treemapLayout(clusters, 'vsize');

    expect(packed).toHaveLength(3);
    expect(cells).toHaveLength(3);
    for (const p of packed) expect(p.c.id).toBeGreaterThan(0);
    for (const c of cells) expect(c.c.id).toBeGreaterThan(0);
  });
});
