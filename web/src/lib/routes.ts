import type { SnapshotMetric } from '../types/generated/SnapshotMetric';

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

export const ApiRoutes = {
  ws: `${WS_BASE_URL}/ws`,
  mempoolSnapshots: (metric: SnapshotMetric) => `/mempool/snapshots/${metric}`,
} as const;
