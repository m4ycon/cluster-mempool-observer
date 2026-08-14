import clsx from 'clsx';
import type { ClusterUpdate } from '../../hooks/useClusterDeltaSocket';
import type { HistogramLayout } from '../../lib/clusterHistogram';
import type { PackedCluster, TreemapCell } from '../../lib/clusterLayout';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { CanvasVizType } from '../../lib/clustersSearch';
import type { ColorScale } from '../../lib/colorTiers';
import { ClusterHistogram } from './ClusterHistogram';

export interface ClusterCanvasProps {
  vizType: CanvasVizType;
  packed: PackedCluster[];
  cells: TreemapCell[];
  histogramLayout: HistogramLayout;
  sizeMetric: ClusterMetric;
  colorMetric: ClusterMetric;
  colorScale: ColorScale;
  lastUpdates: Map<number, ClusterUpdate>;
  selectedId: number | null;
  onSelect: (id: number) => void;
}

const TREEMAP_PAD = 0.1;

const UPDATE_CLASS: Record<ClusterUpdate['kind'], string> = {
  new: 'animate-mark-new',
  changed: 'animate-mark-changed',
};

export function ClusterCanvas({
  vizType,
  packed,
  cells,
  histogramLayout,
  sizeMetric,
  colorMetric,
  colorScale,
  lastUpdates,
  selectedId,
  onSelect,
}: ClusterCanvasProps) {
  if (vizType === 'histogram') {
    return (
      <ClusterHistogram layout={histogramLayout} sizeMetric={sizeMetric} />
    );
  }

  const isEmpty =
    vizType === 'circles' ? packed.length === 0 : cells.length === 0;

  const fillFor = (c: PackedCluster['c']) =>
    ClusterMetrics.colorAt(
      colorScale,
      ClusterMetrics.value(c, colorMetric),
      colorMetric,
    );

  const cueFor = (id: number) => {
    const update = lastUpdates.get(id);
    return {
      // The revision remounts the mark, which is what replays a one-shot
      // animation when the same cluster is updated again.
      key: `${id}:${update?.revision ?? 0}`,
      className: update && UPDATE_CLASS[update.kind],
    };
  };

  if (isEmpty) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        awaiting cluster feed...
      </div>
    );
  }

  return (
    <svg
      width="100%"
      height="100%"
      viewBox="0 0 720 560"
      preserveAspectRatio="xMidYMid meet"
    >
      <title>Cluster graph</title>
      {vizType === 'circles'
        ? packed.map(({ c, x, y, r }) => {
            const selected = c.id === selectedId;
            const cue = cueFor(c.id);
            return (
              <g key={cue.key}>
                {/* biome-ignore lint/a11y/noStaticElementInteractions: bubble is a selectable data point in the cluster explorer */}
                <circle
                  cx={x}
                  cy={y}
                  r={r}
                  fill={fillFor(c)}
                  fillOpacity={0.88}
                  strokeWidth={1.5}
                  className={clsx(
                    'cursor-pointer',
                    selected && 'stroke-ink',
                    cue.className,
                  )}
                  onClick={() => onSelect(c.id)}
                />
                <text
                  x={x}
                  y={y + 3.5}
                  textAnchor="middle"
                  className="pointer-events-none select-none fill-bg font-bold font-mono text-xs"
                >
                  {ClusterMetrics.markLabel(
                    ClusterMetrics.value(c, sizeMetric),
                    sizeMetric,
                  )}
                </text>
              </g>
            );
          })
        : cells.map(({ c, x0, y0, x1, y1 }) => {
            const selected = c.id === selectedId;
            const cue = cueFor(c.id);
            const width = Math.max(0, x1 - x0 - TREEMAP_PAD * 2);
            const height = Math.max(0, y1 - y0 - TREEMAP_PAD * 2);
            return (
              <g key={cue.key}>
                {/* biome-ignore lint/a11y/noStaticElementInteractions: cell is a selectable data point in the cluster explorer */}
                <rect
                  x={x0 + TREEMAP_PAD}
                  y={y0 + TREEMAP_PAD}
                  width={width}
                  height={height}
                  fill={fillFor(c)}
                  strokeWidth={selected ? 1.5 : 1}
                  className={clsx(
                    'cursor-pointer',
                    selected ? 'stroke-ink' : 'stroke-bg',
                    cue.className,
                  )}
                  onClick={() => onSelect(c.id)}
                />
                <text
                  x={x0 + 4}
                  y={y0 + 13}
                  className="pointer-events-none select-none fill-bg font-bold font-mono text-xs"
                >
                  {ClusterMetrics.markLabel(
                    ClusterMetrics.value(c, sizeMetric),
                    sizeMetric,
                  )}
                </text>
              </g>
            );
          })}
    </svg>
  );
}
