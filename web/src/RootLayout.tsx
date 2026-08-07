import { Link, Outlet } from '@tanstack/react-router';
import { ConnectionDot } from './components/ConnectionDot';
import { useChainTip } from './hooks/useChainTip';
import dayjs from './lib/dayjs';
import { NumberFormat } from './lib/format';
import { useWsReadyState } from './ws/useWsReadyState';

export function RootLayout() {
  const block = useChainTip();
  const readyState = useWsReadyState();
  const height = block ? NumberFormat.grouped(block.height) : '-';
  const lastBlock = block ? dayjs(block.mined_at).fromNow(true) : '-';

  return (
    <div className="min-h-screen bg-bg font-mono text-body">
      <div className="mx-auto flex min-h-screen max-w-7xl flex-col border-line border-r border-l">
        {/* Global header */}
        <div className="flex items-center justify-between border-line border-b px-6 py-3">
          <div className="flex items-baseline gap-2">
            <Link
              to="/"
              className="text-xs font-bold text-ink tracking-[0.06em] hover:text-orange"
            >
              CLUSTER_MEMPOOL_OBSERVER
            </Link>
            <span className="animate-blink text-orange">▌</span>
            <span className="text-xs text-dim">v0.1 · EXPERIMENTAL</span>
          </div>
          <div className="flex items-center gap-4">
            <ConnectionDot readyState={readyState} label="STATS FEED" />
            <span className="text-xs text-slate">
              HEIGHT <span className="text-ink">{height}</span>
            </span>
            <span className="text-xs text-slate uppercase">
              LAST BLOCK <span className="text-ink">{lastBlock}</span> AGO
            </span>
          </div>
        </div>

        <Outlet />
      </div>
    </div>
  );
}
