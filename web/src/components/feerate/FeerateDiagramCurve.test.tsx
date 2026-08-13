import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FeerateDiagramPoint } from '../../types/generated/FeerateDiagramPoint';
import type { MempoolFeerateDiagram } from '../../types/generated/MempoolFeerateDiagram';
import { FeerateDiagramCurve } from './FeerateDiagramCurve';

const VIEW_W = 960;

function point(weight: number, fee_sats: number): FeerateDiagramPoint {
  return { weight, fee_sats };
}

/** Real-shaped fixture: origin plus `n` evenly-spread cumulative points. */
function spread(
  n: number,
  totalW: number,
  totalFee: number,
): FeerateDiagramPoint[] {
  const points = [point(0, 0)];
  for (let i = 1; i <= n; i++) {
    points.push(
      point(Math.round((totalW * i) / n), Math.round((totalFee * i) / n)),
    );
  }
  return points;
}

function diagram(
  points: FeerateDiagramPoint[],
  sampledAt: string | null = '2026-08-12T00:00:00Z',
): MempoolFeerateDiagram {
  return { sampled_at: sampledAt, points };
}

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

/** 5 points 224px apart at the default viewBox width -- decimation never merges them. */
const SPREAD = spread(4, 20_000_000, 100_000_000);

describe('FeerateDiagramCurve', () => {
  it('renders the empty case without claiming a failure', () => {
    render(<FeerateDiagramCurve diagram={diagram([])} blockWindow="all" />);

    expect(screen.getByText(/no sample yet/i)).toBeInTheDocument();
    expect(
      screen.queryByTestId('feerate-diagram-chart'),
    ).not.toBeInTheDocument();
  });

  it('renders every point of a populated diagram', () => {
    render(<FeerateDiagramCurve diagram={diagram(SPREAD)} blockWindow="all" />);

    expect(screen.getByTestId('feerate-diagram-chart')).toHaveAttribute(
      'data-point-count',
      '5',
    );
  });

  it('labels the most recent value directly, and no other point', () => {
    render(<FeerateDiagramCurve diagram={diagram(SPREAD)} blockWindow="all" />);

    expect(screen.getByText('100,000,000 sats')).toBeInTheDocument();
    expect(screen.queryByText('25,000,000 sats')).not.toBeInTheDocument();
  });

  it('draws a dashed boundary line for each block edge in a 3-block window over a 12M-weight diagram', () => {
    const d = diagram(spread(12, 12_000_000, 120_000_000));
    const { container } = render(
      <FeerateDiagramCurve diagram={d} blockWindow={3} />,
    );

    expect(container.querySelectorAll('line[stroke-dasharray]')).toHaveLength(
      3,
    );
  });

  it('clips the drawn curve to the window prop', () => {
    const d = diagram(spread(12, 12_000_000, 120_000_000));
    const { container } = render(
      <FeerateDiagramCurve diagram={d} blockWindow={1} />,
    );

    // window=1 keeps only the points up to 4,000,000 WU (origin + 4 more).
    expect(screen.getByTestId('feerate-diagram-chart')).toHaveAttribute(
      'data-point-count',
      '5',
    );
    expect(container.querySelectorAll('line[stroke-dasharray]')).toHaveLength(
      1,
    );
    expect(screen.getByText('40,000,000 sats')).toBeInTheDocument();
    expect(screen.queryByText('120,000,000 sats')).not.toBeInTheDocument();
  });

  it("shows the hovered point's weight, fee, and marginal rate in a tooltip", () => {
    const { container } = render(
      <FeerateDiagramCurve diagram={diagram(SPREAD)} blockWindow="all" />,
    );

    // nearest point to the middle of the plot is the 10,000,000 WU sample
    hoverAt(container, VIEW_W / 2);

    expect(screen.getByText('10,000,000 WU')).toBeInTheDocument();
    expect(screen.getByText('50,000,000 sats')).toBeInTheDocument();
    // (50,000,000 - 25,000,000) sats over (10,000,000 - 5,000,000) WU, in sat/vB
    expect(screen.getByText('20.00 sat/vB (marginal)')).toBeInTheDocument();
  });

  it('renders a dash for the marginal rate at the point with no predecessor, never a fake 0.0 reading', () => {
    const { container } = render(
      <FeerateDiagramCurve diagram={diagram(SPREAD)} blockWindow="all" />,
    );

    hoverAt(container, 0); // nearest the origin point, which has no prior segment

    expect(screen.getByText('-- sat/vB (marginal)')).toBeInTheDocument();
    expect(screen.queryByText('0.0 sat/vB (marginal)')).not.toBeInTheDocument();
  });

  it('shows a sample-time caption when sampled_at is present, not the null-time fallback', () => {
    const d = diagram(spread(1, 4_000_000, 1000), '2026-08-12T12:00:00Z');
    render(<FeerateDiagramCurve diagram={d} blockWindow="all" />);

    expect(screen.queryByText('sample time unknown')).not.toBeInTheDocument();
  });

  it('falls back to a placeholder caption when sampled_at is null despite having points', () => {
    const d = diagram(spread(1, 4_000_000, 1000), null);
    render(<FeerateDiagramCurve diagram={d} blockWindow="all" />);

    expect(screen.getByText('sample time unknown')).toBeInTheDocument();
  });
});
