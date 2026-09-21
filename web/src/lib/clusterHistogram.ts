import { bin as d3Bin, extent } from 'd3-array';
import { scaleLinear } from 'd3-scale';
import type { ClusterRef } from '../types/events';
import type { ChartTick, PlotRect } from './chartPlot';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';

const MARGIN = { left: 64, top: 16, right: 16, bottom: 40 };

export interface HistogramBar {
  /** Value-space bin bounds, for tooltips and axis labels. */
  lo: number;
  hi: number;
  count: number;
  x0: number; // pixel left
  x1: number; // pixel right
  y0: number; // pixel top
  y1: number; // pixel baseline
}

export interface HistogramLayout {
  bars: HistogramBar[];
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  /** Largest bar count; 0 only when `bars` is empty (no input clusters). */
  maxCount: number;
  /** Plot rect inside the viewBox, so the component can place axis labels. */
  plot: PlotRect;
}

/**
 * Buckets clusters into a count histogram over `sizeMetric`, viewBox `W x H`.
 *
 * Bars are plain linear bins over the full input (no log scale, no top-N
 * filtering, no percentile clamping) -- see clusterHistogram.test.ts / the
 * Clusters page for why: this is intentionally the simple first cut.
 */
export function histogramLayout(
  clusters: ClusterRef[],
  sizeMetric: ClusterMetric,
  binCount: number,
  W = 720,
  H = 560,
): HistogramLayout {
  const plot = {
    left: MARGIN.left,
    top: MARGIN.top,
    width: Math.max(0, W - MARGIN.left - MARGIN.right),
    height: Math.max(0, H - MARGIN.top - MARGIN.bottom),
  };

  if (clusters.length === 0) {
    return { bars: [], xTicks: [], yTicks: [], maxCount: 0, plot };
  }

  const step = ClusterMetrics.STEP[sizeMetric];
  const displayValue = (c: ClusterRef) =>
    ClusterMetrics.roundToStep(sizeMetric, ClusterMetrics.value(c, sizeMetric));

  // extent(), not Math.min/max(...values): clusters is the whole mempool for
  // this viz (the top-N showCount filter is bypassed), so a spread here can
  // blow the call stack under congestion. extent() loops instead of spreading.
  const [rawLo = 0, rawHi = 0] = extent(clusters, displayValue);
  // Guard the zero-width domain a single cluster, or an all-identical set,
  // would otherwise produce (mirrors the Math.max guard in clusterLayout.ts).
  // Bumped by one display step, not a hardcoded 1, so it stays correct for
  // sub-1 steps like feerate.
  const hi = rawHi > rawLo ? rawHi : rawLo + step;

  // thresholds() is a hint, not a guarantee: d3 snaps to "nice" round bin
  // edges, so the actual bar count can (and usually does) differ from
  // binCount. That is desirable here, not a bug to chase.
  const maxBinCount = Math.max(1, Math.floor((hi - rawLo) / step));
  const effectiveBinCount = Math.min(binCount, maxBinCount);

  const generator = d3Bin<ClusterRef, number>()
    .value(displayValue)
    .domain([rawLo, hi])
    .thresholds(effectiveBinCount);
  const rawBins = generator(clusters);

  const xScale = scaleLinear()
    .domain([rawLo, hi])
    .range([plot.left, plot.left + plot.width]);

  // No floor needed: clusters is non-empty here (guarded above) and every
  // real value falls into some bin (d3.bin clips edge thresholds to the
  // domain), so at least one bin is non-empty and maxCount is never 0.
  const maxCount = Math.max(...rawBins.map((b) => b.length));
  const yScale = scaleLinear()
    .domain([0, maxCount])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const baseline = yScale(0);
  const bars: HistogramBar[] = rawBins.map((b) => {
    const binLo = b.x0 ?? rawLo;
    const binHi = b.x1 ?? hi;

    return {
      lo: binLo,
      hi: binHi,
      count: b.length,
      x0: xScale(binLo),
      x1: xScale(binHi),
      y0: yScale(b.length),
      y1: baseline,
    };
  });

  const xTicks: ChartTick[] = xScale
    .ticks(6)
    .map((value) => ({ value, px: xScale(value) }));
  const yTicks: ChartTick[] = yScale
    .ticks(5)
    .map((value) => ({ value, px: yScale(value) }));

  return { bars, xTicks, yTicks, maxCount, plot };
}
