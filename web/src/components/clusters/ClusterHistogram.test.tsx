import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { HistogramLayout } from '../../lib/clusterHistogram';
import { ClusterHistogram } from './ClusterHistogram';

/**
 * Small hand-built layout: bar 0 has count 2, bar 1 is an empty bin
 * (count 0), bar 2 has count 3. Pixel values are arbitrary but internally
 * consistent with the plot rect.
 */
const LAYOUT: HistogramLayout = {
  bars: [
    { lo: 0, hi: 10, count: 2, x0: 60, x1: 120, y0: 400, y1: 500 },
    { lo: 10, hi: 20, count: 0, x0: 120, x1: 180, y0: 500, y1: 500 },
    { lo: 20, hi: 30, count: 3, x0: 180, x1: 240, y0: 350, y1: 500 },
  ],
  xTicks: [
    { value: 0, px: 60 },
    { value: 30, px: 240 },
  ],
  yTicks: [
    { value: 0, px: 500 },
    { value: 3, px: 350 },
  ],
  maxCount: 3,
  plot: { left: 56, top: 16, width: 648, height: 504 },
};

const EMPTY_LAYOUT: HistogramLayout = {
  bars: [],
  xTicks: [],
  yTicks: [],
  maxCount: 0,
  plot: { left: 56, top: 16, width: 648, height: 504 },
};

/** The transparent per-bin hover targets, in bar order. */
function hitAreas(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('rect[fill="transparent"]'));
}

/** The visible bar rects (fill via class, not the transparent hit areas). */
function barRects(container: HTMLElement): Element[] {
  return Array.from(
    container.querySelectorAll('rect:not([fill="transparent"])'),
  ).filter((r) => r.getAttribute('class')?.includes('fill-'));
}

describe('ClusterHistogram', () => {
  it('renders one visible rect per bar', () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );
    expect(barRects(container)).toHaveLength(3);
  });

  it('renders the empty state with no bars', () => {
    const { container } = render(
      <ClusterHistogram layout={EMPTY_LAYOUT} sizeMetric="vsize" />,
    );
    expect(screen.getByText('awaiting cluster feed...')).toBeInTheDocument();
    expect(container.querySelectorAll('rect')).toHaveLength(0);
  });

  it('shows a tooltip with the bin count on hover', () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );

    fireEvent.mouseOver(hitAreas(container)[0]);

    expect(screen.getByText('2 clusters')).toBeInTheDocument();
    // Non-final bin [0, 10): the printed upper bound is nudged down by one
    // display step so it doesn't claim the value 10, which belongs to the
    // next bin.
    expect(screen.getByText(/^0-9 /)).toBeInTheDocument();
  });

  it('still shows a tooltip when hovering an empty bin', () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );

    fireEvent.mouseOver(hitAreas(container)[1]);

    expect(screen.getByText('0 clusters')).toBeInTheDocument();
    expect(screen.getByText(/^10-19 /)).toBeInTheDocument();
  });

  it("prints the final bin's closed upper bound unchanged", () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );

    // Bar index 2 (lo:20, hi:30) is the last bar in LAYOUT, i.e. the closed
    // bin -- it genuinely contains 30, so the label must not be nudged.
    fireEvent.mouseOver(hitAreas(container)[2]);

    expect(screen.getByText('3 clusters')).toBeInTheDocument();
    expect(screen.getByText(/^20-30 /)).toBeInTheDocument();
  });

  it('hides the tooltip on mouse out', () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );

    const target = hitAreas(container)[0];
    fireEvent.mouseOver(target);
    expect(screen.getByText('2 clusters')).toBeInTheDocument();

    fireEvent.mouseOut(target);
    expect(screen.queryByText('2 clusters')).not.toBeInTheDocument();
  });

  it('changes the hovered bar fill from fill-bar to fill-orange', () => {
    const { container } = render(
      <ClusterHistogram layout={LAYOUT} sizeMetric="vsize" />,
    );

    const bars = barRects(container);
    expect(bars[0].getAttribute('class')).toContain('fill-bar');

    fireEvent.mouseOver(hitAreas(container)[0]);

    const barsAfterHover = barRects(container);
    expect(barsAfterHover[0].getAttribute('class')).toContain('fill-orange');
    // Untouched bars keep the default fill.
    expect(barsAfterHover[1].getAttribute('class')).toContain('fill-bar');
  });
});
