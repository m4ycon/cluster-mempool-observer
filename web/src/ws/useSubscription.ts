import { useEffect, useRef } from 'react';
import type { WsSubject } from '../types/events';
import { useWsContext } from './context';
import type { PayloadFor } from './payload';

/**
 * Subscribes to `subject` for the lifetime of the component, calling
 * `onEvent` with each payload.
 */
export function useSubscription<S extends WsSubject>(
  subject: S,
  onEvent: (payload: PayloadFor<S>) => void,
): void {
  const { subscribe } = useWsContext();

  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  useEffect(() => {
    return subscribe(subject, (payload) => onEventRef.current(payload));
  }, [subject, subscribe]);
}
