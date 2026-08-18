import { useMempoolMetric } from '../../hooks/useMempoolMetric';
import { useSystemEvents } from '../../hooks/useSystemEvents';
import dayjs from '../../lib/dayjs';
import type { ChartRange } from '../../lib/routes';
import type { SnapshotMetric } from '../../types/generated/SnapshotMetric';
import { MetricLineChart } from './MetricLineChart';

const RANGE_HOURS = 24;

export interface MempoolMetricChartProps {
  metric: SnapshotMetric;
}

/** Rounded to the minute so the fetch path stays stable across renders. */
function currentRange(): ChartRange {
  const to = dayjs().startOf('minute');
  return {
    from: to.subtract(RANGE_HOURS, 'hour').valueOf(),
    to: to.valueOf(),
  };
}

export function MempoolMetricChart({ metric }: MempoolMetricChartProps) {
  const range = currentRange();
  const metricState = useMempoolMetric(metric, range);
  const eventsState = useSystemEvents(range);

  if (metricState.status === 'loading' || eventsState.status === 'loading') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-dim">
        loading snapshot history...
      </div>
    );
  }

  if (metricState.status === 'error') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-alert">
        failed to load snapshot history
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
