import clsx from 'clsx';
import type { PackedCluster, TreemapCell } from '../../lib/clusterLayout';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { ColorScale } from '../../lib/colorTiers';

export type VizType = 'circles' | 'treemap';

export interface ClusterCanvasProps {
  vizType: VizType;
  packed: PackedCluster[];
  cells: TreemapCell[];
  sizeMetric: ClusterMetric;
  colorMetric: ClusterMetric;
  colorScale: ColorScale;
  selectedId: number | null;
  onSelect: (id: number) => void;
}

const TREEMAP_PAD = 0.1;

export function ClusterCanvas({
  vizType,
  packed,
  cells,
  sizeMetric,
  colorMetric,
  colorScale,
  selectedId,
  onSelect,
}: ClusterCanvasProps) {
  const isEmpty =
    vizType === 'circles' ? packed.length === 0 : cells.length === 0;

  const fillFor = (c: PackedCluster['c']) =>
    ClusterMetrics.colorAt(
      colorScale,
      ClusterMetrics.value(c, colorMetric),
      colorMetric,
    );

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
            return (
              <g key={c.id}>
                {/* biome-ignore lint/a11y/noStaticElementInteractions: bubble is a selectable data point in the cluster explorer */}
                <circle
                  cx={x}
                  cy={y}
                  r={r}
                  fill={fillFor(c)}
                  fillOpacity={0.88}
                  strokeWidth={1.5}
                  className={clsx('cursor-pointer', selected && 'stroke-ink')}
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
            const width = Math.max(0, x1 - x0 - TREEMAP_PAD * 2);
            const height = Math.max(0, y1 - y0 - TREEMAP_PAD * 2);
            return (
              <g key={c.id}>
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
