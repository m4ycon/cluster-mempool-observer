import type { ClusterRef } from '../types/events';
import { type ColorScale, ColorTiers } from './colorTiers';
import { NumberFormat } from './format';

export type ClusterMetric = 'vsize' | 'fee' | 'feerate' | 'txs';

const LABEL: Record<ClusterMetric, string> = {
  vsize: 'vsize',
  fee: 'total fee',
  feerate: 'fee-rate',
  txs: 'tx count',
};

const UNIT: Record<ClusterMetric, string> = {
  vsize: 'vB',
  fee: 'sats',
  feerate: 's/vB',
  txs: 'txs',
};

/** Reads the value of a given metric off a cluster. */
function value(c: ClusterRef, key: ClusterMetric): number {
  switch (key) {
    case 'vsize':
      return c.total_vsize;
    case 'fee':
      return c.total_fee;
    case 'feerate':
      return c.total_vsize ? c.total_fee / c.total_vsize : 0;
    case 'txs':
      return c.txids.length;
  }
}

/** Top `n` clusters by the given size metric, descending. */
function top(
  clusters: ClusterRef[],
  sizeMetric: ClusterMetric,
  n: number,
): ClusterRef[] {
  return [...clusters]
    .sort((a, b) => value(b, sizeMetric) - value(a, sizeMetric))
    .slice(0, n);
}

/** Smallest difference a label for this metric can show. */
const STEP: Record<ClusterMetric, number> = {
  vsize: 1,
  fee: 1,
  feerate: 0.1, // rendered with one decimal
  txs: 1,
};

/**
 * Colour scale over the currently visible values, so the legend always spans
 * the on-screen range. Compute once per render and share it between the marks
 * and the legend, so the two can never disagree.
 *
 * Breaks are snapped to the metric's display precision, then kept only where
 * they still split the visible data. A break the legend would print the same as
 * its neighbour, or one snapping past the top of the data, describes a tier
 * nothing falls into -- so it goes, and the ramp is rebuilt for what is left.
 * Marks then bucket on exactly the numbers the legend shows.
 */
function scaleFor(
  colorMetric: ClusterMetric,
  allVisibleVals: number[],
): ColorScale {
  const raw = ColorTiers.scale(allVisibleVals);
  const step = STEP[colorMetric];
  const sorted = allVisibleVals.filter(Number.isFinite).sort((a, b) => a - b);
  const kept: number[] = [];
  let claimed = 0; // values already falling in a kept tier
  for (const b of raw.breaks) {
    const bound = Math.round(b / step) * step;
    if (kept.length > 0 && bound <= kept[kept.length - 1]) continue;
    const below = sorted.filter((v) => v <= bound).length;
    // Leave at least one value below this break and one above the last.
    if (below > claimed && below < sorted.length) {
      kept.push(bound);
      claimed = below;
    }
  }
  return ColorTiers.reTier(kept);
}

/** Mark colour for a value under a given scale. */
function colorAt(
  scale: ColorScale,
  v: number,
  colorMetric: ClusterMetric,
): string {
  const step = STEP[colorMetric];
  const vu = Math.round(v / step);
  for (let i = 0; i < scale.breaks.length; i++) {
    if (vu <= Math.round(scale.breaks[i] / step)) return scale.colors[i];
  }
  return scale.colors[scale.colors.length - 1];
}

function fmtBound(colorMetric: ClusterMetric, v: number): string {
  if (colorMetric === 'fee') return NumberFormat.grouped(Math.round(v));
  if (colorMetric === 'feerate') return v.toFixed(1); // small magnitude
  return String(Math.round(v));
}

/**
 * Legend range labels, one per tier of `scale`, low to high.
 *
 * Tiers are open below and closed above -- `colorAt` puts a value sitting
 * exactly on a break in the tier below it. Printing the raw break as both the
 * previous tier's ceiling and the next tier's floor would make every boundary
 * look like it belongs to two tiers, so each floor is nudged up by one display
 * step. Arithmetic runs in whole steps rather than on the bounds themselves, so
 * a fractional step (feerate) cannot drift the comparison.
 */
function legendRanges(
  colorMetric: ClusterMetric,
  allVisibleVals: number[],
  scale: ColorScale = scaleFor(colorMetric, allVisibleVals),
): string[] {
  const step = STEP[colorMetric];
  const u = scale.breaks.map((v) => Math.round(v / step));
  const fmt = (units: number) => fmtBound(colorMetric, units * step);

  if (u.length === 0) {
    // Single tier: nothing the legend can print would split these values, so
    // state the span instead of inventing ranges.
    const finite = allVisibleVals.filter(Number.isFinite);
    if (finite.length === 0) return [];
    const lo = Math.round(Math.min(...finite) / step);
    const hi = Math.round(Math.max(...finite) / step);
    return [lo === hi ? `all ${fmt(lo)}` : `${fmt(lo)}-${fmt(hi)}`];
  }

  const labels = [`<=${fmt(u[0])}`];
  for (let i = 1; i < u.length; i++) {
    // One step wide: name the single value instead of an empty-looking range.
    const lo = u[i - 1] + 1;
    labels.push(lo === u[i] ? fmt(u[i]) : `${fmt(lo)}-${fmt(u[i])}`);
  }
  labels.push(`>${fmt(u[u.length - 1])}`);
  return labels;
}

/** Formats a metric value for an on-mark label (SIZE-BY value). */
function markLabel(v: number, metric: ClusterMetric): string {
  return metric === 'feerate' ? v.toFixed(1) : NumberFormat.compact(v);
}

/**
 * Nudges `v` onto the metric's display grid (nearest whole STEP) -- e.g. so a
 * histogram can bin, and compare, on the values as they're actually shown
 * rather than on raw floats a label would silently round away. Mirrors what
 * `colorAt` already does when it buckets a value against tier breaks.
 */
function roundToStep(metric: ClusterMetric, v: number): number {
  const step = STEP[metric];
  return Math.round(v / step) * step;
}

/**
 * Label for a histogram bin's `[lo, hi)` range (see histogramLayout in
 * clusterHistogram.ts for why bins are half-open, except the final one,
 * which is closed `[lo, hi]` so the maximum value has somewhere to land).
 */
function binRangeLabel(
  metric: ClusterMetric,
  lo: number,
  hi: number,
  isFinal: boolean,
): string {
  const step = STEP[metric];
  const loU = Math.round(lo / step);
  const hiU = Math.round(hi / step);
  const fmt = (units: number) => fmtBound(metric, units * step);
  const printedHiU = isFinal ? hiU : hiU - 1;
  return loU === printedHiU ? fmt(loU) : `${fmt(loU)}-${fmt(printedHiU)}`;
}

/** Theme-grouped cluster-metric helpers. */
export const ClusterMetrics = {
  LABEL,
  UNIT,
  value,
  top,
  scaleFor,
  colorAt,
  legendRanges,
  markLabel,
  STEP,
  fmtBound,
  roundToStep,
  binRangeLabel,
};
