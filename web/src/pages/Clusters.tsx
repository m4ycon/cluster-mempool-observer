import { getRouteApi } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { ReadyState } from 'react-use-websocket';
import { useDebouncedCallback } from 'use-debounce';
import { BackLink } from '../components/BackLink';
import { ClusterCanvas } from '../components/clusters/ClusterCanvas';
import { ClusterControls } from '../components/clusters/ClusterControls';
import { ClusterLegend } from '../components/clusters/ClusterLegend';
import { ClusterStatsPanel } from '../components/clusters/ClusterStatsPanel';
import { SelectedClusterPanel } from '../components/clusters/SelectedClusterPanel';
import { useClusterDeltaSocket } from '../hooks/useClusterDeltaSocket';
import { histogramLayout } from '../lib/clusterHistogram';
import { packLayout, treemapLayout } from '../lib/clusterLayout';
import type { ClusterMetric } from '../lib/clusterMetrics';
import { ClusterMetrics } from '../lib/clusterMetrics';
import { clusterStats } from '../lib/clusterStats';
import type { ClustersViz } from '../lib/clustersSearch';
import {
  decodeClustersSearch,
  patchClustersSearch,
} from '../lib/clustersSearch';
import { NumberFormat } from '../lib/format';

const route = getRouteApi('/clusters');

/** How long a slider must settle before its move is written to the URL. */
export const SLIDER_COMMIT_MS = 200;

/** Ceiling on that wait, so a drag that never pauses still reaches the URL. */
const SLIDER_COMMIT_MAX_MS = 500;

export function Clusters() {
  const { clusters, lastUpdates, readyState, paused, togglePaused } =
    useClusterDeltaSocket();

  const search = route.useSearch();
  const navigate = route.useNavigate();

  const setViz = (patch: Partial<ClustersViz>) =>
    navigate({
      search: (prev) => patchClustersSearch(prev, patch),
      replace: true,
    });

  const [draft, setDraft] = useState<Partial<ClustersViz>>({});

  const commitDraft = useDebouncedCallback(
    (patch: Partial<ClustersViz>) => {
      setViz(patch);
      setDraft({});
    },
    SLIDER_COMMIT_MS,
    { maxWait: SLIDER_COMMIT_MAX_MS },
  );

  const slide = (patch: Partial<ClustersViz>) => {
    setDraft(patch);
    commitDraft(patch);
  };

  const { vizType, sizeMetric, colorMetric, showCount, bins } = {
    ...decodeClustersSearch(search),
    ...draft,
  };

  const [selectedId, setSelectedId] = useState<number | null>(null);

  const [linked, setLinked] = useState(sizeMetric === colorMetric);

  const handleSizeMetricChange = (m: ClusterMetric) =>
    setViz(linked ? { sizeMetric: m, colorMetric: m } : { sizeMetric: m });

  const visible = useMemo(
    () => ClusterMetrics.top(clusters, sizeMetric, showCount),
    [clusters, sizeMetric, showCount],
  );

  const colorVals = useMemo(
    () => visible.map((c) => ClusterMetrics.value(c, colorMetric)),
    [visible, colorMetric],
  );

  // One scale for both the marks and the legend: they cannot disagree.
  const colorScale = useMemo(
    () => ClusterMetrics.scaleFor(colorMetric, colorVals),
    [colorMetric, colorVals],
  );

  const packed = useMemo(
    () => (vizType === 'circles' ? packLayout(visible, sizeMetric) : []),
    [visible, sizeMetric, vizType],
  );

  const cells = useMemo(
    () => (vizType === 'treemap' ? treemapLayout(visible, sizeMetric) : []),
    [visible, sizeMetric, vizType],
  );

  const histogram = useMemo(
    () =>
      histogramLayout(
        vizType === 'histogram' ? clusters : [],
        sizeMetric,
        bins,
      ),
    [clusters, sizeMetric, bins, vizType],
  );

  // Only computed for the histogram
  const stats = useMemo(
    () => clusterStats(vizType === 'histogram' ? clusters : [], sizeMetric),
    [clusters, sizeMetric, vizType],
  );

  const selected = visible.find((c) => c.id === selectedId) ?? visible[0];

  const totalClusters = clusters.length;
  // A dropped socket is not the same thing as a user-requested pause.
  const disconnected = readyState !== ReadyState.OPEN;

  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Cluster explorer"
    >
      {/* Sub-header */}
      <div className="flex justify-between border-line border-b px-6 py-3">
        <div className="flex items-baseline gap-4">
          <BackLink />
          <span className="text-xs text-ink tracking-widest">
            CLUSTER GRAPH
          </span>
        </div>
        <div className="text-xs text-dim">
          {vizType === 'histogram' ? (
            <>
              SHOWING ALL{' '}
              <span className="text-ink">
                {NumberFormat.grouped(totalClusters)}
              </span>{' '}
              CLUSTERS
            </>
          ) : (
            <>
              SHOWING <span className="text-ink">{visible.length}</span> OF{' '}
              <span className="text-ink">
                {NumberFormat.grouped(totalClusters)}
              </span>{' '}
              CLUSTERS
            </>
          )}
          {paused && <span className="text-orange"> · PAUSED</span>}
          {disconnected && <span className="text-faint"> · RECONNECTING</span>}
          {vizType !== 'histogram' &&
            ` · CLICK A ${vizType === 'circles' ? 'BUBBLE' : 'CELL'} TO INSPECT`}
        </div>
      </div>

      {/* Controls */}
      <ClusterControls
        vizType={vizType}
        onVizTypeChange={(v) => setViz({ vizType: v })}
        sizeMetric={sizeMetric}
        onSizeMetricChange={handleSizeMetricChange}
        colorMetric={colorMetric}
        onColorMetricChange={(m) => setViz({ colorMetric: m })}
        showCount={showCount}
        onShowCountChange={(n) => slide({ showCount: n })}
        bins={bins}
        onBinsChange={(n) => slide({ bins: n })}
        paused={paused}
        onTogglePause={togglePaused}
        linked={linked}
        onLinkedChange={setLinked}
      />

      {/* Body */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(360px,1fr))]">
        <div className="border-line border-r px-6 py-5">
          <div className="h-140">
            <ClusterCanvas
              vizType={vizType}
              packed={packed}
              cells={cells}
              histogramLayout={histogram}
              sizeMetric={sizeMetric}
              colorMetric={colorMetric}
              colorScale={colorScale}
              lastUpdates={lastUpdates}
              selectedId={selected?.id ?? null}
              onSelect={setSelectedId}
            />
          </div>
          {vizType !== 'histogram' && (
            <ClusterLegend
              vizType={vizType}
              colorMetric={colorMetric}
              colorVals={colorVals}
              colorScale={colorScale}
            />
          )}
        </div>

        <div
          className="px-6 py-5"
          data-screen-label={
            vizType === 'histogram'
              ? 'Cluster distribution panel'
              : 'Selected cluster panel'
          }
        >
          {vizType === 'histogram' ? (
            <ClusterStatsPanel stats={stats} metric={sizeMetric} />
          ) : (
            <SelectedClusterPanel cluster={selected} />
          )}
        </div>
      </div>
    </div>
  );
}
