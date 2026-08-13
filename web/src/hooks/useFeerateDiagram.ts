import { ApiRoutes } from '../lib/routes';
import type { MempoolFeerateDiagram } from '../types/generated/MempoolFeerateDiagram';
import { useHttpGet } from './useHttpGet';

export type FeerateDiagramState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; diagram: MempoolFeerateDiagram };

/** Fetches the latest cumulative feerate diagram; loading/error/loaded stay distinguishable. */
export function useFeerateDiagram(): FeerateDiagramState {
  const state = useHttpGet<MempoolFeerateDiagram>(
    ApiRoutes.mempoolFeerateDiagram,
  );

  return state.status === 'loaded'
    ? { status: 'loaded', diagram: state.data }
    : state;
}
