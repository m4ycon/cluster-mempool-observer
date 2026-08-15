import { BackLink } from '../components/BackLink';
import { HelpButton } from '../components/help/HelpButton';
import { MempoolMetricChart } from '../components/snapshots/MempoolMetricChart';

export function ClusterCountOverTime() {
  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Cluster count over time"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="flex items-center gap-2 text-xs text-ink tracking-widest">
          CLUSTER COUNT OVER TIME
          <HelpButton topic="clusterCount.overTime" />
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
