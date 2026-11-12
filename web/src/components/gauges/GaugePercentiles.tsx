import { quantile } from 'd3-array';
import { Fragment } from 'react';
import { GAUGE_METRICS } from '../../lib/gaugeMetrics';
import type { GaugeSeries } from '../../types/generated/GaugeSeries';

export interface GaugePercentilesProps {
  series: GaugeSeries[];
  percentiles: number[];
}

/** Each series' percentiles over the readings it holds; missing readings are skipped, not counted as zero. */
export function GaugePercentiles({
  series,
  percentiles,
}: GaugePercentilesProps) {
  return (
    <div
      className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
      data-testid="gauge-percentiles"
    >
      {series.map(({ metric, points }, i) => {
        const meta = GAUGE_METRICS[metric];
        return (
          <Fragment key={metric}>
            {i > 0 && <span className="h-4 w-px bg-line" />}
            <span className="flex items-center gap-3 text-muted">
              {meta.label}
              {percentiles.map((p) => {
                const value = quantile(points, p / 100, (point) => point.value);
                return (
                  <span key={p}>
                    p{p}{' '}
                    <span className="text-base" style={{ color: meta.color }}>
                      {value === undefined ? '--' : meta.formatValue(value)}
                    </span>
                  </span>
                );
              })}
            </span>
          </Fragment>
        );
      })}
    </div>
  );
}
