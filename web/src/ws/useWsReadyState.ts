import type { ReadyState } from 'react-use-websocket';
import { useWsContext } from './context';

/** The shared socket's current connection state, for `ConnectionDot`. */
export function useWsReadyState(): ReadyState {
  return useWsContext().readyState;
}
