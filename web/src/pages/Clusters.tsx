import { useMemo, useState } from 'react';
import { BackLink } from '../components/BackLink';
import type { VizType } from '../components/clusters/ClusterCanvas';
import { ClusterCanvas } from '../components/clusters/ClusterCanvas';
import { ClusterControls } from '../components/clusters/ClusterControls';
import { ClusterLegend } from '../components/clusters/ClusterLegend';
import { SelectedClusterPanel } from '../components/clusters/SelectedClusterPanel';
import { useClusterDeltaSocket } from '../hooks/useClusterDeltaSocket';
import { packLayout, treemapLayout } from '../lib/clusterLayout';
import type { ClusterMetric } from '../lib/clusterMetrics';
import { ClusterMetrics } from '../lib/clusterMetrics';
import { NumberFormat } from '../lib/format';

export function Clusters() {
  const { clusters } = useClusterDeltaSocket();

  const [vizType, setVizType] = useState<VizType>('circles');
  const [sizeMetric, setSizeMetric] = useState<ClusterMetric>('vsize');
  const [colorMetric, setColorMetric] = useState<ClusterMetric>('feerate');
  const [showCount, setShowCount] = useState(40);
  const [selectedId, setSelectedId] = useState<number | null>(null);

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

  const selected = visible.find((c) => c.id === selectedId) ?? visible[0];

  const totalClusters = clusters.length;
  const markWord = vizType === 'circles' ? 'BUBBLE' : 'CELL';

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
          SHOWING <span className="text-ink">{visible.length}</span> OF{' '}
          <span className="text-ink">
            {NumberFormat.grouped(totalClusters)}
          </span>{' '}
          CLUSTERS · CLICK A {markWord} TO INSPECT
        </div>
      </div>

      {/* Controls */}
      <ClusterControls
        vizType={vizType}
        onVizTypeChange={setVizType}
        sizeMetric={sizeMetric}
        onSizeMetricChange={setSizeMetric}
        colorMetric={colorMetric}
        onColorMetricChange={setColorMetric}
        showCount={showCount}
        onShowCountChange={setShowCount}
      />

      {/* Body */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(360px,1fr))]">
        <div className="border-line border-r px-6 py-5">
          <div className="h-140">
            <ClusterCanvas
              vizType={vizType}
              packed={packed}
              cells={cells}
              sizeMetric={sizeMetric}
              colorMetric={colorMetric}
              colorScale={colorScale}
              selectedId={selected?.id ?? null}
              onSelect={setSelectedId}
            />
          </div>
          <ClusterLegend
            vizType={vizType}
            colorMetric={colorMetric}
            colorVals={colorVals}
            colorScale={colorScale}
          />
        </div>

        <div className="px-6 py-5" data-screen-label="Selected cluster panel">
          <SelectedClusterPanel cluster={selected} />
        </div>
      </div>
    </div>
  );
}
