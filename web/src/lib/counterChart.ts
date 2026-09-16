import { extent } from 'd3-array';
import { scaleLinear, scaleTime } from 'd3-scale';
import type { CounterPoint } from '../types/generated/CounterPoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { ChartTick, PlotRect } from './chartPlot';
import dayjs from './dayjs';
import { downtimeWindows, lifecycleMarkers, splitSegments } from './downtime';
import type { SystemEventMarkerPx } from './gaugeChart';
import type { ChartRange } from './routes';

const MARGIN = { left: 48, top: 16, right: 16, bottom: 28 };

export type CounterSeriesKey = 'added_txs' | 'confirmed_txs' | 'evicted_txs';

const SERIES_KEYS: CounterSeriesKey[] = [
  'added_txs',
  'confirmed_txs',
  'evicted_txs',
];

const valueAt = (key: CounterSeriesKey) => (p: CounterPoint) => p[key];

export interface CounterLayoutInput {
  points: CounterPoint[];
  resolutionSecs: number;
  events: SystemEvent[];
  domain: ChartRange;
  visible: Record<CounterSeriesKey, boolean>;
}

export interface CounterPointPx {
  px: number;
  py: number;
  point: CounterPoint;
}

/** All points on the shared x scale, nulls included -- one index into this covers all three series for hover/tooltip. */
export interface CounterHoverPoint {
  px: number;
  point: CounterPoint;
}

/** One gap-safe run of a single series. No areaPath: three fills stacked on one plot would occlude each other. */
export interface CounterSegment {
  points: CounterPointPx[];
  linePath: string;
}

export interface CounterSeriesLayout {
  segments: CounterSegment[];
}

export interface CounterLayout {
  hoverPoints: CounterHoverPoint[];
  series: Record<CounterSeriesKey, CounterSeriesLayout>;
  markers: SystemEventMarkerPx[];
  xTicks: ChartTick[];
  yTicks: ChartTick[];
  plot: PlotRect;
}

function linePathFor(points: readonly CounterPointPx[]): string {
  return points
    .map((p, i) => `${i ? 'L' : 'M'}${p.px.toFixed(1)},${p.py.toFixed(1)}`)
    .join('');
}

// A null is "not measured", not zero, so it must break the line regardless of
// how close its neighbours sit in time -- unlike a missing bucket, it can't be
// left to the resolution-based gap check in splitSegments (a single null sits
// exactly at the 2x-resolution boundary, which that check treats as healthy).
function nonNullRuns(
  points: CounterPoint[],
  get: (p: CounterPoint) => number | null,
): CounterPoint[][] {
  const runs: CounterPoint[][] = [];
  let current: CounterPoint[] = [];
  for (const p of points) {
    if (get(p) === null) {
      if (current.length) runs.push(current);
      current = [];
    } else {
      current.push(p);
    }
  }
  if (current.length) runs.push(current);
  return runs;
}

/** Three-series time layout sharing one pair of scales, viewBox `W x H`. */
export function counterLayout(
  input: CounterLayoutInput,
  W: number,
  H: number,
): CounterLayout {
  const { points, resolutionSecs, events, domain, visible } = input;

  const plot = {
    left: MARGIN.left,
    top: MARGIN.top,
    width: Math.max(0, W - MARGIN.left - MARGIN.right),
    height: Math.max(0, H - MARGIN.top - MARGIN.bottom),
  };

  const tMax = domain.to > domain.from ? domain.to : domain.from + 1;
  const xScale = scaleTime()
    .domain([domain.from, tMax])
    .range([plot.left, plot.left + plot.width]);

  // The y-domain only looks at visible series so hiding one lets the rest use
  // the full height, and only at non-null values so a masked-out gap can't
  // stretch the scale.
  const visibleKeys = SERIES_KEYS.filter((key) => visible[key]);
  const visibleValues: number[] = [];
  for (const p of points) {
    for (const key of visibleKeys) {
      const v = valueAt(key)(p);
      if (v !== null) visibleValues.push(v);
    }
  }
  const [, vMax = 0] = extent(visibleValues);
  const yMax = vMax > 0 ? vMax : 1;
  const yScale = scaleLinear()
    .domain([0, yMax])
    .nice()
    .range([plot.top + plot.height, plot.top]);

  const windows = downtimeWindows(events, domain.to);

  const series = Object.fromEntries(
    SERIES_KEYS.map((key) => {
      const get = valueAt(key);
      const segments: CounterSegment[] = nonNullRuns(points, get)
        .flatMap((run) => splitSegments(run, resolutionSecs, windows))
        .map((seg) => {
          const segPoints: CounterPointPx[] = seg.map((p) => ({
            px: xScale(dayjs(p.sampled_at).valueOf()),
            py: yScale(get(p) as number),
            point: p,
          }));
          return { points: segPoints, linePath: linePathFor(segPoints) };
        });
      return [key, { segments }];
    }),
  ) as Record<CounterSeriesKey, CounterSeriesLayout>;

  const hoverPoints: CounterHoverPoint[] = points.map((p) => ({
    px: xScale(dayjs(p.sampled_at).valueOf()),
    point: p,
  }));

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

  return { hoverPoints, series, markers, xTicks, yTicks, plot };
}
