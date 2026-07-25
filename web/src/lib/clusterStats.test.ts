import { describe, expect, it } from 'vitest';
import type { ClusterRef } from '../types/events';
import { clusterStats } from './clusterStats';

function cluster(overrides: Partial<ClusterRef> = {}): ClusterRef {
  return {
    id: 1,
    txids: [],
    total_vsize: 1000,
    total_fee: 1000,
    ...overrides,
  };
}

describe('clusterStats', () => {
  it('returns sane zeros for no clusters, not NaN', () => {
    const stats = clusterStats([], 'vsize');
    expect(stats).toEqual({
      count: 0,
      totalVsize: 0,
      totalFee: 0,
      min: 0,
      median: 0,
      p90: 0,
      max: 0,
    });
  });

  it('handles a single cluster: min/median/max/p90 all collapse to its value', () => {
    const stats = clusterStats(
      [cluster({ txids: ['a'], total_vsize: 500, total_fee: 250 })],
      'vsize',
    );
    expect(stats.count).toBe(1);
    expect(stats.totalVsize).toBe(500);
    expect(stats.totalFee).toBe(250);
    expect(stats.min).toBe(500);
    expect(stats.median).toBe(500);
    expect(stats.p90).toBe(500);
    expect(stats.max).toBe(500);
  });

  it('interpolates the median for an even count', () => {
    const clusters = [10, 20, 30, 40].map((v) =>
      cluster({ txids: ['a', 'b'], total_vsize: v, total_fee: v }),
    );
    const stats = clusterStats(clusters, 'vsize');
    // Linear-interpolated quantile at p=0.5 over [10,20,30,40] -> midpoint of 20,30.
    expect(stats.median).toBe(25);
    expect(stats.min).toBe(10);
    expect(stats.max).toBe(40);
  });

  it('lands exactly on a value for an odd count', () => {
    const clusters = [10, 20, 30, 40, 50].map((v) =>
      cluster({ txids: ['a'], total_vsize: v, total_fee: v }),
    );
    const stats = clusterStats(clusters, 'vsize');
    expect(stats.median).toBe(30);
  });

  it('computes p90 via the same interpolated quantile', () => {
    const clusters = Array.from({ length: 11 }, (_, i) =>
      cluster({ txids: ['a'], total_vsize: (i + 1) * 10, total_fee: 1 }),
    ); // 10..110, evenly spaced
    const stats = clusterStats(clusters, 'vsize');
    // p90 at n=11 -> pos = 0.9 * 10 = 9 (exact index), value = 100.
    expect(stats.p90).toBe(100);
  });

  it('sums totals across all clusters regardless of the binned metric', () => {
    const clusters = [
      cluster({ txids: ['a'], total_vsize: 100, total_fee: 500 }),
      cluster({ txids: ['b'], total_vsize: 300, total_fee: 1500 }),
    ];
    const stats = clusterStats(clusters, 'feerate');
    expect(stats.totalVsize).toBe(400);
    expect(stats.totalFee).toBe(2000);
  });

  it('handles a mempool-sized input without blowing the call stack', () => {
    // Regression, mirroring clusterHistogram.test.ts: Math.min/max(...values)
    // throws RangeError around 65-125k arguments. clusterStats must sort/loop
    // instead of spreading.
    const n = 200_000;
    const clusters: ClusterRef[] = new Array(n);
    for (let i = 0; i < n; i++) {
      clusters[i] = cluster({
        id: i + 1,
        txids: i % 3 === 0 ? ['solo'] : ['a', 'b'],
        total_vsize: (i % 5000) + 1,
        total_fee: (i % 5000) + 1,
      });
    }

    let stats: ReturnType<typeof clusterStats> | undefined;
    expect(() => {
      stats = clusterStats(clusters, 'vsize');
    }).not.toThrow();

    expect(stats?.count).toBe(n);
    expect(stats?.min).toBe(1);
    expect(stats?.max).toBe(5000);
  });
});
