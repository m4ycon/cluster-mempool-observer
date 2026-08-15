import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { GLOSSARY } from '../../lib/glossary';
import type { ClusterRef } from '../../types/events';
import { Dialog } from '../dialog/Dialog';
import { SelectedClusterPanel } from './SelectedClusterPanel';

const CLUSTER: ClusterRef = {
  id: 1,
  txids: ['a1', 'a2'],
  total_vsize: 200,
  total_fee: 1000,
};

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
});
