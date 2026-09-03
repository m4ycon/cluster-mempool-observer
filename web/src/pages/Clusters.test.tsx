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
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Mock } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebRoutes } from '../lib/routes';
import { routeTree } from '../router';
import { lookupResponse, requestedTxids, tx } from '../test/transactions';
import type { ClusterDeltaEvent, ClusterRef } from '../types/events';
import { SLIDER_COMMIT_MS } from './Clusters';

const SEEN_BASE = Date.parse('2026-01-01T00:00:00.000Z');
function seenAt(minutes: number): Pick<ClusterRef, 'first_seen_at'> {
  return {
    first_seen_at: new Date(SEEN_BASE + minutes * 60_000).toISOString(),
  };
}

// Five distinct total_vsize/total_fee combinations, so sizeMetric and
// colorMetric genuinely produce different values and the colour scale has
// something to tier.
const clusters: ClusterRef[] = [
  { id: 1, txids: ['a1'], total_vsize: 100, total_fee: 500, ...seenAt(1) },
  { id: 2, txids: ['a2'], total_vsize: 300, total_fee: 3000, ...seenAt(2) },
  { id: 3, txids: ['a3'], total_vsize: 600, total_fee: 12000, ...seenAt(3) },
  { id: 4, txids: ['a4'], total_vsize: 900, total_fee: 45000, ...seenAt(4) },
  { id: 5, txids: ['a5'], total_vsize: 1200, total_fee: 96000, ...seenAt(5) },
];

let deltaHandler: ((event: ClusterDeltaEvent) => void) | null = null;

vi.mock('../ws/useSubscription', () => ({
  useSubscription: (
    subject: string,
    onEvent: (event: ClusterDeltaEvent) => void,
  ) => {
    if (subject === 'cluster.delta') deltaHandler = onEvent;
  },
}));

/** Plays one cluster delta through the page's feed. */
function sendDelta(upserted: ClusterRef[], removed: number[] = []) {
  const event: ClusterDeltaEvent = {
    upserted,
    removed: removed.map((id) => BigInt(id)),
  };
  act(() => deltaHandler?.(event));
}

// RootLayout wraps every route and reads the shared socket; the header it
// feeds is not what these tests are about.
vi.mock('../hooks/useChainTip', () => ({
  useChainTip: () => null,
}));

vi.mock('../ws/useWsReadyState', () => ({
  useWsReadyState: () => 1,
}));

// Selecting a cluster fetches its transactions via SelectedClusterPanel's
// cache; a generic empty-found stub keeps that quiet.
beforeEach(() => {
  deltaHandler = null;
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: () => Promise.resolve({ found: [], missing: [] }),
    } as Response),
  );
});

// A failed fake-timer test must not leave them installed for the next one.
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

// A Slider's <label> wraps both the input and a span showing the current
// value, so its accessible name is e.g. "SHOW40" -- match on the prefix.
const SHOW_SLIDER = /^SHOW/;
const BINS_SLIDER = /^BINS/;

/**
 * Renders the real route tree at `url`, so the page reads its viz config
 * through the route's own validateSearch rather than through a stub.
 */
async function renderClusters(url: string = WebRoutes.clusters) {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [url] }),
  });
  const utils = render(<RouterProvider router={router} />);
  await screen.findByText('CLUSTER GRAPH');
  sendDelta(clusters);
  await act(async () => {});
  return { ...utils, router };
}

/** Renders the page and switches to the table viz, waiting for its rows. */
async function renderClustersTable() {
  const user = userEvent.setup();
  const utils = await renderClusters();
  await user.click(screen.getByText('TABLE'));
  await screen.findByText('#1');
  return { user, ...utils };
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
    await renderClusters(`${WebRoutes.clusters}?v=h&s=f&b=12`);

    expect(screen.getByLabelText('BIN BY')).toHaveValue('fee');
    expect(screen.getByLabelText(BINS_SLIDER)).toHaveValue('12');
  });

  it('reads a split size/colour pair back as unlinked', async () => {
    // No `linked` param: two different metrics is what an unlinked pair looks
    // like, so the page must reopen with the two separate selects.
    await renderClusters(`${WebRoutes.clusters}?s=f&c=t`);

    expect(screen.getByLabelText('SIZE BY')).toHaveValue('fee');
    expect(screen.getByLabelText('COLOR BY')).toHaveValue('txs');
    expect(screen.queryByLabelText('SIZE/COLOR BY')).not.toBeInTheDocument();
  });

  it('reads a matching size/colour pair back as linked', async () => {
    await renderClusters(`${WebRoutes.clusters}?s=v&c=v`);

    expect(screen.getByLabelText('SIZE/COLOR BY')).toHaveValue('vsize');
    expect(screen.queryByLabelText('COLOR BY')).not.toBeInTheDocument();
  });

  it('heals a mangled URL instead of rendering a broken view', async () => {
    await renderClusters(`${WebRoutes.clusters}?v=zzz&s=nope&n=9999`);

    // Unknown codes fall back to the defaults; out-of-range numbers clamp.
    expect(screen.getByLabelText('SIZE/COLOR BY')).toHaveValue('feerate');
    expect(screen.getByLabelText(SHOW_SLIDER)).toHaveValue('250');
  });
});

describe('Clusters page: config changes are written to the URL', () => {
  it('records a viz switch, and keeps it pinned on the way back to the default', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.click(screen.getByText('TREEMAP'));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ v: 't' }),
    );

    // Back to circles, today's default -- still written out, so the link keeps
    // showing circles even if the default moves later.
    await user.click(screen.getByText('CIRCLES'));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ v: 'c' }),
    );
  });

  it('writes only the params the user touched', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.click(screen.getByText('TREEMAP'));
    await waitFor(() =>
      expect(router.state.location.search).toEqual({ v: 't' }),
    );

    fireEvent.change(screen.getByLabelText(SHOW_SLIDER), {
      target: { value: '60' },
    });
    await waitFor(() =>
      expect(router.state.location.search).toEqual({ v: 't', n: 60 }),
    );
  });

  it('writes a linked metric change as one size+colour pair', async () => {
    const user = userEvent.setup();
    const { router } = await renderClusters();

    await user.selectOptions(screen.getByLabelText('SIZE/COLOR BY'), 'vsize');

    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ s: 'v', c: 'v' }),
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
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ n: 60 }),
    );

    fireEvent.change(slider, { target: { value: '61' } });
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ n: 61 }),
    );

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
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ n: 60 }),
    );
  });
});

describe('Clusters page: the header help button follows the active viz', () => {
  // jsdom doesn't implement the native <dialog> API, so showModal()/close()
  // are no-ops there; stub them the same way Dialog.test.tsx does.
  beforeEach(() => {
    HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
      this.removeAttribute('open');
      this.dispatchEvent(new Event('close'));
    };
  });

  it('opens the topic matching each viz type, not a fixed one', async () => {
    const user = userEvent.setup();
    await renderClusters();

    // Only the selected viz carries a "?"; the others show nothing to click.
    expect(
      screen.queryByRole('button', { name: 'Help: Treemap' }),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole('button', { name: 'Help: Cluster graph' }),
    );
    expect(
      screen.getByRole('dialog', { name: 'Cluster graph' }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close dialog' }));

    await user.click(screen.getByText('TREEMAP'));
    await user.click(screen.getByRole('button', { name: 'Help: Treemap' }));
    expect(screen.getByRole('dialog', { name: 'Treemap' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close dialog' }));

    await user.click(screen.getByText('HISTOGRAM'));
    await user.click(
      screen.getByRole('button', { name: 'Help: Cluster distribution' }),
    );
    expect(
      screen.getByRole('dialog', { name: 'Cluster distribution' }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close dialog' }));

    await user.click(screen.getByText('TABLE'));
    await user.click(
      screen.getByRole('button', { name: 'Help: Cluster table' }),
    );
    expect(
      screen.getByRole('dialog', { name: 'Cluster table' }),
    ).toBeInTheDocument();
  });
});

describe('Clusters page: table viz', () => {
  it('renders a row for every cluster in the feed', async () => {
    await renderClustersTable();

    for (const c of clusters) {
      expect(screen.getByText(`#${c.id}`)).toBeInTheDocument();
    }
  });

  it('re-sorts on a header click and pins the sort into the URL', async () => {
    const { user, router } = await renderClustersTable();

    await user.click(screen.getByText('ID'));

    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ k: 'i', d: 'd' }),
    );
  });

  it('sorts by first seen, newest first, and pins the column into the URL', async () => {
    const { user, router } = await renderClustersTable();

    await user.click(screen.getByText('FIRST SEEN'));

    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ k: 's', d: 'd' }),
    );
    expect(rowIds()).toEqual(['#5', '#4', '#3', '#2', '#1']);
  });

  it('filters down to the cluster owning a txid fragment', async () => {
    const { user } = await renderClustersTable();

    await user.type(screen.getByLabelText('Search'), 'a3');

    await waitFor(() => {
      expect(screen.getByText('#3')).toBeInTheDocument();
      expect(screen.queryByText('#1')).not.toBeInTheDocument();
    });
  });

  it('resets the page param when the sort or the query changes', async () => {
    const { user, router } = await renderClustersTable();

    await user.click(screen.getByText('ID'));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ k: 'i', p: 1 }),
    );

    await user.type(screen.getByLabelText('Search'), 'a2');
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({
        q: 'a2',
        p: 1,
      }),
    );
  });

  it('shows the clicked row in the selected panel', async () => {
    const { user } = await renderClustersTable();

    expect(screen.getByText('select a row to inspect')).toBeInTheDocument();

    await user.click(screen.getByText('#4'));

    await waitFor(() =>
      expect(screen.getByTestId('txid-row')).toHaveTextContent('a4'),
    );
    expect(
      screen.queryByText('select a row to inspect'),
    ).not.toBeInTheDocument();
  });
});

describe('Clusters page: the SINGLETONS toggle', () => {
  const EXCLUDE = { name: 'Exclude singleton clusters' } as const;
  const INCLUDE = { name: 'Include singleton clusters' } as const;

  const expectHeaderCount = (n: number) =>
    expect(
      screen.getByText('SHOWING ALL', { exact: false, selector: 'div' }),
    ).toHaveTextContent(`SHOWING ALL ${n} CLUSTERS`);

  /** The five singleton fixtures plus one cluster that holds two txs. */
  async function renderWithAMultiTxCluster() {
    const utils = await renderClustersTable();
    sendDelta([
      {
        id: 6,
        txids: ['f1', 'f2'],
        total_vsize: 400,
        total_fee: 2000,
        ...seenAt(6),
      },
    ]);
    await screen.findByText('#6');
    return utils;
  }

  it('includes singletons by default', async () => {
    await renderWithAMultiTxCluster();

    expect(screen.getByRole('button', EXCLUDE)).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expectHeaderCount(6);
  });

  it('drops single-tx clusters from the table and the header count when off', async () => {
    const { user } = await renderWithAMultiTxCluster();

    await user.click(screen.getByRole('button', EXCLUDE));

    await waitFor(() => {
      expectHeaderCount(1);
      expect(screen.queryByText('#1')).not.toBeInTheDocument();
    });
    expect(screen.getByText('#6')).toBeInTheDocument();
  });

  it('pins the choice into the URL', async () => {
    const { user, router } = await renderWithAMultiTxCluster();

    await user.click(screen.getByRole('button', EXCLUDE));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ g: 0 }),
    );
  });

  it('opens with singletons hidden when the URL says so', async () => {
    await renderClusters(`${WebRoutes.clusters}?v=b&g=0`);

    expect(await screen.findByRole('button', INCLUDE)).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    // Every fixture is a singleton, so hiding them empties the feed.
    expectHeaderCount(0);
    expect(screen.queryByText('#1')).not.toBeInTheDocument();
  });
});

/** The cluster ids of the table's body rows, in the order they are rendered. */
function rowIds(): string[] {
  return screen
    .getAllByRole('row')
    .slice(1)
    .map((row) => within(row).getByText(/^#\d+$/).textContent ?? '');
}

async function openClustersTable() {
  const user = userEvent.setup();
  const utils = await renderClusters(`${WebRoutes.clusters}?v=b`);
  await screen.findByText('#1');
  return { user, ...utils };
}

/** Every /transactions request so far, as the txid batch each one asked for. */
function requestedBatches(fetchMock: Mock): string[][] {
  return fetchMock.mock.calls.map(([url]) => requestedTxids(url as string));
}

/**
 * Answers each lookup with a complete row per requested txid -- an incomplete
 * one (hollow, or no input_txids) is deliberately retried by the cache, which
 * would confuse "did revisiting a cluster refetch it?".
 */
function stubTxLookupFetch(): Mock {
  const fetchMock = vi.fn((url: string) =>
    Promise.resolve(
      lookupResponse({
        found: requestedTxids(url).map((txid) => tx(txid)),
        missing: [],
      }),
    ),
  );
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

describe('Clusters page: revisiting a cluster reuses its cached transactions', () => {
  it('fetches each cluster once, however often the selection comes back to it', async () => {
    const fetchMock = stubTxLookupFetch();
    const { user } = await openClustersTable();

    // Nothing is selected on arrival, so nothing has been asked for yet.
    expect(fetchMock).not.toHaveBeenCalled();

    await user.click(screen.getByText('#1'));
    await waitFor(() => expect(requestedBatches(fetchMock)).toEqual([['a1']]));

    await user.click(screen.getByText('#2'));
    await waitFor(() =>
      expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2']]),
    );

    await user.click(screen.getByText('#3'));
    await waitFor(() =>
      expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2'], ['a3']]),
    );

    // Back over the same three, twice around: every txid is already cached, so
    // the round trip must cost nothing.
    for (const id of ['#1', '#2', '#3', '#1', '#2', '#3']) {
      await user.click(screen.getByText(id));
    }

    // The panel really is showing #3 again -- the clicks landed, they just
    // didn't fetch.
    await waitFor(() =>
      expect(screen.getByTestId('txid-row')).toHaveTextContent('a3'),
    );
    expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2'], ['a3']]);
  });

  it('fetches only the transaction that arrived while the cluster was away', async () => {
    const fetchMock = stubTxLookupFetch();
    const { user } = await openClustersTable();

    await user.click(screen.getByText('#1'));
    await waitFor(() => expect(requestedBatches(fetchMock)).toEqual([['a1']]));

    await user.click(screen.getByText('#2'));
    await waitFor(() =>
      expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2']]),
    );

    // A cluster delta adds a transaction to #1 while #2 is the selected one.
    sendDelta([
      {
        ...clusters[0],
        txids: ['a1', 'a1b'],
        total_vsize: 320,
        total_fee: 1800,
      },
    ]);

    // The delta alone changes nothing for the cluster on screen.
    expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2']]);

    await user.click(screen.getByText('#1'));

    // Only the newly arrived txid is asked for; the cached 'a1' is reused.
    await waitFor(() =>
      expect(requestedBatches(fetchMock)).toEqual([['a1'], ['a2'], ['a1b']]),
    );
    await waitFor(() =>
      expect(screen.getAllByTestId('txid-row')).toHaveLength(2),
    );
  });
});

describe('Clusters page: the DAG panel caption is fixed, not page-selectable', () => {
  it('stays on vsize/fee-rate when SIZE/COLOR BY changes', async () => {
    const { user } = await renderClustersTable();
    await user.click(screen.getByText('#1'));

    await user.click(screen.getByText('TREEMAP'));

    const caption = 'SIZE vsize · COLOR fee-rate';
    await waitFor(() =>
      expect(
        screen.getByText((_content, el) => el?.textContent === caption),
      ).toBeInTheDocument(),
    );

    await user.selectOptions(screen.getByLabelText('SIZE/COLOR BY'), 'txs');

    expect(
      screen.getByText((_content, el) => el?.textContent === caption),
    ).toBeInTheDocument();
  });
});
