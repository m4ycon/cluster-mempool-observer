import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import dayjs from '../../lib/dayjs';
import type { ChartRange } from '../../lib/routes';
import type { GaugeSeries } from '../../types/generated/GaugeSeries';
import type { SystemEvent } from '../../types/generated/SystemEvent';
import type { SystemEventKind } from '../../types/generated/SystemEventKind';
import { MetricLineChart } from './MetricLineChart';

const VIEW_W = 960;
const NOW = Date.UTC(2026, 6, 1, 12, 0, 0);

function point(minutesAgo: number, value: number) {
  return {
    sampled_at: new Date(NOW - minutesAgo * 60_000).toISOString(),
    value,
  };
}

function event(
  id: number,
  kind: SystemEventKind,
  minutesAgo: number,
): SystemEvent {
  return {
    id,
    kind,
    details: {},
    created_at: new Date(NOW - minutesAgo * 60_000).toISOString(),
  };
}

function series(
  points: GaugeSeries['points'],
  resolutionSecs = 60,
): GaugeSeries {
  return { metric: 'cluster-count', resolution_secs: resolutionSecs, points };
}

/** Evenly spaced, so a hover at x maps to a predictable index. */
const SPREAD = series([
  point(3, 10),
  point(2, 1234),
  point(1, 9),
  point(0, 15),
]);

/** Matches SPREAD's own extent, so pixel math in the pre-existing tests is unchanged. */
const RANGE: ChartRange = { from: NOW - 3 * 60_000, to: NOW };

/** jsdom reports a zero-width box, which the hover handler bails out on. */
function hoverAt(container: HTMLElement, clientX: number) {
  const svg = container.querySelector('svg') as SVGSVGElement;
  vi.spyOn(svg, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    width: VIEW_W,
  } as DOMRect);

  const overlay = container.querySelector(
    'rect[fill="transparent"]',
  ) as SVGRectElement;
  fireEvent.mouseMove(overlay, { clientX });
  return overlay;
}

describe('MetricLineChart', () => {
  it('renders the empty case without claiming a failure', () => {
    render(<MetricLineChart series={series([])} range={RANGE} events={[]} />);

    expect(screen.getByText(/no samples in range/i)).toBeInTheDocument();
    expect(screen.queryByTestId('gauge-metric-chart')).not.toBeInTheDocument();
  });

  it('renders every point of a series', () => {
    render(<MetricLineChart series={SPREAD} range={RANGE} events={[]} />);

    expect(screen.getByTestId('gauge-metric-chart')).toHaveAttribute(
      'data-point-count',
      '4',
    );
  });

  it('labels the chart with the series resolution, not a hardcoded one', () => {
    render(
      <MetricLineChart
        series={series([point(0, 1)], 3600)}
        range={RANGE}
        events={[]}
      />,
    );

    expect(screen.getByText('1-hour samples')).toBeInTheDocument();
  });

  it('labels the most recent value directly, and no other point', () => {
    render(<MetricLineChart series={SPREAD} range={RANGE} events={[]} />);

    expect(screen.getByText('15')).toBeInTheDocument();
    expect(screen.queryByText('1,234')).not.toBeInTheDocument();
  });

  it('shows the hovered point in a tooltip', () => {
    const { container } = render(
      <MetricLineChart series={SPREAD} range={RANGE} events={[]} />,
    );

    // nearest point to the middle of the plot is the second sample
    hoverAt(container, VIEW_W / 2);

    expect(screen.getByText('1,234')).toBeInTheDocument();
  });

  it('drops the tooltip when the pointer leaves the plot', () => {
    const { container } = render(
      <MetricLineChart series={SPREAD} range={RANGE} events={[]} />,
    );

    const overlay = hoverAt(container, VIEW_W / 2);
    expect(screen.getByText('1,234')).toBeInTheDocument();

    fireEvent.mouseLeave(overlay);
    expect(screen.queryByText('1,234')).not.toBeInTheDocument();
  });

  it('pins the x-domain to the requested range, not the data extent', () => {
    // Both points sit in the first tenth of a 24h window; under a data-extent
    // domain the last one would land at the right edge instead.
    const early = series(
      [point(23 * 60, 5), point(22 * 60, 8)],
      3600, // matches the points' 1-hour spacing, so they draw as one segment
    );
    const wideRange: ChartRange = { from: NOW - 24 * 60 * 60_000, to: NOW };

    const { container } = render(
      <MetricLineChart series={early} range={wideRange} events={[]} />,
    );

    const lastPoint = container.querySelector('circle[fill="#f7931a"]');
    expect(Number(lastPoint?.getAttribute('cx'))).toBeLessThan(VIEW_W / 3);
  });
});

describe('MetricLineChart lifecycle markers', () => {
  const LIFECYCLE_EVENTS = [
    event(1, 'server_stopped', 2),
    event(2, 'server_started', 1),
  ];

  it('draws a dashed marker for each in-range lifecycle event by default', () => {
    const { container } = render(
      <MetricLineChart
        series={SPREAD}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    expect(
      container.querySelectorAll('[data-testid="event-marker"]'),
    ).toHaveLength(2);
  });

  it('colours a stop marker differently from a start marker', () => {
    const { container } = render(
      <MetricLineChart
        series={SPREAD}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    const markers = container.querySelectorAll('[data-testid="event-marker"]');
    expect(markers[0]).toHaveClass('stroke-alert'); // server_stopped, 2 min ago
    expect(markers[1]).toHaveClass('stroke-live'); // server_started, 1 min ago
  });

  it('draws no marker for non-lifecycle event kinds', () => {
    const mixed = [
      ...LIFECYCLE_EVENTS,
      event(3, 'node_connected', 2),
      event(4, 'bootstrap_started', 1),
    ];

    const { container } = render(
      <MetricLineChart series={SPREAD} range={RANGE} events={mixed} />,
    );

    expect(
      container.querySelectorAll('[data-testid="event-marker"]'),
    ).toHaveLength(2);
  });

  it('hides and re-shows markers via the EVENTS toggle', () => {
    const { container } = render(
      <MetricLineChart
        series={SPREAD}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    const toggle = screen.getByRole('button', { name: 'EVENTS' });

    fireEvent.click(toggle);
    expect(
      container.querySelectorAll('[data-testid="event-marker"]'),
    ).toHaveLength(0);

    fireEvent.click(toggle);
    expect(
      container.querySelectorAll('[data-testid="event-marker"]'),
    ).toHaveLength(2);
  });

  // The visible 1px line sits under the hover overlay and can never be hovered,
  // so the title has to hang off a separate wide hit target above it.
  it('exposes each marker timestamp on a hoverable hit target', () => {
    const { container } = render(
      <MetricLineChart
        series={SPREAD}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    const hits = Array.from(
      container.querySelectorAll('[data-testid="event-marker-hit"]'),
    );
    // Derived, not hardcoded: the label renders in local time.
    expect(hits.map((h) => h.querySelector('title')?.textContent)).toEqual(
      LIFECYCLE_EVENTS.map(
        (e) => `${e.kind} ${dayjs(e.created_at).format('YYYY-MM-DD HH:mm:ss')}`,
      ),
    );
    for (const hit of hits) {
      expect(Number(hit.getAttribute('stroke-width'))).toBeGreaterThan(1);
      expect(hit.closest('.pointer-events-none')).toBeNull();
    }
  });

  it('removes the marker hit targets when the toggle is off', () => {
    const { container } = render(
      <MetricLineChart
        series={SPREAD}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'EVENTS' }));

    expect(
      container.querySelectorAll('[data-testid="event-marker-hit"]'),
    ).toHaveLength(0);
  });
});
