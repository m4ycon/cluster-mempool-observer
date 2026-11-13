import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen, waitFor } from '@testing-library/react';
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

function serveVersion(response: Promise<Response>) {
  vi.stubGlobal(
    'fetch',
    vi.fn(() => response),
  );
}

function versionJson(sha: string): Promise<Response> {
  return Promise.resolve({
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve({ sha }),
  } as Response);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('RootLayout', () => {
  it('points the version tag at the commit the container was stamped with', async () => {
    serveVersion(versionJson('7cd3450'));

    renderLayout();

    expect(await screen.findByText('BUILD 7cd3450')).toBeInTheDocument();
    expect(screen.getByText('alpha')).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledWith('/version.json', expect.anything());
  });

  it('shortens a full-length sha to the seven characters the tooltip has room for', async () => {
    serveVersion(versionJson('538e41e9b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6'));

    renderLayout();

    expect(await screen.findByText('BUILD 538e41e')).toBeInTheDocument();
  });

  it('leaves the version tag bare when the container carries no commit', async () => {
    serveVersion(versionJson(''));

    renderLayout();

    expect(await screen.findByText('alpha')).toBeInTheDocument();
    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(screen.queryByText(/BUILD/)).not.toBeInTheDocument();
  });

  it('leaves the version tag bare when there is no version to fetch', async () => {
    serveVersion(Promise.reject(new SyntaxError('Unexpected token <')));

    renderLayout();

    expect(await screen.findByText('alpha')).toBeInTheDocument();
    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(screen.queryByText(/BUILD/)).not.toBeInTheDocument();
  });
});
