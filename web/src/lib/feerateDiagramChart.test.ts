import { describe, expect, it } from 'vitest';
import type { FeerateDiagramPoint } from '../types/generated/FeerateDiagramPoint';
import type { FeerateDiagramPointPx } from './feerateDiagramChart';
import {
  BLOCK_WEIGHT,
  blockBoundaryWeights,
  decimatePoints,
  feerateDiagramLayout,
  filterToWindow,
  marginalFeerateAt,
  windowMaxWeight,
} from './feerateDiagramChart';

function point(weight: number, fee_sats: number): FeerateDiagramPoint {
  return { weight, fee_sats };
}

function pxPoint(px: number, p: FeerateDiagramPoint): FeerateDiagramPointPx {
  return { px, py: 0, point: p, marginalSatPerVb: null };
}

/** Real-shaped fixture: origin plus `n` evenly-spread cumulative points. */
function diagram(
  n: number,
  totalW: number,
  totalFee: number,
): FeerateDiagramPoint[] {
  const points = [point(0, 0)];
  for (let i = 1; i <= n; i++) {
    points.push(
      point(Math.round((totalW * i) / n), Math.round((totalFee * i) / n)),
    );
  }
  return points;
}

describe('windowMaxWeight / filterToWindow', () => {
  const points = diagram(1000, 20_000_000, 100_000_000); // 5 blocks total

  it('clips numeric windows to window * BLOCK_WEIGHT', () => {
    expect(windowMaxWeight(points, 1)).toBe(BLOCK_WEIGHT);
    expect(windowMaxWeight(points, 2)).toBe(2 * BLOCK_WEIGHT);
    expect(windowMaxWeight(points, 3)).toBe(3 * BLOCK_WEIGHT);
  });

  it('"all" resolves to the diagram total weight', () => {
    expect(windowMaxWeight(points, 'all')).toBe(20_000_000);
  });

  it('caps a window at the total weight when the window would exceed it', () => {
    const small = diagram(100, 5_000_000, 10_000_000); // under 2 blocks total
    expect(windowMaxWeight(small, 3)).toBe(5_000_000);
    expect(filterToWindow(small, 3)).toHaveLength(small.length);
  });

  it('filters out points beyond the window', () => {
    const filtered = filterToWindow(points, 1);
    expect(filtered.length).toBeGreaterThan(0);
    expect(filtered.length).toBeLessThan(points.length);
    for (const p of filtered) {
      expect(p.weight).toBeLessThanOrEqual(BLOCK_WEIGHT);
    }
  });

  it('"all" keeps every point', () => {
    expect(filterToWindow(points, 'all')).toHaveLength(points.length);
  });

  it('handles an empty diagram without crashing', () => {
    expect(windowMaxWeight([], 1)).toBe(0);
    expect(filterToWindow([], 'all')).toEqual([]);
  });
});

describe('blockBoundaryWeights', () => {
  it('lands on every multiple of BLOCK_WEIGHT up to the max', () => {
    expect(blockBoundaryWeights(10_000_000)).toEqual([4_000_000, 8_000_000]);
  });

  it('excludes multiples beyond the window max', () => {
    // 6,000,000 is not itself a multiple, so the next boundary (8M) must be excluded.
    expect(blockBoundaryWeights(6_000_000)).toEqual([4_000_000]);
  });

  it('returns none when the max is below one block', () => {
    expect(blockBoundaryWeights(1_000_000)).toEqual([]);
  });

  it('matches the documented ~21 boundaries for a real full-mempool total', () => {
    const boundaries = blockBoundaryWeights(85_000_000);
    expect(boundaries).toHaveLength(21);
    expect(boundaries[0]).toBe(4_000_000);
    expect(boundaries[boundaries.length - 1]).toBe(84_000_000);
  });
});

describe('decimatePoints', () => {
  it('returns inputs of 2 or fewer points unchanged', () => {
    const pts = [pxPoint(0, point(0, 0)), pxPoint(5, point(100, 50))];
    expect(decimatePoints(pts)).toEqual(pts);
  });

  it('keeps only the last point in each pixel bucket', () => {
    const pts = [
      pxPoint(10.0, point(0, 0)),
      pxPoint(10.2, point(100, 50)),
      pxPoint(10.8, point(200, 100)), // same bucket (floor 10) as the two above
      pxPoint(20.0, point(300, 150)),
    ];
    const out = decimatePoints(pts);
    expect(out.map((p) => p.point.weight)).toEqual([0, 200, 300]);
  });

  it('preserves the true first and last points even when bucketed with a neighbor', () => {
    const pts = [
      pxPoint(10.0, point(0, 0)), // origin
      pxPoint(10.1, point(50, 25)), // same bucket as the origin
      pxPoint(500.0, point(1000, 500)),
    ];
    const out = decimatePoints(pts);
    expect(out[0].point.weight).toBe(0);
    expect(out[out.length - 1].point.weight).toBe(1000);
  });

  it('collapses a many-thousand-point input to roughly one point per pixel', () => {
    const plotWidthPx = 896; // 960 - 48 - 16, matching feerateDiagramLayout's MARGIN
    const pts: FeerateDiagramPointPx[] = Array.from(
      { length: 11_000 },
      (_, i) => pxPoint(48 + (i / 10_999) * plotWidthPx, point(i, i)),
    );
    const out = decimatePoints(pts);
    expect(out.length).toBeLessThanOrEqual(plotWidthPx + 2);
    expect(out[0]).toBe(pts[0]);
    expect(out[out.length - 1]).toBe(pts[pts.length - 1]);
  });
});

describe('marginalFeerateAt', () => {
  it('matches the documented 441/2712 sanity check (~24.6 sat/vB)', () => {
    const points = [point(0, 0), point(441, 2712)];
    expect(marginalFeerateAt(points, 1)).toBeCloseTo(24.6, 1);
  });

  it('returns null for the origin point, which has no predecessor', () => {
    const points = [point(0, 0), point(441, 2712)];
    expect(marginalFeerateAt(points, 0)).toBeNull();
  });

  it('returns null for a zero-width segment instead of dividing by zero', () => {
    const points = [point(0, 0), point(441, 2712), point(441, 3000)];
    expect(marginalFeerateAt(points, 2)).toBeNull();
  });

  it('returns null for an out-of-range index', () => {
    const points = [point(0, 0), point(441, 2712)];
    expect(marginalFeerateAt(points, -1)).toBeNull();
    expect(marginalFeerateAt(points, 2)).toBeNull();
  });

  // 0 is a real reading at the tail, so it must not collide with the null case.
  it('returns 0 for a genuinely zero-fee segment', () => {
    const points = [point(0, 0), point(441, 0)];
    expect(marginalFeerateAt(points, 1)).toBe(0);
  });
});

describe('marginalSatPerVb carried on layout points', () => {
  /** Convex like the real diagram: rate decays from 100 sat/vB toward ~1. */
  function decayingDiagram(): FeerateDiagramPoint[] {
    const points = [point(0, 0)];
    let w = 0;
    let fee = 0;
    for (let i = 0; i < 3000; i++) {
      const rate = 100 / (1 + i / 5);
      w += 5000;
      fee += (rate * 5000) / 4; // sat/vB -> sats over 5000 WU
      points.push(point(w, Math.round(fee)));
    }
    return points;
  }

  it('gives each drawn point the rate of its own raw chunk', () => {
    const raw = decayingDiagram();
    const rawIndexOf = new Map(raw.map((p, i) => [p, i]));
    const layout = feerateDiagramLayout(raw, 'all', 960, 320);

    for (const p of layout.points) {
      const rawIndex = rawIndexOf.get(p.point) as number;
      expect(p.marginalSatPerVb).toBe(marginalFeerateAt(raw, rawIndex));
    }
    expect(layout.points[0].marginalSatPerVb).toBeNull();
  });

  // Regression guard: proves the assertion above would actually catch the old
  // behavior, where the rate came from the decimated array and so averaged over
  // every chunk a pixel bucket swallowed.
  it('differs from the decimated-array rate where the curve is steep', () => {
    const layout = feerateDiagramLayout(decayingDiagram(), 'all', 960, 320);
    const drawn = layout.points.map((p) => p.point);

    const smeared = layout.points.filter((p, i) => {
      const fromDecimated = marginalFeerateAt(drawn, i);
      return (
        p.marginalSatPerVb !== null &&
        fromDecimated !== null &&
        Math.abs(fromDecimated - p.marginalSatPerVb) > 1
      );
    });

    expect(smeared.length).toBeGreaterThan(0);
  });
});

describe('feerateDiagramLayout', () => {
  it('returns empty paths and no crash for empty points', () => {
    const layout = feerateDiagramLayout([], 'all', 960, 320);
    expect(layout.points).toEqual([]);
    expect(layout.linePath).toBe('');
    expect(layout.areaPath).toBe('');
    expect(layout.xTicks).toEqual([]);
    expect(layout.yTicks).toEqual([]);
    expect(layout.blockBoundaries).toEqual([]);
  });

  it('handles a single point without a NaN scale or zero-width domain', () => {
    const layout = feerateDiagramLayout([point(500, 250)], 'all', 960, 320);
    expect(layout.points).toHaveLength(1);
    expect(Number.isNaN(layout.points[0].px)).toBe(false);
    expect(Number.isNaN(layout.points[0].py)).toBe(false);
  });

  it('handles a single origin-only point (zero total weight)', () => {
    const layout = feerateDiagramLayout([point(0, 0)], 1, 960, 320);
    expect(layout.points).toHaveLength(1);
    expect(Number.isNaN(layout.points[0].px)).toBe(false);
    expect(Number.isNaN(layout.points[0].py)).toBe(false);
  });

  it('scopes plotted points and boundaries to the selected window', () => {
    const points = diagram(2000, 20_000_000, 100_000_000);
    const layout = feerateDiagramLayout(points, 2, 960, 320);

    const last = layout.points[layout.points.length - 1];
    expect(last.point.weight).toBeLessThanOrEqual(2 * BLOCK_WEIGHT);

    expect(layout.blockBoundaries.length).toBeGreaterThan(0);
    for (const b of layout.blockBoundaries) {
      expect(b.weight).toBeLessThanOrEqual(2 * BLOCK_WEIGHT);
      expect(b.px).toBeGreaterThanOrEqual(layout.plot.left - 0.001);
      expect(b.px).toBeLessThanOrEqual(
        layout.plot.left + layout.plot.width + 0.001,
      );
    }
  });

  it('decimates a many-thousand-point curve to roughly the plot width, keeping both endpoints', () => {
    const points = diagram(11_000, 85_000_000, 500_000_000);
    const layout = feerateDiagramLayout(points, 'all', 960, 320);

    expect(layout.points.length).toBeLessThanOrEqual(
      Math.ceil(layout.plot.width) + 2,
    );
    expect(layout.points[0].point.weight).toBe(0);
    expect(layout.points[layout.points.length - 1].point.weight).toBe(
      85_000_000,
    );
    // ~21 boundaries at this total, matching blockBoundaryWeights directly.
    expect(layout.blockBoundaries).toHaveLength(21);
  });
});
