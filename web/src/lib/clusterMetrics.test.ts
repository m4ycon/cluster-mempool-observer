import { describe, expect, it } from 'vitest';
import type { ClusterRef } from '../types/events';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';
import type { ColorScale } from './colorTiers';

function cluster(overrides: Partial<ClusterRef> = {}): ClusterRef {
  return {
    id: 1,
    txids: [],
    total_vsize: 1000,
    total_fee: 1000,
    first_seen_at: null,
    ...overrides,
  };
}

describe('ClusterMetrics.value', () => {
  it('reads vsize', () => {
    expect(ClusterMetrics.value(cluster({ total_vsize: 500 }), 'vsize')).toBe(
      500,
    );
  });

  it('reads fee', () => {
    expect(ClusterMetrics.value(cluster({ total_fee: 750 }), 'fee')).toBe(750);
  });

  it('computes feerate', () => {
    expect(
      ClusterMetrics.value(
        cluster({ total_vsize: 200, total_fee: 1000 }),
        'feerate',
      ),
    ).toBe(5);
  });

  it('feerate is 0 when vsize is 0', () => {
    expect(
      ClusterMetrics.value(
        cluster({ total_vsize: 0, total_fee: 1000 }),
        'feerate',
      ),
    ).toBe(0);
  });

  it('reads txs as txid count', () => {
    expect(
      ClusterMetrics.value(cluster({ txids: ['a', 'b', 'c'] }), 'txs'),
    ).toBe(3);
  });
});

describe('ClusterMetrics.colorAt', () => {
  it('buckets feerate by tier over visible values (not fixed thresholds)', () => {
    const vals = [10, 20, 30, 40, 50];
    const scale = ClusterMetrics.scaleFor('feerate', vals);
    expect(ClusterMetrics.colorAt(scale, 10, 'feerate')).toBe('#4d5a6b');
    expect(ClusterMetrics.colorAt(scale, 50, 'feerate')).toBe('#ffc46b');
  });

  it('buckets other metrics by tier over visible values', () => {
    const vals = [10, 20, 30, 40, 50];
    const scale = ClusterMetrics.scaleFor('vsize', vals);
    expect(ClusterMetrics.colorAt(scale, 10, 'vsize')).toBe('#4d5a6b');
    expect(ClusterMetrics.colorAt(scale, 50, 'vsize')).toBe('#ffc46b');
  });
});

/** Which tier of a scale a colour corresponds to. */
const tierOfColor = (scale: ColorScale, color: string) =>
  scale.colors.indexOf(color);

/** Does a legend label (`all X`, `<=X`, `X-Y`, `X`, `>X`) actually cover `v`? */
function labelCovers(label: string, v: number): boolean {
  const num = (s: string) => Number(s.replaceAll(',', ''));
  if (label.startsWith('all ')) return v === num(label.slice(4));
  if (label.startsWith('<=')) return v <= num(label.slice(2));
  if (label.startsWith('>')) return v > num(label.slice(1));
  const [lo, hi] = label.split('-').map(num);
  return hi === undefined ? v === lo : v >= lo && v <= hi;
}

describe('ClusterMetrics colour/legend agreement', () => {
  it('collapses an all-equal set to one named tier', () => {
    // Real scenario: every visible cluster has 25 txs. A "<25" tier would be a
    // lie, so the scale shrinks to a single tier that names the value.
    const vals = new Array(20).fill(25);
    expect(ClusterMetrics.legendRanges('txs', vals)).toEqual(['all 25']);
    expect(
      ClusterMetrics.colorAt(ClusterMetrics.scaleFor('txs', vals), 25, 'txs'),
    ).toBe('#f7931a');
  });

  it('paints every value with a tier whose legend label covers it', () => {
    const cases: number[][] = [
      new Array(20).fill(25), // flat
      [...new Array(20).fill(25), 40], // near-flat
      [...new Array(10).fill(1), ...new Array(10).fill(2)], // two values
      [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], // spread
      [1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89], // skewed
    ];
    for (const vals of cases) {
      const scale = ClusterMetrics.scaleFor('txs', vals);
      const labels = ClusterMetrics.legendRanges('txs', vals, scale);
      expect(labels).toHaveLength(scale.colors.length);
      for (const v of vals) {
        const idx = tierOfColor(scale, ClusterMetrics.colorAt(scale, v, 'txs'));
        expect(
          labelCovers(labels[idx], v),
          `value ${v} in [${vals}] got tier ${idx} labelled "${labels[idx]}"`,
        ).toBe(true);
      }
    }
  });

  it('covers a value sitting exactly on the lowest break', () => {
    const vals = [0, 6, 12, 18, 24, 30]; // breaks 5/10/15/20/25
    const scale = ClusterMetrics.scaleFor('txs', vals);
    const labels = ClusterMetrics.legendRanges('txs', vals, scale);
    expect(labels[0]).toBe('<=5');
    const idx = tierOfColor(scale, ClusterMetrics.colorAt(scale, 5, 'txs'));
    expect(idx).toBe(0);
    expect(labelCovers(labels[idx], 5)).toBe(true);
  });

  it('gives every metric a colour per label and no repeated label', () => {
    // Marks bucket on the same numbers the legend prints, so the swatch a
    // cluster gets is always the swatch its label sits next to.
    const cases: [ClusterMetric, number[]][] = [
      ['feerate', [2.71, 2.72, 2.735, 2.74, 2.75, 2.76, 2.78, 2.79]], // packed
      ['feerate', [1.2, 1.9, 2.4, 2.9, 3.6, 5.1, 9.8, 14.3, 30.7]],
      ['txs', [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]],
      ['fee', [100, 900, 2500, 4000, 12000, 80000]],
      ['vsize', [200, 400, 800, 1600, 3200, 6400, 12800]],
    ];
    for (const [metric, vals] of cases) {
      const scale = ClusterMetrics.scaleFor(metric, vals);
      const labels = ClusterMetrics.legendRanges(metric, vals, scale);
      expect(labels).toHaveLength(scale.colors.length);
      expect(new Set(labels).size, `duplicate label in [${labels}]`).toBe(
        labels.length,
      );
      // Every tier is reachable: some visible value lands on each colour.
      const painted = new Set(
        vals.map((v) => ClusterMetrics.colorAt(scale, v, metric)),
      );
      expect(painted.size, `[${labels}] painted ${painted.size} tiers`).toBe(
        scale.colors.length,
      );
    }
  });

  it('drops tiers whose bounds the legend cannot tell apart', () => {
    // Feerates packed inside a tenth: 6 raw tiers would print the same bound
    // several times, so the scale shrinks to what the labels can distinguish.
    const vals = [2.71, 2.72, 2.735, 2.74, 2.75, 2.76, 2.78, 2.79];
    const scale = ClusterMetrics.scaleFor('feerate', vals);
    expect(scale.colors).toHaveLength(1);
    // Honest about the span rather than inventing ranges nothing falls into.
    expect(ClusterMetrics.legendRanges('feerate', vals, scale)).toEqual([
      '2.7-2.8',
    ]);
  });

  it('paints a mark with a tier whose label covers its displayed feerate', () => {
    // Real report: a cluster of 2 txs, 293 vB, 1,623 sats -> 5.539 s/vB, which
    // the UI shows as "5.5". A break snaps to 5.5, and because colorAt compares
    // the raw 5.539 (> 5.5) it drops into the tier above, whose legend floor is
    // "5.6". The mark reads 5.5 but wears the 5.6+ colour.
    const vals = [1, 2, 3, 5.44, 5.48, 1623 / 293, 7.5, 9, 14];
    const scale = ClusterMetrics.scaleFor('feerate', vals);
    const labels = ClusterMetrics.legendRanges('feerate', vals, scale);
    for (const v of vals) {
      const idx = tierOfColor(
        scale,
        ClusterMetrics.colorAt(scale, v, 'feerate'),
      );
      // What the user actually sees on the mark / panel for this feerate.
      const shown = Number(ClusterMetrics.markLabel(v, 'feerate'));
      expect(
        labelCovers(labels[idx], shown),
        `feerate ${v} shows "${shown}" but tier ${idx} is labelled "${labels[idx]}"`,
      ).toBe(true);
    }
  });
});

describe('ClusterMetrics.top', () => {
  it('sorts descending by the given metric and caps at n', () => {
    const clusters = [
      cluster({ id: 1, total_vsize: 100 }),
      cluster({ id: 2, total_vsize: 300 }),
      cluster({ id: 3, total_vsize: 200 }),
    ];
    const top = ClusterMetrics.top(clusters, 'vsize', 2);
    expect(top.map((c) => c.id)).toEqual([2, 3]);
  });

  it('does not mutate the input array', () => {
    const clusters = [
      cluster({ id: 1, total_vsize: 100 }),
      cluster({ id: 2, total_vsize: 300 }),
    ];
    const original = [...clusters];
    ClusterMetrics.top(clusters, 'vsize', 1);
    expect(clusters).toEqual(original);
  });
});

describe('ClusterMetrics.legendRanges', () => {
  // n <= tiers (6) → jenks falls back to interpolated quantile, so these are
  // stable regardless of the default ColorTiers.DEFAULT_METHOD.
  it('formats feerate bounds with one decimal (one label per tier)', () => {
    // quantile breaks of [0,6,12,18,24,30] = 5 / 10 / 15 / 20 / 25
    expect(
      ClusterMetrics.legendRanges('feerate', [0, 6, 12, 18, 24, 30]),
    ).toEqual([
      '<=5.0',
      '5.1-10.0',
      '10.1-15.0',
      '15.1-20.0',
      '20.1-25.0',
      '>25.0',
    ]);
  });

  it('formats fee totals with thousands separators', () => {
    const ranges = ClusterMetrics.legendRanges(
      'fee',
      [0, 600, 1200, 1800, 2400, 3000],
    );
    // quantile breaks = 500 / 1000 / 1500 / 2000 / 2500
    expect(ranges).toEqual([
      '<=500',
      '501-1,000',
      '1,001-1,500',
      '1,501-2,000',
      '2,001-2,500',
      '>2,500',
    ]);
  });

  it('never repeats a bound between adjacent tiers', () => {
    const cases: [ClusterMetric, number[]][] = [
      ['feerate', [0, 6, 12, 18, 24, 30]],
      ['feerate', [1.2, 1.9, 2.4, 2.9, 3.6, 5.1, 9.8, 14.3, 30.7]],
      ['txs', [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]],
      ['fee', [100, 900, 2500, 4000, 12000, 80000]],
      ['vsize', [200, 400, 800, 1600, 3200, 6400, 12800]],
    ];
    for (const [metric, vals] of cases) {
      const labels = ClusterMetrics.legendRanges(metric, vals);
      const upper = (l: string) =>
        l
          .replace(/^(<=|>)/, '')
          .split('-')
          .pop() ?? '';
      const lower = (l: string) => l.replace(/^(<=|>)/, '').split('-')[0];
      for (let i = 1; i < labels.length - 1; i++) {
        expect(
          lower(labels[i]),
          `"${labels[i - 1]}" and "${labels[i]}" share a bound`,
        ).not.toBe(upper(labels[i - 1]));
      }
    }
  });

  it('names the single value of a one-step-wide tier', () => {
    // Tier 1 spans (2.7, 2.8]: at one decimal that is the lone value 2.8.
    const scale = { colors: ['a', 'b', 'c'], breaks: [2.7, 2.8] };
    expect(ClusterMetrics.legendRanges('feerate', [], scale)).toEqual([
      '<=2.7',
      '2.8',
      '>2.8',
    ]);
  });
});

describe('ClusterMetrics.binRangeLabel', () => {
  it("nudges a non-final bin's upper bound down by one display step", () => {
    // Adjacent bins must not print the same boundary value.
    expect(ClusterMetrics.binRangeLabel('feerate', 2.0, 3.0, false)).toBe(
      '2.0-2.9',
    );
    expect(ClusterMetrics.binRangeLabel('feerate', 3.0, 4.0, false)).toBe(
      '3.0-3.9',
    );
  });

  it('prints the final bin unchanged -- it is genuinely closed', () => {
    expect(ClusterMetrics.binRangeLabel('feerate', 4.0, 5.0, true)).toBe(
      '4.0-5.0',
    );
  });

  it('names the single value of a one-step-wide non-final bin', () => {
    expect(ClusterMetrics.binRangeLabel('feerate', 3.0, 3.1, false)).toBe(
      '3.0',
    );
  });

  it('still prints distinguishable bounds for a vsize-scale bin (no compaction)', () => {
    // markLabel/NumberFormat.compact would render both 1000 and the nudged
    // 1999 as "1.0k"/"2.0k"-ish and hide the fix; fmtBound must not compact.
    expect(ClusterMetrics.binRangeLabel('vsize', 1000, 2000, false)).toBe(
      '1000-1999',
    );
  });

  it('never repeats a bound between two adjacent non-final bins', () => {
    const lo1 = ClusterMetrics.binRangeLabel('vsize', 0, 10, false);
    const lo2 = ClusterMetrics.binRangeLabel('vsize', 10, 20, false);
    expect(lo1).not.toBe(lo2);
    expect(lo1.split('-').pop()).not.toBe(lo2.split('-')[0]);
  });
});

describe('ClusterMetrics.markLabel', () => {
  it('keeps one decimal for feerate regardless of magnitude', () => {
    expect(ClusterMetrics.markLabel(12.44, 'feerate')).toBe('12.4');
    expect(ClusterMetrics.markLabel(0, 'feerate')).toBe('0.0');
  });

  it('uses compact formatting for every other metric', () => {
    expect(ClusterMetrics.markLabel(1234, 'vsize')).toBe('1.2K');
    expect(ClusterMetrics.markLabel(500, 'txs')).toBe('500');
    expect(ClusterMetrics.markLabel(7000, 'fee')).toBe('7K');
  });
});
