import clsx from 'clsx';
import type { ReactNode } from 'react';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { ClusterStats } from '../../lib/clusterStats';
import { NumberFormat } from '../../lib/format';
import { Term } from '../glossary/Term';
import { HelpButton } from '../help/HelpButton';

export interface ClusterStatsPanelProps {
  stats: ClusterStats;
  metric: ClusterMetric;
}

interface CellProps {
  label: ReactNode;
  value: string;
  unit?: string;
  orange?: boolean;
  /** Right-hand cell in a two-up row: adds the divider SelectedClusterPanel uses. */
  divided?: boolean;
}

/** One labelled cell, matching SelectedClusterPanel's TRANSACTIONS/FEE-RATE grid. */
function Cell({ label, value, unit, orange, divided }: CellProps) {
  return (
    <div
      className={clsx('flex-1 px-4 py-3', divided && 'border-line border-l')}
    >
      <div className="text-xs text-dim tracking-[0.12em]">{label}</div>
      <div
        className={clsx('mt-1 text-lg', orange ? 'text-orange' : 'text-ink')}
      >
        {value} {unit && <span className="text-xs text-dim">{unit}</span>}
      </div>
    </div>
  );
}

/**
 * Distribution summary for the histogram, which bins the whole mempool
 * rather than a top-N slice -- so there is no single clicked cluster for it
 * to describe, unlike SelectedClusterPanel. Shows the shape of the
 * distribution the histogram is drawing instead: totals plus the spread of
 * the currently-binned metric.
 */
export function ClusterStatsPanel({ stats, metric }: ClusterStatsPanelProps) {
  if (stats.count === 0) {
    return (
      <div className="mt-4 text-xs text-faint">awaiting cluster feed...</div>
    );
  }

  const unit = ClusterMetrics.UNIT[metric];
  const label = ClusterMetrics.LABEL[metric].toUpperCase();
  const fmt = (v: number) => ClusterMetrics.markLabel(v, metric);

  return (
    <div>
      <div className="text-xs text-dim tracking-[0.12em]">
        <span className="inline-flex items-center gap-2">
          DISTRIBUTION
          <HelpButton topic="panel.distribution" />
        </span>{' '}
        <span className="mt-1 text-sm text-ink">
          {NumberFormat.grouped(stats.count)} CLUSTERS
        </span>
      </div>

      <div className="mt-4 border border-line">
        <div className="flex border-line border-b">
          <Cell
            label={
              <>
                TOTAL <Term term="vsize">VSIZE</Term>
              </>
            }
            value={NumberFormat.grouped(stats.totalVsize)}
            unit="vB"
          />
          <Cell
            label="TOTAL FEE"
            value={NumberFormat.grouped(stats.totalFee)}
            unit="sats"
            orange
            divided
          />
        </div>

        <div className="flex border-line border-b">
          <Cell label={`MIN ${label}`} value={fmt(stats.min)} unit={unit} />
          <Cell
            label={`MEDIAN ${label}`}
            value={fmt(stats.median)}
            unit={unit}
            divided
          />
        </div>

        {/* Last row: no border-b, the container's own border closes the grid. */}
        <div className="flex">
          <Cell
            label={
              <>
                <Term term="p90">P90</Term> {label}
              </>
            }
            value={fmt(stats.p90)}
            unit={unit}
          />
          <Cell
            label={`MAX ${label}`}
            value={fmt(stats.max)}
            unit={unit}
            divided
          />
        </div>
      </div>
    </div>
  );
}
