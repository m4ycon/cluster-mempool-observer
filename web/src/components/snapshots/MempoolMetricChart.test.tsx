import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MempoolMetricSeries } from '../../types/generated/MempoolMetricSeries';
import { MempoolMetricChart } from './MempoolMetricChart';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Internal Server Error',
    json: () => Promise.resolve(body),
  } as Response;
}

function series(
  points: MempoolMetricSeries['points'],
  resolutionSecs = 60,
): MempoolMetricSeries {
  return { metric: 'cluster-count', resolution_secs: resolutionSecs, points };
}

function point(minutesAgo: number, value: number) {
  return {
    sampled_at: new Date(
      Date.UTC(2026, 6, 1, 12, 0, 0) - minutesAgo * 60_000,
    ).toISOString(),
    value,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('MempoolMetricChart', () => {
  it('shows a loading affordance before the fetch resolves', () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );

    render(<MempoolMetricChart metric="cluster-count" />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('shows an error affordance on a failed request, not an empty chart', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    render(<MempoolMetricChart metric="cluster-count" />);

    expect(await screen.findByText(/failed to load/i)).toBeInTheDocument();
    expect(screen.queryByText(/no snapshots/i)).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('mempool-metric-chart'),
    ).not.toBeInTheDocument();
  });

  it('renders the empty-but-successful case distinctly from loading/error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(series([]))));

    render(<MempoolMetricChart metric="cluster-count" />);

    expect(
      await screen.findByText(/no snapshots in range/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
  });

  it('renders every point of a normal series', async () => {
    const s = series([point(3, 10), point(2, 12), point(1, 9), point(0, 15)]);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(s)));

    render(<MempoolMetricChart metric="cluster-count" />);

    const chart = await screen.findByTestId('mempool-metric-chart');
    expect(chart).toHaveAttribute('data-point-count', '4');
  });

  it('labels the chart with the real server-chosen resolution', async () => {
    const s = series([point(1, 10), point(0, 12)], 3600);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(s)));

    render(<MempoolMetricChart metric="cluster-count" />);

    expect(await screen.findByText('1-hour samples')).toBeInTheDocument();
  });
});
