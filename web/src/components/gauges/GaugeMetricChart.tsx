import { useGaugeMetrics } from '../../hooks/useGaugeMetrics';
import { useSystemEvents } from '../../hooks/useSystemEvents';
import dayjs from '../../lib/dayjs';
import type { ChartRange } from '../../lib/routes';
import type { GaugeMetric } from '../../types/generated/GaugeMetric';
import { MetricLineChart } from './MetricLineChart';

const RANGE_HOURS = 24;

export interface GaugeMetricChartProps {
  /** Drawn together on one y axis, so they must share a unit. */
  metrics: GaugeMetric[];
}

/** Rounded to the minute so the fetch path stays stable across renders. */
function currentRange(): ChartRange {
  const to = dayjs().startOf('minute');
  return {
    from: to.subtract(RANGE_HOURS, 'hour').valueOf(),
    to: to.valueOf(),
  };
}

export function GaugeMetricChart({ metrics }: GaugeMetricChartProps) {
  const range = currentRange();
  const metricState = useGaugeMetrics(metrics, range);
  const eventsState = useSystemEvents(range);

  if (metricState.status === 'loading' || eventsState.status === 'loading') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-dim">
        loading sample history...
      </div>
    );
  }

  if (metricState.status === 'error') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-alert">
        failed to load sample history
      </div>
    );
  }

  // Markers are an overlay; a failed secondary request must not blank a working chart.
  const events = eventsState.status === 'loaded' ? eventsState.events : [];

  return (
    <MetricLineChart
      series={metricState.series}
      range={range}
      events={events}
    />
  );
}
