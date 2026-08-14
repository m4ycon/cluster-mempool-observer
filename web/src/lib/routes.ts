import type { SnapshotMetric } from '../types/generated/SnapshotMetric';

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

export const ApiRoutes = {
  ws: `${WS_BASE_URL}/ws`,
  mempoolSnapshots: (metric: SnapshotMetric) => `/mempool/snapshots/${metric}`,
  mempoolFeerateDiagram: '/mempool/feerate-diagram',
} as const;

export const WebRoutes = {
  home: '/',
  clusters: '/clusters',
  clusterCount: '/mempool/snapshots/cluster-count',
  mempoolSize: '/mempool/snapshots/mempool-tx-count',
  feerateDiagram: '/mempool/feerate-diagram',
} as const;
