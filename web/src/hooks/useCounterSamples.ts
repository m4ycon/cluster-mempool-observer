import { ApiRoutes, type ChartRange } from '../lib/routes';
import type { CounterSeries } from '../types/generated/CounterSeries';
import { useHttpGet } from './useHttpGet';

export type CounterSamplesState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; series: CounterSeries };

/** Fetches the three-series (arrivals/confirmed/evicted) projection within `range`. */
export function useCounterSamples(range: ChartRange): CounterSamplesState {
  const state = useHttpGet<CounterSeries>(ApiRoutes.mempoolCounters(range));
  // Re-shaped to `series` (not `data`) so callers stay symmetric with useGaugeMetric.
  return state.status === 'loaded'
    ? { status: 'loaded', series: state.data }
    : state;
}
