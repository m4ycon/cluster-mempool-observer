import clsx from 'clsx';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { ColorScale } from '../../lib/colorTiers';
import type { VizType } from './ClusterCanvas';

export interface ClusterLegendProps {
  vizType: VizType;
  colorMetric: ClusterMetric;
  colorVals: number[];
  colorScale: ColorScale;
}

export function ClusterLegend({
  vizType,
  colorMetric,
  colorVals,
  colorScale,
}: ClusterLegendProps) {
  const ranges = ClusterMetrics.legendRanges(
    colorMetric,
    colorVals,
    colorScale,
  );
  const unit = ClusterMetrics.UNIT[colorMetric];
  const shapeClass = vizType === 'circles' ? 'rounded-full' : 'rounded-[2px]';

  return (
    <div
      className="mt-3 flex items-center gap-3 text-xs text-dim"
      data-testid="cluster-legend"
    >
      {ranges.map((range, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: tier order is the data
        <span key={i} className="flex items-center gap-1">
          <span
            className={clsx('h-2 w-2', shapeClass)}
            style={{ backgroundColor: colorScale.colors[i] }}
          />
          {range}
          {i === ranges.length - 1 ? ` ${unit}` : null}
        </span>
      ))}
    </div>
  );
}
