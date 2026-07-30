import {
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { NotPorted } from './components/NotPorted';
import { validateClustersSearch } from './lib/clustersSearch';
import { Clusters } from './pages/Clusters';
import { Home } from './pages/Home';
import { RootLayout } from './RootLayout';

const rootRoute = createRootRoute({
  component: RootLayout,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: Home,
});

const feesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/fees',
  component: () => <NotPorted label="FEE-RATE" />,
});

const clustersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/clusters',
  component: Clusters,
  validateSearch: validateClustersSearch,
});

const distRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/dist',
  component: () => <NotPorted label="SIZE DISTRIBUTION" />,
});

export const routeTree = rootRoute.addChildren([
  homeRoute,
  feesRoute,
  clustersRoute,
  distRoute,
]);

export const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
