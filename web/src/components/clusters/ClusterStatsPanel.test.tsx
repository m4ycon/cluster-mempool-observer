import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { GLOSSARY } from '../../lib/glossary';
import { Dialog } from '../dialog/Dialog';
import { ClusterStatsPanel } from './ClusterStatsPanel';

const STATS = {
  count: 5,
  totalVsize: 3100,
  totalFee: 156500,
  min: 5,
  median: 20,
  p90: 80,
  max: 96,
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

function byFullText(text: string) {
  return (_content: string, element: Element | null) =>
    element?.textContent === text;
}

describe('ClusterStatsPanel', () => {
  it('keeps the TOTAL VSIZE and P90 labels intact despite splitting them across glossary links, and leaves MIN/MEDIAN/MAX alone', () => {
    render(<ClusterStatsPanel stats={STATS} metric="feerate" />);

    expect(screen.getByText(byFullText('TOTAL VSIZE'))).toBeInTheDocument();
    expect(screen.getByText(byFullText('P90 FEE-RATE'))).toBeInTheDocument();
    expect(screen.getByText('MIN FEE-RATE')).toBeInTheDocument();
    expect(screen.getByText('MEDIAN FEE-RATE')).toBeInTheDocument();
    expect(screen.getByText('MAX FEE-RATE')).toBeInTheDocument();
  });

  it('opens the vsize definition when the dotted VSIZE label is clicked', async () => {
    render(
      <>
        <ClusterStatsPanel stats={STATS} metric="feerate" />
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

  it('opens the p90 definition when the dotted P90 label is clicked', async () => {
    render(
      <>
        <ClusterStatsPanel stats={STATS} metric="feerate" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Definition: p90' }));
    });

    expect(
      screen.getByRole('dialog', { name: GLOSSARY.p90.label }),
    ).toBeInTheDocument();
  });
});
