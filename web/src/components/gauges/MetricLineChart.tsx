import { type MouseEvent, useRef, useState } from 'react';
import { nearestPointIndex, viewBoxX } from '../../lib/chartPlot';
import dayjs from '../../lib/dayjs';
import { gaugeLayout } from '../../lib/gaugeChart';
import { GAUGE_METRICS } from '../../lib/gaugeMetrics';
import { resolutionLabel } from '../../lib/resolutionLabel';
import type { ChartRange } from '../../lib/routes';
import type { GaugeSeries } from '../../types/generated/GaugeSeries';
import type { SystemEvent } from '../../types/generated/SystemEvent';
import { AxisX } from '../charts/AxisX';
import { AxisY } from '../charts/AxisY';
import { ChartTooltip } from '../charts/ChartTooltip';
import { VizButton } from '../VizButton';

export interface MetricLineChartProps {
  /** Same resolution and unit; the first one drives hover. */
  series: GaugeSeries[];
  range: ChartRange;
  events: SystemEvent[];
}

const VIEW_W = 960;
const VIEW_H = 320;

const TOOLTIP_GAP = 12; // offset from the hovered point

const MARKER_HIT_WIDTH = 8; // a 1px dashed line is too thin to hover

/** Time series as line charts on one axis, with a crosshair tooltip on hover. */
export function MetricLineChart({
  series,
  range,
  events,
}: MetricLineChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [showEvents, setShowEvents] = useState(true);

  if (series.every((s) => s.points.length === 0)) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        no samples in range
      </div>
    );
  }

  const metas = series.map((s) => GAUGE_METRICS[s.metric]);
  const { formatTick, formatValue, unit } = metas[0];
  const resolutionSecs = series[0].resolution_secs;
  const layout = gaugeLayout(
    {
      series: series.map((s) => s.points),
      resolutionSecs,
      events,
      domain: range,
    },
    VIEW_W,
    VIEW_H,
  );
  const { plot } = layout;
  // One series reads best filled with its latest value labelled; several
  // would hide each other, so they draw as bare lines with a legend instead.
  const single = series.length === 1;
  const hoverPoints = layout.lines[0].points;
  const last = single ? hoverPoints[hoverPoints.length - 1] : undefined;
  const hovered = hoveredIndex !== null ? hoverPoints[hoveredIndex] : null;
  const hasEvents = layout.markers.length > 0;

  const handleMouseMove = (e: MouseEvent<SVGRectElement>) => {
    const svg = svgRef.current;
    if (!svg) return;

    const localX = viewBoxX(
      svg.getBoundingClientRect(),
      e.clientX,
      VIEW_W,
      VIEW_H,
    );
    if (localX === null) return;
    setHoveredIndex(nearestPointIndex(hoverPoints, localX));
  };

  const valueAt = (i: number, sampledAt: string): string => {
    const point = series[i].points.find((p) => p.sampled_at === sampledAt);
    return point?.value == null ? '--' : formatValue(point.value);
  };

  return (
    <div
      className="flex h-full w-full flex-col gap-1"
      data-testid="gauge-metric-chart"
      data-point-count={hoverPoints.length}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-dim">
          {resolutionLabel(resolutionSecs)}
        </span>
        <div className="flex items-center gap-4">
          {!single &&
            metas.map((meta) => (
              <span
                key={meta.label}
                className="flex items-center gap-1.5 text-muted text-xs"
              >
                <svg width="16" height="8" aria-hidden="true">
                  <line
                    x1="0"
                    y1="4"
                    x2="16"
                    y2="4"
                    stroke={meta.color}
                    strokeWidth={2}
                  />
                </svg>
                {meta.label}
              </span>
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
        aria-label={`Mempool ${series.map((s) => s.metric).join(' and ')} over time`}
      >
        <AxisY
          plot={plot}
          ticks={layout.yTicks}
          format={formatTick}
          label={unit}
          gridlines
        />

        <AxisX
          plot={plot}
          ticks={layout.xTicks}
          format={(v) => dayjs(v).format('HH:mm')}
          label="time (local)"
        />

        {/* No reading: a muted column, so a gap is never read as a flat value */}
        {layout.gaps.map((gap) => (
          <rect
            key={gap.x0}
            data-testid="no-reading"
            x={gap.x0}
            y={plot.top}
            width={Math.max(0, gap.x1 - gap.x0)}
            height={plot.height}
            className="fill-idle"
            opacity={0.6}
          />
        ))}

        {/* Lifecycle markers, drawn under the value line so data stays dominant */}
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

        {/* Area + line, one pair per gap-safe segment; a lone point draws as a dot */}
        {layout.lines.map((line, li) => {
          const { color } = metas[li];
          return line.segments.map((segment, si) =>
            segment.points.length === 1 ? (
              <circle
                // biome-ignore lint/suspicious/noArrayIndexKey: segments are a stable, non-reordered layout output
                key={`${li}-${si}`}
                cx={segment.points[0].px}
                cy={segment.points[0].py}
                r={single ? 3 : 1.5}
                fill={color}
              />
            ) : (
              // biome-ignore lint/suspicious/noArrayIndexKey: segments are a stable, non-reordered layout output
              <g key={`${li}-${si}`}>
                {single && (
                  <path
                    d={segment.areaPath}
                    fill="rgba(247,147,26,0.09)"
                    stroke="none"
                  />
                )}
                <path
                  d={segment.linePath}
                  fill="none"
                  stroke={color}
                  strokeWidth={single ? 2 : 1.5}
                />
              </g>
            ),
          );
        })}

        {/* Direct label on the most recent value -- not every point */}
        {last && (
          <>
            <circle cx={last.px} cy={last.py} r={3} fill={metas[0].color} />
            <text
              x={last.px}
              y={last.py - 8}
              textAnchor="end"
              className="fill-ink font-mono text-xs"
            >
              {formatValue(last.point.value)}
            </text>
          </>
        )}

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

        {/* Crosshair + tooltip */}
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
            <circle
              cx={hovered.px}
              cy={hovered.py}
              r={3}
              fill={metas[0].color}
            />
            <ChartTooltip
              lines={[
                dayjs(hovered.point.sampled_at).format('YYYY-MM-DD HH:mm:ss'),
                ...(single
                  ? [formatValue(hovered.point.value)]
                  : metas.map(
                      (meta, i) =>
                        `${meta.label} ${valueAt(i, hovered.point.sampled_at)}`,
                    )),
              ]}
              lineColors={
                single ? undefined : [undefined, ...metas.map((m) => m.color)]
              }
              plot={plot}
              gap={TOOLTIP_GAP}
              rightOf={hovered.px}
              leftOf={hovered.px}
              anchorY={{ at: hovered.py, align: 'center' }}
            />
          </g>
        )}
      </svg>
    </div>
  );
}
