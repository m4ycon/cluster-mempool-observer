import { BackLink } from '../components/BackLink';
import { CounterChart } from '../components/counters/CounterChart';
import { HelpButton } from '../components/help/HelpButton';

export function TxsPerMinOverTime() {
  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Txs/min over time"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="flex items-center gap-2 text-xs text-ink tracking-widest">
          TXS/MIN OVER TIME
          <HelpButton topic="txsPerMin.overTime" />
        </span>
      </div>

      <div className="flex-1 px-6 py-5">
        <div className="h-140">
          <CounterChart />
        </div>
      </div>
    </div>
  );
}
