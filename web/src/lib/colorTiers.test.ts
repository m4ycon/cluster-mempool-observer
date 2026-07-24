import { describe, expect, it } from 'vitest';
import { type ColorTierMethod, ColorTiers } from './colorTiers';

/** Tier a value lands in: breaks are upper-inclusive bounds. */
const tierOf = (breaks: number[], v: number): number => {
  for (let i = 0; i < breaks.length; i++) if (v <= breaks[i]) return i;
  return breaks.length;
};

describe('ColorTiers.scale', () => {
  const methods: ColorTierMethod[] = ['quantile', 'jenks'];

  it('collapses to one orange tier when every value is equal', () => {
    // Real scenario: many clusters, all with 25 txs.
    const s = ColorTiers.scale(new Array(20).fill(25));
    expect(s.colors).toEqual(['#f7931a']);
    expect(s.breaks).toEqual([]);
  });

  it('uses one tier per distinct value below the colour count', () => {
    expect(
      ColorTiers.scale([...new Array(10).fill(1), ...new Array(10).fill(2)])
        .colors,
    ).toHaveLength(2);
    expect(ColorTiers.scale([1, 1, 2, 2, 3, 3, 3]).colors).toHaveLength(3);
  });

  it('keeps both ends of the ramp on a short scale', () => {
    const { colors } = ColorTiers.scale([1, 2]);
    expect(colors[0]).toBe(ColorTiers.COLORS[0]);
    expect(colors[colors.length - 1]).toBe(
      ColorTiers.COLORS[ColorTiers.COLORS.length - 1],
    );
  });

  it('uses the full ramp once there are enough distinct values', () => {
    const { colors, breaks } = ColorTiers.scale([1, 2, 3, 4, 5, 6, 7, 8]);
    expect(colors).toEqual([...ColorTiers.COLORS]);
    expect(breaks).toHaveLength(ColorTiers.COLORS.length - 1);
  });

  it('quantile method computes evenly-ranked breaks', () => {
    // n=7 distinct, evenly spaced → breaks land on the interior points 1..5
    expect(ColorTiers.scale([0, 1, 2, 3, 4, 5, 6], 'quantile').breaks).toEqual([
      1, 2, 3, 4, 5,
    ]);
  });

  it('jenks separates two far-apart groups into different tiers', () => {
    // tight low group 1..7 and tight high group 100..106
    const { breaks } = ColorTiers.scale(
      [1, 2, 3, 4, 5, 6, 7, 100, 101, 102, 103, 104, 105, 106],
      'jenks',
    );
    expect(tierOf(breaks, 7)).toBeLessThan(tierOf(breaks, 100));
  });

  it('handles a single value as one tier', () => {
    expect(ColorTiers.scale([42])).toEqual({ colors: ['#f7931a'], breaks: [] });
  });

  it('handles an empty set as a single tier', () => {
    const s = ColorTiers.scale([]);
    expect(s.colors).toHaveLength(1);
    expect(s.breaks).toEqual([]);
  });

  it('keeps colors.length === breaks.length + 1, finite and ascending', () => {
    const cases = [
      [],
      [42],
      new Array(20).fill(25),
      [...new Array(20).fill(25), 40],
      [...new Array(10).fill(1), ...new Array(10).fill(2)],
      [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
      [1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89],
    ];
    for (const m of methods) {
      for (const vals of cases) {
        const { colors, breaks } = ColorTiers.scale(vals, m);
        expect(colors).toHaveLength(breaks.length + 1);
        expect(colors.length).toBeLessThanOrEqual(ColorTiers.COLORS.length);
        for (const b of breaks) expect(Number.isFinite(b)).toBe(true);
        for (let i = 1; i < breaks.length; i++) {
          expect(breaks[i - 1]).toBeLessThanOrEqual(breaks[i]);
        }
      }
    }
  });

  it('never leaves a tier empty', () => {
    // Every tier must claim at least one input value, else the legend shows a
    // range nothing falls into.
    const cases = [
      new Array(20).fill(25),
      [...new Array(20).fill(25), 40],
      [...new Array(10).fill(1), ...new Array(10).fill(2)],
      [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    ];
    const tierOf = (breaks: number[], v: number) => {
      for (let i = 0; i < breaks.length; i++) if (v <= breaks[i]) return i;
      return breaks.length;
    };
    for (const m of methods) {
      for (const vals of cases) {
        const { colors, breaks } = ColorTiers.scale(vals, m);
        const used = new Set(vals.map((v) => tierOf(breaks, v)));
        expect(
          used.size,
          `[${vals}] via ${m} used ${used.size} of ${colors.length}`,
        ).toBe(colors.length);
      }
    }
  });
});
