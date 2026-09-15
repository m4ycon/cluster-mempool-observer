import { ApiRoutes, type ChartRange } from '../lib/routes';
import type { GaugeMetric } from '../types/generated/GaugeMetric';
import type { GaugeSeries } from '../types/generated/GaugeSeries';
import { useHttpGet } from './useHttpGet';

export type GaugeMetricState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; series: GaugeSeries };

/** Fetches `metric`'s series within `range`. */
export function useGaugeMetric(
  metric: GaugeMetric,
  range: ChartRange,
): GaugeMetricState {
  const state = useHttpGet<GaugeSeries>(ApiRoutes.mempoolGauges(metric, range));
  // Re-shaped to `series` (not `data`) so GaugeMetricChart needs no changes.
  return state.status === 'loaded'
    ? { status: 'loaded', series: state.data }
    : state;
}
