import { useState } from 'react';
import type { MempoolStatsEvent } from '../types/events';
import { useSubscription } from '../ws/useSubscription';

export function useMempoolStats(): MempoolStatsEvent | null {
  const [stats, setStats] = useState<MempoolStatsEvent | null>(null);

  useSubscription('mempool.stats', setStats);

  return stats;
}
