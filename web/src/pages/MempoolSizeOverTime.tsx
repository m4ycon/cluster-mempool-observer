import { BackLink } from '../components/BackLink';
import { GaugeMetricChart } from '../components/gauges/GaugeMetricChart';
import { HelpButton } from '../components/help/HelpButton';

export function MempoolSizeOverTime() {
  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Mempool size over time"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="flex items-center gap-2 text-xs text-ink tracking-widest">
          MEMPOOL SIZE OVER TIME
          <HelpButton topic="mempoolSize.overTime" />
        </span>
      </div>

      <div className="flex-1 px-6 py-5">
        <div className="h-140">
          <GaugeMetricChart metric="mempool-tx-count" />
        </div>
      </div>
    </div>
  );
}
