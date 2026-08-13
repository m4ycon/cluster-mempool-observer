import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { routeTree } from '../router';
import type { FeerateDiagramPoint } from '../types/generated/FeerateDiagramPoint';
import type { MempoolFeerateDiagram as FeerateDiagramDto } from '../types/generated/MempoolFeerateDiagram';

// RootLayout wraps every route and reads the shared socket; irrelevant here.
vi.mock('../hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('../ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

const ROUTE = '/mempool/feerate-diagram';

function renderAtRoute() {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [ROUTE] }),
  });
  render(<RouterProvider router={router} />);
}

function jsonResponse(body: unknown): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(body),
  } as Response;
}

function point(weight: number, fee_sats: number): FeerateDiagramPoint {
  return { weight, fee_sats };
}

/** Origin plus `n` evenly-spread cumulative points spanning `totalW` weight units. */
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

// 3 blocks' worth of weight (BLOCK_WEIGHT = 4,000,000), so a 1-block window visibly clips it.
const DIAGRAM: FeerateDiagramDto = {
  sampled_at: '2026-08-12T00:00:00Z',
  points: spread(6, 12_000_000, 60_000_000),
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('MempoolFeerateDiagram', () => {
  it('renders at its route and requests the feerate diagram', async () => {
    const fetchSpy = vi.fn(() => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchSpy);

    renderAtRoute();

    await screen.findByText('MEMPOOL FEERATE DIAGRAM');
    expect(fetchSpy).toHaveBeenCalledWith(
      expect.stringContaining(ROUTE),
      expect.anything(),
    );
  });

  it('defaults to "all" and clips the drawn curve once a block window is picked', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(DIAGRAM)));

    renderAtRoute();

    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '7');

    fireEvent.click(screen.getByRole('button', { name: '1 block' }));

    expect(chart).toHaveAttribute('data-point-count', '3');
  });

  it('marks the selected window with aria-pressed, not colour alone', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(DIAGRAM)));

    renderAtRoute();
    await screen.findByTestId('feerate-diagram-chart');

    expect(screen.getByRole('button', { name: 'all' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByRole('button', { name: '1 block' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );

    fireEvent.click(screen.getByRole('button', { name: '1 block' }));

    expect(screen.getByRole('button', { name: '1 block' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByRole('button', { name: 'all' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });
});
