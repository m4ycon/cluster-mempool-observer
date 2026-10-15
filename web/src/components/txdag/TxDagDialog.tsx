import { useMemo, useState } from 'react';
import { useChunkView } from '../../hooks/useChunkView';
import type { TransactionCache } from '../../hooks/useTransactionCache';
import { clusterTxCount, clusterTxids } from '../../lib/clusterChunks';
import { encodingCaption } from '../../lib/txMetrics';
import type { ClusterRef } from '../../types/events';
import { Dialog } from '../dialog/Dialog';
import { HelpButton } from '../help/HelpButton';
import { ChunkStrip } from './ChunkStrip';
import { TxDagCanvas } from './TxDagCanvas';
import { TxidList } from './TxidList';

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
  const txids = useMemo(() => clusterTxids(cluster), [cluster]);
  const chunkView = useChunkView(cluster, txs);

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="text-xs text-dim tracking-[0.12em]">
        <span className="inline-flex items-center gap-2">
          TRANSACTION GRAPH
          <HelpButton topic="panel.clusterDag" />
        </span>
      </div>
      <div className="flex min-h-0 flex-1 gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <div className="min-h-0 flex-1">
            <TxDagCanvas
              txids={txids}
              txs={txs}
              missing={missing}
              loading={loading}
              error={error}
              selectedTxid={selectedTxid}
              onSelectTxid={setSelectedTxid}
              emptyLabel="no transactions to graph"
              resetKey={cluster.id}
              chunkOf={chunkView.chunkOf}
              chunkColors={chunkView.colors}
              highlightedChunk={chunkView.highlighted}
              onHoverChunk={chunkView.setHighlighted}
            />
          </div>
          <ChunkStrip
            chunks={cluster.chunks}
            colors={chunkView.colors}
            highlighted={chunkView.highlighted}
            selected={
              selectedTxid === null
                ? null
                : (chunkView.chunkOf.get(selectedTxid) ?? null)
            }
            onHoverChunk={chunkView.setHighlighted}
          />
        </div>
        <TxidList
          txids={txids}
          selectedTxid={selectedTxid}
          onSelectTxid={setSelectedTxid}
          compact
          className="shrink-0 border border-line"
        />
      </div>
      <div className="text-xs text-dim">
        SIZE {encodingCaption('vsize')} · COLOR {encodingCaption('feerate')} ·
        RING chunk · STRIP chunks in mining order
      </div>
    </div>
  );
}

/**
 * Opens a cluster's transaction DAG in a near-fullscreen dialog, rendering
 * the caller's already-loaded transaction cache.
 */
export function openTxDagDialog(cluster: ClusterRef, cache: TransactionCache) {
  const txCount = clusterTxCount(cluster);
  Dialog.call({
    title:
      `#${cluster.id} · ${txCount} ` + `transaction${txCount === 1 ? '' : 's'}`,
    size: 'full',
    body: <TxDagDialogContent cluster={cluster} {...cache} />,
  });
}
