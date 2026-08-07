import { createContext, useContext } from 'react';
import type { ReadyState } from 'react-use-websocket';
import type { WsSubject } from '../types/events';
import type { PayloadFor } from './payload';

export type SubjectHandler<S extends WsSubject = WsSubject> = (
  payload: PayloadFor<S>,
) => void;

export interface WsContextValue {
  subscribe<S extends WsSubject>(
    subject: S,
    handler: SubjectHandler<S>,
  ): () => void;
  readyState: ReadyState;
}

export const WsContext = createContext<WsContextValue | null>(null);

export function useWsContext(): WsContextValue {
  const ctx = useContext(WsContext);
  if (!ctx) {
    throw new Error(
      'useSubscription/useWsReadyState must be used within a WsProvider',
    );
  }
  return ctx;
}
