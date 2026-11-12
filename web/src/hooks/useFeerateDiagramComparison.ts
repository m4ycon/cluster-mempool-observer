import { ApiRoutes } from '../lib/routes';
import type { FeerateDiagramComparison } from '../types/generated/FeerateDiagramComparison';
import { useHttpGet } from './useHttpGet';

export type FeerateDiagramComparisonState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; comparison: FeerateDiagramComparison | null };

/** Fetches the last node-vs-ours feerate diagram pair. */
export function useFeerateDiagramComparison(): FeerateDiagramComparisonState {
  const state = useHttpGet<FeerateDiagramComparison | null>(
    ApiRoutes.mempoolFeerateDiagramComparison,
  );

  return state.status === 'loaded'
    ? { status: 'loaded', comparison: state.data }
    : state;
}
