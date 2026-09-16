import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { CounterPoint } from '../../types/generated/CounterPoint';
import type { CounterSeries } from '../../types/generated/CounterSeries';
import { CounterChart } from './CounterChart';

const NOW = Date.UTC(2026, 6, 1, 12, 0, 0);

function point(
  minutesAgo: number,
  added_txs: number | null,
  confirmed_txs: number | null,
  evicted_txs: number | null,
): CounterPoint {
  return {
    sampled_at: new Date(NOW - minutesAgo * 60_000).toISOString(),
    added_txs,
    confirmed_txs,
    evicted_txs,
  };
}

function series(points: CounterPoint[], resolutionSecs = 60): CounterSeries {
  return { resolution_secs: resolutionSecs, points };
}

/** A dominant `added_txs` series alongside much smaller `confirmed_txs`/`evicted_txs` ones. */
const POINTS = series([
  point(3, 1000, 5, 2),
  point(2, 1100, 6, 1),
  point(1, 900, 4, 3),
  point(0, 1200, 7, 0),
]);

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Internal Server Error',
    json: () => Promise.resolve(body),
  } as Response;
}

/** Two endpoints fire per render; branch the stub on the URL. */
function stubFetch(countersBody: unknown, eventsBody: unknown = []) {
  return vi.fn((url: string) =>
    Promise.resolve(
      jsonResponse(url.includes('/system-events') ? eventsBody : countersBody),
    ),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('CounterChart', () => {
  it('shows a loading affordance while both requests are pending', () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );

    render(<CounterChart />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('shows an error affordance when counters fail, even if events load fine', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) =>
        Promise.resolve(
          url.includes('/system-events')
            ? jsonResponse([])
            : jsonResponse(null, false, 500),
        ),
      ),
    );

    render(<CounterChart />);

    expect(await screen.findByText(/failed to load/i)).toBeInTheDocument();
    expect(screen.queryByText(/no samples/i)).not.toBeInTheDocument();
    expect(screen.queryByTestId('counter-chart')).not.toBeInTheDocument();
  });

  it('renders the chart with no markers when only the system-events request fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) =>
        Promise.resolve(
          url.includes('/system-events')
            ? jsonResponse(null, false, 500)
            : jsonResponse(series([point(0, 1, 1, 1)])),
        ),
      ),
    );

    render(<CounterChart />);

    expect(await screen.findByTestId('counter-chart')).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
    expect(screen.queryAllByTestId('event-marker')).toHaveLength(0);
  });

  it('renders the empty-but-successful case distinctly from loading/error', async () => {
    vi.stubGlobal('fetch', stubFetch(series([])));

    render(<CounterChart />);

    expect(await screen.findByText(/no samples in range/i)).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
  });

  it('renders every point of a normal series', async () => {
    vi.stubGlobal('fetch', stubFetch(POINTS));

    render(<CounterChart />);

    const chart = await screen.findByTestId('counter-chart');
    expect(chart).toHaveAttribute('data-point-count', '4');
  });
});
