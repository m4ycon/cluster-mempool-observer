import { useCallback, useEffect, useMemo, useReducer } from 'react';
import useWebSocket, { type ReadyState } from 'react-use-websocket';
import { ApiRoutes } from '../lib/routes';
import type { ClusterDeltaEvent, ClusterRef } from '../types/events';

type ClusterMap = Map<number, ClusterRef>;

export type ClusterUpdateKind = 'new' | 'changed';

export interface ClusterUpdate {
  /**
   * How many upserts this cluster has had since it entered the set.
   * It will help re-rendering as "key" will change when the cluster is updated.
   */
  revision: number;
  kind: ClusterUpdateKind;
}

type ClusterUpdateMap = Map<number, ClusterUpdate>;

interface Frozen {
  clusters: ClusterRef[];
  updates: ClusterUpdateMap;
}

interface State {
  live: ClusterMap;
  liveUpdates: ClusterUpdateMap;
  /** What callers see while paused; `null` means the live view passes through. */
  frozen: Frozen | null;
}

type Action =
  | { type: 'delta'; delta: ClusterDeltaEvent }
  | { type: 'togglePause' };

function applyDelta(state: State, delta: ClusterDeltaEvent): State {
  const live = new Map(state.live);
  const liveUpdates = new Map(state.liveUpdates);

  for (const cluster of delta.upserted) {
    const known = live.has(cluster.id);
    live.set(cluster.id, cluster);
    const prev = liveUpdates.get(cluster.id);
    liveUpdates.set(cluster.id, {
      revision: (prev?.revision ?? 0) + 1,
      kind: known ? 'changed' : 'new',
    });
  }

  for (const removed of delta.removed) {
    const id = Number(removed);
    live.delete(id);
    liveUpdates.delete(id);
  }

  return { ...state, live, liveUpdates };
}

function reduce(state: State, action: Action): State {
  switch (action.type) {
    case 'delta':
      return applyDelta(state, action.delta);
    case 'togglePause':
      return {
        ...state,
        frozen:
          state.frozen === null
            ? {
                clusters: Array.from(state.live.values()),
                updates: state.liveUpdates,
              }
            : null,
      };
  }
}

const initialState: State = {
  live: new Map(),
  liveUpdates: new Map(),
  frozen: null,
};

export interface ClusterDeltaSocket {
  clusters: ClusterRef[];
  lastUpdates: ClusterUpdateMap;
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
    clusters: state.frozen?.clusters ?? live,
    lastUpdates: state.frozen?.updates ?? state.liveUpdates,
    readyState,
    paused: state.frozen !== null,
    togglePaused,
  };
}
