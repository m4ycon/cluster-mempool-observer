import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebRoutes } from '../lib/routes';
import { routeTree } from '../router';

vi.mock('../hooks/useMempoolStats', () => ({
  useMempoolStats: () => null,
}));

// RootLayout wraps every route and reads the shared socket; irrelevant here.
vi.mock('../hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('../ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Home preview cards: the snapshot previews are decorative, not live', () => {
  it('renders the previews without issuing any fetch', async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);

    const router = createRouter({
      routeTree,
      history: createMemoryHistory({ initialEntries: [WebRoutes.home] }),
    });
    render(<RouterProvider router={router} />);

    await screen.findByText('CLUSTER COUNT OVER TIME');
    expect(screen.getByText('cluster count, last 24h')).toBeInTheDocument();

    expect(screen.getByText('MEMPOOL SIZE OVER TIME')).toBeInTheDocument();
    expect(screen.getByText('txs in mempool, last 24h')).toBeInTheDocument();

    expect(screen.getByText('MEMPOOL FEERATE DIAGRAM')).toBeInTheDocument();
    expect(screen.getByText('cumulative fee by weight')).toBeInTheDocument();

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
