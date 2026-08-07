import { type ReactNode, useCallback, useEffect, useRef } from 'react';
import useWebSocket, { ReadyState } from 'react-use-websocket';
import { ApiRoutes } from '../lib/routes';
import type { ClientFrame, ServerEvent, WsSubject } from '../types/events';
import { type SubjectHandler, WsContext } from './context';

export interface WsProviderProps {
  children: ReactNode;
}

type ErasedHandler = (payload: unknown) => void;
type Registry = Map<WsSubject, Set<ErasedHandler>>;

/**
 * One websocket connection for the whole app. Pages subscribe to named
 * subjects through `useSubscription` instead of opening their own socket, so
 * switching pages becomes a subscription change on an already-open
 * connection rather than a socket teardown and reconnect.
 */
export function WsProvider({ children }: WsProviderProps) {
  const registryRef = useRef<Registry>(new Map());
  // Mirrors readyState so the stable `subscribe` callback below can read the
  // current connection state without depending on it (which would make a
  // new `subscribe` on every readyState change and force resubscribes).
  const readyStateRef = useRef<ReadyState>(ReadyState.UNINSTANTIATED);

  const { sendJsonMessage, readyState } = useWebSocket<ServerEvent>(
    ApiRoutes.ws,
    {
      shouldReconnect: () => true,
      reconnectAttempts: Infinity,
      reconnectInterval: (attempt) => Math.min(1000 * 2 ** attempt, 30_000),
      onMessage: (event) => {
        let parsed: ServerEvent;
        try {
          parsed = JSON.parse(event.data);
        } catch (e) {
          console.error('Failed to parse websocket message:', e);
          return;
        }

        if (parsed.subject === 'error') {
          console.error('Websocket error frame:', parsed.payload.message);
          return;
        }

        const handlers = registryRef.current.get(parsed.subject);
        if (!handlers) return;
        for (const handler of handlers) handler(parsed.payload);
      },
    },
  );

  readyStateRef.current = readyState;

  const subscribe = useCallback(
    <S extends WsSubject>(subject: S, handler: SubjectHandler<S>) => {
      const registry = registryRef.current;
      let handlers = registry.get(subject);
      if (!handlers) {
        handlers = new Set();
        registry.set(subject, handlers);
      }
      handlers.add(handler as unknown as ErasedHandler);

      if (handlers.size === 1 && readyStateRef.current === ReadyState.OPEN) {
        sendJsonMessage<ClientFrame>({ action: 'subscribe', subject });
      }

      return () => {
        const current = registry.get(subject);
        if (!current) return;
        current.delete(handler as unknown as ErasedHandler);
        if (current.size === 0) {
          registry.delete(subject);
          if (readyStateRef.current === ReadyState.OPEN) {
            sendJsonMessage<ClientFrame>({ action: 'unsubscribe', subject });
          }
        }
      };
    },
    [sendJsonMessage],
  );

  const isFirstRenderRef = useRef(true);

  // Handles a reconnect by resubscribing to all subjects with active handlers.
  // Handles the startup case where components mount and call `subscribe` before
  // the socket has finished connecting.
  useEffect(() => {
    if (isFirstRenderRef.current) {
      isFirstRenderRef.current = false;
      return;
    }
    if (readyState !== ReadyState.OPEN) return;
    for (const subject of registryRef.current.keys()) {
      sendJsonMessage<ClientFrame>({ action: 'subscribe', subject });
    }
  }, [readyState, sendJsonMessage]);

  return (
    <WsContext.Provider value={{ subscribe, readyState }}>
      {children}
    </WsContext.Provider>
  );
}
