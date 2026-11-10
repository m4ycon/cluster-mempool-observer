import type { GaugeMetric } from '../types/generated/GaugeMetric';

/** What one unit of each gauge series counts, for the y axis of its chart. */
export const GAUGE_METRIC_UNIT: Record<GaugeMetric, string> = {
  'cluster-count': 'clusters',
  'clustered-tx-count': 'clustered transactions',
  'mempool-tx-count': 'transactions',
  'total-vsize': 'virtual bytes (vB)',
  'total-fee': 'fees (sat)',
  'feerate-missing-percentage': "share of the node's feerate diagram",
  'feerate-extra-percentage': "share of the node's feerate diagram",
};
