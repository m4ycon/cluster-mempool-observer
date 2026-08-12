import { extent } from 'd3-array';
import { scaleLinear, scaleTime } from 'd3-scale';
import type { MempoolMetricPoint } from '../types/generated/MempoolMetricPoint';
import type { ChartTick, PlotRect } from './chartPlot';

const MARGIN = { left: 48, top: 16, right: 16, bottom: 28 };

export interface MempoolMetricPointPx {
  px: number;
  py: number;
  point: MempoolMetricPoint;
}

export interface MempoolMetricLayout {
  points: MempoolMetricPointPx[];
  linePath: string;
  areaPath: string;
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  plot: PlotRect;
}

const sampledAtMs = (p: MempoolMetricPoint) => new Date(p.sampled_at).getTime();
const value = (p: MempoolMetricPoint) => p.value;

/** Time-series line/area layout for `points`, viewBox `W x H`. */
export function mempoolMetricLayout(
  points: MempoolMetricPoint[],
  W: number,
  H: number,
): MempoolMetricLayout {
  const plot = {
    left: MARGIN.left,
    top: MARGIN.top,
    width: Math.max(0, W - MARGIN.left - MARGIN.right),
    height: Math.max(0, H - MARGIN.top - MARGIN.bottom),
  };

  if (points.length === 0) {
    return {
      points: [],
      linePath: '',
      areaPath: '',
      xTicks: [],
      yTicks: [],
      plot,
    };
  }

  const [t0 = 0, t1 = 0] = extent(points, sampledAtMs);
  // Guard the zero-width domain a single point (or an all-identical
  // timestamp set) would otherwise produce.
  const tMax = t1 > t0 ? t1 : t0 + 1;

  const [, vMax = 0] = extent(points, value);
  // Baseline the domain at 0 so the area fill always reads "from zero", and
  // an all-zero series still renders a flat line instead of a [0, 0] scale.
  const yMax = vMax > 0 ? vMax : 1;

  const xScale = scaleTime()
    .domain([t0, tMax])
    .range([plot.left, plot.left + plot.width]);
  const yScale = scaleLinear()
    .domain([0, yMax])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const pxPoints: MempoolMetricPointPx[] = points.map((p) => ({
    px: xScale(sampledAtMs(p)),
    py: yScale(p.value),
    point: p,
  }));

  const linePath = pxPoints
    .map((p, i) => `${i ? 'L' : 'M'}${p.px.toFixed(1)},${p.py.toFixed(1)}`)
    .join('');

  const baseline = yScale(0);
  const first = pxPoints[0];
  const last = pxPoints[pxPoints.length - 1];
  const areaPath = `${linePath}L${last.px.toFixed(1)},${baseline.toFixed(1)}L${first.px.toFixed(1)},${baseline.toFixed(1)}Z`;

  const xTicks: ChartTick[] = xScale
    .ticks(4)
    .map((d) => ({ value: d.getTime(), px: xScale(d) }));
  const yTicks: ChartTick[] = yScale
    .ticks(4)
    .map((v) => ({ value: v, px: yScale(v) }));

  return { points: pxPoints, linePath, areaPath, xTicks, yTicks, plot };
}

/** Index of the point nearest `px` in pixel-x, for hover hit-testing. */
export function nearestPointIndex(
  points: MempoolMetricPointPx[],
  px: number,
): number {
  let nearest = 0;
  let nearestDist = Number.POSITIVE_INFINITY;
  for (let i = 0; i < points.length; i++) {
    const dist = Math.abs(points[i].px - px);
    if (dist < nearestDist) {
      nearestDist = dist;
      nearest = i;
    }
  }
  return nearest;
}
