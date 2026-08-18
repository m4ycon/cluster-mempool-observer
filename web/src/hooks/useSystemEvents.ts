import { ApiRoutes, type ChartRange } from '../lib/routes';
import type { SystemEvent } from '../types/generated/SystemEvent';
import { useHttpGet } from './useHttpGet';

export type SystemEventsState =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; events: SystemEvent[] };

/** Fetches system lifecycle events within `range`. */
export function useSystemEvents(range: ChartRange): SystemEventsState {
  const state = useHttpGet<SystemEvent[]>(ApiRoutes.systemEvents(range));
  return state.status === 'loaded'
    ? { status: 'loaded', events: state.data }
    : state;
}
