import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RootLayout } from './RootLayout';

vi.mock('./hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('./ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

// The real route tree would drag every page's fetching in behind the header.
function renderLayout() {
  const rootRoute = createRootRoute({ component: RootLayout });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
    component: () => null,
  });
  const router = createRouter({
    routeTree: rootRoute.addChildren([indexRoute]),
    history: createMemoryHistory({ initialEntries: ['/'] }),
  });
  render(<RouterProvider router={router} />);
}

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('RootLayout', () => {
  it('points the version tag at the commit a stamped build came from', async () => {
    vi.stubEnv('VITE_GIT_SHA', '7cd3450');

    renderLayout();

    expect(await screen.findByText('alpha')).toBeInTheDocument();
    expect(screen.getByText('BUILD 7cd3450')).toBeInTheDocument();
  });

  it('shortens a full-length sha to the seven characters the tooltip has room for', async () => {
    vi.stubEnv('VITE_GIT_SHA', '538e41e9b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6');

    renderLayout();

    expect(await screen.findByText('BUILD 538e41e')).toBeInTheDocument();
  });

  it('leaves the version tag bare when the build carries no commit', async () => {
    vi.stubEnv('VITE_GIT_SHA', '');

    renderLayout();

    expect(await screen.findByText('alpha')).toBeInTheDocument();
    expect(screen.queryByText(/BUILD/)).not.toBeInTheDocument();
  });
});
