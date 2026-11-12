import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FeerateDiagramCurves } from '../../lib/feerateDiagramChart';
import { GLOSSARY } from '../../lib/glossary';
import type { FeerateDiagramPoint } from '../../types/generated/FeerateDiagramPoint';
import { Dialog } from '../dialog/Dialog';
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

function nodeOnly(node: FeerateDiagramPoint[]): FeerateDiagramCurves {
  return { node, ours: [], missing_chunks: [], extra_chunks: [] };
}

/** jsdom reports a zero-width box, which the hover handler bails out on. */
function hoverAt(container: HTMLElement, clientX: number) {
  const svg = container.querySelector('svg[role="img"]') as SVGSVGElement;
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
  // jsdom doesn't implement the native <dialog> API, so showModal()/close()
  // are no-ops there; stub them the same way Dialog.test.tsx does.
  beforeEach(() => {
    HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
      this.removeAttribute('open');
      this.dispatchEvent(new Event('close'));
    };
  });

  it('opens the sigops-adjusted-weight definition when the x-axis label is clicked', async () => {
    render(
      <>
        <FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByText('cumulative sigops-adjusted weight (WU)'),
      );
    });

    expect(
      screen.getByRole('dialog', {
        name: GLOSSARY['sigops-adjusted-weight'].label,
      }),
    ).toBeInTheDocument();
  });

  it('opens the sigops-adjusted-weight definition when the x-axis label is activated with the keyboard', async () => {
    render(
      <>
        <FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />
        <Dialog />
      </>,
    );

    const label = screen.getByText('cumulative sigops-adjusted weight (WU)');
    label.focus();
    expect(label).toHaveFocus();

    await act(async () => {
      fireEvent.keyDown(label, { key: 'Enter' });
    });

    expect(
      screen.getByRole('dialog', {
        name: GLOSSARY['sigops-adjusted-weight'].label,
      }),
    ).toBeInTheDocument();
  });

  it('renders the empty case without claiming a failure', () => {
    render(<FeerateDiagramCurve curves={nodeOnly([])} blockWindow="all" />);

    expect(screen.getByText(/no sample yet/i)).toBeInTheDocument();
    expect(
      screen.queryByTestId('feerate-diagram-chart'),
    ).not.toBeInTheDocument();
  });

  it('renders every point of a populated diagram', () => {
    render(<FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />);

    expect(screen.getByTestId('feerate-diagram-chart')).toHaveAttribute(
      'data-point-count',
      '5',
    );
  });

  it('labels the most recent value directly, and no other point', () => {
    render(<FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />);

    expect(screen.getByText('100,000,000 sats')).toBeInTheDocument();
    expect(screen.queryByText('25,000,000 sats')).not.toBeInTheDocument();
  });

  it('draws a dashed boundary line for each block edge in a 3-block window over a 12M-weight diagram', () => {
    const d = nodeOnly(spread(12, 12_000_000, 120_000_000));
    const { container } = render(
      <FeerateDiagramCurve curves={d} blockWindow={3} />,
    );

    expect(container.querySelectorAll('line[stroke-dasharray]')).toHaveLength(
      3,
    );
  });

  it('clips the drawn curve to the window prop', () => {
    const d = nodeOnly(spread(12, 12_000_000, 120_000_000));
    const { container } = render(
      <FeerateDiagramCurve curves={d} blockWindow={1} />,
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
      <FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />,
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
      <FeerateDiagramCurve curves={nodeOnly(SPREAD)} blockWindow="all" />,
    );

    hoverAt(container, 0); // nearest the origin point, which has no prior segment

    expect(screen.getByText('-- sat/vB (marginal)')).toBeInTheDocument();
    expect(screen.queryByText('0.0 sat/vB (marginal)')).not.toBeInTheDocument();
  });

  describe('against our curve', () => {
    // Chunk 2 (points 1 -> 2) is missing from ours; ours has one extra chunk on top.
    const node = SPREAD;
    const ours = [
      ...node.slice(0, 2),
      ...node
        .slice(3)
        .map((p) => point(p.weight - 5_000_000, p.fee_sats - 25_000_000)),
      point(18_000_000, 80_000_000),
    ];
    const curves: FeerateDiagramCurves = {
      node,
      ours,
      missing_chunks: [2],
      extra_chunks: [4],
    };

    it('draws our curve dashed', () => {
      render(<FeerateDiagramCurve curves={curves} blockWindow="all" />);

      expect(screen.getByTestId('ours-curve')).toHaveAttribute(
        'stroke-dasharray',
      );
    });

    it('marks a missing chunk with an x and an extra one with a ring', () => {
      render(<FeerateDiagramCurve curves={curves} blockWindow="all" />);

      const marks = screen.getAllByTestId('chunk-mark');
      expect(marks.map((m) => m.dataset.diff)).toEqual(['missing', 'extra']);
      expect(marks[0].querySelector('path')).not.toBeNull();
      expect(marks[1].querySelector('circle')).not.toBeNull();
    });

    it("shows a mark's chunk in a tooltip on hover", () => {
      render(<FeerateDiagramCurve curves={curves} blockWindow="all" />);

      const [missingHit] = screen.getAllByTestId('chunk-mark-hit');
      fireEvent.mouseEnter(missingHit);

      expect(screen.getByText('missing chunk')).toBeInTheDocument();
      expect(
        screen.getByText('25,000,000 sats · 5,000,000 WU'),
      ).toBeInTheDocument();
      expect(screen.getByText('20.00 sat/vB')).toBeInTheDocument();
      expect(
        screen.getByText('node has it, ours does not'),
      ).toBeInTheDocument();

      fireEvent.mouseLeave(missingHit);
      expect(screen.queryByText('missing chunk')).not.toBeInTheDocument();
    });

    it('keeps the mark hit targets reachable above the hover overlay', () => {
      const { container } = render(
        <FeerateDiagramCurve curves={curves} blockWindow="all" />,
      );

      const overlay = container.querySelector(
        'rect[fill="transparent"]',
      ) as Element;
      for (const hit of screen.getAllByTestId('chunk-mark-hit')) {
        expect(hit.closest('.pointer-events-none')).toBeNull();
        expect(
          overlay.compareDocumentPosition(hit) &
            Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
    });
  });
});
