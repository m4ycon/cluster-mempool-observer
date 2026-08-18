import { extent } from 'd3-array';
import { scaleLinear, scaleTime } from 'd3-scale';
import type { MempoolMetricPoint } from '../types/generated/MempoolMetricPoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { ChartTick, PlotRect } from './chartPlot';
import dayjs from './dayjs';
import { downtimeWindows, lifecycleMarkers, splitSegments } from './downtime';
import type { ChartRange } from './routes';

const MARGIN = { left: 48, top: 16, right: 16, bottom: 28 };

export interface MempoolMetricLayoutInput {
  points: MempoolMetricPoint[];
  resolutionSecs: number;
  events: SystemEvent[];
  domain: ChartRange;
}

export interface MempoolMetricPointPx {
  px: number;
  py: number;
  point: MempoolMetricPoint;
}

/** One gap-safe run of points. */
export interface MempoolMetricSegment {
  points: MempoolMetricPointPx[];
  linePath: string;
  areaPath: string;
}

export interface SystemEventMarkerPx {
  px: number;
  event: SystemEvent;
}

export interface MempoolMetricLayout {
  /** Flat, ordered, all points -- hover hit-testing uses this, not the segments. */
  points: MempoolMetricPointPx[];
  segments: MempoolMetricSegment[];
  markers: SystemEventMarkerPx[];
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  plot: PlotRect;
}

const value = (p: MempoolMetricPoint) => p.value;

function segmentPaths(
  points: readonly MempoolMetricPointPx[],
  baseline: number,
): { linePath: string; areaPath: string } {
  const linePath = points
    .map((p, i) => `${i ? 'L' : 'M'}${p.px.toFixed(1)},${p.py.toFixed(1)}`)
    .join('');

  const first = points[0];
  const last = points[points.length - 1];
  const areaPath = `${linePath}L${last.px.toFixed(1)},${baseline.toFixed(1)}L${first.px.toFixed(1)},${baseline.toFixed(1)}Z`;

  return { linePath, areaPath };
}

/** Time-series line/area layout for `input`, split into gap-safe segments, viewBox `W x H`. */
export function mempoolMetricLayout(
  input: MempoolMetricLayoutInput,
  W: number,
  H: number,
): MempoolMetricLayout {
  const { points, resolutionSecs, events, domain } = input;

  const plot = {
    left: MARGIN.left,
    top: MARGIN.top,
    width: Math.max(0, W - MARGIN.left - MARGIN.right),
    height: Math.max(0, H - MARGIN.top - MARGIN.bottom),
  };

  // Domain is the requested window, not the data extent -- that's what keeps
  // an in-progress outage visible as empty space at the right edge instead of
  // squeezed out of view.
  const tMax = domain.to > domain.from ? domain.to : domain.from + 1;
  const xScale = scaleTime()
    .domain([domain.from, tMax])
    .range([plot.left, plot.left + plot.width]);

  const [, vMax = 0] = extent(points, value);
  // Baseline the domain at 0 so the area fill always reads "from zero", and
  // an all-zero (or empty) series still gets [0, 1] instead of a [0, 0] scale.
  const yMax = vMax > 0 ? vMax : 1;
  const yScale = scaleLinear()
    .domain([0, yMax])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const toPx = (pts: readonly MempoolMetricPoint[]): MempoolMetricPointPx[] =>
    pts.map((p) => ({
      px: xScale(dayjs(p.sampled_at).valueOf()),
      py: yScale(p.value),
      point: p,
    }));

  const pxPoints = toPx(points);

  const baseline = yScale(0);
  const windows = downtimeWindows(events, domain.to);
  const segments: MempoolMetricSegment[] = splitSegments(
    points,
    resolutionSecs,
    windows,
  ).map((seg) => {
    const segPoints = toPx(seg);
    return { points: segPoints, ...segmentPaths(segPoints, baseline) };
  });

  const markers: SystemEventMarkerPx[] = lifecycleMarkers(events)
    .filter((event) => {
      const t = dayjs(event.created_at).valueOf();
      return t >= domain.from && t <= domain.to;
    })
    .map((event) => ({ px: xScale(dayjs(event.created_at).valueOf()), event }));

  const xTicks: ChartTick[] = xScale
    .ticks(4)
    .map((d) => ({ value: d.getTime(), px: xScale(d) }));
  const yTicks: ChartTick[] = yScale
    .ticks(4)
    .map((v) => ({ value: v, px: yScale(v) }));

  return { points: pxPoints, segments, markers, xTicks, yTicks, plot };
}
