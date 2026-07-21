import { Link, Outlet } from '@tanstack/react-router';

export function RootLayout() {
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
            <span className="text-xs text-dim">v0.5 · simulated feed</span>
          </div>
          <div className="flex items-center gap-4">
            <span className="text-xs text-slate">
              HEIGHT <span className="text-ink">903,417</span>
            </span>
            <span className="text-xs text-slate">
              LAST BLOCK <span className="text-ink">6 MIN</span> AGO
            </span>
          </div>
        </div>

        <Outlet />
      </div>
    </div>
  );
}
