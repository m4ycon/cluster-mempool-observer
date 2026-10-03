import clsx from 'clsx';
import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { TxidFormat } from '../../lib/format';
import { ExplorerRoutes } from '../../lib/routes';

/** Column heads aligned to TxidList's rows. */
function TxidListHeader() {
  return (
    <div className="flex items-center gap-3 border-line border-b px-4 py-2 text-xs text-dim tracking-[0.12em]">
      <span className="w-5 shrink-0 text-right">#</span>
      <span>TXID</span>
    </div>
  );
}

export interface TxidListProps {
  txids: string[];
  selectedTxid: string | null;
  onSelectTxid: (txid: string) => void;
  /** Show both ends of each txid instead of all 64 chars; link and copy keep the full one. */
  compact?: boolean;
  className?: string;
}

/** Key from the graph's reference numbers to the txids they stand for. */
export function TxidList({
  txids,
  selectedTxid,
  onSelectTxid,
  compact = false,
  className,
}: TxidListProps) {
  const [copiedTxid, setCopiedTxid] = useState<string | null>(null);
  const rowRefs = useRef(new Map<string, HTMLDivElement>());

  // Scrolls the row a graph-node click selected into view; 'nearest' so it
  // doesn't fight the list's own overflow-y-auto scrolling.
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

  // Only the rows scroll, so the column heads stay in view.
  return (
    <div className={clsx('flex min-h-0 flex-col', className)}>
      <TxidListHeader />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {txids.map((txid, i) => {
          const copied = copiedTxid === txid;
          const selected = txid === selectedTxid;
          return (
            // Can't be a <button>: it contains the txid link and the copy
            // <button> below.
            // biome-ignore lint/a11y/useSemanticElements: nested button
            <div
              key={txid}
              ref={(el) => {
                if (el) rowRefs.current.set(txid, el);
                else rowRefs.current.delete(txid);
              }}
              role="button"
              tabIndex={0}
              onClick={() => onSelectTxid(txid)}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.key !== 'Enter' && e.key !== ' ') return;
                e.preventDefault();
                onSelectTxid(txid);
              }}
              data-testid="txid-row"
              data-txid={txid}
              data-selected={selected}
              className={clsx(
                'group flex w-full items-center gap-3 border-line border-b px-4 py-1.5 text-left text-xs text-body last:border-b-0',
                selected && 'outline-2 outline-orange -outline-offset-2',
              )}
            >
              <span
                className="w-5 shrink-0 text-right text-dim"
                data-testid="txid-ref"
              >
                {i + 1}
              </span>
              <a
                href={ExplorerRoutes.tx(txid)}
                target="_blank"
                rel="noreferrer"
                title={compact ? txid : undefined}
                className="min-w-0 flex-1 break-all hover:text-orange hover:underline focus-visible:text-orange"
              >
                {compact ? TxidFormat.compact(txid) : txid}
              </a>
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
  );
}
