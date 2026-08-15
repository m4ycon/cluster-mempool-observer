import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { Dialog } from './Dialog';

// jsdom doesn't implement the native <dialog> API, so showModal()/close() are
// no-ops there. Stub them: showModal() marks it open, close() fires the
// native 'close' event our component listens on -- the same event a real
// browser fires after Esc runs its default close action.
beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute('open');
    this.dispatchEvent(new Event('close'));
  };
});

describe('Dialog', () => {
  it('renders the title and body of a call', async () => {
    render(<Dialog />);

    await act(async () => {
      Dialog.call({ title: 'VSIZE', body: <p>Virtual size explainer.</p> });
    });

    expect(screen.getByRole('dialog', { name: 'VSIZE' })).toBeInTheDocument();
    expect(screen.getByText('Virtual size explainer.')).toBeInTheDocument();
  });

  it('ends the call and unmounts when the close button is clicked', async () => {
    render(<Dialog />);
    let resolved = false;

    await act(async () => {
      Dialog.call({ title: 'WU', body: <p>Weight units.</p> }).then(() => {
        resolved = true;
      });
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Close dialog' }));

    await waitFor(() =>
      expect(
        screen.queryByRole('dialog', { name: 'WU' }),
      ).not.toBeInTheDocument(),
    );
    expect(resolved).toBe(true);
  });

  it('ends the call when Esc closes the dialog', async () => {
    render(<Dialog />);
    let resolved = false;

    await act(async () => {
      Dialog.call({ title: 'FEE RATE', body: <p>Sat/vB info.</p> }).then(() => {
        resolved = true;
      });
    });

    const dialog = screen.getByRole('dialog', { name: 'FEE RATE' });
    await act(async () => {
      dialog.dispatchEvent(new Event('close'));
    });

    await waitFor(() =>
      expect(
        screen.queryByRole('dialog', { name: 'FEE RATE' }),
      ).not.toBeInTheDocument(),
    );
    expect(resolved).toBe(true);
  });

  it('stacks two calls and closes only the top one', async () => {
    render(<Dialog />);

    await act(async () => {
      Dialog.call({ title: 'BOTTOM', body: <p>First.</p> });
    });
    await act(async () => {
      Dialog.call({ title: 'TOP', body: <p>Second.</p> });
    });

    expect(screen.getByRole('dialog', { name: 'BOTTOM' })).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: 'TOP' })).toBeInTheDocument();

    const user = userEvent.setup();
    const top = screen.getByRole('dialog', { name: 'TOP' });
    await user.click(within(top).getByRole('button', { name: 'Close dialog' }));

    await waitFor(() =>
      expect(
        screen.queryByRole('dialog', { name: 'TOP' }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('dialog', { name: 'BOTTOM' })).toBeInTheDocument();
  });
});
