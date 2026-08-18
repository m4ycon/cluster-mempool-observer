import { ApiRoutes, type ChartRange } from '../lib/routes';
import type { MempoolMetricSeries } from '../types/generated/MempoolMetricSeries';
import type { SnapshotMetric } from '../types/generated/SnapshotMetric';
import { useHttpGet } from './useHttpGet';

export type MempoolMetricState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; series: MempoolMetricSeries };

/** Fetches `metric`'s series within `range`. */
export function useMempoolMetric(
  metric: SnapshotMetric,
  range: ChartRange,
): MempoolMetricState {
  const state = useHttpGet<MempoolMetricSeries>(
    ApiRoutes.mempoolSnapshots(metric, range),
  );
  // Re-shaped to `series` (not `data`) so MempoolMetricChart needs no changes.
  return state.status === 'loaded'
    ? { status: 'loaded', series: state.data }
    : state;
}
