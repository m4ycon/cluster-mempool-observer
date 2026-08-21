import { describe, expect, it } from 'vitest';
import type { TransactionRef } from '../types/events';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';
import {
  txColor,
  txRadius,
  txScaleFor,
  txValue,
  UNIFORM_COLOR,
  UNIFORM_R,
  UNKNOWN_COLOR,
} from './txMetrics';

function tx(overrides: Partial<TransactionRef> = {}): TransactionRef {
  return {
    txid: 'a'.repeat(64),
    fee: 500,
    vsize: 200,
    first_seen_at: '2026-01-01T00:00:00Z',
    cluster_id: 1,
    hollow: false,
    input_txids: ['b'.repeat(64)],
    ...overrides,
  };
}

const METRICS: ClusterMetric[] = ['vsize', 'fee', 'feerate', 'txs'];

describe('txValue', () => {
  it('reads every metric off a fully-populated row', () => {
    const row = tx({ fee: 500, vsize: 200 });
    expect(txValue(row, 'vsize')).toBe(200);
    expect(txValue(row, 'fee')).toBe(500);
    expect(txValue(row, 'feerate')).toBe(2.5);
  });

  it('reads fee as null on a fee-null row, other metrics unaffected', () => {
    const row = tx({ fee: null, vsize: 200 });
    expect(txValue(row, 'fee')).toBeNull();
    expect(txValue(row, 'feerate')).toBeNull();
    expect(txValue(row, 'vsize')).toBe(200);
  });

  it('reads vsize as null on a hollow row (vsize 0 means unknown)', () => {
    const row = tx({ vsize: 0, fee: 500 });
    expect(txValue(row, 'vsize')).toBeNull();
    expect(txValue(row, 'feerate')).toBeNull();
  });

  it('reads a coinbase row (empty input_txids) like any known row', () => {
    const row = tx({ input_txids: [], fee: 0, vsize: 100 });
    expect(txValue(row, 'vsize')).toBe(100);
    expect(txValue(row, 'fee')).toBe(0);
    expect(txValue(row, 'feerate')).toBe(0);
  });

  it('returns null for txs regardless of the row', () => {
    for (const row of [
      tx(),
      tx({ fee: null }),
      tx({ vsize: 0 }),
      tx({ input_txids: [] }),
    ]) {
      expect(txValue(row, 'txs')).toBeNull();
    }
  });
});

describe('txRadius', () => {
  it('returns UNIFORM_R for a null value', () => {
    expect(txRadius(null, { min: 0, max: 100 })).toBe(UNIFORM_R);
  });

  it('does not divide by zero on a degenerate min === max domain', () => {
    const r = txRadius(50, { min: 50, max: 50 });
    expect(Number.isFinite(r)).toBe(true);
  });

  it('is monotonic in the value across the domain', () => {
    const domain = { min: 0, max: 1000 };
    const rLow = txRadius(10, domain);
    const rMid = txRadius(500, domain);
    const rHigh = txRadius(1000, domain);
    expect(rMid).toBeGreaterThan(rLow);
    expect(rHigh).toBeGreaterThan(rMid);
  });

  it('is area-proportional: a 4x value gives a 2x radius delta', () => {
    // Area ~ r^2, so area proportional to v means r ~ sqrt(v): moving from
    // v/4 to v should cover half the min..max radius span, not all of it.
    const domain = { min: 0, max: 400 };
    const rQuarter = txRadius(100, domain);
    const rFull = txRadius(400, domain);
    const rMin = txRadius(0, domain);
    const rMax = txRadius(400, domain);
    const half = rMin + (rMax - rMin) / 2;
    expect(rQuarter).toBeCloseTo(half, 5);
    expect(rFull).toBe(rMax);
  });
});

describe('txColor', () => {
  const values = [10, 20, 30, 40, 50];
  const scale = txScaleFor('vsize', values);

  it('returns the brand orange when null because the metric is txs', () => {
    expect(txColor(null, scale, 'txs')).toBe(UNIFORM_COLOR);
  });

  it('returns the idle colour when null for any other metric', () => {
    for (const metric of METRICS.filter((m) => m !== 'txs')) {
      expect(txColor(null, scale, metric)).toBe(UNKNOWN_COLOR);
    }
  });

  it('returns a scale colour for a real value', () => {
    expect(txColor(10, scale, 'vsize')).toBe(
      ClusterMetrics.colorAt(scale, 10, 'vsize'),
    );
  });
});

describe('txScaleFor', () => {
  it('builds the scale over non-null values only', () => {
    const scale = txScaleFor('vsize', [10, null, 20, null, 30]);
    expect(scale).toEqual(ClusterMetrics.scaleFor('vsize', [10, 20, 30]));
  });
});
