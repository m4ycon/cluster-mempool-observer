import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebRoutes } from '../lib/routes';
import { routeTree } from '../router';

// RootLayout wraps every route and reads the shared socket; irrelevant here.
vi.mock('../hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('../ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

const ROUTE = WebRoutes.mempoolSize;

function renderAtRoute() {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [ROUTE] }),
  });
  render(<RouterProvider router={router} />);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('MempoolSizeOverTime', () => {
  it('plots the mempool tx-count metric, not another column of the gauge sample row', async () => {
    const fetchSpy = vi.fn(() => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchSpy);

    renderAtRoute();

    await screen.findByText('MEMPOOL SIZE OVER TIME');
    expect(fetchSpy).toHaveBeenCalledWith(
      expect.stringContaining(ROUTE),
      expect.anything(),
    );
  });
});
