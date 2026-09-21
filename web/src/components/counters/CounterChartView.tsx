import { type MouseEvent, useRef, useState } from 'react';
import { nearestPointIndex } from '../../lib/chartPlot';
import {
  type CounterPointPx,
  type CounterSeriesKey,
  counterLayout,
} from '../../lib/counterChart';
import dayjs from '../../lib/dayjs';
import { NumberFormat } from '../../lib/format';
import { resolutionLabel } from '../../lib/resolutionLabel';
import type { ChartRange } from '../../lib/routes';
import type { CounterPoint } from '../../types/generated/CounterPoint';
import type { CounterSeries } from '../../types/generated/CounterSeries';
import type { SystemEvent } from '../../types/generated/SystemEvent';
import { AxisX } from '../charts/AxisX';
import { ChartTooltip } from '../charts/ChartTooltip';
import { VizButton, type VizButtonVariant } from '../VizButton';

const VIEW_W = 960;
const VIEW_H = 320;

const TOOLTIP_GAP = 12; // offset from the hovered point

const MARKER_HIT_WIDTH = 8; // a 1px dashed line is too thin to hover

const SERIES_ORDER: CounterSeriesKey[] = [
  'added_txs',
  'confirmed_txs',
  'evicted_txs',
];

const SERIES_META: Record<
  CounterSeriesKey,
  { label: string; color: string; variant: VizButtonVariant }
> = {
  added_txs: { label: 'ARRIVALS', color: '#3fb950', variant: 'live' },
  confirmed_txs: { label: 'MINED', color: '#f7931a', variant: 'accent' },
  evicted_txs: { label: 'EVICTED', color: '#d9544d', variant: 'alert' },
};

/** Finds the pixel-space point matching `point` within `key`'s segments, by reference. */
function pxPointFor(
  segments: { points: CounterPointPx[] }[],
  point: CounterPoint,
): CounterPointPx | undefined {
  for (const seg of segments) {
    const found = seg.points.find((p) => p.point === point);
    if (found) return found;
  }
  return undefined;
}

export interface CounterChartViewProps {
  series: CounterSeries;
  range: ChartRange;
  events: SystemEvent[];
}

/** Three-series line chart (arrivals/confirmed/evicted) with a shared crosshair tooltip. */
export function CounterChartView({
  series,
  range,
  events,
}: CounterChartViewProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [showEvents, setShowEvents] = useState(true);
  const [visible, setVisible] = useState<Record<CounterSeriesKey, boolean>>({
    added_txs: true,
    confirmed_txs: true,
    evicted_txs: true,
  });

  if (series.points.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        no samples in range
      </div>
    );
  }

  const layout = counterLayout(
    {
      points: series.points,
      resolutionSecs: series.resolution_secs,
      events,
      domain: range,
      visible,
    },
    VIEW_W,
    VIEW_H,
  );
  const { plot } = layout;
  const hovered =
    hoveredIndex !== null ? layout.hoverPoints[hoveredIndex] : null;
  const visibleKeys = SERIES_ORDER.filter((key) => visible[key]);
  const hasEvents = layout.markers.length > 0;

  const handleMouseMove = (e: MouseEvent<SVGRectElement>) => {
    const svg = svgRef.current;
    if (!svg) return;

    const rect = svg.getBoundingClientRect();
    if (rect.width === 0) return;

    const localX = ((e.clientX - rect.left) / rect.width) * VIEW_W;
    setHoveredIndex(nearestPointIndex(layout.hoverPoints, localX));
  };

  const toggleSeries = (key: CounterSeriesKey) =>
    setVisible((v) => ({ ...v, [key]: !v[key] }));

  // Only visible series appear here, so the tooltip and the plot always agree.
  const tooltipLines = hovered
    ? [
        dayjs(hovered.point.sampled_at).format('YYYY-MM-DD HH:mm:ss'),
        ...visibleKeys.map((key) => {
          const value = hovered.point[key];
          const shown =
            value === null ? 'not measured' : NumberFormat.grouped(value);
          return `${SERIES_META[key].label}: ${shown}`;
        }),
      ]
    : [];
  const tooltipColors = hovered
    ? [undefined, ...visibleKeys.map((key) => SERIES_META[key].color)]
    : [];

  return (
    <div
      className="flex h-full w-full flex-col gap-1"
      data-testid="counter-chart"
      data-point-count={layout.hoverPoints.length}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs text-dim">
          {resolutionLabel(series.resolution_secs)}
        </span>

        <div className="flex items-center gap-2">
          {SERIES_ORDER.map((key) => (
            <VizButton
              key={key}
              active={visible[key]}
              onClick={() => toggleSeries(key)}
              variant={SERIES_META[key].variant}
            >
              {SERIES_META[key].label}
            </VizButton>
          ))}
          {hasEvents && (
            <VizButton
              active={showEvents}
              onClick={() => setShowEvents((v) => !v)}
            >
              EVENTS
            </VizButton>
          )}
        </div>
      </div>

      <svg
        ref={svgRef}
        width="100%"
        height="100%"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="xMidYMid meet"
        className="flex-1"
        role="img"
        aria-label="Mempool arrivals, confirmations and evictions over time"
      >
        {/* Y axis + gridlines */}
        {layout.yTicks.map((t) => (
          <g key={t.value}>
            <line
              x1={plot.left}
              y1={t.px}
              x2={plot.left + plot.width}
              y2={t.px}
              className="stroke-line"
              strokeWidth={1}
            />
            <text
              x={plot.left - 8}
              y={t.px + 3}
              textAnchor="end"
              className="fill-dim font-mono text-xs"
            >
              {NumberFormat.compact(t.value)}
            </text>
          </g>
        ))}

        <AxisX
          plot={plot}
          ticks={layout.xTicks}
          format={(v) => dayjs(v).format('HH:mm')}
        />

        {/* Lifecycle markers, drawn under the value lines so data stays dominant */}
        {showEvents && (
          <g className="pointer-events-none">
            {layout.markers.map((marker) => (
              <line
                key={marker.event.id}
                data-testid="event-marker"
                x1={marker.px}
                y1={plot.top}
                x2={marker.px}
                y2={plot.top + plot.height}
                strokeDasharray="4 4"
                strokeWidth={1}
                className={
                  marker.event.kind === 'server_stopped'
                    ? 'stroke-alert'
                    : 'stroke-live'
                }
              />
            ))}
          </g>
        )}

        {/* Lines, one per gap-safe segment per visible series; a lone point draws as a dot.
            No area fill: three stacked fills on one plot would occlude each other. */}
        {visibleKeys.map((key) =>
          layout.series[key].segments.map((segment, i) =>
            segment.points.length === 1 ? (
              <circle
                // biome-ignore lint/suspicious/noArrayIndexKey: segments are a stable, non-reordered layout output
                key={i}
                data-testid={`series-${key}`}
                cx={segment.points[0].px}
                cy={segment.points[0].py}
                r={3}
                fill={SERIES_META[key].color}
              />
            ) : (
              <path
                // biome-ignore lint/suspicious/noArrayIndexKey: segments are a stable, non-reordered layout output
                key={i}
                data-testid={`series-${key}`}
                d={segment.linePath}
                fill="none"
                stroke={SERIES_META[key].color}
                strokeWidth={2}
              />
            ),
          ),
        )}

        {/* Direct label on the most recent value of each visible series -- not every point */}
        {visibleKeys.map((key) => {
          const segments = layout.series[key].segments;
          if (segments.length === 0) return null;
          const lastSegment = segments[segments.length - 1];
          const last = lastSegment.points[lastSegment.points.length - 1];
          const color = SERIES_META[key].color;
          return (
            <g key={key} data-testid={`series-${key}-last`}>
              <circle cx={last.px} cy={last.py} r={3} fill={color} />
              <text
                x={last.px}
                y={last.py - 8}
                textAnchor="end"
                className="font-mono text-xs"
                style={{ fill: color }}
              >
                {NumberFormat.grouped(last.point[key] as number)}
              </text>
            </g>
          );
        })}

        {/* Hover overlay: one large hit target, nearest point found by x */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: hover-only hit area for the crosshair tooltip */}
        <rect
          x={plot.left}
          y={plot.top}
          width={plot.width}
          height={plot.height}
          fill="transparent"
          onMouseMove={handleMouseMove}
          onMouseLeave={() => setHoveredIndex(null)}
        />

        {/* Marker hit targets: wide and transparent, above the hover overlay so
            the native title is reachable -- the visible 1px lines never are. */}
        {showEvents &&
          layout.markers.map((marker) => (
            <line
              key={marker.event.id}
              data-testid="event-marker-hit"
              x1={marker.px}
              y1={plot.top}
              x2={marker.px}
              y2={plot.top + plot.height}
              stroke="transparent"
              strokeWidth={MARKER_HIT_WIDTH}
            >
              <title>
                {`${marker.event.kind} ${dayjs(marker.event.created_at).format('YYYY-MM-DD HH:mm:ss')}`}
              </title>
            </line>
          ))}

        {/* Crosshair + per-series dots + tooltip */}
        {hovered && (
          <g className="pointer-events-none">
            <line
              x1={hovered.px}
              y1={plot.top}
              x2={hovered.px}
              y2={plot.top + plot.height}
              className="stroke-dim"
              strokeWidth={1}
            />
            {visibleKeys.map((key) => {
              const px = pxPointFor(layout.series[key].segments, hovered.point);
              if (!px) return null;
              return (
                <circle
                  key={key}
                  cx={px.px}
                  cy={px.py}
                  r={3}
                  fill={SERIES_META[key].color}
                />
              );
            })}
            <ChartTooltip
              lines={tooltipLines}
              lineColors={tooltipColors}
              plot={plot}
              gap={TOOLTIP_GAP}
              rightOf={hovered.px}
              leftOf={hovered.px}
              anchorY={{ at: plot.top, align: 'above' }}
            />
          </g>
        )}
      </svg>
    </div>
  );
}
