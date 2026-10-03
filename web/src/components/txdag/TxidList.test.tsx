import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ExplorerRoutes } from '../../lib/routes';
import { TxidList } from './TxidList';

const TXID = '8e1fe92e3bb2925fb39ccee3a4e543bfaa79d5b72e42d7af05cb33f4658d2de7';

describe('TxidList compact', () => {
  it('shows both ends of the txid but links, titles and copies the full one', async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    render(
      <TxidList
        txids={[TXID]}
        selectedTxid={null}
        onSelectTxid={() => {}}
        compact
      />,
    );

    const link = screen.getByRole('link');
    expect(link).toHaveTextContent(/^8e1fe92e…658d2de7$/);
    expect(link).toHaveAttribute('href', ExplorerRoutes.tx(TXID));
    expect(link).toHaveAttribute('title', TXID);

    await user.click(screen.getByLabelText('Copy txid'));
    expect(writeText).toHaveBeenCalledWith(TXID);
  });

  it('shows the full txid by default', () => {
    render(
      <TxidList txids={[TXID]} selectedTxid={null} onSelectTxid={() => {}} />,
    );

    expect(screen.getByRole('link')).toHaveTextContent(TXID);
  });
});
