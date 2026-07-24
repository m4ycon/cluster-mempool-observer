import { useCallback, useEffect, useMemo, useReducer } from 'react';
import useWebSocket, { type ReadyState } from 'react-use-websocket';
import { ApiRoutes } from '../lib/routes';
import type { ClusterDeltaEvent, ClusterRef } from '../types/events';

type ClusterMap = Map<number, ClusterRef>;

interface State {
  live: ClusterMap;
  /** What callers see while paused; `null` means the live list passes through. */
  frozen: ClusterRef[] | null;
}

type Action =
  | { type: 'delta'; delta: ClusterDeltaEvent }
  | { type: 'togglePause' };

function applyDelta(state: ClusterMap, delta: ClusterDeltaEvent): ClusterMap {
  const next = new Map(state);
  for (const cluster of delta.upserted) {
    next.set(cluster.id, cluster);
  }
  for (const id of delta.removed) {
    next.delete(Number(id));
  }
  return next;
}

function reduce(state: State, action: Action): State {
  switch (action.type) {
    case 'delta':
      return { ...state, live: applyDelta(state.live, action.delta) };
    case 'togglePause':
      return {
        ...state,
        frozen: state.frozen === null ? Array.from(state.live.values()) : null,
      };
  }
}

const initialState: State = { live: new Map(), frozen: null };

export interface ClusterDeltaSocket {
  clusters: ClusterRef[];
  readyState: ReadyState;
  paused: boolean;
  togglePaused: () => void;
}

/** Subscribes to clusters-delta and keeps an up-to-date cluster list. */
export function useClusterDeltaSocket(): ClusterDeltaSocket {
  const [state, dispatch] = useReducer(reduce, initialState);

  const { lastJsonMessage, readyState } = useWebSocket<ClusterDeltaEvent>(
    ApiRoutes.clustersDelta,
    {
      share: true,
      shouldReconnect: () => true,
      reconnectAttempts: Infinity,
      reconnectInterval: (attempt) => Math.min(1000 * 2 ** attempt, 30_000),
    },
  );

  useEffect(() => {
    if (lastJsonMessage) dispatch({ type: 'delta', delta: lastJsonMessage });
  }, [lastJsonMessage]);

  const live = useMemo(() => Array.from(state.live.values()), [state.live]);

  const togglePaused = useCallback(() => dispatch({ type: 'togglePause' }), []);

  return {
    clusters: state.frozen ?? live,
    readyState,
    paused: state.frozen !== null,
    togglePaused,
  };
}
