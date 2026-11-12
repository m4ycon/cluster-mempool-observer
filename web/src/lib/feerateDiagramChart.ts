import { scaleLinear } from 'd3-scale';
import type { FeerateDiagramComparison } from '../types/generated/FeerateDiagramComparison';
import type { FeerateDiagramPoint } from '../types/generated/FeerateDiagramPoint';
import type { ChartTick, PlotRect } from './chartPlot';

const MARGIN = { left: 64, top: 16, right: 16, bottom: 40 };

/** Weight units in one block; the window selector and boundary lines are multiples of this. */
export const BLOCK_WEIGHT = 4_000_000;

export type FeerateDiagramWindow = 1 | 2 | 3 | 'all';

export const NODE_COLOR = '#f7931a';
export const OURS_COLOR = '#58a6ff';

/** The parts of a comparison the chart draws. */
export type FeerateDiagramCurves = Pick<
  FeerateDiagramComparison,
  'node' | 'ours' | 'missing_chunks' | 'extra_chunks'
>;

export type FeerateChunkDiff = 'missing' | 'extra';

export const CHUNK_DIFF_COLOR: Record<FeerateChunkDiff, string> = {
  missing: NODE_COLOR,
  extra: OURS_COLOR,
};

/** A divergent chunk, placed at its midpoint on the curve of the side that has it. */
export interface FeerateChunkMark {
  diff: FeerateChunkDiff;
  px: number;
  py: number;
  feeSats: number;
  weight: number;
  satPerVb: number;
}

export interface FeerateDiagramPointPx {
  px: number;
  py: number;
  point: FeerateDiagramPoint;
  /** This chunk's own feerate, from its raw neighbour before decimation. */
  marginalSatPerVb: number | null;
}

/** One block-edge line; `weight` is its multiple of BLOCK_WEIGHT, for optional labeling. */
export interface FeerateDiagramBlockBoundary {
  weight: number;
  px: number;
}

export interface FeerateDiagramLayout {
  node: {
    points: FeerateDiagramPointPx[];
    linePath: string;
    areaPath: string;
  };
  ours: { points: FeerateDiagramPointPx[]; linePath: string };
  marks: FeerateChunkMark[];
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  plot: PlotRect;
  blockBoundaries: FeerateDiagramBlockBoundary[];
}

function totalWeight(points: readonly FeerateDiagramPoint[]): number {
  // Points are cumulative and monotonically non-decreasing,
  // so the last point alone holds the total.
  return points.length === 0 ? 0 : points[points.length - 1].weight;
}

/** X-domain max for `blockWindow`: `blockWindow * BLOCK_WEIGHT`, capped at the diagram's total weight. */
export function windowMaxWeight(
  points: readonly FeerateDiagramPoint[],
  blockWindow: FeerateDiagramWindow,
): number {
  const total = totalWeight(points);
  return blockWindow === 'all'
    ? total
    : Math.min(blockWindow * BLOCK_WEIGHT, total);
}

/** Keeps only points within `blockWindow`'s weight range. */
export function filterToWindow(
  points: readonly FeerateDiagramPoint[],
  blockWindow: FeerateDiagramWindow,
): FeerateDiagramPoint[] {
  const max = windowMaxWeight(points, blockWindow);
  return points.filter((p) => p.weight <= max);
}

/** Every multiple of BLOCK_WEIGHT up to and including `windowMax`. */
export function blockBoundaryWeights(windowMax: number): number[] {
  const weights: number[] = [];
  for (let w = BLOCK_WEIGHT; w <= windowMax; w += BLOCK_WEIGHT) {
    weights.push(w);
  }
  return weights;
}

/**
 * Collapses `points` to at most one per horizontal pixel, keeping the last
 * (highest-value) point per bucket -- the curve is monotonic, so that point
 * is the curve's value at the bucket's right edge. Always preserves the
 * true first and last points so the curve still spans the full domain, even
 * if they'd otherwise be decimated away into a shared bucket.
 */
export function decimatePoints(
  points: readonly FeerateDiagramPointPx[],
): FeerateDiagramPointPx[] {
  if (points.length <= 2) return [...points];

  const out: FeerateDiagramPointPx[] = [points[0]];
  let bucket = Math.floor(points[0].px);

  for (let i = 1; i < points.length; i++) {
    const p = points[i];
    const b = Math.floor(p.px);
    if (b === bucket) {
      out[out.length - 1] = p;
    } else {
      out.push(p);
      bucket = b;
    }
  }

  // The last input point always ends up as out's last entry (it's the final
  // loop iteration, so it either replaces or is pushed). The first input
  // point has no such guarantee -- an early bucket collision can overwrite
  // out[0] before the first push -- so it needs restoring explicitly.
  if (out[0] !== points[0]) {
    out.unshift(points[0]);
  }

  return out;
}

/**
 * Slope between `points[index]` and its predecessor, in sat/vB (weight is in
 * WU; 1 vB = 4 WU). Call with the RAW windowed points, before decimation --
 * on the decimated array the predecessor is a pixel away, so the result is an
 * average over every chunk in between rather than this chunk's own rate.
 * Null where no rate exists: index 0 (the origin has no predecessor) and a
 * repeated weight. Not 0 -- the mempool tail really does approach 0 sat/vB,
 * so a 0 sentinel would render as a plausible reading.
 */
export function marginalFeerateAt(
  points: readonly FeerateDiagramPoint[],
  index: number,
): number | null {
  if (index <= 0 || index >= points.length) return null;
  const cur = points[index];
  const prev = points[index - 1];
  const dWeight = cur.weight - prev.weight;
  if (dWeight <= 0) return null;
  return (4 * (cur.fee_sats - prev.fee_sats)) / dWeight;
}

function linePathOf(points: readonly FeerateDiagramPointPx[]): string {
  return points
    .map((p, i) => `${i ? 'L' : 'M'}${p.px.toFixed(1)},${p.py.toFixed(1)}`)
    .join('');
}

/** The chunk that ends at `points[index]`, in data space; none for the origin. */
function chunkEndingAt(
  diff: FeerateChunkDiff,
  points: readonly FeerateDiagramPoint[],
  index: number,
) {
  if (index <= 0 || index >= points.length) return null;
  const start = points[index - 1];
  const end = points[index];
  const weight = end.weight - start.weight;
  const feeSats = end.fee_sats - start.fee_sats;
  return {
    diff,
    feeSats,
    weight,
    satPerVb: (4 * feeSats) / weight,
    endWeight: end.weight,
    midWeight: start.weight + weight / 2,
    midFee: start.fee_sats + feeSats / 2,
  };
}

const EMPTY_CURVE = { points: [], linePath: '' };
const EMPTY_NODE_CURVE = { ...EMPTY_CURVE, areaPath: '' };

/** Both curves clipped to `blockWindow` and the divergent chunks on them, viewBox `W x H`. */
export function feerateDiagramLayout(
  curves: FeerateDiagramCurves,
  blockWindow: FeerateDiagramWindow,
  W: number,
  H: number,
): FeerateDiagramLayout {
  const points = curves.node;
  const plot = {
    left: MARGIN.left,
    top: MARGIN.top,
    width: Math.max(0, W - MARGIN.left - MARGIN.right),
    height: Math.max(0, H - MARGIN.top - MARGIN.bottom),
  };

  if (points.length === 0) {
    return {
      node: EMPTY_NODE_CURVE,
      ours: EMPTY_CURVE,
      marks: [],
      xTicks: [],
      yTicks: [],
      plot,
      blockBoundaries: [],
    };
  }

  // Extra chunks can push our curve past the node's; the window covers both.
  const longest =
    totalWeight(curves.ours) > totalWeight(points) ? curves.ours : points;
  const windowMax = windowMaxWeight(longest, blockWindow);
  // Guard the zero-width domain a window that clips to nothing (or a
  // single origin-only point) would otherwise produce.
  const xDomainMax = windowMax > 0 ? windowMax : 1;
  const filtered = filterToWindow(points, blockWindow);
  // Monotonic cumulative points always keep the origin, so this is unreachable
  // today; it stops a future non-monotonic response from crashing on yMax below.
  if (filtered.length === 0) {
    return {
      node: EMPTY_NODE_CURVE,
      ours: EMPTY_CURVE,
      marks: [],
      xTicks: [],
      yTicks: [],
      plot,
      blockBoundaries: [],
    };
  }

  const xScale = scaleLinear()
    .domain([0, xDomainMax])
    .range([plot.left, plot.left + plot.width]);

  // Fee axis rescales to what's actually visible in this window, not the
  // whole-mempool total -- otherwise a 1-block window would draw as a
  // near-flat sliver against a scale sized for the full curve.
  const oursFiltered = filterToWindow(curves.ours, blockWindow);
  const yMax = Math.max(
    filtered[filtered.length - 1].fee_sats,
    oursFiltered.length > 0
      ? oursFiltered[oursFiltered.length - 1].fee_sats
      : 0,
  );
  const yDomainMax = yMax > 0 ? yMax : 1;
  const yScale = scaleLinear()
    .domain([0, yDomainMax])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const toPx = (curve: FeerateDiagramPoint[]): FeerateDiagramPointPx[] =>
    decimatePoints(
      curve.map((p, i) => ({
        px: xScale(p.weight),
        py: yScale(p.fee_sats),
        point: p,
        marginalSatPerVb: marginalFeerateAt(curve, i),
      })),
    );
  const decimated = toPx(filtered);
  const linePath = linePathOf(decimated);
  const oursPoints = toPx(oursFiltered);

  const marks: FeerateChunkMark[] = [
    ...curves.missing_chunks.map((i) => chunkEndingAt('missing', points, i)),
    ...curves.extra_chunks.map((i) => chunkEndingAt('extra', curves.ours, i)),
  ].flatMap((chunk) =>
    // Only chunks the clipped curve draws in full, so no mark floats past its end.
    chunk && chunk.endWeight <= windowMax
      ? [
          {
            diff: chunk.diff,
            px: xScale(chunk.midWeight),
            py: yScale(chunk.midFee),
            feeSats: chunk.feeSats,
            weight: chunk.weight,
            satPerVb: chunk.satPerVb,
          },
        ]
      : [],
  );

  const baseline = yScale(0);
  const first = decimated[0];
  const last = decimated[decimated.length - 1];
  const areaPath = `${linePath}L${last.px.toFixed(1)},${baseline.toFixed(1)}L${first.px.toFixed(1)},${baseline.toFixed(1)}Z`;

  const xTicks: ChartTick[] = xScale
    .ticks(4)
    .map((v) => ({ value: v, px: xScale(v) }));
  const yTicks: ChartTick[] = yScale
    .ticks(4)
    .map((v) => ({ value: v, px: yScale(v) }));

  const blockBoundaries: FeerateDiagramBlockBoundary[] = blockBoundaryWeights(
    windowMax,
  ).map((weight) => ({ weight, px: xScale(weight) }));

  return {
    node: { points: decimated, linePath, areaPath },
    ours: { points: oursPoints, linePath: linePathOf(oursPoints) },
    marks,
    xTicks,
    yTicks,
    plot,
    blockBoundaries,
  };
}
