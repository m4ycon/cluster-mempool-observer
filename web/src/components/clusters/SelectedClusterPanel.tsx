import { useState } from 'react';
import { useTransactionCache } from '../../hooks/useTransactionCache';
import { NumberFormat, TimeFormat } from '../../lib/format';
import { encodingCaption } from '../../lib/txMetrics';
import type { ClusterRef } from '../../types/events';
import { Term } from '../glossary/Term';
import { HelpButton } from '../help/HelpButton';
import { Tooltip } from '../Tooltip';
import { TxDagCanvas } from '../txdag/TxDagCanvas';
import { openTxDagDialog } from '../txdag/TxDagDialog';
import { TxidList } from '../txdag/TxidList';
import { VizButton } from '../VizButton';

export interface SelectedClusterPanelProps {
  cluster?: ClusterRef;
  /** Shown while nothing is selected yet. */
  emptyLabel?: string;
}

export function SelectedClusterPanel({
  cluster,
  emptyLabel = 'awaiting cluster feed...',
}: SelectedClusterPanelProps) {
  const [selectedTxid, setSelectedTxid] = useState<string | null>(null);
  const cache = useTransactionCache(cluster?.txids ?? []);

  if (!cluster) {
    return <div className="mt-4 text-xs text-faint">{emptyLabel}</div>;
  }

  // openTxDagDialog snapshots the cache once and never updates -- hide
  // EXPAND until there's something to show, or it opens frozen on loading.
  const dagReady =
    !cache.loading || cluster.txids.some((t) => cache.txs.has(t));

  return (
    <div>
      <div className="text-xs text-dim tracking-[0.12em]">
        <span className="inline-flex items-center gap-2">
          SELECTED CLUSTER
          <HelpButton topic="panel.selectedCluster" />
        </span>{' '}
        <span className="mt-1 text-sm text-ink">#{cluster.id}</span>
      </div>

      <div className="mt-4 border border-line">
        <div className="flex border-line border-b">
          <div className="flex-1 px-4 py-3">
            <div className="text-xs text-dim tracking-[0.12em]">
              TRANSACTIONS
            </div>
            <div className="mt-1 text-lg text-ink">{cluster.txids.length}</div>
          </div>
          <div className="flex-1 border-line border-l px-4 py-3">
            <div className="text-xs text-dim tracking-[0.12em]">
              <Term term="fee-rate">FEE-RATE</Term>
            </div>
            <div className="mt-1 text-lg text-orange">
              {(cluster.total_vsize
                ? cluster.total_fee / cluster.total_vsize
                : 0
              ).toFixed(1)}{' '}
              <span className="text-xs text-dim">s/vB</span>
            </div>
          </div>
        </div>

        <div className="flex border-line border-b">
          <div className="flex-1 px-4 py-3">
            <div className="text-xs text-dim tracking-[0.12em]">
              TOTAL <Term term="vsize">VSIZE</Term>
            </div>
            <div className="mt-1 text-lg text-ink">
              {NumberFormat.grouped(cluster.total_vsize)}{' '}
              <span className="text-xs text-dim">vB</span>
            </div>
          </div>
          <div className="flex-1 border-line border-l px-4 py-3">
            <div className="text-xs text-dim tracking-[0.12em]">TOTAL FEE</div>
            <div className="mt-1 text-lg text-orange">
              {NumberFormat.grouped(cluster.total_fee)}{' '}
              <span className="text-xs text-dim">sats</span>
            </div>
          </div>
        </div>

        <div className="px-4 py-3">
          <div className="text-xs text-dim tracking-[0.12em]">FIRST SEEN</div>
          <div className="mt-1 text-lg text-ink">
            <Tooltip label={TimeFormat.at(cluster.first_seen_at)}>
              <span>
                {TimeFormat.ago(cluster.first_seen_at)}{' '}
                <span className="text-xs text-dim">AGO</span>
              </span>
            </Tooltip>
          </div>
        </div>
      </div>

      <div className="border border-line border-t-0">
        <div className="flex items-center justify-between border-line border-b px-4 py-2 text-xs text-dim tracking-[0.12em]">
          <span className="inline-flex items-center gap-2">
            TRANSACTION GRAPH
            <HelpButton topic="panel.clusterDag" />
          </span>
          {dagReady && (
            <VizButton
              active={false}
              onClick={() => openTxDagDialog(cluster, cache)}
              ariaLabel="view transaction graph"
            >
              EXPAND
            </VizButton>
          )}
        </div>
        <div className="h-96">
          <TxDagCanvas
            txids={cluster.txids}
            txs={cache.txs}
            missing={cache.missing}
            loading={cache.loading}
            error={cache.error}
            selectedTxid={selectedTxid}
            onSelectTxid={setSelectedTxid}
            resetKey={cluster.id}
          />
        </div>
        <div className="border-line border-t px-4 py-2 text-xs text-dim">
          SIZE {encodingCaption('vsize')} · COLOR {encodingCaption('feerate')}
        </div>
      </div>

      <div className="border border-line border-t-0">
        <TxidList
          txids={cluster.txids}
          selectedTxid={selectedTxid}
          onSelectTxid={setSelectedTxid}
          className="max-h-48"
        />
      </div>
    </div>
  );
}
