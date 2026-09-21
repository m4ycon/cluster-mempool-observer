import { Link, Outlet } from '@tanstack/react-router';
import { ConnectionDot } from './components/ConnectionDot';
import { Dialog } from './components/dialog/Dialog';
import { Tooltip } from './components/Tooltip';
import { useChainTip } from './hooks/useChainTip';
import dayjs from './lib/dayjs';
import { NumberFormat } from './lib/format';
import { WebRoutes } from './lib/routes';
import { useWsReadyState } from './ws/useWsReadyState';

export function RootLayout() {
  const block = useChainTip();
  const readyState = useWsReadyState();
  const height = block ? NumberFormat.grouped(block.height) : '-';
  const lastBlock = block ? dayjs(block.mined_at).fromNow(true) : '-';
  const minedAt = block
    ? dayjs(block.mined_at).format('YYYY-MM-DD HH:mm:ss')
    : 'WAITING FOR CHAIN TIP';

  return (
    <div className="min-h-screen bg-bg font-mono text-body">
      <div className="mx-auto flex min-h-screen max-w-7xl flex-col border-line border-r border-l">
        {/* Global header */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-line border-b px-6 py-3">
          <div className="flex flex-wrap items-baseline">
            <Link
              to={WebRoutes.home}
              className="text-xs font-bold text-ink tracking-[0.06em] hover:text-orange"
            >
              CLUSTER_MEMPOOL_OBSERVER
            </Link>
            <span className="animate-blink text-orange">▌</span>
            <VersionTag />
          </div>

          <div className="flex items-center gap-4">
            <ConnectionDot readyState={readyState} label="STATS FEED" />
            <span className="text-xs text-slate">
              HEIGHT <span className="text-ink">{height}</span>
            </span>
            <Tooltip label={minedAt} align="right">
              <span className="text-xs text-slate uppercase">
                LAST BLOCK <span className="text-ink">{lastBlock}</span> AGO
              </span>
            </Tooltip>
          </div>
        </div>

        <Outlet />
      </div>

      <Dialog />
    </div>
  );
}

function VersionTag() {
  const sha = import.meta.env.VITE_GIT_SHA?.slice(0, 7);
  const tag = <span className="text-xs text-dim">alpha</span>;

  if (!sha) return tag;

  return <Tooltip label={`BUILD ${sha}`}>{tag}</Tooltip>;
}
