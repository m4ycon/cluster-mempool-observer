import {
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { NotPorted } from './components/NotPorted';
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
  component: () => <NotPorted label="CLUSTER" />,
});

const distRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/dist',
  component: () => <NotPorted label="SIZE DISTRIBUTION" />,
});

const routeTree = rootRoute.addChildren([
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
