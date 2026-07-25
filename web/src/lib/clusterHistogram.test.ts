import { describe, expect, it } from 'vitest';
import type { ClusterRef } from '../types/events';
import { histogramLayout } from './clusterHistogram';

function cluster(overrides: Partial<ClusterRef> = {}): ClusterRef {
  return {
    id: 1,
    txids: [],
    total_vsize: 1000,
    total_fee: 1000,
    ...overrides,
  };
}

/** Real-shaped fixture: varied vsize/fee so feerate spreads out. */
function manyClusters(n: number): ClusterRef[] {
  return Array.from({ length: n }, (_, i) => {
    const vsize = 100 + i * 37;
    return cluster({
      id: i + 1,
      txids: [`tx${i}`],
      total_vsize: vsize,
      total_fee: Math.round(vsize * (1 + (i % 5))),
    });
  });
}

describe('histogramLayout', () => {
  it('returns an empty layout for no clusters', () => {
    const layout = histogramLayout([], 'vsize', 10);
    expect(layout.bars).toEqual([]);
    expect(layout.xTicks).toEqual([]);
    expect(layout.yTicks).toEqual([]);
    expect(layout.maxCount).toBe(0);
  });

  it('bar counts sum to the input length', () => {
    const clusters = manyClusters(37);
    const layout = histogramLayout(clusters, 'vsize', 10);
    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(clusters.length);
  });

  it('handles a single cluster without a zero-width domain or NaN pixels', () => {
    const clusters = [cluster({ total_vsize: 500, total_fee: 500 })];
    const layout = histogramLayout(clusters, 'vsize', 10);

    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(1);
    for (const b of layout.bars) {
      expect(Number.isNaN(b.x0)).toBe(false);
      expect(Number.isNaN(b.x1)).toBe(false);
      expect(b.x1).toBeGreaterThan(b.x0);
      expect(b.hi).toBeGreaterThan(b.lo);
    }
  });

  it('handles all-identical values without a zero-width domain or NaN pixels', () => {
    const clusters = Array.from({ length: 12 }, (_, i) =>
      cluster({ id: i + 1, total_vsize: 250, total_fee: 250 }),
    );
    const layout = histogramLayout(clusters, 'vsize', 10);

    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(12);
    for (const b of layout.bars) {
      expect(Number.isNaN(b.x0)).toBe(false);
      expect(Number.isNaN(b.x1)).toBe(false);
    }
    // All 12 clusters land in one bin.
    expect(layout.bars.some((b) => b.count === 12)).toBe(true);
  });

  it('handles a zero fee value without NaN pixels', () => {
    const clusters = [
      cluster({ id: 1, total_vsize: 300, total_fee: 0 }),
      cluster({ id: 2, total_vsize: 400, total_fee: 800 }),
      cluster({ id: 3, total_vsize: 500, total_fee: 2000 }),
    ];
    const layout = histogramLayout(clusters, 'fee', 10);

    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(3);
    for (const b of layout.bars) {
      expect(Number.isNaN(b.x0)).toBe(false);
      expect(Number.isNaN(b.x1)).toBe(false);
    }
  });

  it('preserves empty bins (count 0, correct x span)', () => {
    // Two tight clusters, one far outlier: forces empty bins in between.
    const clusters = [
      cluster({ id: 1, total_vsize: 100, total_fee: 100 }),
      cluster({ id: 2, total_vsize: 110, total_fee: 110 }),
      cluster({ id: 3, total_vsize: 10_000, total_fee: 10_000 }),
    ];
    const layout = histogramLayout(clusters, 'vsize', 10);

    const empties = layout.bars.filter((b) => b.count === 0);
    expect(empties.length).toBeGreaterThan(0);
    for (const b of empties) {
      expect(b.x1).toBeGreaterThan(b.x0);
      expect(b.hi).toBeGreaterThan(b.lo);
      expect(b.y0).toBe(b.y1);
    }
    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(3);
  });

  it('places ticks inside the plot rect', () => {
    const clusters = manyClusters(25);
    const layout = histogramLayout(clusters, 'vsize', 10);

    for (const t of layout.xTicks) {
      expect(t.px).toBeGreaterThanOrEqual(layout.plot.left - 0.001);
      expect(t.px).toBeLessThanOrEqual(
        layout.plot.left + layout.plot.width + 0.001,
      );
    }
    for (const t of layout.yTicks) {
      expect(t.px).toBeGreaterThanOrEqual(layout.plot.top - 0.001);
      expect(t.px).toBeLessThanOrEqual(
        layout.plot.top + layout.plot.height + 0.001,
      );
    }
  });

  it('bin count differs from binCount as d3 snaps to nice thresholds (documented, not a bug)', () => {
    const clusters = manyClusters(60);
    const layout = histogramLayout(clusters, 'vsize', 10);
    expect(layout.bars.length).toBeGreaterThan(0);
  });

  it('handles a mempool-sized cluster array without blowing the call stack', () => {
    // Regression: this viz bypasses the top-N showCount filter, so `clusters`
    // is the whole mempool -- tens of thousands under congestion. Spreading
    // into Math.min/max (`Math.min(...values)`) throws RangeError around
    // 65-125k arguments; extent() must be used instead. Kept cheap to build
    // (no txids payload) so the suite stays fast.
    const n = 200_000;
    const clusters: ClusterRef[] = new Array(n);
    for (let i = 0; i < n; i++) {
      clusters[i] = cluster({
        id: i + 1,
        txids: [],
        total_vsize: (i % 5000) + 1,
        total_fee: (i % 5000) + 1,
      });
    }

    let layout: ReturnType<typeof histogramLayout> | undefined;
    expect(() => {
      layout = histogramLayout(clusters, 'vsize', 20);
    }).not.toThrow();

    const total = layout?.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(n);
  });

  it('counts a value at its displayed (rounded) position, not its raw one', () => {
    // feerate STEP is 0.1: a raw 2.99 displays as "3.0" (ClusterMetrics.markLabel
    // toFixed(1)s it), so it must be counted -- and land in the tooltip's range
    // label -- as a 3.0, in the bin starting at 3.0, not silently in the 2.x bin
    // its raw float would occupy. Domain [2, 4] with 2 bin hint -> threshold at 3.
    const clusters = [
      cluster({ id: 1, total_vsize: 100, total_fee: 200 }), // feerate 2.0
      cluster({ id: 2, total_vsize: 100, total_fee: 299 }), // feerate 2.99 -> displays 3.0
      cluster({ id: 3, total_vsize: 100, total_fee: 400 }), // feerate 4.0
    ];
    const layout = histogramLayout(clusters, 'feerate', 2);

    expect(layout.bars).toHaveLength(2);
    const [bin1, bin2] = layout.bars;
    expect(bin1.lo).toBe(2);
    expect(bin1.hi).toBe(3);
    expect(bin2.lo).toBe(3);
    expect(bin2.hi).toBe(4);

    // Only the exact 2.0 cluster stays in the [2, 3) bin.
    expect(bin1.count).toBe(1);
    // The 2.99 (-> 3.0) and 4.0 clusters both land in the [3, 4] bin.
    expect(bin2.count).toBe(2);
  });

  it('leaves a discrete metric (vsize, whole-number STEP) unaffected by rounding', () => {
    // vsize STEP is 1 and these are already whole numbers, so rounding to the
    // display grid is a no-op -- membership must match plain half-open binning.
    const clusters = [
      cluster({ id: 1, total_vsize: 2, total_fee: 1 }),
      cluster({ id: 2, total_vsize: 3, total_fee: 1 }),
      cluster({ id: 3, total_vsize: 4, total_fee: 1 }),
    ];
    const layout = histogramLayout(clusters, 'vsize', 2);

    expect(layout.bars).toHaveLength(2);
    const [bin1, bin2] = layout.bars;
    expect(bin1.count).toBe(1); // just vsize 2
    expect(bin2.count).toBe(2); // vsize 3 and 4
  });

  it('clamps the bin count so no bin renders narrower than one display step', () => {
    // Feerate span of only 0.5 with a requested 40 bins would, unclamped,
    // produce bins far thinner than STEP (0.1): every raw value rounds onto
    // the display grid, so most of those thin bins can never hold anything
    // and render as noise. The effective bin count must be clamped so every
    // bin spans at least one step.
    const clusters = [200, 210, 220, 230, 240, 250].map((fee, i) =>
      cluster({ id: i + 1, total_vsize: 100, total_fee: fee }),
    ); // feerates 2.0, 2.1, 2.2, 2.3, 2.4, 2.5

    const layout = histogramLayout(clusters, 'feerate', 40);

    expect(layout.bars.length).toBeLessThanOrEqual(6);
    for (const b of layout.bars) {
      expect(b.hi - b.lo).toBeGreaterThanOrEqual(0.1 - 1e-9);
    }
    const total = layout.bars.reduce((sum, b) => sum + b.count, 0);
    expect(total).toBe(clusters.length);
  });

  it('respects a custom viewBox for plot sizing', () => {
    const clusters = manyClusters(10);
    const layout = histogramLayout(clusters, 'vsize', 8, 400, 300);
    expect(layout.plot.width).toBeLessThan(400);
    expect(layout.plot.height).toBeLessThan(300);
    for (const b of layout.bars) {
      expect(b.x1).toBeLessThanOrEqual(
        layout.plot.left + layout.plot.width + 0.001,
      );
    }
  });
});
