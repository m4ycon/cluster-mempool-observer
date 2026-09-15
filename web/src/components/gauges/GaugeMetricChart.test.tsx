import { act, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GaugeSeries } from '../../types/generated/GaugeSeries';
import { GaugeMetricChart } from './GaugeMetricChart';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Internal Server Error',
    json: () => Promise.resolve(body),
  } as Response;
}

function series(
  points: GaugeSeries['points'],
  resolutionSecs = 60,
): GaugeSeries {
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

/** Two endpoints fire per render now; branch the stub on the URL. */
function stubFetch(gaugeBody: unknown, eventsBody: unknown = []) {
  return vi.fn((url: string) =>
    Promise.resolve(
      jsonResponse(url.includes('/system-events') ? eventsBody : gaugeBody),
    ),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('GaugeMetricChart', () => {
  it('shows a loading affordance while both requests are pending', () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );

    render(<GaugeMetricChart metric="cluster-count" />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('keeps the loading affordance when only system events are still pending', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) =>
        url.includes('/system-events')
          ? new Promise(() => {})
          : Promise.resolve(jsonResponse(series([point(0, 1)]))),
      ),
    );

    render(<GaugeMetricChart metric="cluster-count" />);

    // Flush the resolved gauge request; the still-pending events request
    // must keep the chart loading regardless.
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
    expect(screen.queryByTestId('gauge-metric-chart')).not.toBeInTheDocument();
  });

  it('shows an error affordance when samples fail, even if events load fine', async () => {
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

    render(<GaugeMetricChart metric="cluster-count" />);

    expect(await screen.findByText(/failed to load/i)).toBeInTheDocument();
    expect(screen.queryByText(/no samples/i)).not.toBeInTheDocument();
    expect(screen.queryByTestId('gauge-metric-chart')).not.toBeInTheDocument();
  });

  it('renders the chart with no markers when only the system-events request fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) =>
        Promise.resolve(
          url.includes('/system-events')
            ? jsonResponse(null, false, 500)
            : jsonResponse(series([point(0, 1)])),
        ),
      ),
    );

    render(<GaugeMetricChart metric="cluster-count" />);

    expect(await screen.findByTestId('gauge-metric-chart')).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
    expect(screen.queryAllByTestId('event-marker')).toHaveLength(0);
  });

  it('renders the empty-but-successful case distinctly from loading/error', async () => {
    vi.stubGlobal('fetch', stubFetch(series([])));

    render(<GaugeMetricChart metric="cluster-count" />);

    expect(await screen.findByText(/no samples in range/i)).toBeInTheDocument();
    expect(screen.queryByText(/failed to load/i)).not.toBeInTheDocument();
  });

  it('renders every point of a normal series', async () => {
    const s = series([point(3, 10), point(2, 12), point(1, 9), point(0, 15)]);
    vi.stubGlobal('fetch', stubFetch(s));

    render(<GaugeMetricChart metric="cluster-count" />);

    const chart = await screen.findByTestId('gauge-metric-chart');
    expect(chart).toHaveAttribute('data-point-count', '4');
  });

  it('labels the chart with the real server-chosen resolution', async () => {
    const s = series([point(1, 10), point(0, 12)], 3600);
    vi.stubGlobal('fetch', stubFetch(s));

    render(<GaugeMetricChart metric="cluster-count" />);

    expect(await screen.findByText('1-hour samples')).toBeInTheDocument();
  });

  it('does not refetch on a re-render within the same minute', async () => {
    const fetchMock = stubFetch(series([point(0, 1)]));
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date('2026-01-01T12:00:00.000Z'));

    const { rerender } = render(<GaugeMetricChart metric="cluster-count" />);
    await screen.findByTestId('gauge-metric-chart');

    vi.setSystemTime(new Date('2026-01-01T12:00:59.999Z'));
    rerender(<GaugeMetricChart metric="cluster-count" />);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it('refetches once the minute rolls over, without blanking the chart', async () => {
    const gaugeFetch = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(series([point(0, 1)])))
      .mockReturnValue(new Promise<Response>(() => {}));
    const eventsFetch = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse([]))
      .mockReturnValue(new Promise<Response>(() => {}));
    const fetchMock = vi.fn((url: string) =>
      url.includes('/system-events') ? eventsFetch() : gaugeFetch(),
    );
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date('2026-01-01T12:00:00.000Z'));

    const { rerender } = render(<GaugeMetricChart metric="cluster-count" />);
    await screen.findByTestId('gauge-metric-chart');

    vi.setSystemTime(new Date('2026-01-01T12:01:00.000Z'));
    rerender(<GaugeMetricChart metric="cluster-count" />);

    expect(fetchMock).toHaveBeenCalledTimes(4);
    expect(screen.getByTestId('gauge-metric-chart')).toBeInTheDocument();
    expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
    vi.useRealTimers();
  });
});
