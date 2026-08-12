import { useNavigate } from '@tanstack/react-router';
import { Bubbles } from '../components/Bubbles';
import { ClusterCountPreview } from '../components/ClusterCountPreview';
import { FeeRatePreview } from '../components/FeeRatePreview';
import { MempoolSizePreview } from '../components/MempoolSizePreview';
import { PreviewCard } from '../components/PreviewCard';
import { StatTile } from '../components/StatTile';
import { useMempoolStats } from '../hooks/useMempoolStats';
import dayjs from '../lib/dayjs';
import { NumberFormat } from '../lib/format';
import { sim } from '../lib/sim';

const PREVIEW_CARD_WIDTH = 340;
const PREVIEW_CARD_HEIGHT = 144;

export function Home() {
  const navigate = useNavigate();
  const stats = useMempoolStats();
  const prevClusters = sim.clusters(40, 1);
  const prevBars = sim.hbars(PREVIEW_CARD_HEIGHT, 'log');
  const today = dayjs().format('YYYY-MM-DD');

  return (
    <div className="flex flex-1 flex-col bg-bg">
      {/* Stat tiles */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(170px,1fr))]">
        <StatTile
          label="TXS IN MEMPOOL"
          value={stats ? NumberFormat.grouped(stats.mempool_size) : '-'}
        />

        <StatTile
          label="TX / MIN"
          value={stats ? NumberFormat.grouped(stats.tx_per_min) : '-'}
        />

        <StatTile
          label="CLUSTERS"
          value={stats ? NumberFormat.grouped(stats.cluster_count) : '-'}
        />
      </div>

      {/* Preview cards */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))]">
        <PreviewCard
          title="FEE-RATE"
          caption="sat/vB over time"
          onClick={() => navigate({ to: '/fees' })}
        >
          <FeeRatePreview
            width={PREVIEW_CARD_WIDTH}
            height={PREVIEW_CARD_HEIGHT}
          />
        </PreviewCard>

        <PreviewCard
          title="CLUSTER GRAPH"
          caption="clusters packed by vsize"
          onClick={() => navigate({ to: '/clusters' })}
        >
          <Bubbles sim={sim} clusters={prevClusters} />
        </PreviewCard>

        <PreviewCard
          title="CLUSTER SIZE DISTRIBUTION"
          caption="tx per cluster · log count"
          onClick={() => navigate({ to: '/dist' })}
        >
          <div className="flex h-full items-end gap-1">
            {prevBars.map((b) => (
              <div
                key={b.lab}
                className="flex h-full flex-1 flex-col justify-end"
              >
                <div className="bg-bar" style={{ height: b.h }} />
              </div>
            ))}
          </div>
        </PreviewCard>

        <PreviewCard
          title="CLUSTER COUNT OVER TIME"
          caption="cluster count, last 24h"
          onClick={() => navigate({ to: '/mempool/snapshots/cluster-count' })}
        >
          <ClusterCountPreview
            width={PREVIEW_CARD_WIDTH}
            height={PREVIEW_CARD_HEIGHT}
          />
        </PreviewCard>

        <PreviewCard
          title="MEMPOOL SIZE OVER TIME"
          caption="txs in mempool, last 24h"
          onClick={() =>
            navigate({ to: '/mempool/snapshots/mempool-tx-count' })
          }
        >
          <MempoolSizePreview
            width={PREVIEW_CARD_WIDTH}
            height={PREVIEW_CARD_HEIGHT}
          />
        </PreviewCard>
      </div>

      <div className="flex-1" />

      {/* Footer */}
      <div className="flex justify-between border-line border-t px-6 py-3 text-xs text-faint">
        <span>ALL FIGURES SIMULATED · NOT CONNECTED TO A NODE</span>
        <span>CLUSTER_MEMPOOL_OBSERVER · {today}</span>
      </div>
    </div>
  );
}
