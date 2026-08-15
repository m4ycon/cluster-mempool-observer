import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { GLOSSARY } from '../../lib/glossary';
import { Dialog } from '../dialog/Dialog';
import { Term } from './Term';

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

describe('Term', () => {
  it('renders children when given', () => {
    render(<Term term="weight">WU</Term>);

    expect(
      screen.getByRole('button', { name: 'Definition: weight (WU)' }),
    ).toHaveTextContent('WU');
  });

  it('falls back to the label when no children are given', () => {
    render(<Term term="cluster" />);

    expect(
      screen.getByRole('button', { name: 'Definition: cluster' }),
    ).toHaveTextContent('cluster');
  });

  it('opens the dialog with the term label and text on click', async () => {
    render(
      <>
        <Term term="p90" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Definition: p90' }));
    });

    const dialog = screen.getByRole('dialog', { name: GLOSSARY.p90.label });
    expect(dialog).toBeInTheDocument();
    // p90's own text mentions "clusters", which now becomes an inline link,
    // so check the dialog's full text content rather than a single text node.
    expect(dialog).toHaveTextContent(GLOSSARY.p90.text);
  });

  it('never links a definition to its own term, but does link a different term it mentions', async () => {
    render(
      <>
        <Term term="weight" />
        <Dialog />
      </>,
    );

    const user = userEvent.setup();
    await act(async () => {
      await user.click(
        screen.getByRole('button', { name: 'Definition: weight (WU)' }),
      );
    });

    const dialog = screen.getByRole('dialog', { name: GLOSSARY.weight.label });
    // "weight" (and its alias "WU") appear in the entry's own text, but must
    // never link back to the dialog that is already showing weight's definition.
    expect(
      within(dialog).queryByRole('button', { name: 'Definition: weight (WU)' }),
    ).not.toBeInTheDocument();

    // The text also mentions vsize (via its "vB" alias), a different term, which does link.
    const vsizeLink = within(dialog).getByRole('button', {
      name: 'Definition: vsize',
    });
    expect(vsizeLink).toHaveTextContent('vB');

    await act(async () => {
      await user.click(vsizeLink);
    });
    expect(
      screen.getByRole('dialog', { name: GLOSSARY.vsize.label }),
    ).toBeInTheDocument();
  });
});
