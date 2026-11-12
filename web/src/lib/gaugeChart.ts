import { max } from 'd3-array';
import { scaleLinear, scaleTime } from 'd3-scale';
import type { GaugePoint } from '../types/generated/GaugePoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { ChartTick, PlotRect } from './chartPlot';
import dayjs from './dayjs';
import {
  downtimeWindows,
  GAP_FACTOR,
  lifecycleMarkers,
  splitSegments,
} from './downtime';
import type { ChartRange } from './routes';

const MARGIN = { left: 64, top: 16, right: 16, bottom: 40 };

export interface GaugeLayoutInput {
  /** One array per drawn series, each ascending by `sampled_at`. */
  series: GaugePoint[][];
  resolutionSecs: number;
  events: SystemEvent[];
  domain: ChartRange;
}

/** A point that holds a reading; the layout leaves the others out as gaps. */
export type MeasuredGaugePoint = GaugePoint & { value: number };

export interface GaugePointPx {
  px: number;
  py: number;
  point: MeasuredGaugePoint;
}

/** One gap-safe run of points. */
export interface GaugeSegment {
  points: GaugePointPx[];
  linePath: string;
  areaPath: string;
}

/** One series: its points and the gap-safe runs it draws as. */
export interface GaugeLine {
  /** Flat, ordered, measured points -- hover hit-testing uses this, not the segments. */
  points: GaugePointPx[];
  segments: GaugeSegment[];
}

/** A stretch of the x axis where no series holds a reading. */
export interface GaugeGapPx {
  x0: number;
  x1: number;
}

export interface SystemEventMarkerPx {
  px: number;
  event: SystemEvent;
}

export interface GaugeLayout {
  /** Aligned by index with the input series. */
  lines: GaugeLine[];
  gaps: GaugeGapPx[];
  markers: SystemEventMarkerPx[];
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  plot: PlotRect;
}

const isMeasured = (p: GaugePoint): p is MeasuredGaugePoint => p.value !== null;

const epoch = (p: GaugePoint) => dayjs(p.sampled_at).valueOf();

/** Splits at every sample without a reading, dropping those samples. */
function measuredRuns(points: readonly GaugePoint[]): MeasuredGaugePoint[][] {
  const runs: MeasuredGaugePoint[][] = [];
  let current: MeasuredGaugePoint[] = [];
  for (const p of points) {
    if (isMeasured(p)) {
      current.push(p);
    } else if (current.length > 0) {
      runs.push(current);
      current = [];
    }
  }
  if (current.length > 0) runs.push(current);
  return runs;
}

/**
 * One sample per instant any series was sampled at, measured when any series
 * holds a reading there.
 */
function anyReading(series: readonly GaugePoint[][]): GaugePoint[] {
  const byTime = new Map<string, GaugePoint>();
  for (const p of series.flat()) {
    const seen = byTime.get(p.sampled_at);
    if (!seen || seen.value === null) byTime.set(p.sampled_at, p);
  }
  return [...byTime.values()].sort((a, b) => epoch(a) - epoch(b));
}

/** The domain left uncovered between, before and after the gap-safe runs. */
function gapWindows(
  runs: readonly GaugePoint[][],
  domain: ChartRange,
  maxGapMs: number,
): ChartRange[] {
  if (runs.length === 0) return [domain];

  const gaps: ChartRange[] = [];
  const firstAt = epoch(runs[0][0]);
  if (firstAt - domain.from > maxGapMs) {
    gaps.push({ from: domain.from, to: firstAt });
  }
  for (let i = 1; i < runs.length; i++) {
    const prev = runs[i - 1];
    gaps.push({ from: epoch(prev[prev.length - 1]), to: epoch(runs[i][0]) });
  }
  const lastRun = runs[runs.length - 1];
  const lastAt = epoch(lastRun[lastRun.length - 1]);
  if (domain.to - lastAt > maxGapMs) {
    gaps.push({ from: lastAt, to: domain.to });
  }
  return gaps;
}

function segmentPaths(
  points: readonly GaugePointPx[],
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
export function gaugeLayout(
  input: GaugeLayoutInput,
  W: number,
  H: number,
): GaugeLayout {
  const { resolutionSecs, events, domain } = input;

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

  const vMax = max(input.series.flat(), (p) => p.value) ?? 0;
  // Baseline the domain at 0 so the area fill always reads "from zero", and
  // an all-zero (or empty) series still gets [0, 1] instead of a [0, 0] scale.
  const yMax = vMax > 0 ? vMax : 1;
  const yScale = scaleLinear()
    .domain([0, yMax])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const toPx = (pts: readonly MeasuredGaugePoint[]): GaugePointPx[] =>
    pts.map((p) => ({
      px: xScale(epoch(p)),
      py: yScale(p.value),
      point: p,
    }));

  const baseline = yScale(0);
  const windows = downtimeWindows(events, domain.to);
  const gapSafeRuns = <T extends GaugePoint>(runs: T[][]): T[][] =>
    runs.flatMap((run) => splitSegments(run, resolutionSecs, windows));

  const lines: GaugeLine[] = input.series.map((points) => ({
    points: toPx(points.filter(isMeasured)),
    segments: gapSafeRuns(measuredRuns(points)).map((seg) => {
      const segPoints = toPx(seg);
      return { points: segPoints, ...segmentPaths(segPoints, baseline) };
    }),
  }));

  const gaps: GaugeGapPx[] = gapWindows(
    gapSafeRuns(measuredRuns(anyReading(input.series))),
    domain,
    GAP_FACTOR * resolutionSecs * 1000,
  ).map((gap) => ({ x0: xScale(gap.from), x1: xScale(gap.to) }));

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

  return { lines, gaps, markers, xTicks, yTicks, plot };
}
