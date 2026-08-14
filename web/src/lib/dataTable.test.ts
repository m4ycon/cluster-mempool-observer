import { describe, expect, it } from 'vitest';
import {
  type Column,
  clampPage,
  filterRows,
  pageCount,
  type SortState,
  sortRows,
  tableView,
} from './dataTable';

interface Row {
  id: number;
  name: string;
  amount: number;
}

function row(overrides: Partial<Row> = {}): Row {
  return { id: 1, name: 'alice', amount: 1000, ...overrides };
}

const COLUMNS: Column<Row, 'id' | 'name' | 'amount'>[] = [
  { key: 'id', label: 'ID', value: (r) => r.id, sortable: true },
  {
    key: 'name',
    label: 'Name',
    value: (r) => r.name,
    sortable: true,
    searchable: true,
  },
  {
    key: 'amount',
    label: 'Amount',
    value: (r) => r.amount,
    text: (r) => r.amount.toLocaleString('en-US'),
    align: 'right',
    sortable: true,
    searchable: true,
  },
];

describe('filterRows', () => {
  it('matches on display text', () => {
    const rows = [row({ id: 1, amount: 1234 }), row({ id: 2, amount: 5 })];
    const result = filterRows(rows, COLUMNS, '1,234');
    expect(result.map((r) => r.id)).toEqual([1]);
  });

  it('matches on the raw unformatted value even when display text differs', () => {
    const rows = [row({ id: 1, amount: 1234 }), row({ id: 2, amount: 5 })];
    const result = filterRows(rows, COLUMNS, '1234');
    expect(result.map((r) => r.id)).toEqual([1]);
  });

  it('is case-insensitive', () => {
    const rows = [row({ id: 1, name: 'Alice' }), row({ id: 2, name: 'Bob' })];
    const result = filterRows(rows, COLUMNS, 'ALICE');
    expect(result.map((r) => r.id)).toEqual([1]);
  });

  it('matches via searchExtra strings not shown in any column', () => {
    const rows = [row({ id: 1 }), row({ id: 2 })];
    const searchExtra = (r: Row) => (r.id === 1 ? ['deadbeef'] : ['cafef00d']);
    const result = filterRows(rows, COLUMNS, 'deadbeef', searchExtra);
    expect(result.map((r) => r.id)).toEqual([1]);
  });

  it('empty query matches everything', () => {
    const rows = [row({ id: 1 }), row({ id: 2 })];
    expect(filterRows(rows, COLUMNS, '')).toHaveLength(2);
  });

  it('whitespace-only query matches everything', () => {
    const rows = [row({ id: 1 }), row({ id: 2 })];
    expect(filterRows(rows, COLUMNS, '   ')).toHaveLength(2);
  });

  it('undefined query matches everything', () => {
    const rows = [row({ id: 1 }), row({ id: 2 })];
    expect(filterRows(rows, COLUMNS, undefined)).toHaveLength(2);
  });

  it('does not mutate the input array', () => {
    const rows = [row({ id: 2 }), row({ id: 1 })];
    const snapshot = [...rows];
    filterRows(rows, COLUMNS, 'alice');
    expect(rows).toEqual(snapshot);
  });
});

describe('sortRows', () => {
  it('sorts ascending by a numeric column', () => {
    const rows = [row({ id: 3 }), row({ id: 1 }), row({ id: 2 })];
    const sort: SortState<'id' | 'name' | 'amount'> = { key: 'id', dir: 'asc' };
    expect(sortRows(rows, COLUMNS, sort).map((r) => r.id)).toEqual([1, 2, 3]);
  });

  it('sorts descending by a numeric column', () => {
    const rows = [row({ id: 3 }), row({ id: 1 }), row({ id: 2 })];
    const sort: SortState<'id' | 'name' | 'amount'> = {
      key: 'id',
      dir: 'desc',
    };
    expect(sortRows(rows, COLUMNS, sort).map((r) => r.id)).toEqual([3, 2, 1]);
  });

  it('sorts strings with localeCompare', () => {
    const rows = [row({ id: 1, name: 'bob' }), row({ id: 2, name: 'alice' })];
    const sort: SortState<'id' | 'name' | 'amount'> = {
      key: 'name',
      dir: 'asc',
    };
    expect(sortRows(rows, COLUMNS, sort).map((r) => r.name)).toEqual([
      'alice',
      'bob',
    ]);
  });

  it('is stable: equal keys keep their original relative order', () => {
    const rows = [
      row({ id: 1, name: 'x', amount: 5 }),
      row({ id: 2, name: 'x', amount: 5 }),
      row({ id: 3, name: 'x', amount: 5 }),
    ];
    const sort: SortState<'id' | 'name' | 'amount'> = {
      key: 'name',
      dir: 'asc',
    };
    expect(sortRows(rows, COLUMNS, sort).map((r) => r.id)).toEqual([1, 2, 3]);
  });

  it('leaves order untouched for a sort key not present in columns', () => {
    const rows = [row({ id: 3 }), row({ id: 1 }), row({ id: 2 })];
    const sort = { key: 'nope', dir: 'asc' } as unknown as SortState<
      'id' | 'name' | 'amount'
    >;
    expect(sortRows(rows, COLUMNS, sort).map((r) => r.id)).toEqual([3, 1, 2]);
  });

  it('does not mutate the input array', () => {
    const rows = [row({ id: 3 }), row({ id: 1 }), row({ id: 2 })];
    const snapshot = [...rows];
    sortRows(rows, COLUMNS, { key: 'id', dir: 'asc' });
    expect(rows).toEqual(snapshot);
  });
});

describe('pageCount', () => {
  it('is 1 for zero rows', () => {
    expect(pageCount(0, 10)).toBe(1);
  });

  it('ceils the division', () => {
    expect(pageCount(21, 10)).toBe(3);
    expect(pageCount(20, 10)).toBe(2);
  });

  it('treats a zero pageSize as one page containing everything', () => {
    expect(pageCount(50, 0)).toBe(1);
  });

  it('treats a negative pageSize as one page containing everything', () => {
    expect(pageCount(50, -5)).toBe(1);
  });

  it('treats a non-finite pageSize as one page containing everything', () => {
    expect(pageCount(50, Number.NaN)).toBe(1);
    expect(pageCount(50, Number.POSITIVE_INFINITY)).toBe(1);
  });
});

describe('clampPage', () => {
  it('keeps an in-range page unchanged', () => {
    expect(clampPage(2, 30, 10)).toBe(2);
  });

  it('clamps a page beyond the (shrunk) row set down to the last page', () => {
    // e.g. a live feed delta, or a typed query, shrinks matched from under page 5.
    expect(clampPage(5, 12, 10)).toBe(2);
  });

  it('clamps below 1 up to 1', () => {
    expect(clampPage(0, 30, 10)).toBe(1);
    expect(clampPage(-3, 30, 10)).toBe(1);
  });

  it('floors a fractional page', () => {
    expect(clampPage(1.9, 30, 10)).toBe(1);
  });

  it('heals a non-finite page to 1', () => {
    expect(clampPage(Number.NaN, 30, 10)).toBe(1);
  });

  it('is always 1 for zero rows', () => {
    expect(clampPage(5, 0, 10)).toBe(1);
  });
});

describe('tableView', () => {
  function rows10(): Row[] {
    return Array.from({ length: 10 }, (_, i) =>
      row({ id: i + 1, name: `row-${i + 1}`, amount: i + 1 }),
    );
  }

  it('runs the full filter -> sort -> clamp -> slice pipeline', () => {
    const view = tableView({
      rows: rows10(),
      columns: COLUMNS,
      sort: { key: 'id', dir: 'desc' },
      page: 1,
      pageSize: 3,
    });
    expect(view.rows.map((r) => r.id)).toEqual([10, 9, 8]);
    expect(view.page).toBe(1);
    expect(view.pageCount).toBe(4);
    expect(view.matched).toBe(10);
    expect(view.total).toBe(10);
  });

  it('applies the query before paginating, and reports matched separately from total', () => {
    const view = tableView({
      rows: rows10(),
      columns: COLUMNS,
      sort: { key: 'id', dir: 'asc' },
      page: 1,
      pageSize: 10,
      query: 'row-1', // matches row-1 and row-10
    });
    expect(view.rows.map((r) => r.id)).toEqual([1, 10]);
    expect(view.matched).toBe(2);
    expect(view.total).toBe(10);
  });

  it('heals and returns a clamped page when the query shrinks the row set', () => {
    const view = tableView({
      rows: rows10(),
      columns: COLUMNS,
      sort: { key: 'id', dir: 'asc' },
      page: 5,
      pageSize: 3,
      query: 'row-1', // only 2 rows match -> 1 page
    });
    expect(view.page).toBe(1);
    expect(view.pageCount).toBe(1);
    expect(view.rows.map((r) => r.id)).toEqual([1, 10]);
  });

  it('returns page 1 of 1 for zero rows', () => {
    const view = tableView({
      rows: [],
      columns: COLUMNS,
      sort: { key: 'id', dir: 'asc' },
      page: 3,
      pageSize: 10,
    });
    expect(view.rows).toEqual([]);
    expect(view.page).toBe(1);
    expect(view.pageCount).toBe(1);
    expect(view.matched).toBe(0);
    expect(view.total).toBe(0);
  });

  it('treats a non-positive pageSize as one page containing everything', () => {
    const view = tableView({
      rows: rows10(),
      columns: COLUMNS,
      sort: { key: 'id', dir: 'asc' },
      page: 1,
      pageSize: 0,
    });
    expect(view.rows).toHaveLength(10);
    expect(view.pageCount).toBe(1);
  });

  it('does not mutate the input rows array', () => {
    const rows = rows10();
    const snapshot = [...rows];
    tableView({
      rows,
      columns: COLUMNS,
      sort: { key: 'id', dir: 'desc' },
      page: 1,
      pageSize: 3,
      query: 'row',
    });
    expect(rows).toEqual(snapshot);
  });
});
