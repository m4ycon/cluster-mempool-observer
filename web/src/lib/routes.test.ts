import { describe, expect, it } from 'vitest';
import { ApiRoutes, type ChartRange, ExplorerRoutes } from './routes';

const range: ChartRange = {
  from: Date.UTC(2026, 0, 1, 0, 0, 0),
  to: Date.UTC(2026, 0, 2, 3, 30, 0),
};

describe('ApiRoutes.mempoolSnapshots', () => {
  it('builds the metric path with an encoded from/to range', () => {
    const path = ApiRoutes.mempoolSnapshots('cluster-count', range);
    const url = new URL(path, 'http://test');

    expect(url.pathname).toBe('/mempool/snapshots/cluster-count');
    expect(url.searchParams.get('from')).toBe('2026-01-01T00:00:00.000Z');
    expect(url.searchParams.get('to')).toBe('2026-01-02T03:30:00.000Z');
    expect(path).toContain('%3A'); // colons must be percent-encoded, not raw
  });
});

describe('ApiRoutes.systemEvents', () => {
  it('builds the system-events path with an encoded from/to range', () => {
    const path = ApiRoutes.systemEvents(range);
    const url = new URL(path, 'http://test');

    expect(url.pathname).toBe('/system-events');
    expect(url.searchParams.get('from')).toBe('2026-01-01T00:00:00.000Z');
    expect(url.searchParams.get('to')).toBe('2026-01-02T03:30:00.000Z');
    expect(path).toContain('%3A');
  });
});

describe('ApiRoutes.transactions', () => {
  it('builds the transactions path with commas percent-encoded', () => {
    const path = ApiRoutes.transactions(['aaaa', 'bbbb', 'cccc']);
    const url = new URL(path, 'http://test');

    expect(url.pathname).toBe('/transactions');
    expect(url.searchParams.get('txids')).toBe('aaaa,bbbb,cccc');
    expect(path).toBe('/transactions?txids=aaaa%2Cbbbb%2Ccccc');
  });
});

describe('ExplorerRoutes.tx', () => {
  it('builds a mempool.space transaction URL', () => {
    expect(ExplorerRoutes.tx('deadbeef')).toBe(
      'https://mempool.space/pt/tx/deadbeef',
    );
  });
});
