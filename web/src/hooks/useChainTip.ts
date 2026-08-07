import { useState } from 'react';
import type { NewBlockInfoEvent } from '../types/events';
import { useSubscription } from '../ws/useSubscription';

/** Subscribes to chain.tip and returns the latest block, null until the first. */
export function useChainTip(): NewBlockInfoEvent | null {
  const [block, setBlock] = useState<NewBlockInfoEvent | null>(null);

  useSubscription('chain.tip', setBlock);

  return block;
}
