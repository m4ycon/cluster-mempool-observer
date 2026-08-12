import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { MempoolMetricSeries } from '../../types/generated/MempoolMetricSeries';
import { MetricLineChart } from './MetricLineChart';

const VIEW_W = 960;

function point(minutesAgo: number, value: number) {
  return {
    sampled_at: new Date(
      Date.UTC(2026, 6, 1, 12, 0, 0) - minutesAgo * 60_000,
    ).toISOString(),
    value,
  };
}

function series(
  points: MempoolMetricSeries['points'],
  resolutionSecs = 60,
): MempoolMetricSeries {
  return { metric: 'cluster-count', resolution_secs: resolutionSecs, points };
}

/** Evenly spaced, so a hover at x maps to a predictable index. */
const SPREAD = series([
  point(3, 10),
  point(2, 1234),
  point(1, 9),
  point(0, 15),
]);

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
    render(<MetricLineChart series={series([])} />);

    expect(screen.getByText(/no snapshots in range/i)).toBeInTheDocument();
    expect(
      screen.queryByTestId('mempool-metric-chart'),
    ).not.toBeInTheDocument();
  });

  it('renders every point of a series', () => {
    render(<MetricLineChart series={SPREAD} />);

    expect(screen.getByTestId('mempool-metric-chart')).toHaveAttribute(
      'data-point-count',
      '4',
    );
  });

  it('labels the chart with the series resolution, not a hardcoded one', () => {
    render(<MetricLineChart series={series([point(0, 1)], 3600)} />);

    expect(screen.getByText('1-hour samples')).toBeInTheDocument();
  });

  it('labels the most recent value directly, and no other point', () => {
    render(<MetricLineChart series={SPREAD} />);

    expect(screen.getByText('15')).toBeInTheDocument();
    expect(screen.queryByText('1,234')).not.toBeInTheDocument();
  });

  it('shows the hovered point in a tooltip', () => {
    const { container } = render(<MetricLineChart series={SPREAD} />);

    // nearest point to the middle of the plot is the second sample
    hoverAt(container, VIEW_W / 2);

    expect(screen.getByText('1,234')).toBeInTheDocument();
  });

  it('drops the tooltip when the pointer leaves the plot', () => {
    const { container } = render(<MetricLineChart series={SPREAD} />);

    const overlay = hoverAt(container, VIEW_W / 2);
    expect(screen.getByText('1,234')).toBeInTheDocument();

    fireEvent.mouseLeave(overlay);
    expect(screen.queryByText('1,234')).not.toBeInTheDocument();
  });
});
