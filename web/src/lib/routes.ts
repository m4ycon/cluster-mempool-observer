import type { GaugeMetric } from '../types/generated/GaugeMetric';
import dayjs from './dayjs';

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

/** Epoch-ms window a chart requests; gauge samples and system events must share one. */
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
  mempoolGauges: (metric: GaugeMetric, range: ChartRange) =>
    `/mempool/gauges/${metric}?${rangeParams(range)}`,
  mempoolCounters: (range: ChartRange) =>
    `/mempool/counters?${rangeParams(range)}`,
  mempoolFeerateDiagram: '/mempool/feerate-diagram',
  systemEvents: (range: ChartRange) => `/system-events?${rangeParams(range)}`,
  transactions: (txids: string[]) =>
    `/transactions?${new URLSearchParams({ txids: txids.join(',') })}`,
} as const;

export const ExplorerRoutes = {
  tx: (txid: string) => `https://mempool.space/pt/tx/${txid}`,
} as const;

export const WebRoutes = {
  home: '/',
  clusters: '/clusters',
  clusterCount: '/mempool/gauges/cluster-count',
  mempoolSize: '/mempool/gauges/mempool-tx-count',
  txsPerMin: '/mempool/counters',
  feerateDiagram: '/mempool/feerate-diagram',
} as const;
