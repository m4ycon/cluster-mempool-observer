import { type MouseEvent, useRef, useState } from 'react';
import dayjs from '../../lib/dayjs';
import { NumberFormat } from '../../lib/format';
import {
  mempoolMetricLayout,
  nearestPointIndex,
} from '../../lib/mempoolMetricChart';
import { resolutionLabel } from '../../lib/resolutionLabel';
import type { MempoolMetricSeries } from '../../types/generated/MempoolMetricSeries';
import { AxisX } from '../charts/AxisX';
import { ChartTooltip } from '../charts/ChartTooltip';

export interface MetricLineChartProps {
  series: MempoolMetricSeries;
}

const VIEW_W = 960;
const VIEW_H = 320;

const TOOLTIP_GAP = 12; // offset from the hovered point

/** Time series as a line+area chart, with a crosshair tooltip on hover. */
export function MetricLineChart({ series }: MetricLineChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (series.points.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        no snapshots in range
      </div>
    );
  }

  const layout = mempoolMetricLayout(series.points, VIEW_W, VIEW_H);
  const { plot } = layout;
  const last = layout.points[layout.points.length - 1];
  const hovered = hoveredIndex !== null ? layout.points[hoveredIndex] : null;

  const handleMouseMove = (e: MouseEvent<SVGRectElement>) => {
    const svg = svgRef.current;
    if (!svg) return;

    const rect = svg.getBoundingClientRect();
    if (rect.width === 0) return;

    const localX = ((e.clientX - rect.left) / rect.width) * VIEW_W;
    setHoveredIndex(nearestPointIndex(layout.points, localX));
  };

  return (
    <div
      className="flex h-full w-full flex-col gap-1"
      data-testid="mempool-metric-chart"
      data-point-count={layout.points.length}
    >
      <div className="text-xs text-dim">
        {resolutionLabel(series.resolution_secs)}
      </div>
      <svg
        ref={svgRef}
        width="100%"
        height="100%"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="xMidYMid meet"
        className="flex-1"
        role="img"
        aria-label={`Mempool ${series.metric} over time`}
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

        {/* Area + line */}
        <path d={layout.areaPath} fill="rgba(247,147,26,0.09)" stroke="none" />
        <path
          d={layout.linePath}
          fill="none"
          stroke="#f7931a"
          strokeWidth={2}
        />

        {/* Direct label on the most recent value -- not every point */}
        <circle cx={last.px} cy={last.py} r={3} fill="#f7931a" />
        <text
          x={last.px}
          y={last.py - 8}
          textAnchor="end"
          className="fill-ink font-mono text-xs"
        >
          {NumberFormat.grouped(last.point.value)}
        </text>

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
            <circle cx={hovered.px} cy={hovered.py} r={3} fill="#f7931a" />
            <ChartTooltip
              lines={[
                dayjs(hovered.point.sampled_at).format('YYYY-MM-DD HH:mm:ss'),
                NumberFormat.grouped(hovered.point.value),
              ]}
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
