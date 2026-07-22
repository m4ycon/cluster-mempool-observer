import { useEffect, useReducer } from 'react';
import useWebSocket, { type ReadyState } from 'react-use-websocket';
import { ApiRoutes } from '../lib/routes';
import type {
  HomeMessage,
  MempoolStatsEvent,
  NewBlockInfoEvent,
} from '../types/events';

interface MempoolStatsState {
  stats: MempoolStatsEvent | null;
  block: NewBlockInfoEvent | null;
}

const INITIAL: MempoolStatsState = { stats: null, block: null };

function reduce(state: MempoolStatsState, msg: HomeMessage): MempoolStatsState {
  switch (msg.type) {
    case 'stats':
      return { ...state, stats: msg };
    case 'block':
      return { ...state, block: msg };
  }
}

export interface MempoolStatsSocket {
  stats: MempoolStatsEvent | null;
  block: NewBlockInfoEvent | null;
  readyState: ReadyState;
}

/** Subscribes to /mempool/stats: latest live counters + latest chain tip. */
export function useMempoolStatsSocket(): MempoolStatsSocket {
  const [state, dispatch] = useReducer(reduce, INITIAL);

  const { lastJsonMessage, readyState } = useWebSocket<HomeMessage>(
    ApiRoutes.mempoolStats,
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

  return { ...state, readyState };
}
