import {
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { validateClustersSearch } from './lib/clustersSearch';
import { WebRoutes } from './lib/routes';
import { ClusterCountOverTime } from './pages/ClusterCountOverTime';
import { Clusters } from './pages/Clusters';
import { Home } from './pages/Home';
import { MempoolFeerateDiagram } from './pages/MempoolFeerateDiagram';
import { MempoolSizeOverTime } from './pages/MempoolSizeOverTime';
import { TxsPerMinOverTime } from './pages/TxsPerMinOverTime';
import { RootLayout } from './RootLayout';

const rootRoute = createRootRoute({
  component: RootLayout,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.home,
  component: Home,
});

const clustersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.clusters,
  component: Clusters,
  validateSearch: validateClustersSearch,
});

const clusterCountRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.clusterCount,
  component: ClusterCountOverTime,
});

const mempoolSizeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.mempoolSize,
  component: MempoolSizeOverTime,
});

const txsPerMinRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.txsPerMin,
  component: TxsPerMinOverTime,
});

const feerateDiagramRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: WebRoutes.feerateDiagram,
  component: MempoolFeerateDiagram,
});

export const routeTree = rootRoute.addChildren([
  homeRoute,
  clustersRoute,
  clusterCountRoute,
  mempoolSizeRoute,
  txsPerMinRoute,
  feerateDiagramRoute,
]);

export const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
