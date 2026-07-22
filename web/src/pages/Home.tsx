import { useNavigate } from '@tanstack/react-router';
import { Bubbles } from '../components/Bubbles';
import { PreviewCard } from '../components/PreviewCard';
import { StatTile } from '../components/StatTile';
import { useMempoolStatsSocket } from '../hooks/useMempoolStatsSocket';
import dayjs from '../lib/dayjs';
import { sim } from '../lib/sim';

const PREVIEW_CARD_HEIGHT = 144;

export function Home() {
  const navigate = useNavigate();
  const { stats } = useMempoolStatsSocket();
  const prevFee = sim.feeAt('24h', 340, PREVIEW_CARD_HEIGHT);
  const prevClusters = sim.clusters(40, 1);
  const prevBars = sim.hbars(PREVIEW_CARD_HEIGHT, 'log');
  const today = dayjs().format('YYYY-MM-DD');

  return (
    <div className="flex flex-1 flex-col bg-bg">
      {/* Stat tiles */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(170px,1fr))]">
        <StatTile
          label="TXS IN MEMPOOL"
          value={stats ? stats.mempool_size.toLocaleString() : '-'}
        />

        <StatTile
          label="TX / MIN"
          value={stats ? stats.tx_per_min.toLocaleString() : '-'}
        />

        <StatTile
          label="CLUSTERS"
          value={stats ? stats.cluster_count.toLocaleString() : '-'}
        />
      </div>

      {/* Preview cards */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))]">
        <PreviewCard
          title="FEE-RATE"
          caption="sat/vB over time"
          onClick={() => navigate({ to: '/fees' })}
        >
          <svg
            width="100%"
            height={PREVIEW_CARD_HEIGHT}
            viewBox={`0 0 340 ${PREVIEW_CARD_HEIGHT}`}
            preserveAspectRatio="none"
            className="block"
          >
            <title>Fee-rate preview</title>
            <path d={prevFee.area} fill="rgba(247,147,26,0.09)" stroke="none" />
            <path
              d={prevFee.med}
              fill="none"
              stroke="#f7931a"
              strokeWidth={1.5}
            />
          </svg>
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
