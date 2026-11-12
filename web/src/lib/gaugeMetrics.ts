import type { GaugeMetric } from '../types/generated/GaugeMetric';
import { CHUNK_DIFF_COLOR } from './feerateDiagramChart';
import { NumberFormat } from './format';

export interface GaugeMetricMeta {
  /** What one unit of the series counts, for the y axis of its chart. */
  unit: string;
  /** Legend name when a chart draws more than one series. */
  label: string;
  color: string;
  formatTick: (value: number) => string;
  formatValue: (value: number) => string;
}

const COUNT = {
  color: '#f7931a',
  formatTick: NumberFormat.compact,
  formatValue: NumberFormat.grouped,
};

const SHARE = {
  unit: "share of the node's feerate diagram",
  formatTick: NumberFormat.percent,
  formatValue: NumberFormat.percent,
};

export const GAUGE_METRICS: Record<GaugeMetric, GaugeMetricMeta> = {
  'cluster-count': { ...COUNT, unit: 'clusters', label: 'clusters' },
  'clustered-tx-count': {
    ...COUNT,
    unit: 'clustered transactions',
    label: 'clustered transactions',
  },
  'mempool-tx-count': {
    ...COUNT,
    unit: 'transactions',
    label: 'transactions',
  },
  'total-vsize': { ...COUNT, unit: 'virtual bytes (vB)', label: 'vsize' },
  'total-fee': { ...COUNT, unit: 'fees (sat)', label: 'fees' },
  'feerate-diagram-missing-percentage': {
    ...SHARE,
    label: 'missing',
    color: CHUNK_DIFF_COLOR.missing,
  },
  'feerate-diagram-extra-percentage': {
    ...SHARE,
    label: 'extra',
    color: CHUNK_DIFF_COLOR.extra,
  },
};
