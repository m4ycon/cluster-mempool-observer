import { useMempoolMetric } from '../../hooks/useMempoolMetric';
import type { SnapshotMetric } from '../../types/generated/SnapshotMetric';
import { MetricLineChart } from './MetricLineChart';

export interface MempoolMetricChartProps {
  metric: SnapshotMetric;
}

export function MempoolMetricChart({ metric }: MempoolMetricChartProps) {
  const state = useMempoolMetric(metric);

  if (state.status === 'loading') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-dim">
        loading snapshot history...
      </div>
    );
  }

  if (state.status === 'error') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-alert">
        failed to load snapshot history
      </div>
    );
  }

  return <MetricLineChart series={state.series} />;
}
