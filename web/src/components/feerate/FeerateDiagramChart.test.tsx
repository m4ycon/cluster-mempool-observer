import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FeerateDiagramComparison } from '../../types/generated/FeerateDiagramComparison';
import { FeerateDiagramChart } from './FeerateDiagramChart';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Internal Server Error',
    json: () => Promise.resolve(body),
  } as Response;
}

const NODE = [
  { weight: 0, fee_sats: 0 },
  { weight: 4_000_000, fee_sats: 100_000 },
  { weight: 8_000_000, fee_sats: 250_000 },
];

const COMPARISON: FeerateDiagramComparison = {
  sampled_at: '2026-08-12T00:00:00Z',
  node: NODE,
  ours: NODE.slice(0, 2),
  missing: 1,
  extra: 0,
  node_total: 2,
  unit: 'chunk_count',
  missing_chunks: [2],
  extra_chunks: [],
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('FeerateDiagramChart', () => {
  it('shows a loading affordance before the fetch resolves', () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );

    render(<FeerateDiagramChart blockWindow="all" />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('shows an error affordance on a failed request, not an empty chart', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    render(<FeerateDiagramChart blockWindow="all" />);

    expect(await screen.findByText(/failed to load/i)).toBeInTheDocument();
    expect(
      screen.queryByTestId('feerate-diagram-chart'),
    ).not.toBeInTheDocument();
  });

  it('says no comparison yet before the first one, distinct from loading/error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(null)));

    render(<FeerateDiagramChart blockWindow="all" />);

    expect(await screen.findByText(/no comparison yet/i)).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
  });

  it('renders the readings and both curves once the fetch resolves', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(COMPARISON)));

    render(<FeerateDiagramChart blockWindow="all" />);

    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '3');
    expect(screen.getByTestId('ours-curve')).toBeInTheDocument();
    expect(
      screen.getByText('node chunks', { exact: false }),
    ).toBeInTheDocument();
  });

  it('passes the window prop through to clip the drawn curve', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(COMPARISON)));

    render(<FeerateDiagramChart blockWindow={1} />);

    // window=1 keeps only points up to 4,000,000 WU: the origin and the 4M point.
    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '2');
  });
});
