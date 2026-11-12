import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebRoutes } from '../lib/routes';
import { routeTree } from '../router';
import type { FeerateDiagramComparison } from '../types/generated/FeerateDiagramComparison';
import type { FeerateDiagramPoint } from '../types/generated/FeerateDiagramPoint';
import type { GaugeSeries } from '../types/generated/GaugeSeries';

// RootLayout wraps every route and reads the shared socket; irrelevant here.
vi.mock('../hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('../ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

const ROUTE = WebRoutes.feerateDiagram;

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
const NODE = spread(6, 12_000_000, 60_000_000);
const COMPARISON: FeerateDiagramComparison = {
  sampled_at: '2026-08-12T00:00:00Z',
  node: NODE,
  ours: NODE,
  missing: 0,
  extra: 0,
  node_total: 6,
  unit: 'chunk_count',
  missing_chunks: [],
  extra_chunks: [],
};

function gaugeSeries(metric: GaugeSeries['metric']): GaugeSeries {
  return {
    metric,
    resolution_secs: 60,
    points: [{ sampled_at: '2026-08-12T00:00:00Z', value: 0.001 }],
  };
}

/** Answers each of the page's requests by its URL. */
function stubApi() {
  const fetchMock = vi.fn((url: string) => {
    if (url.includes('/feerate-diagram/comparison')) {
      return Promise.resolve(jsonResponse(COMPARISON));
    }
    if (url.includes('/system-events'))
      return Promise.resolve(jsonResponse([]));
    const metric = url.match(/gauges\/([a-z-]+)/)?.[1] as GaugeSeries['metric'];
    return Promise.resolve(jsonResponse(gaugeSeries(metric)));
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('MempoolFeerateDiagram', () => {
  it('renders at its route and requests the comparison and both correctness series', async () => {
    const fetchSpy = vi.fn((_url: string) => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchSpy);

    renderAtRoute();

    await screen.findByText('MEMPOOL FEERATE DIAGRAM');
    const urls = fetchSpy.mock.calls.map(([url]) => url);
    expect(urls).toEqual(
      expect.arrayContaining([
        expect.stringContaining('/mempool/feerate-diagram/comparison'),
        expect.stringContaining('/gauges/feerate-diagram-missing-percentage'),
        expect.stringContaining('/gauges/feerate-diagram-extra-percentage'),
      ]),
    );
  });

  it('draws the correctness history below the diagram', async () => {
    stubApi();

    renderAtRoute();

    expect(
      await screen.findByText('CORRECTNESS OVER TIME'),
    ).toBeInTheDocument();
    const history = await screen.findByTestId('gauge-metric-chart');
    expect(history).toHaveTextContent('missing');
    expect(history).toHaveTextContent('extra');
  });

  it('defaults to "all" and clips the drawn curve once a block window is picked', async () => {
    stubApi();

    renderAtRoute();

    const chart = await screen.findByTestId('feerate-diagram-chart');
    expect(chart).toHaveAttribute('data-point-count', '7');

    fireEvent.click(screen.getByRole('button', { name: '1 block' }));

    expect(chart).toHaveAttribute('data-point-count', '3');
  });

  it('marks the selected window with aria-pressed, not colour alone', async () => {
    stubApi();

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
