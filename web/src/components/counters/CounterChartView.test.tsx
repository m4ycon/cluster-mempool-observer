import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ChartRange } from '../../lib/routes';
import type { CounterPoint } from '../../types/generated/CounterPoint';
import type { CounterSeries } from '../../types/generated/CounterSeries';
import type { SystemEvent } from '../../types/generated/SystemEvent';
import type { SystemEventKind } from '../../types/generated/SystemEventKind';
import { CounterChartView } from './CounterChartView';

const VIEW_W = 960;
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

/** A dominant `added_txs` series alongside much smaller `confirmed_txs`/`evicted_txs` ones. */
const POINTS = series([
  point(3, 1000, 5, 2),
  point(2, 1100, 6, 1),
  point(1, 900, 4, 3),
  point(0, 1200, 7, 0),
]);

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

function lastCy(
  container: HTMLElement,
  key: 'added_txs' | 'confirmed_txs' | 'evicted_txs',
) {
  const circle = container.querySelector(
    `[data-testid="series-${key}-last"] circle`,
  );
  return Number(circle?.getAttribute('cy'));
}

describe('CounterChartView', () => {
  it('renders the empty case without claiming a failure', () => {
    render(<CounterChartView series={series([])} range={RANGE} events={[]} />);

    expect(screen.getByText(/no samples in range/i)).toBeInTheDocument();
    expect(screen.queryByTestId('counter-chart')).not.toBeInTheDocument();
  });

  it('renders every point across all three series', () => {
    render(<CounterChartView series={POINTS} range={RANGE} events={[]} />);

    expect(screen.getByTestId('counter-chart')).toHaveAttribute(
      'data-point-count',
      '4',
    );
    expect(screen.getAllByTestId('series-added_txs').length).toBeGreaterThan(0);
    expect(
      screen.getAllByTestId('series-confirmed_txs').length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByTestId('series-evicted_txs').length).toBeGreaterThan(
      0,
    );
  });

  it('labels the chart with the series resolution', () => {
    render(
      <CounterChartView
        series={series([point(0, 1, 1, 1)], 3600)}
        range={RANGE}
        events={[]}
      />,
    );

    expect(screen.getByText('1-hour samples')).toBeInTheDocument();
  });

  describe('per-series toggles', () => {
    it('hides and re-shows only the toggled series', () => {
      render(<CounterChartView series={POINTS} range={RANGE} events={[]} />);

      fireEvent.click(screen.getByRole('button', { name: 'ARRIVALS' }));

      expect(screen.queryAllByTestId('series-added_txs')).toHaveLength(0);
      expect(
        screen.getAllByTestId('series-confirmed_txs').length,
      ).toBeGreaterThan(0);
      expect(
        screen.getAllByTestId('series-evicted_txs').length,
      ).toBeGreaterThan(0);

      fireEvent.click(screen.getByRole('button', { name: 'ARRIVALS' }));
      expect(screen.getAllByTestId('series-added_txs').length).toBeGreaterThan(
        0,
      );
    });

    it('hides confirmed independently of evicted', () => {
      render(<CounterChartView series={POINTS} range={RANGE} events={[]} />);

      fireEvent.click(screen.getByRole('button', { name: 'MINED' }));

      expect(screen.queryAllByTestId('series-confirmed_txs')).toHaveLength(0);
      expect(screen.getAllByTestId('series-added_txs').length).toBeGreaterThan(
        0,
      );
      expect(
        screen.getAllByTestId('series-evicted_txs').length,
      ).toBeGreaterThan(0);
    });

    it('hides evicted independently of confirmed', () => {
      render(<CounterChartView series={POINTS} range={RANGE} events={[]} />);

      fireEvent.click(screen.getByRole('button', { name: 'EVICTED' }));

      expect(screen.queryAllByTestId('series-evicted_txs')).toHaveLength(0);
      expect(screen.getAllByTestId('series-added_txs').length).toBeGreaterThan(
        0,
      );
      expect(
        screen.getAllByTestId('series-confirmed_txs').length,
      ).toBeGreaterThan(0);
    });

    it('rescales the y-axis when a dominant series is hidden, giving the rest more height', () => {
      const { container } = render(
        <CounterChartView series={POINTS} range={RANGE} events={[]} />,
      );

      const beforeCy = lastCy(container, 'confirmed_txs');

      fireEvent.click(screen.getByRole('button', { name: 'ARRIVALS' }));

      const afterCy = lastCy(container, 'confirmed_txs');

      // Hiding the dominant (added) series shrinks the y-domain, so the same
      // confirmed value now maps closer to the plot's top edge (a smaller cy).
      expect(afterCy).toBeLessThan(beforeCy);
    });
  });

  describe('null handling', () => {
    const NULL_RANGE: ChartRange = { from: NOW - 2 * 60_000, to: NOW };
    const NULL_POINTS = series([
      point(2, 100, 5, 1),
      point(1, null, 6, 2),
      point(0, 120, 7, 0),
    ]);

    it('reads a null value as "not measured" in the tooltip, never as zero', () => {
      const { container } = render(
        <CounterChartView
          series={NULL_POINTS}
          range={NULL_RANGE}
          events={[]}
        />,
      );

      hoverAt(container, VIEW_W / 2);

      expect(screen.getByText('ARRIVALS: not measured')).toBeInTheDocument();
      expect(screen.queryByText('ARRIVALS: 0')).not.toBeInTheDocument();
      expect(screen.getByText('MINED: 6')).toBeInTheDocument();
      expect(screen.getByText('EVICTED: 2')).toBeInTheDocument();
    });

    it('omits a hidden series from the tooltip entirely', () => {
      const { container } = render(
        <CounterChartView
          series={NULL_POINTS}
          range={NULL_RANGE}
          events={[]}
        />,
      );

      fireEvent.click(screen.getByRole('button', { name: 'MINED' }));
      hoverAt(container, VIEW_W / 2);

      expect(screen.queryByText('MINED: 6')).not.toBeInTheDocument();
      expect(screen.getByText('EVICTED: 2')).toBeInTheDocument();
    });
  });
});

describe('CounterChartView lifecycle markers', () => {
  const LIFECYCLE_EVENTS = [
    event(1, 'server_stopped', 2),
    event(2, 'server_started', 1),
  ];

  it('draws a dashed marker for each in-range lifecycle event by default', () => {
    const { container } = render(
      <CounterChartView
        series={POINTS}
        range={RANGE}
        events={LIFECYCLE_EVENTS}
      />,
    );

    expect(
      container.querySelectorAll('[data-testid="event-marker"]'),
    ).toHaveLength(2);
  });

  it('hides and re-shows markers via the EVENTS toggle', () => {
    const { container } = render(
      <CounterChartView
        series={POINTS}
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
});
