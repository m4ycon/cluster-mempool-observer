import type { ClusterRef } from '../types/events';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';
import { ColorTiers } from './colorTiers';

/** Distribution summary over a set of clusters, keyed on one metric. */
export interface ClusterStats {
  count: number;
  totalVsize: number;
  totalFee: number;
  min: number;
  median: number;
  p90: number;
  max: number;
}

const EMPTY_STATS: ClusterStats = {
  count: 0,
  totalVsize: 0,
  totalFee: 0,
  min: 0,
  median: 0,
  p90: 0,
  max: 0,
};

/**
 * Distribution stats over `clusters`, e.g. for the histogram's summary panel:
 * that viz draws the whole mempool rather than a top-N slice, so its panel
 * should describe the whole mempool rather than one arbitrarily-selected
 * cluster. `metric` supplies the subject for min/median/p90/max; totals are
 * always vsize/fee regardless of it.
 *
 * A single sort (ascending, by `metric`) covers min/max/median/p90 -- no
 * spread into Math.min/max, which throws past ~65-125k arguments and this can
 * run over tens of thousands of clusters (see clusterHistogram.ts).
 */
export function clusterStats(
  clusters: ClusterRef[],
  metric: ClusterMetric,
): ClusterStats {
  if (clusters.length === 0) return EMPTY_STATS;

  let totalVsize = 0;
  let totalFee = 0;
  const vals = new Array<number>(clusters.length);
  for (let i = 0; i < clusters.length; i++) {
    const c = clusters[i];
    totalVsize += c.total_vsize;
    totalFee += c.total_fee;
    vals[i] = ClusterMetrics.value(c, metric);
  }

  const sorted = vals.sort((a, b) => a - b);

  return {
    count: clusters.length,
    totalVsize,
    totalFee,
    min: sorted[0],
    median: ColorTiers.quantile(sorted, 0.5),
    p90: ColorTiers.quantile(sorted, 0.9),
    max: sorted[sorted.length - 1],
  };
}
