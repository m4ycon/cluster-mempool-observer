import type { VizType } from '../components/clusters/ClusterCanvas';
import type { ClusterMetric } from './clusterMetrics';

/** The clusters page's visualization config, as the page reads it. */
export interface ClustersViz {
  vizType: VizType;
  sizeMetric: ClusterMetric;
  colorMetric: ClusterMetric;
  showCount: number;
  bins: number;
}

export const CLUSTERS_VIZ_DEFAULTS: ClustersViz = {
  vizType: 'circles',
  sizeMetric: 'feerate',
  colorMetric: 'feerate',
  showCount: 40,
  bins: 30,
};

/** Slider bounds, shared by the controls and by the URL validator. */
export const SHOW_COUNT_RANGE = { min: 10, max: 250 } as const;
export const BINS_RANGE = { min: 5, max: 50 } as const;

/**
 * The same config as it travels in the URL: single-letter keys holding
 * single-letter enum codes, so a shared link stays short. Nothing outside this
 * module handles this shape -- the page works in `ClustersViz` names only.
 *
 * A key left out means "default". The codes below are a compatibility surface:
 * changing one silently repoints every link already shared.
 */
export type ClustersSearch = {
  /** vizType */ v?: string;
  /** sizeMetric */ s?: string;
  /** colorMetric */ c?: string;
  /** showCount */ n?: number;
  /** bins */ b?: number;
};

const VIZ_CODE: Record<VizType, string> = {
  circles: 'c',
  treemap: 't',
  histogram: 'h',
};

const METRIC_CODE: Record<ClusterMetric, string> = {
  feerate: 'r',
  txs: 't',
  vsize: 'v',
  fee: 'f',
};

function byCode<T extends string>(codes: Record<T, string>): Record<string, T> {
  return Object.fromEntries(
    Object.entries<string>(codes).map(([name, code]) => [code, name]),
  ) as Record<string, T>;
}

const VIZ_BY_CODE = byCode(VIZ_CODE);
const METRIC_BY_CODE = byCode(METRIC_CODE);

/** Reads a coded enum param. Anything unrecognised falls back to the default. */
function readCode<T extends string>(
  raw: unknown,
  codes: Record<string, T>,
  fallback: T,
): T {
  return (typeof raw === 'string' && codes[raw]) || fallback;
}

/** Reads a numeric param, rounded and clamped into range. */
function readNumber(
  raw: unknown,
  range: { min: number; max: number },
  fallback: number,
): number {
  if (raw === '' || raw === null) return fallback;
  const n = typeof raw === 'number' ? raw : Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(range.max, Math.max(range.min, Math.round(n)));
}

/** URL search -> viz config, with every missing or invalid value healed. */
export function decodeClustersSearch(
  wire: Record<string, unknown>,
): ClustersViz {
  const d = CLUSTERS_VIZ_DEFAULTS;
  return {
    vizType: readCode(wire.v, VIZ_BY_CODE, d.vizType),
    sizeMetric: readCode(wire.s, METRIC_BY_CODE, d.sizeMetric),
    colorMetric: readCode(wire.c, METRIC_BY_CODE, d.colorMetric),
    showCount: readNumber(wire.n, SHOW_COUNT_RANGE, d.showCount),
    bins: readNumber(wire.b, BINS_RANGE, d.bins),
  };
}

/** Viz config -> URL search, dropping everything still at its default. */
export function encodeClustersSearch(viz: ClustersViz): ClustersSearch {
  const d = CLUSTERS_VIZ_DEFAULTS;
  const wire: ClustersSearch = {};
  if (viz.vizType !== d.vizType) wire.v = VIZ_CODE[viz.vizType];
  if (viz.sizeMetric !== d.sizeMetric) wire.s = METRIC_CODE[viz.sizeMetric];
  if (viz.colorMetric !== d.colorMetric) wire.c = METRIC_CODE[viz.colorMetric];
  if (viz.showCount !== d.showCount) wire.n = viz.showCount;
  if (viz.bins !== d.bins) wire.b = viz.bins;
  return wire;
}

export function validateClustersSearch(
  raw: Record<string, unknown>,
): ClustersSearch {
  return encodeClustersSearch(decodeClustersSearch(raw));
}

export function patchClustersSearch(
  prev: ClustersSearch,
  patch: Partial<ClustersViz>,
): ClustersSearch {
  return encodeClustersSearch({ ...decodeClustersSearch(prev), ...patch });
}
