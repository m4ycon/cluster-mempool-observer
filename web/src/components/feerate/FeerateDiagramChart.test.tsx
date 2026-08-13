import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MempoolFeerateDiagram } from '../../types/generated/MempoolFeerateDiagram';
import { FeerateDiagramChart } from './FeerateDiagramChart';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Internal Server Error',
    json: () => Promise.resolve(body),
  } as Response;
}

function diagram(
  points: MempoolFeerateDiagram['points'],
): MempoolFeerateDiagram {
  return { sampled_at: '2026-08-12T00:00:00Z', points };
}

const DIAGRAM = diagram([
  { weight: 0, fee_sats: 0 },
  { weight: 4_000_000, fee_sats: 100_000 },
  { weight: 8_000_000, fee_sats: 250_000 },
]);

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
    expect(screen.queryByText(/no sample yet/i)).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('feerate-diagram-chart'),
    ).not.toBeInTheDocument();
  });

  it('renders the empty-but-successful case distinctly from loading/error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(diagram([]))),
    );

    render(<FeerateDiagramChart blockWindow="all" />);

    expect(await screen.findByText(/no sample yet/i)).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
  });

  it('renders the chart once the fetch resolves', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(DIAGRAM)));

    render(<FeerateDiagramChart blockWindow="all" />);

    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '3');
  });

  it('passes the window prop through to clip the drawn curve', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(DIAGRAM)));

    render(<FeerateDiagramChart blockWindow={1} />);

    // window=1 keeps only points up to 4,000,000 WU: the origin and the 4M point.
    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '2');
  });
});
