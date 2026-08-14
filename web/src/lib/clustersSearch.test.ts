import { describe, expect, it } from 'vitest';
import type { ClusterMetric } from './clusterMetrics';
import {
  BINS_RANGE,
  CLUSTERS_VIZ_DEFAULTS,
  type ClusterColumnKey,
  type ClustersViz,
  decodeClustersSearch,
  encodeClustersSearch,
  PAGE_RANGE,
  patchClustersSearch,
  QUERY_MAX_LEN,
  SHOW_COUNT_RANGE,
  type VizType,
  validateClustersSearch,
} from './clustersSearch';

const VIZ_TYPES: VizType[] = ['circles', 'treemap', 'histogram', 'table'];
const METRICS: ClusterMetric[] = ['vsize', 'fee', 'feerate', 'txs'];
const COLUMNS: ClusterColumnKey[] = ['id', 'txs', 'vsize', 'fee', 'feerate'];

/** A non-default config, so nothing under test can pass by falling back. */
const custom: ClustersViz = {
  vizType: 'treemap',
  sizeMetric: 'fee',
  colorMetric: 'txs',
  showCount: 60,
  bins: 12,
  sortKey: 'txs',
  sortDir: 'asc',
  page: 3,
  query: 'abc',
};

describe('clustersSearch round trip', () => {
  it('survives every viz type', () => {
    for (const vizType of VIZ_TYPES) {
      const viz = { ...custom, vizType };
      expect(decodeClustersSearch(encodeClustersSearch(viz))).toEqual(viz);
    }
  });

  it('survives every metric, in both the size and the colour slot', () => {
    for (const metric of METRICS) {
      const sized = { ...custom, sizeMetric: metric };
      expect(decodeClustersSearch(encodeClustersSearch(sized))).toEqual(sized);

      const colored = { ...custom, colorMetric: metric };
      expect(decodeClustersSearch(encodeClustersSearch(colored))).toEqual(
        colored,
      );
    }
  });

  it('keeps the two enum code sets internally collision-free', () => {
    // A duplicated code would make one value decode as another, silently
    // repointing shared links -- so assert it at the encoding boundary.
    const vizCodes = VIZ_TYPES.map(
      (v) => encodeClustersSearch({ ...custom, vizType: v }).v,
    );
    expect(new Set(vizCodes).size).toBe(VIZ_TYPES.length);

    const metricCodes = METRICS.map(
      (m) => encodeClustersSearch({ ...custom, sizeMetric: m }).s,
    );
    expect(new Set(metricCodes).size).toBe(METRICS.length);

    const sortCodes = COLUMNS.map(
      (c) => encodeClustersSearch({ ...custom, sortKey: c }).k,
    );
    expect(new Set(sortCodes).size).toBe(COLUMNS.length);
  });

  it('survives every sort column and direction', () => {
    for (const sortKey of COLUMNS) {
      const viz = { ...custom, sortKey };
      expect(decodeClustersSearch(encodeClustersSearch(viz))).toEqual(viz);
    }
    for (const sortDir of ['asc', 'desc'] as const) {
      const viz = { ...custom, sortDir };
      expect(decodeClustersSearch(encodeClustersSearch(viz))).toEqual(viz);
    }
  });

  it('encodes to single-letter keys and codes', () => {
    expect(encodeClustersSearch(custom)).toEqual({
      v: 't',
      s: 'f',
      c: 't',
      n: 60,
      b: 12,
      k: 't',
      d: 'a',
      p: 3,
      q: 'abc',
    });
  });
});

describe('clustersSearch defaults', () => {
  it('spells out the params still at their default', () => {
    // A link must not follow a later change to CLUSTERS_VIZ_DEFAULTS.
    const viz = { ...CLUSTERS_VIZ_DEFAULTS, vizType: 'histogram' as const };
    expect(encodeClustersSearch(viz)).toEqual({
      v: 'h',
      s: 'r',
      c: 'r',
      n: 40,
      b: 30,
      k: 'r',
      d: 'd',
      p: 1,
      q: '',
    });
  });

  it('round-trips an all-default config', () => {
    const pinned = encodeClustersSearch(CLUSTERS_VIZ_DEFAULTS);
    expect(decodeClustersSearch(pinned)).toEqual(CLUSTERS_VIZ_DEFAULTS);
  });

  it('decodes an empty search to the defaults', () => {
    expect(decodeClustersSearch({})).toEqual(CLUSTERS_VIZ_DEFAULTS);
  });
});

describe('clustersSearch healing', () => {
  it('falls back on unknown enum codes', () => {
    const viz = decodeClustersSearch({
      v: 'zzz',
      s: '',
      c: 42,
      k: 'zzz',
      d: 'x',
    });
    expect(viz.vizType).toBe(CLUSTERS_VIZ_DEFAULTS.vizType);
    expect(viz.sizeMetric).toBe(CLUSTERS_VIZ_DEFAULTS.sizeMetric);
    expect(viz.colorMetric).toBe(CLUSTERS_VIZ_DEFAULTS.colorMetric);
    expect(viz.sortKey).toBe(CLUSTERS_VIZ_DEFAULTS.sortKey);
    expect(viz.sortDir).toBe(CLUSTERS_VIZ_DEFAULTS.sortDir);
  });

  it('clamps numbers into slider range', () => {
    expect(decodeClustersSearch({ n: 9999, b: 9999 })).toMatchObject({
      showCount: SHOW_COUNT_RANGE.max,
      bins: BINS_RANGE.max,
    });
    expect(decodeClustersSearch({ n: -5, b: 0 })).toMatchObject({
      showCount: SHOW_COUNT_RANGE.min,
      bins: BINS_RANGE.min,
    });
  });

  it('rounds fractional numbers and reads numeric strings', () => {
    expect(decodeClustersSearch({ n: 60.6, b: '12' })).toMatchObject({
      showCount: 61,
      bins: 12,
    });
  });

  it('falls back on unparseable or empty numbers', () => {
    expect(decodeClustersSearch({ n: 'abc', b: '' })).toMatchObject({
      showCount: CLUSTERS_VIZ_DEFAULTS.showCount,
      bins: CLUSTERS_VIZ_DEFAULTS.bins,
    });
  });

  it('clamps page to a positive integer', () => {
    expect(decodeClustersSearch({ p: 0 })).toMatchObject({
      page: PAGE_RANGE.min,
    });
    expect(decodeClustersSearch({ p: -5 })).toMatchObject({
      page: PAGE_RANGE.min,
    });
    expect(decodeClustersSearch({ p: Number.NaN })).toMatchObject({
      page: CLUSTERS_VIZ_DEFAULTS.page,
    });
    expect(decodeClustersSearch({ p: 'abc' })).toMatchObject({
      page: CLUSTERS_VIZ_DEFAULTS.page,
    });
    expect(decodeClustersSearch({ p: 4.6 })).toMatchObject({ page: 5 });
  });

  it('heals a non-string query to empty and truncates an over-long one', () => {
    expect(decodeClustersSearch({ q: null })).toMatchObject({ query: '' });
    expect(decodeClustersSearch({ q: 42 })).toMatchObject({ query: '' });

    const blob = 'x'.repeat(QUERY_MAX_LEN + 50);
    expect(decodeClustersSearch({ q: blob })).toMatchObject({
      query: blob.slice(0, QUERY_MAX_LEN),
    });
  });
});

describe('validateClustersSearch', () => {
  it('keeps params that spell out a default', () => {
    expect(validateClustersSearch({ v: 'c', s: 'r', n: 40 })).toEqual({
      v: 'c',
      s: 'r',
      n: 40,
    });
  });

  it('adds nothing to a search that names no params', () => {
    expect(validateClustersSearch({})).toEqual({});
  });

  it('drops unknown keys and heals junk values in place', () => {
    expect(validateClustersSearch({ v: 'zzz', bogus: 1 })).toEqual({ v: 'c' });
  });

  it('keeps non-default params, clamped', () => {
    expect(validateClustersSearch({ v: 'h', b: 9999 })).toEqual({
      v: 'h',
      b: BINS_RANGE.max,
    });
  });

  it('is idempotent', () => {
    const once = validateClustersSearch({ v: 't', n: 60.6 });
    expect(validateClustersSearch(once)).toEqual(once);
  });
});

describe('patchClustersSearch', () => {
  it('changes one param and leaves the rest of the search alone', () => {
    const prev = encodeClustersSearch(custom);
    expect(patchClustersSearch(prev, { bins: 20 })).toEqual({
      ...prev,
      b: 20,
    });
  });

  it('applies a linked size+colour change in one patch', () => {
    const prev = encodeClustersSearch(custom);
    const next = patchClustersSearch(prev, {
      sizeMetric: 'vsize',
      colorMetric: 'vsize',
    });
    expect(decodeClustersSearch(next)).toMatchObject({
      sizeMetric: 'vsize',
      colorMetric: 'vsize',
    });
  });

  it('spells out a param patched back to its default', () => {
    const prev = encodeClustersSearch(custom);
    const next = patchClustersSearch(prev, {
      vizType: CLUSTERS_VIZ_DEFAULTS.vizType,
    });
    expect(next.v).toBe('c');
    expect(next.s).toBe('f');
  });

  it('pins only what the user touched, healing what was already there', () => {
    expect(patchClustersSearch({ v: 'zzz' }, { bins: 20 })).toEqual({
      v: 'c',
      b: 20,
    });
  });

  it('leaves untouched params following the default', () => {
    const next = patchClustersSearch({}, { vizType: 'treemap' });
    expect(next).toEqual({ v: 't' });

    // A second change adds its own param without disturbing the first.
    expect(patchClustersSearch(next, { showCount: 60 })).toEqual({
      v: 't',
      n: 60,
    });
  });

  it('resets page to 1 when sorting, direction or query changes', () => {
    const prev = encodeClustersSearch(custom); // page: 3

    expect(patchClustersSearch(prev, { sortKey: 'id' })).toMatchObject({
      p: 1,
    });
    expect(patchClustersSearch(prev, { sortDir: 'desc' })).toMatchObject({
      p: 1,
    });
    expect(patchClustersSearch(prev, { query: 'zzz' })).toMatchObject({
      p: 1,
    });
  });

  it('does not reset page when the patch sets it explicitly', () => {
    const prev = encodeClustersSearch(custom); // page: 3
    expect(patchClustersSearch(prev, { sortKey: 'id', page: 7 })).toMatchObject(
      { p: 7 },
    );
  });

  it('leaves the page alone for a change unrelated to sort or query', () => {
    const prev = encodeClustersSearch(custom); // page: 3
    expect(patchClustersSearch(prev, { bins: 20 })).toMatchObject({ p: 3 });
  });
});
