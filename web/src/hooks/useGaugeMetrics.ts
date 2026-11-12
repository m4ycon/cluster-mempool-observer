import { ApiRoutes, type ChartRange } from '../lib/routes';
import type { GaugeMetric } from '../types/generated/GaugeMetric';
import type { GaugeSeries } from '../types/generated/GaugeSeries';
import { useHttpGetAll } from './useHttpGet';

export type GaugeMetricsState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; series: GaugeSeries[] };

/** Fetches each of `metrics`' series within `range`, in the same order. */
export function useGaugeMetrics(
  metrics: readonly GaugeMetric[],
  range: ChartRange,
): GaugeMetricsState {
  const state = useHttpGetAll<GaugeSeries>(
    metrics.map((metric) => ApiRoutes.mempoolGauges(metric, range)),
  );
  return state.status === 'loaded'
    ? { status: 'loaded', series: state.data }
    : state;
}
