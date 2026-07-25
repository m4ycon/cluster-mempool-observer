import { useState } from 'react';
import type { HistogramBar, HistogramLayout } from '../../lib/clusterHistogram';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import { NumberFormat } from '../../lib/format';

export interface ClusterHistogramProps {
  layout: HistogramLayout;
  sizeMetric: ClusterMetric;
}

const VIEW_W = 720;
const VIEW_H = 560;

const TOOLTIP_WIDTH = 152;
const TOOLTIP_PAD = 8;
const TOOLTIP_LINE_HEIGHT = 14;
const TOOLTIP_GAP = 10; // offset from the hovered bar's edge
const TOOLTIP_HEIGHT = TOOLTIP_PAD * 2 + TOOLTIP_LINE_HEIGHT * 2;

/** `lo-hi` bin range label, full precision */
function binRange(
  bar: HistogramBar,
  sizeMetric: ClusterMetric,
  isFinal: boolean,
): string {
  return ClusterMetrics.binRangeLabel(sizeMetric, bar.lo, bar.hi, isFinal);
}

/** Bins are non-overlapping and ascending, so lo-hi is a stable identity key. */
function binKey(bar: HistogramBar): string {
  return `${bar.lo}-${bar.hi}`;
}

export function ClusterHistogram({
  layout,
  sizeMetric,
}: ClusterHistogramProps) {
  const [hovered, setHovered] = useState<string | null>(null);

  if (layout.bars.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        awaiting cluster feed...
      </div>
    );
  }

  const { plot } = layout;
  const bar =
    hovered !== null
      ? layout.bars.find((b) => binKey(b) === hovered)
      : undefined;

  // Tooltip geometry, computed only while a bar is hovered. Anchored to the
  // hovered bar (not the raw cursor position) since hover is driven by a
  // per-bin hit area, not pixel-level mouse tracking.
  let tooltipX = 0;
  let tooltipY = 0;
  if (bar) {
    const preferredX = bar.x1 + TOOLTIP_GAP;
    const overflowsRight = preferredX + TOOLTIP_WIDTH > plot.left + plot.width;
    tooltipX = overflowsRight
      ? Math.max(plot.left, bar.x0 - TOOLTIP_GAP - TOOLTIP_WIDTH)
      : preferredX;

    tooltipY = Math.min(
      Math.max(bar.y0 - TOOLTIP_PAD, plot.top),
      plot.top + plot.height - TOOLTIP_HEIGHT,
    );
  }

  return (
    <svg
      width="100%"
      height="100%"
      viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
      preserveAspectRatio="xMidYMid meet"
    >
      <title>Cluster histogram</title>

      {/* Y axis */}
      <line
        x1={plot.left}
        y1={plot.top}
        x2={plot.left}
        y2={plot.top + plot.height}
        className="stroke-line"
        strokeWidth={1}
      />
      {layout.yTicks.map((t) => (
        <g key={t.value}>
          <line
            x1={plot.left - 4}
            y1={t.px}
            x2={plot.left}
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
            {NumberFormat.grouped(Math.round(t.value))}
          </text>
        </g>
      ))}
      <text
        transform={`translate(${plot.left - 40}, ${plot.top + plot.height / 2}) rotate(-90)`}
        textAnchor="middle"
        className="fill-faint text-xs"
      >
        clusters
      </text>

      {/* X axis */}
      <line
        x1={plot.left}
        y1={plot.top + plot.height}
        x2={plot.left + plot.width}
        y2={plot.top + plot.height}
        className="stroke-line"
        strokeWidth={1}
      />
      {layout.xTicks.map((t) => (
        <g key={t.value}>
          <line
            x1={t.px}
            y1={plot.top + plot.height}
            x2={t.px}
            y2={plot.top + plot.height + 4}
            className="stroke-line"
            strokeWidth={1}
          />
          <text
            x={t.px}
            y={plot.top + plot.height + 16}
            textAnchor="middle"
            className="fill-dim font-mono text-xs"
          >
            {ClusterMetrics.markLabel(t.value, sizeMetric)}
          </text>
        </g>
      ))}
      <text
        x={plot.left + plot.width / 2}
        y={plot.top + plot.height + 34}
        textAnchor="middle"
        className="fill-faint text-xs"
      >
        {`${ClusterMetrics.LABEL[sizeMetric]} (${ClusterMetrics.UNIT[sizeMetric]})`}
      </text>

      {/* Bars */}
      {layout.bars.map((b) => {
        const key = binKey(b);
        return (
          <g key={key}>
            <rect
              x={b.x0}
              y={b.y0}
              width={Math.max(0, b.x1 - b.x0)}
              height={Math.max(0, b.y1 - b.y0)}
              className={hovered === key ? 'fill-orange' : 'fill-bar'}
            />
            {/* biome-ignore lint/a11y/noStaticElementInteractions: hover-only hit area for a data bin's tooltip, not a clickable data point */}
            {/* biome-ignore lint/a11y/useKeyWithMouseEvents: not focusable -- selection stays click-driven elsewhere, this is a supplementary hover tooltip */}
            <rect
              x={b.x0}
              y={plot.top}
              width={Math.max(0, b.x1 - b.x0)}
              height={plot.height}
              fill="transparent"
              onMouseOver={() => setHovered(key)}
              onMouseOut={() => setHovered((h) => (h === key ? null : h))}
            />
          </g>
        );
      })}

      {/* Tooltip */}
      {bar && (
        <g
          transform={`translate(${tooltipX}, ${tooltipY})`}
          className="pointer-events-none"
        >
          <rect
            width={TOOLTIP_WIDTH}
            height={TOOLTIP_HEIGHT}
            className="fill-bg stroke-line"
            strokeWidth={1}
          />
          <text
            x={TOOLTIP_PAD}
            y={TOOLTIP_PAD + 9}
            className="fill-ink font-mono text-xs"
          >
            {`${binRange(bar, sizeMetric, bar === layout.bars[layout.bars.length - 1])} ${ClusterMetrics.UNIT[sizeMetric]}`}
          </text>
          <text
            x={TOOLTIP_PAD}
            y={TOOLTIP_PAD + 9 + TOOLTIP_LINE_HEIGHT}
            className="fill-dim text-xs"
          >
            {`${NumberFormat.grouped(bar.count)} clusters`}
          </text>
        </g>
      )}
    </svg>
  );
}
