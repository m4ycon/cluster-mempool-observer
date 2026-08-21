import { useState } from 'react';
import type { TransactionCache } from '../../hooks/useTransactionCache';
import { encodingCaption } from '../../lib/txMetrics';
import type { ClusterRef } from '../../types/events';
import { Dialog } from '../dialog/Dialog';
import { HelpButton } from '../help/HelpButton';
import { TxDagCanvas } from './TxDagCanvas';

interface TxDagDialogContentProps extends TransactionCache {
  cluster: ClusterRef;
}

/** Dialog body: interactive graph over the transactions the caller already fetched. */
function TxDagDialogContent({
  cluster,
  txs,
  missing,
  loading,
  error,
}: TxDagDialogContentProps) {
  const [selectedTxid, setSelectedTxid] = useState<string | null>(null);

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="text-xs text-dim tracking-[0.12em]">
        <span className="inline-flex items-center gap-2">
          TRANSACTION GRAPH
          <HelpButton topic="panel.clusterDag" />
        </span>
      </div>
      <div className="min-h-0 flex-1">
        <TxDagCanvas
          txids={cluster.txids}
          txs={txs}
          missing={missing}
          loading={loading}
          error={error}
          selectedTxid={selectedTxid}
          onSelectTxid={setSelectedTxid}
          emptyLabel="no transactions to graph"
        />
      </div>
      <div className="text-xs text-dim">
        SIZE {encodingCaption('vsize')} · COLOR {encodingCaption('feerate')}
      </div>
    </div>
  );
}

/**
 * Opens a cluster's transaction DAG in a near-fullscreen dialog, rendering
 * the caller's already-loaded transaction cache.
 */
export function openTxDagDialog(cluster: ClusterRef, cache: TransactionCache) {
  Dialog.call({
    title: `#${cluster.id} · ${cluster.txids.length} transactions`,
    size: 'full',
    body: <TxDagDialogContent cluster={cluster} {...cache} />,
  });
}
