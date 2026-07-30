import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { routeTree } from '../router';
import type { ClusterRef } from '../types/events';
import { SLIDER_COMMIT_MS } from './Clusters';

// Five distinct total_vsize/total_fee combinations, so sizeMetric and
// colorMetric genuinely produce different values and the colour scale has
// something to tier.
const clusters: ClusterRef[] = [
  { id: 1, txids: ['a1'], total_vsize: 100, total_fee: 500 },
  { id: 2, txids: ['a2'], total_vsize: 300, total_fee: 3000 },
  { id: 3, txids: ['a3'], total_vsize: 600, total_fee: 12000 },
  { id: 4, txids: ['a4'], total_vsize: 900, total_fee: 45000 },
  { id: 5, txids: ['a5'], total_vsize: 1200, total_fee: 96000 },
];

vi.mock('../hooks/useClusterDeltaSocket', () => ({
  useClusterDeltaSocket: () => ({
    clusters,
    lastUpdates: new Map(),
    readyState: 1, // ReadyState.OPEN
    paused: false,
    togglePaused: vi.fn(),
  }),
}));

// RootLayout wraps every route and opens its own socket; the header it feeds
// is not what these tests are about.
vi.mock('../hooks/useMempoolStatsSocket', () => ({
  useMempoolStatsSocket: () => ({ stats: null, block: null, readyState: 1 }),
}));

// A failed fake-timer test must not leave them installed for the next one.
afterEach(() => {
  vi.useRealTimers();
});

// A Slider's <label> wraps both the input and a span showing the current
// value, so its accessible name is e.g. "SHOW40" -- match on the prefix.
const SHOW_SLIDER = /^SHOW/;
const BINS_SLIDER = /^BINS/;

/**
 * Renders the real route tree at `url`, so the page reads its viz config
 * through the route's own validateSearch rather than through a stub.
 */
async function renderClusters(url = '/clusters') {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [url] }),
  });
  const utils = render(<RouterProvider router={router} />);
  await screen.findByText('CLUSTER GRAPH');
  return { ...utils, router };
}

describe('Clusters page regression: the linked toggle must survive a viz switch', () => {
  it('keeps the legend tiering by the newly linked metric after treemap -> histogram -> treemap', async () => {
    const user = userEvent.setup();
    await renderClusters();

    // Start on treemap: linked defaults to true, a single merged
    // SIZE/COLOR BY select, both metrics default to 'feerate' (unit s/vB).
    await user.click(screen.getByText('TREEMAP'));
    expect(await screen.findByLabelText('SIZE/COLOR BY')).toBeInTheDocument();
    expect(screen.getByTestId('cluster-legend').textContent).toContain('s/vB');

    // Switch to histogram -- this unmounts MetricSelects -- and rebind the
    // only metric it exposes (BIN BY, which drives sizeMetric) to TOTAL FEE.
    await user.click(screen.getByText('HISTOGRAM'));
    await user.selectOptions(await screen.findByLabelText('BIN BY'), 'fee');

    // Back to treemap: MetricSelects remounts. It shows the merged control,
    // meaning linked is (still) true, so the invariant it advertises --
    // sizeMetric === colorMetric -- must hold: the legend must now tier by
    // 'fee' (sats), not the stale 'feerate' (s/vB) from before the round
    // trip. The merged select's own value can't tell them apart -- it's
    // bound to sizeMetric either way -- so the legend, which is driven by
    // colorMetric alone, is what's actually sensitive to this bug.
    await user.click(screen.getByText('TREEMAP'));

    expect(await screen.findByLabelText('SIZE/COLOR BY')).toBeInTheDocument();
    const legend = screen.getByTestId('cluster-legend').textContent ?? '';
    expect(legend).toContain('sats');
    expect(legend).not.toContain('s/vB');
  });
});

describe('Clusters page: histogram swaps the right-hand panel for a distribution summary', () => {
  it('shows the stats panel (not the selected-cluster panel) on histogram, and restores it after switching back', async () => {
    const user = userEvent.setup();
    const { container } = await renderClusters();

    const panelLabel = () =>
      container
        .querySelector('[data-screen-label$="panel"]')
        ?.getAttribute('data-screen-label');

    // Circles: the per-cluster panel is showing (TXIDS list only exists there).
    expect(panelLabel()).toBe('Selected cluster panel');
    expect(screen.getByText('TXIDS')).toBeInTheDocument();

    await user.click(screen.getByText('HISTOGRAM'));

    // Histogram: no clicked cluster to describe, so the distribution summary
    // takes over instead -- this is the whole point of this panel swap.
    await waitFor(() =>
      expect(panelLabel()).toBe('Cluster distribution panel'),
    );
    expect(screen.queryByText('TXIDS')).not.toBeInTheDocument();
    expect(screen.getByText('5 CLUSTERS')).toBeInTheDocument();
    // A stats-only label: "TOTAL VSIZE" would also match the BIN BY option.
    expect(screen.getByText('MEDIAN FEE-RATE')).toBeInTheDocument();

    await user.click(screen.getByText('CIRCLES'));

    // Back to circles: the selected-cluster panel is restored.
    await waitFor(() => expect(panelLabel()).toBe('Selected cluster panel'));
    expect(screen.getByText('TXIDS')).toBeInTheDocument();
    expect(screen.queryByText('5 CLUSTERS')).not.toBeInTheDocument();
  });
});

describe('Clusters page: the URL restores the viz config', () => {
  it('opens on the viz, metrics and bin count the search params name', async () => {
    await renderClusters('/clusters?v=h&s=f&b=12');

    expect(screen.getByLabelText('BIN BY')).toHaveValue('fee');
    expect(screen.getByLabelText(BINS_SLIDER)).toHaveValue('12');
  });

  it('reads a split size/colour pair back as unlinked', async () => {
    // No `linked` param: two different metrics is what an unlinked pair looks
    // like, so the page must reopen with the two separate selects.
    await renderClusters('/clusters?s=f&c=t');

    expect(screen.getByLabelText('SIZE BY')).toHaveValue('fee');
    expect(screen.getByLabelText('COLOR BY')).toHaveValue('txs');
    expect(screen.queryByLabelText('SIZE/COLOR BY')).not.toBeInTheDocument();
  });

  it('reads a matching size/colour pair back as linked', async () => {
    await renderClusters('/clusters?s=v&c=v');

    expect(screen.getByLabelText('SIZE/COLOR BY')).toHaveValue('vsize');
    expect(screen.queryByLabelText('COLOR BY')).not.toBeInTheDocument();
  });

  it('heals a mangled URL instead of rendering a broken view', async () => {
    await renderClusters('/clusters?v=zzz&s=nope&n=9999');

    // Unknown codes fall back to the defaults; out-of-range numbers clamp.
    expect(screen.getByLabelText('SIZE/COLOR BY')).toHaveValue('feerate');
    expect(screen.getByLabelText(SHOW_SLIDER)).toHaveValue('250');
  });
});

describe('Clusters page: config changes are written to the URL', () => {
  it('records a viz switch, and drops the param again on the way back to the default', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.click(screen.getByText('TREEMAP'));
    await waitFor(() => expect(router.state.location.searchStr).toBe('?v=t'));

    // Back to circles, the default -- the URL should go clean, not carry
    // ?v=c around forever.
    await user.click(screen.getByText('CIRCLES'));
    await waitFor(() => expect(router.state.location.searchStr).toBe(''));
  });

  it('writes a linked metric change as one size+colour pair', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.selectOptions(screen.getByLabelText('SIZE/COLOR BY'), 'vsize');

    await waitFor(() =>
      expect(router.state.location.search).toEqual({ s: 'v', c: 'v' }),
    );
  });

  it('writes only the size metric once the pair is unlinked', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.click(
      screen.getByLabelText('Size and colour by separate metrics'),
    );
    await user.selectOptions(await screen.findByLabelText('SIZE BY'), 'vsize');

    await waitFor(() =>
      expect(router.state.location.search).toEqual({ s: 'v' }),
    );
    expect(screen.getByLabelText('COLOR BY')).toHaveValue('feerate');
  });

  it('records a slider move without stacking a history entry per step', async () => {
    const { router } = await renderClusters();
    const before = router.history.length;

    const slider = screen.getByLabelText(SHOW_SLIDER);
    fireEvent.change(slider, { target: { value: '60' } });
    await waitFor(() => expect(router.state.location.searchStr).toBe('?n=60'));

    fireEvent.change(slider, { target: { value: '61' } });
    await waitFor(() => expect(router.state.location.searchStr).toBe('?n=61'));

    expect(router.history.length).toBe(before);
  });

  it('writes one URL update for a whole drag, not one per step', async () => {
    const { router } = await renderClusters();
    const slider = screen.getByLabelText(SHOW_SLIDER);

    vi.useFakeTimers();

    for (const value of ['41', '48', '55', '60']) {
      fireEvent.change(slider, { target: { value } });
    }

    // Mid-drag: the slider already shows the new value, the URL is untouched.
    expect(slider).toHaveValue('60');
    expect(router.state.location.searchStr).toBe('');

    act(() => vi.advanceTimersByTime(SLIDER_COMMIT_MS));
    vi.useRealTimers();

    // Only the value the drag ended on reaches the URL.
    await waitFor(() => expect(router.state.location.searchStr).toBe('?n=60'));
  });
});
