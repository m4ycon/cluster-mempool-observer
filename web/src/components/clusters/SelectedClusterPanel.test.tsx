import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import dayjs from '../../lib/dayjs';
import { GLOSSARY } from '../../lib/glossary';
import type {
  ClusterRef,
  TransactionLookup,
  TransactionRef,
} from '../../types/events';
import { Dialog } from '../dialog/Dialog';
import { SelectedClusterPanel } from './SelectedClusterPanel';

const CLUSTER: ClusterRef = {
  id: 1,
  txids: ['a1', 'a2'],
  total_vsize: 200,
  total_fee: 1000,
  first_seen_at: '2026-01-01T00:00:00.000Z',
};

function tx(
  txid: string,
  overrides: Partial<TransactionRef> = {},
): TransactionRef {
  return {
    txid,
    fee: 100,
    vsize: 200,
    first_seen_at: '2026-01-01T00:00:00.000Z',
    cluster_id: 1,
    hollow: false,
    input_txids: [],
    ...overrides,
  };
}

function lookupResponse(lookup: TransactionLookup): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(lookup),
  } as Response;
}

/** Stubs fetch so the panel's useTransactionCache resolves both cluster txids. */
function stubTransactionFetch(
  lookup: TransactionLookup = {
    found: [tx('a1'), tx('a2')],
    missing: [],
  },
) {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(lookupResponse(lookup)));
}

beforeEach(() => {
  // jsdom doesn't implement the native <dialog> API, so showModal()/close()
  // are no-ops there. Stub them the same way Dialog.test.tsx does.
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute('open');
    this.dispatchEvent(new Event('close'));
  };
  // jsdom has no layout engine, so scrollIntoView is unimplemented.
  Element.prototype.scrollIntoView = vi.fn();
  // The panel owns the transaction cache, so it fetches on every render.
  stubTransactionFetch();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// The VSIZE word is its own glossary-link element, so the label's visible
// text is split across nodes; match on an element's full aggregated text
// content rather than plain getByText, which only reads direct text nodes.
function byFullText(text: string) {
  return (_content: string, element: Element | null) =>
    element?.textContent === text;
}

describe('SelectedClusterPanel', () => {
  it('keeps the TOTAL VSIZE label intact despite splitting it across a glossary link', () => {
    render(<SelectedClusterPanel cluster={CLUSTER} />);

    expect(screen.getByText(byFullText('TOTAL VSIZE'))).toBeInTheDocument();
  });

  it('opens the vsize definition when the dotted VSIZE label is clicked', async () => {
    render(
      <>
        <SelectedClusterPanel cluster={CLUSTER} />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByRole('button', { name: 'Definition: vsize' }),
      );
    });

    expect(
      screen.getByRole('dialog', { name: GLOSSARY.vsize.label }),
    ).toBeInTheDocument();
  });

  it('reads FIRST SEEN as an age, keeping the wall-clock time in the tooltip', () => {
    render(<SelectedClusterPanel cluster={CLUSTER} />);

    const wallClock = dayjs(CLUSTER.first_seen_at).format(
      'YYYY-MM-DD HH:mm:ss',
    );
    const age = dayjs(CLUSTER.first_seen_at).fromNow(true);

    expect(screen.getByText('FIRST SEEN')).toBeInTheDocument();

    const bubble = screen.getByRole('tooltip');
    expect(bubble).toHaveTextContent(wallClock);
    expect(bubble.parentElement).toHaveTextContent(`${age} AGO`);
  });

  it('renders the inline transaction graph with one node per txid', async () => {
    render(<SelectedClusterPanel cluster={CLUSTER} />);

    expect(await screen.findByTestId('tx-dag')).toHaveAttribute(
      'data-node-count',
      '2',
    );
  });

  it('outlines and scrolls to the TXIDS row matching a clicked graph node', async () => {
    render(<SelectedClusterPanel cluster={CLUSTER} />);

    await screen.findByTestId('tx-dag');
    const node = screen
      .getAllByTestId('tx-dag-node')
      .find((n) => n.getAttribute('data-txid') === 'a2');
    if (!node) throw new Error('expected a graph node for a2');
    fireEvent.click(node);

    const row = screen
      .getAllByTestId('txid-row')
      .find((r) => r.getAttribute('data-txid') === 'a2');
    expect(row).toHaveAttribute('data-selected', 'true');
    expect(Element.prototype.scrollIntoView).toHaveBeenCalled();
  });

  it('opens the transaction graph dialog when EXPAND is clicked', async () => {
    render(
      <>
        <SelectedClusterPanel cluster={CLUSTER} />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', { name: 'view transaction graph' }),
    );

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByTestId('tx-dag')).toBeInTheDocument();
  });

  it('hides EXPAND while nothing is cached yet', () => {
    vi.stubGlobal('fetch', vi.fn().mockReturnValue(new Promise(() => {})));

    render(<SelectedClusterPanel cluster={CLUSTER} />);

    expect(
      screen.queryByRole('button', { name: 'view transaction graph' }),
    ).not.toBeInTheDocument();
  });

  it('outlines a TXIDS row when clicked, without copying it', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);

    render(<SelectedClusterPanel cluster={CLUSTER} />);

    const user = userEvent.setup();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    const row = screen
      .getAllByTestId('txid-row')
      .find((r) => r.getAttribute('data-txid') === 'a1');
    if (!row) throw new Error('expected a TXIDS row for a1');

    await user.click(row);

    expect(row).toHaveAttribute('data-selected', 'true');
    expect(writeText).not.toHaveBeenCalled();
  });

  it('links each txid to its mempool.space page, still selecting the row on click', async () => {
    render(<SelectedClusterPanel cluster={CLUSTER} />);

    const link = screen.getByRole('link', { name: 'a1' });
    expect(link).toHaveAttribute('href', 'https://mempool.space/tx/a1');
    expect(link).toHaveAttribute('target', '_blank');

    // jsdom has no navigation, so the click only exercises the row handler.
    await userEvent.setup().click(link);

    const row = screen
      .getAllByTestId('txid-row')
      .find((r) => r.getAttribute('data-txid') === 'a1');
    expect(row).toHaveAttribute('data-selected', 'true');
  });

  it('copies a txid to the clipboard when its row copy button is clicked, without changing selection', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);

    render(<SelectedClusterPanel cluster={CLUSTER} />);

    const user = userEvent.setup();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    await user.click(screen.getAllByLabelText('Copy txid')[0]);

    expect(writeText).toHaveBeenCalledWith('a1');
    await waitFor(() =>
      expect(screen.getByLabelText('Copied')).toBeInTheDocument(),
    );
    const row = screen
      .getAllByTestId('txid-row')
      .find((r) => r.getAttribute('data-txid') === 'a1');
    expect(row).toHaveAttribute('data-selected', 'false');
  });
});
