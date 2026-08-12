import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
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

describe('Home preview cards: CLUSTER COUNT OVER TIME is decorative, not live', () => {
  it('renders the preview without issuing any fetch', async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);

    const router = createRouter({
      routeTree,
      history: createMemoryHistory({ initialEntries: ['/'] }),
    });
    render(<RouterProvider router={router} />);

    await screen.findByText('CLUSTER COUNT OVER TIME');
    expect(screen.getByText('cluster count, last 24h')).toBeInTheDocument();

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
