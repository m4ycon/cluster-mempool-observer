import { useState } from 'react';
import type { HistogramBar, HistogramLayout } from '../../lib/clusterHistogram';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import { NumberFormat } from '../../lib/format';
import { AxisX } from '../charts/AxisX';
import { AxisY } from '../charts/AxisY';
import { ChartTooltip } from '../charts/ChartTooltip';

export interface ClusterHistogramProps {
  layout: HistogramLayout;
  sizeMetric: ClusterMetric;
}

const VIEW_W = 720;
const VIEW_H = 560;

const TOOLTIP_GAP = 10; // offset from the hovered bar's edge

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

  return (
    <svg
      width="100%"
      height="100%"
      viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label="Cluster histogram"
    >
      <AxisY
        plot={plot}
        ticks={layout.yTicks}
        format={(v) => NumberFormat.grouped(Math.round(v))}
        label="clusters"
      />

      <AxisX
        plot={plot}
        ticks={layout.xTicks}
        format={(v) => ClusterMetrics.markLabel(v, sizeMetric)}
        label={`${ClusterMetrics.LABEL[sizeMetric]} (${ClusterMetrics.UNIT[sizeMetric]})`}
      />

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
        <ChartTooltip
          lines={[
            `${binRange(bar, sizeMetric, bar === layout.bars[layout.bars.length - 1])} ${ClusterMetrics.UNIT[sizeMetric]}`,
            `${NumberFormat.grouped(bar.count)} cluster${bar.count === 1 ? '' : 's'}`,
          ]}
          plot={plot}
          gap={TOOLTIP_GAP}
          rightOf={bar.x1}
          leftOf={bar.x0}
          anchorY={{ at: bar.y0, align: 'above' }}
        />
      )}
    </svg>
  );
}
