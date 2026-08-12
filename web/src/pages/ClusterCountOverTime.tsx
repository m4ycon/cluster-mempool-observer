import { BackLink } from '../components/BackLink';
import { MempoolMetricChart } from '../components/snapshots/MempoolMetricChart';

export function ClusterCountOverTime() {
  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Cluster count over time"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="text-xs text-ink tracking-widest">
          CLUSTER COUNT OVER TIME
        </span>
      </div>

      <div className="flex-1 px-6 py-5">
        <div className="h-140">
          <MempoolMetricChart metric="cluster-count" />
        </div>
      </div>
    </div>
  );
}
