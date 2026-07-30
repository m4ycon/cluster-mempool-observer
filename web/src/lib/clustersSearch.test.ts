import { describe, expect, it } from 'vitest';
import type { VizType } from '../components/clusters/ClusterCanvas';
import type { ClusterMetric } from './clusterMetrics';
import {
  BINS_RANGE,
  CLUSTERS_VIZ_DEFAULTS,
  type ClustersViz,
  decodeClustersSearch,
  encodeClustersSearch,
  patchClustersSearch,
  SHOW_COUNT_RANGE,
  validateClustersSearch,
} from './clustersSearch';

const VIZ_TYPES: VizType[] = ['circles', 'treemap', 'histogram'];
const METRICS: ClusterMetric[] = ['vsize', 'fee', 'feerate', 'txs'];

/** A non-default config, so nothing under test can pass by falling back. */
const custom: ClustersViz = {
  vizType: 'treemap',
  sizeMetric: 'fee',
  colorMetric: 'txs',
  showCount: 60,
  bins: 12,
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
  });

  it('encodes to single-letter keys and codes', () => {
    expect(encodeClustersSearch(custom)).toEqual({
      v: 't',
      s: 'f',
      c: 't',
      n: 60,
      b: 12,
    });
  });
});

describe('clustersSearch defaults', () => {
  it('encodes an all-default config as an empty search', () => {
    expect(encodeClustersSearch(CLUSTERS_VIZ_DEFAULTS)).toEqual({});
  });

  it('omits only the params still at their default', () => {
    const viz = { ...CLUSTERS_VIZ_DEFAULTS, vizType: 'histogram' as const };
    expect(encodeClustersSearch(viz)).toEqual({ v: 'h' });
  });

  it('decodes an empty search to the defaults', () => {
    expect(decodeClustersSearch({})).toEqual(CLUSTERS_VIZ_DEFAULTS);
  });
});

describe('clustersSearch healing', () => {
  it('falls back on unknown enum codes', () => {
    const viz = decodeClustersSearch({ v: 'zzz', s: '', c: 42 });
    expect(viz.vizType).toBe(CLUSTERS_VIZ_DEFAULTS.vizType);
    expect(viz.sizeMetric).toBe(CLUSTERS_VIZ_DEFAULTS.sizeMetric);
    expect(viz.colorMetric).toBe(CLUSTERS_VIZ_DEFAULTS.colorMetric);
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
});

describe('validateClustersSearch', () => {
  it('strips params that spell out a default', () => {
    expect(validateClustersSearch({ v: 'c', s: 'r', n: 40 })).toEqual({});
  });

  it('drops junk instead of carrying it into the URL', () => {
    expect(validateClustersSearch({ v: 'zzz', bogus: 1 })).toEqual({});
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

  it('drops a param patched back to its default', () => {
    const prev = encodeClustersSearch(custom);
    const next = patchClustersSearch(prev, {
      vizType: CLUSTERS_VIZ_DEFAULTS.vizType,
    });
    expect(next.v).toBeUndefined();
    expect(next.s).toBe('f');
  });

  it('heals a junk search on the way through', () => {
    expect(patchClustersSearch({ v: 'zzz' }, { bins: 20 })).toEqual({ b: 20 });
  });
});
