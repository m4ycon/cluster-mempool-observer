import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { Column, SortState } from '../../lib/dataTable';
import { DataTable } from './DataTable';

interface Row {
  id: number;
  name: string;
  amount: number;
  secret: string;
}

type RowKey = 'name' | 'amount';

const columns: readonly Column<Row, RowKey>[] = [
  { key: 'name', label: 'Name', value: (r) => r.name, searchable: true },
  {
    key: 'amount',
    label: 'Amount',
    value: (r) => r.amount,
    align: 'right',
    searchable: true,
  },
];

function makeRows(count: number): Row[] {
  return Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    name: `row-${i + 1}`,
    amount: (i + 1) * 10,
    secret: `hidden-token-${i + 1}`,
  }));
}

/** Stateful wrapper: owns sort/page/query/selection, exactly what a page would. */
function ControlledTable({
  rows,
  pageSize = 2,
  onSelect,
}: {
  rows: Row[];
  pageSize?: number;
  onSelect?: (row: Row) => void;
}) {
  const [sort, setSort] = useState<SortState<RowKey>>({
    key: 'amount',
    dir: 'desc',
  });
  const [page, setPage] = useState(1);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<number | null>(null);

  return (
    <div>
      <div data-testid="page-state">{page}</div>
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(r) => r.id}
        sort={sort}
        onSortChange={setSort}
        page={page}
        onPageChange={setPage}
        query={query}
        onQueryChange={setQuery}
        pageSize={pageSize}
        searchExtra={(r) => [r.secret]}
        selectedKey={selectedKey}
        onSelect={(row) => {
          setSelectedKey(row.id);
          onSelect?.(row);
        }}
        emptyLabel="awaiting feed..."
      />
    </div>
  );
}

function nameCells() {
  return screen
    .getAllByRole('row')
    .slice(1) // drop header row
    .map((row) => row.querySelector('td')?.textContent);
}

describe('DataTable sorting', () => {
  it('sorts a newly clicked column descending, and toggles direction on the active column', async () => {
    const user = userEvent.setup();
    render(<ControlledTable rows={makeRows(3)} pageSize={10} />);

    // Default sort is amount desc: row-3 (30) first.
    expect(nameCells()).toEqual(['row-3', 'row-2', 'row-1']);

    await user.click(screen.getByRole('button', { name: /^Name/ }));
    expect(nameCells()).toEqual(['row-3', 'row-2', 'row-1']);
    expect(screen.getByRole('columnheader', { name: /Name/ })).toHaveAttribute(
      'aria-sort',
      'descending',
    );
    expect(
      screen.getByRole('columnheader', { name: /Amount/ }),
    ).toHaveAttribute('aria-sort', 'none');

    await user.click(screen.getByRole('button', { name: /^Name/ }));
    expect(nameCells()).toEqual(['row-1', 'row-2', 'row-3']);
    expect(screen.getByRole('columnheader', { name: /Name/ })).toHaveAttribute(
      'aria-sort',
      'ascending',
    );
  });
});

describe('DataTable search', () => {
  it('filters visible rows by typed text and finds a row through a hidden searchExtra string', async () => {
    render(<ControlledTable rows={makeRows(5)} pageSize={10} />);

    const input = screen.getByRole('textbox', { name: 'Search' });
    fireEvent.change(input, { target: { value: 'row-2' } });

    await waitFor(() => expect(nameCells()).toEqual(['row-2']));

    fireEvent.change(input, { target: { value: 'hidden-token-4' } });
    await waitFor(() => expect(nameCells()).toEqual(['row-4']));
  });

  it('searches only the columns opted in, since `searchable` is off by default', async () => {
    const optedOut: readonly Column<Row, RowKey>[] = [
      columns[0],
      { ...columns[1], searchable: undefined },
    ];
    render(
      <DataTable
        columns={optedOut}
        rows={makeRows(5)}
        rowKey={(r) => r.id}
        sort={{ key: 'amount', dir: 'desc' }}
        onSortChange={vi.fn()}
        page={1}
        onPageChange={vi.fn()}
        query="30"
        onQueryChange={vi.fn()}
        emptyLabel="awaiting feed..."
      />,
    );

    // "30" is row-3's amount, but that column no longer answers the query.
    expect(screen.getByText('no rows match')).toBeInTheDocument();
  });

  it('shows emptyLabel with no rows, and noMatchLabel when a query matches nothing', async () => {
    const { rerender } = render(<ControlledTable rows={[]} />);
    expect(screen.getByText('awaiting feed...')).toBeInTheDocument();

    rerender(<ControlledTable rows={makeRows(3)} />);
    const input = screen.getByRole('textbox', { name: 'Search' });
    fireEvent.change(input, { target: { value: 'nothing-matches-this' } });

    await waitFor(() =>
      expect(screen.getByText('no rows match')).toBeInTheDocument(),
    );
  });
});

describe('DataTable selection', () => {
  it('selects a row on click, marking it aria-selected', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <ControlledTable rows={makeRows(3)} pageSize={10} onSelect={onSelect} />,
    );

    const row = screen.getByText('row-2').closest('tr');
    if (!row) throw new Error('row not found');
    await user.click(row);

    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'row-2' }),
    );
    expect(row).toHaveAttribute('aria-selected', 'true');
  });

  it('selects a row via keyboard (Enter and Space)', async () => {
    const onSelect = vi.fn();
    render(
      <ControlledTable rows={makeRows(3)} pageSize={10} onSelect={onSelect} />,
    );

    const row = screen.getByText('row-1').closest('tr');
    if (!row) throw new Error('row not found');
    row.focus();
    fireEvent.keyDown(row, { key: 'Enter' });

    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'row-1' }),
    );

    fireEvent.keyDown(row, { key: ' ' });
    expect(onSelect).toHaveBeenCalledTimes(2);
  });

  it('is inert (no tabIndex) when onSelect is not given', () => {
    render(
      <DataTable
        columns={columns}
        rows={makeRows(2)}
        rowKey={(r) => r.id}
        sort={{ key: 'amount', dir: 'desc' }}
        onSortChange={vi.fn()}
        page={1}
        onPageChange={vi.fn()}
        query=""
        onQueryChange={vi.fn()}
        emptyLabel="awaiting feed..."
      />,
    );

    const row = screen.getByText('row-1').closest('tr');
    expect(row).not.toHaveAttribute('tabindex');
  });
});

describe('DataTable row count', () => {
  it('reads TOTAL when nothing is filtered out, and stays visible once a query narrows to a single page', async () => {
    render(<ControlledTable rows={makeRows(5)} pageSize={10} />);

    expect(screen.getByText(/TOTAL/)).toBeInTheDocument();

    fireEvent.change(screen.getByRole('textbox', { name: 'Search' }), {
      target: { value: 'row-2' },
    });

    await waitFor(() => expect(screen.queryByText(/TOTAL/)).toBeNull());
    expect(screen.getByText(/OF/)).toBeInTheDocument();
  });
});

describe('DataTable pagination healing', () => {
  it('reports a healed page upward when the row set shrinks under the current page', async () => {
    const many = makeRows(6); // pageSize 2 -> 3 pages
    const { rerender, getByTestId, getByLabelText } = render(
      <ControlledTable rows={many} />,
    );

    fireEvent.click(getByLabelText('Next page'));
    fireEvent.click(getByLabelText('Next page'));
    expect(getByTestId('page-state')).toHaveTextContent('3');

    rerender(<ControlledTable rows={many.slice(0, 2)} />); // now only 1 page

    await waitFor(() =>
      expect(getByTestId('page-state')).toHaveTextContent('1'),
    );
  });
});
