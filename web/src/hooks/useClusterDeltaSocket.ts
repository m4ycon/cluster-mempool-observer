import { useEffect, useReducer } from 'react';
import useWebSocket, { type ReadyState } from 'react-use-websocket';
import { ApiRoutes } from '../lib/routes';
import type { ClusterDeltaEvent, ClusterRef } from '../types/events';

type ClusterMap = Map<number, ClusterRef>;

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

export interface ClusterDeltaSocket {
  clusters: ClusterRef[];
  readyState: ReadyState;
}

/** Subscribes to clusters-delta and keeps an up-to-date cluster list */
export function useClusterDeltaSocket(): ClusterDeltaSocket {
  const [clusters, dispatch] = useReducer(
    applyDelta,
    new Map<number, ClusterRef>(),
  );

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
    if (lastJsonMessage) dispatch(lastJsonMessage);
  }, [lastJsonMessage]);

  return { clusters: Array.from(clusters.values()), readyState };
}
