import {
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { validateClustersSearch } from './lib/clustersSearch';
import { ClusterCountOverTime } from './pages/ClusterCountOverTime';
import { Clusters } from './pages/Clusters';
import { Home } from './pages/Home';
import { MempoolFeerateDiagram } from './pages/MempoolFeerateDiagram';
import { MempoolSizeOverTime } from './pages/MempoolSizeOverTime';
import { RootLayout } from './RootLayout';

const rootRoute = createRootRoute({
  component: RootLayout,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: Home,
});

const clustersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/clusters',
  component: Clusters,
  validateSearch: validateClustersSearch,
});

const clusterCountRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/mempool/snapshots/cluster-count',
  component: ClusterCountOverTime,
});

const mempoolSizeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/mempool/snapshots/mempool-tx-count',
  component: MempoolSizeOverTime,
});

const feerateDiagramRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/mempool/feerate-diagram',
  component: MempoolFeerateDiagram,
});

export const routeTree = rootRoute.addChildren([
  homeRoute,
  clustersRoute,
  clusterCountRoute,
  mempoolSizeRoute,
  feerateDiagramRoute,
]);

export const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
