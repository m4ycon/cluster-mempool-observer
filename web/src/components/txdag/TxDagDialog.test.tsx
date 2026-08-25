import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import type { ClusterRef, TransactionRef } from '../../types/events';
import { Dialog } from '../dialog/Dialog';
import { openTxDagDialog } from './TxDagDialog';

const CLUSTER: ClusterRef = {
  id: 7,
  txids: ['b1', 'b2'],
  total_vsize: 400,
  total_fee: 2000,
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
    cluster_id: 7,
    hollow: false,
    input_txids: [],
    ...overrides,
  };
}

// jsdom doesn't implement the native <dialog> API, so showModal()/close() are
// no-ops there. Stub them the same way Dialog.test.tsx does.
beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute('open');
    this.dispatchEvent(new Event('close'));
  };
});

describe('TxDagDialog', () => {
  it('renders the graph from the transaction cache it is handed, with no fetch of its own', async () => {
    render(<Dialog />);

    await act(async () => {
      openTxDagDialog(CLUSTER, {
        txs: new Map([
          ['b1', tx('b1')],
          ['b2', tx('b2', { input_txids: ['b1'] })],
        ]),
        missing: new Set(),
        loading: false,
        error: null,
      });
    });

    const dag = await screen.findByTestId('tx-dag');
    expect(dag).toHaveAttribute('data-node-count', '2');
  });
});
