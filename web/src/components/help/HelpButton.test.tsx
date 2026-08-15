import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { GLOSSARY } from '../../lib/glossary';
import { Dialog } from '../dialog/Dialog';
import { HelpButton } from './HelpButton';
import { PANEL_HELP } from './panelHelp';

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

describe('HelpButton', () => {
  it('opens a dialog showing the topic title and body on click', async () => {
    render(
      <>
        <HelpButton topic="controls.bins" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Help: Bins' }));
    });

    const dialog = screen.getByRole('dialog', { name: 'Bins' });
    expect(dialog).toBeInTheDocument();
    // The body text may now be split across an inline link, so check the
    // dialog's full text content rather than a single exact text node.
    expect(dialog).toHaveTextContent(
      PANEL_HELP['controls.bins'].body as string,
    );
  });

  it('links a term the body mentions inline, and stacks its definition on click', async () => {
    render(
      <>
        <HelpButton topic="clusters.histogram" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByRole('button', { name: 'Help: Cluster distribution' }),
      );
    });

    const link = screen.getByRole('button', { name: 'Definition: cluster' });
    expect(link).toBeInTheDocument();

    await act(async () => {
      await user.click(link);
    });

    expect(
      screen.getByRole('dialog', { name: 'Cluster distribution' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('dialog', { name: GLOSSARY.cluster.label }),
    ).toBeInTheDocument();
    expect(screen.getByText(GLOSSARY.cluster.text)).toBeInTheDocument();
  });

  it('keeps a term in "See also" when the body never mentions it, and drops one it does', async () => {
    render(
      <>
        <HelpButton topic="clusters.histogram" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByRole('button', { name: 'Help: Cluster distribution' }),
      );
    });

    // "cluster" is mentioned in the body (linked there instead), "bin" is not.
    const footer = screen.getByText('See also').parentElement as HTMLElement;
    expect(
      within(footer).getByRole('button', { name: 'Definition: bin' }),
    ).toBeInTheDocument();
    expect(
      within(footer).queryByRole('button', { name: 'Definition: cluster' }),
    ).not.toBeInTheDocument();
  });

  it('renders no "See also" footer when every listed term already got linked in the body', async () => {
    render(
      <>
        <HelpButton topic="clusters.circles" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByRole('button', { name: 'Help: Cluster graph' }),
      );
    });

    expect(screen.queryByText('See also')).not.toBeInTheDocument();
    // The only related term ("cluster") is still reachable, just inline in the body.
    expect(
      screen.getByRole('button', { name: 'Definition: cluster' }),
    ).toBeInTheDocument();
  });
});
