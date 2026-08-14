import type { ClusterMetric } from './clusterMetrics';

export type VizType = 'circles' | 'treemap' | 'histogram' | 'table';
/** The viz types ClusterCanvas can draw; the table is not one of them. */
export type CanvasVizType = Exclude<VizType, 'table'>;

/** The clusters table's sortable columns. */
export type ClusterColumnKey = 'id' | 'txs' | 'vsize' | 'fee' | 'feerate';

/** The clusters page's visualization config, as the page reads it. */
export interface ClustersViz {
  vizType: VizType;
  sizeMetric: ClusterMetric;
  colorMetric: ClusterMetric;
  showCount: number;
  bins: number;
  sortKey: ClusterColumnKey;
  sortDir: 'asc' | 'desc';
  /** 1-based */
  page: number;
  query: string;
}

export const CLUSTERS_VIZ_DEFAULTS: ClustersViz = {
  vizType: 'circles',
  sizeMetric: 'feerate',
  colorMetric: 'feerate',
  showCount: 40,
  bins: 30,
  sortKey: 'feerate',
  sortDir: 'desc',
  page: 1,
  query: '',
};

/** Slider bounds, shared by the controls and by the URL validator. */
export const SHOW_COUNT_RANGE = { min: 10, max: 250 } as const;
export const BINS_RANGE = { min: 5, max: 50 } as const;
/** URL-level sanity bound only; the real ceiling is the actual row count. */
export const PAGE_RANGE = { min: 1, max: 1_000_000 } as const;
export const QUERY_MAX_LEN = 100;

/**
 * The same config as it travels in the URL: single-letter keys holding
 * single-letter enum codes, so a shared link stays short. Nothing outside this
 * module handles this shape -- the page works in `ClustersViz` names only.
 *
 * A key left out means "whatever the default is *now*", which is why every param
 * the user has touched is written out even when it lands on today's default: a
 * later change to `CLUSTERS_VIZ_DEFAULTS` must not repoint links shared before
 * it. The codes below are the same kind of compatibility surface -- changing one
 * silently repoints every link already shared.
 */
export type ClustersSearch = {
  /** vizType */ v?: string;
  /** sizeMetric */ s?: string;
  /** colorMetric */ c?: string;
  /** showCount */ n?: number;
  /** bins */ b?: number;
  /** sortKey */ k?: string;
  /** sortDir */ d?: string;
  /** page */ p?: number;
  /** query */ q?: string;
};

const VIZ_CODE: Record<VizType, string> = {
  circles: 'c',
  treemap: 't',
  histogram: 'h',
  table: 'b',
};

const METRIC_CODE: Record<ClusterMetric, string> = {
  feerate: 'r',
  txs: 't',
  vsize: 'v',
  fee: 'f',
};

// Exhaustive over ClusterColumnKey: a new column without a code is a compile error.
const SORT_CODE: Record<ClusterColumnKey, string> = {
  id: 'i',
  txs: 't',
  vsize: 'v',
  fee: 'f',
  feerate: 'r',
};

const DIR_CODE: Record<'asc' | 'desc', string> = {
  asc: 'a',
  desc: 'd',
};

function byCode<T extends string>(codes: Record<T, string>): Record<string, T> {
  return Object.fromEntries(
    Object.entries<string>(codes).map(([name, code]) => [code, name]),
  ) as Record<string, T>;
}

const VIZ_BY_CODE = byCode(VIZ_CODE);
const METRIC_BY_CODE = byCode(METRIC_CODE);
const SORT_BY_CODE = byCode(SORT_CODE);
const DIR_BY_CODE = byCode(DIR_CODE);

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

/** Reads a string param, healing non-strings to '' and capping its length. */
function readQuery(raw: unknown, maxLen: number): string {
  if (typeof raw !== 'string') return '';
  return raw.slice(0, maxLen);
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
    sortKey: readCode(wire.k, SORT_BY_CODE, d.sortKey),
    sortDir: readCode(wire.d, DIR_BY_CODE, d.sortDir),
    page: readNumber(wire.p, PAGE_RANGE, d.page),
    query: readQuery(wire.q, QUERY_MAX_LEN),
  };
}

/** Viz config -> URL search, every param pinned, defaults included. */
export function encodeClustersSearch(viz: ClustersViz): ClustersSearch {
  return {
    v: VIZ_CODE[viz.vizType],
    s: METRIC_CODE[viz.sizeMetric],
    c: METRIC_CODE[viz.colorMetric],
    n: viz.showCount,
    b: viz.bins,
    k: SORT_CODE[viz.sortKey],
    d: DIR_CODE[viz.sortDir],
    p: viz.page,
    q: viz.query,
  };
}

const WIRE_KEYS = ['v', 's', 'c', 'n', 'b', 'k', 'd', 'p', 'q'] as const;

/** Which URL key each viz field travels under. */
const WIRE_KEY: Record<keyof ClustersViz, keyof ClustersSearch> = {
  vizType: 'v',
  sizeMetric: 's',
  colorMetric: 'c',
  showCount: 'n',
  bins: 'b',
  sortKey: 'k',
  sortDir: 'd',
  page: 'p',
  query: 'q',
};

/** Encodes `viz`, then keeps only the params the URL is meant to carry. */
function encodePinned(
  viz: ClustersViz,
  pinned: Set<keyof ClustersSearch>,
): ClustersSearch {
  const wire = encodeClustersSearch(viz);
  for (const key of WIRE_KEYS) if (!pinned.has(key)) delete wire[key];
  return wire;
}

/**
 * Heals the params the URL carries, and only those: a visitor who has chosen
 * nothing keeps a bare `/clusters`, and a pinned param survives even when it
 * spells out today's default.
 */
export function validateClustersSearch(
  raw: Record<string, unknown>,
): ClustersSearch {
  const present = WIRE_KEYS.filter((key) => key in raw);
  return encodePinned(decodeClustersSearch(raw), new Set(present));
}

/**
 * Applies `patch`, pinning what the user has touched -- the params already in
 * the URL plus the ones just changed, each spelled out even when it lands on
 * today's default, so a later change to `CLUSTERS_VIZ_DEFAULTS` cannot move a
 * shared link. Untouched params stay absent and keep following the default.
 */
export function patchClustersSearch(
  prev: ClustersSearch,
  patch: Partial<ClustersViz>,
): ClustersSearch {
  // Re-sorting or filtering can strand the user on a page that no longer
  // exists (or means something else), so it implicitly returns to page 1
  // unless the caller is itself setting the page.
  const resetsPage =
    ('sortKey' in patch || 'sortDir' in patch || 'query' in patch) &&
    !('page' in patch);
  const effectivePatch = resetsPage ? { ...patch, page: 1 } : patch;

  const touched = Object.keys(effectivePatch).map(
    (field) => WIRE_KEY[field as keyof ClustersViz],
  );
  const pinned = new Set([
    ...WIRE_KEYS.filter((key) => key in prev),
    ...touched,
  ]);
  return encodePinned(
    { ...decodeClustersSearch(prev), ...effectivePatch },
    pinned,
  );
}
