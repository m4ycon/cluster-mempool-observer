import clsx from 'clsx';
import { Check, Copy } from 'lucide-react';
import { useState } from 'react';
import { NumberFormat } from '../../lib/format';
import type { ClusterRef } from '../../types/events';

export interface SelectedClusterPanelProps {
  cluster?: ClusterRef;
}

export function SelectedClusterPanel({ cluster }: SelectedClusterPanelProps) {
  const [copiedTxid, setCopiedTxid] = useState<string | null>(null);

  const handleCopy = (txid: string) => {
    navigator.clipboard?.writeText(txid).catch(() => {});
    setCopiedTxid(txid);
    setTimeout(() => {
      setCopiedTxid((current) => (current === txid ? null : current));
    }, 1000);
  };

  if (!cluster) {
    return (
      <div className="mt-4 text-xs text-faint">awaiting cluster feed...</div>
    );
  }

  return (
    <div>
      <div className="text-xs text-dim tracking-[0.12em]">
        SELECTED CLUSTER{' '}
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
            <div className="text-xs text-dim tracking-[0.12em]">FEE-RATE</div>
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
              TOTAL VSIZE
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
      </div>

      <div className="border border-line border-t-0">
        <div className="flex border-line border-b px-4 py-2 text-xs text-dim tracking-[0.12em]">
          <span>TXIDS</span>
        </div>
        <div className="max-h-[232px] overflow-y-auto">
          {cluster.txids.map((txid) => {
            const copied = copiedTxid === txid;
            return (
              <button
                key={txid}
                type="button"
                onClick={() => handleCopy(txid)}
                className="mco-reset group flex w-full items-center justify-between gap-3 border-line border-b px-4 py-1.5 text-left text-xs text-body last:border-b-0"
              >
                <span className="break-all">{txid}</span>
                <span
                  className={clsx(
                    'flex-shrink-0 transition-opacity',
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
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
