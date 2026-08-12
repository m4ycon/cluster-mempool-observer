import { BackLink } from '../components/BackLink';
import { MempoolMetricChart } from '../components/snapshots/MempoolMetricChart';

export function MempoolSizeOverTime() {
  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Mempool size over time"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="text-xs text-ink tracking-widest">
          MEMPOOL SIZE OVER TIME
        </span>
      </div>

      <div className="flex-1 px-6 py-5">
        <div className="h-140">
          <MempoolMetricChart metric="mempool-tx-count" />
        </div>
      </div>
    </div>
  );
}
