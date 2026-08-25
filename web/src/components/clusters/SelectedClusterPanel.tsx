import clsx from 'clsx';
import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTransactionCache } from '../../hooks/useTransactionCache';
import { NumberFormat, TimeFormat } from '../../lib/format';
import { encodingCaption } from '../../lib/txMetrics';
import type { ClusterRef } from '../../types/events';
import { Term } from '../glossary/Term';
import { HelpButton } from '../help/HelpButton';
import { Tooltip } from '../Tooltip';
import { TxDagCanvas } from '../txdag/TxDagCanvas';
import { openTxDagDialog } from '../txdag/TxDagDialog';
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
  const [copiedTxid, setCopiedTxid] = useState<string | null>(null);
  const [selectedTxid, setSelectedTxid] = useState<string | null>(null);
  const rowRefs = useRef(new Map<string, HTMLDivElement>());
  const cache = useTransactionCache(cluster?.txids ?? []);

  // Scrolls the TXIDS row a graph-node click selected into view; 'nearest'
  // so it doesn't fight the list's own overflow-y-auto scrolling.
  useEffect(() => {
    if (!selectedTxid) return;
    rowRefs.current.get(selectedTxid)?.scrollIntoView({ block: 'nearest' });
  }, [selectedTxid]);

  const handleCopy = (txid: string) => {
    navigator.clipboard?.writeText(txid).catch(() => {});
    setCopiedTxid(txid);
    setTimeout(() => {
      setCopiedTxid((current) => (current === txid ? null : current));
    }, 1000);
  };

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
            {cluster.first_seen_at ? (
              <Tooltip label={TimeFormat.at(cluster.first_seen_at)}>
                <span>
                  {TimeFormat.ago(cluster.first_seen_at)}{' '}
                  <span className="text-xs text-dim">AGO</span>
                </span>
              </Tooltip>
            ) : (
              <span className="text-dim">unknown</span>
            )}
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
            interactive={false}
          />
        </div>
        <div className="border-line border-t px-4 py-2 text-xs text-dim">
          SIZE {encodingCaption('vsize')} · COLOR {encodingCaption('feerate')}
        </div>
      </div>

      <div className="border border-line border-t-0">
        <div className="flex items-center justify-between border-line border-b px-4 py-2 text-xs text-dim tracking-[0.12em]">
          <span>TXIDS</span>
        </div>
        <div className="max-h-40 overflow-y-auto">
          {cluster.txids.map((txid) => {
            const copied = copiedTxid === txid;
            const selected = txid === selectedTxid;
            return (
              // Can't be a <button>: it contains the copy <button> below.
              // biome-ignore lint/a11y/useSemanticElements: nested button
              <div
                key={txid}
                ref={(el) => {
                  if (el) rowRefs.current.set(txid, el);
                  else rowRefs.current.delete(txid);
                }}
                role="button"
                tabIndex={0}
                onClick={() => setSelectedTxid(txid)}
                onKeyDown={(e) => {
                  if (e.key !== 'Enter' && e.key !== ' ') return;
                  e.preventDefault();
                  setSelectedTxid(txid);
                }}
                data-testid="txid-row"
                data-txid={txid}
                data-selected={selected}
                className={clsx(
                  'group flex w-full items-center justify-between gap-3 border-line border-b px-4 py-1.5 text-left text-xs text-body last:border-b-0',
                  selected && 'outline-2 outline-orange -outline-offset-2',
                )}
              >
                <span className="break-all">{txid}</span>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCopy(txid);
                  }}
                  className={clsx(
                    'mco-reset shrink-0 transition-opacity',
                    copied
                      ? 'text-orange opacity-100'
                      : 'text-dim opacity-0 group-hover:text-orange group-hover:opacity-100',
                  )}
                >
                  {copied ? (
                    <Check size={14} aria-label="Copied" />
                  ) : (
                    <Copy size={14} aria-label="Copy txid" />
                  )}
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
