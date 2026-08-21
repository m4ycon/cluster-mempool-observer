import type { SnapshotMetric } from '../types/generated/SnapshotMetric';
import dayjs from './dayjs';

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

/** Epoch-ms window a chart requests; snapshots and system events must share one. */
export interface ChartRange {
  from: number;
  to: number;
}

function rangeParams(range: ChartRange): string {
  return new URLSearchParams({
    from: dayjs(range.from).toISOString(),
    to: dayjs(range.to).toISOString(),
  }).toString();
}

export const ApiRoutes = {
  ws: `${WS_BASE_URL}/ws`,
  mempoolSnapshots: (metric: SnapshotMetric, range: ChartRange) =>
    `/mempool/snapshots/${metric}?${rangeParams(range)}`,
  mempoolFeerateDiagram: '/mempool/feerate-diagram',
  systemEvents: (range: ChartRange) => `/system-events?${rangeParams(range)}`,
  transactions: (txids: string[]) =>
    `/transactions?${new URLSearchParams({ txids: txids.join(',') })}`,
} as const;

export const WebRoutes = {
  home: '/',
  clusters: '/clusters',
  clusterCount: '/mempool/snapshots/cluster-count',
  mempoolSize: '/mempool/snapshots/mempool-tx-count',
  feerateDiagram: '/mempool/feerate-diagram',
} as const;
