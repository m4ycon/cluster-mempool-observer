import clsx from 'clsx';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useEffect, useMemo } from 'react';
import type { Column, SortState } from '../../lib/dataTable';
import { tableView } from '../../lib/dataTable';
import { NumberFormat } from '../../lib/format';
import { TablePagination } from './TablePagination';
import { TableSearch } from './TableSearch';

export interface DataTableProps<T, K extends string = string> {
  columns: readonly Column<T, K>[];
  rows: readonly T[];
  rowKey: (row: T) => string | number;
  sort: SortState<K>;
  onSortChange: (sort: SortState<K>) => void;
  page: number; // 1-based
  onPageChange: (page: number) => void;
  query: string;
  onQueryChange: (query: string) => void;
  pageSize?: number;
  searchExtra?: (row: T) => readonly string[];
  /** Stable identity, used to break sort ties. */
  tiebreak?: (row: T) => string | number;
  selectedKey?: string | number | null;
  onSelect?: (row: T) => void;
  rowClassName?: (row: T) => string | undefined;
  /** Shown when there are no rows at all, e.g. "awaiting cluster feed...". */
  emptyLabel: string;
  /** Shown when a query filters every row out. Defaults to "no rows match". */
  noMatchLabel?: string;
  searchPlaceholder?: string;
}

const DEFAULT_PAGE_SIZE = 25;

/** Generic, fully controlled table: renders what it's given, reports intent (sort/page/query) upward. */
export function DataTable<T, K extends string = string>({
  columns,
  rows,
  rowKey,
  sort,
  onSortChange,
  page,
  onPageChange,
  query,
  onQueryChange,
  pageSize = DEFAULT_PAGE_SIZE,
  searchExtra,
  tiebreak,
  selectedKey,
  onSelect,
  rowClassName,
  emptyLabel,
  noMatchLabel = 'no rows match',
  searchPlaceholder,
}: DataTableProps<T, K>) {
  const view = useMemo(
    () =>
      tableView({
        rows,
        columns,
        sort,
        page,
        pageSize,
        query,
        searchExtra,
        tiebreak,
      }),
    [rows, columns, sort, page, pageSize, query, searchExtra, tiebreak],
  );

  // The row set can shrink under a live feed or a query -- heal the URL's page.
  useEffect(() => {
    if (view.page !== page) onPageChange(view.page);
  }, [view.page, page, onPageChange]);

  const handleHeaderClick = (column: Column<T, K>) => {
    if (column.sortable === false) return;
    onSortChange(
      sort.key === column.key
        ? { key: column.key, dir: sort.dir === 'asc' ? 'desc' : 'asc' }
        : { key: column.key, dir: 'desc' },
    );
  };

  return (
    <div>
      <TableSearch
        value={query}
        onChange={onQueryChange}
        placeholder={searchPlaceholder}
      />

      <div className="mt-3 overflow-x-auto">
        {view.matched === 0 ? (
          <div className="text-faint text-xs">
            {rows.length === 0 ? emptyLabel : noMatchLabel}
          </div>
        ) : (
          <table className="w-full border-collapse text-xs">
            <thead>
              <tr className="border-line border-b">
                {columns.map((column) => {
                  const sortable = column.sortable !== false;
                  const active = sort.key === column.key;
                  const ariaSort = active
                    ? sort.dir === 'asc'
                      ? 'ascending'
                      : 'descending'
                    : 'none';
                  return (
                    <th
                      key={column.key}
                      aria-sort={ariaSort}
                      className={clsx(
                        'px-3 py-2 text-dim tracking-[0.12em] uppercase',
                        column.align === 'right' ? 'text-right' : 'text-left',
                      )}
                    >
                      {sortable ? (
                        <button
                          type="button"
                          onClick={() => handleHeaderClick(column)}
                          className={clsx(
                            'mco-reset inline-flex items-center gap-1 hover:text-orange',
                            column.align === 'right' && 'flex-row-reverse',
                          )}
                        >
                          {column.label}
                          {active &&
                            (sort.dir === 'asc' ? (
                              <ChevronUp size={12} />
                            ) : (
                              <ChevronDown size={12} />
                            ))}
                        </button>
                      ) : (
                        column.label
                      )}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {view.rows.map((row) => {
                const key = rowKey(row);
                const selected = selectedKey != null && key === selectedKey;
                const interactive = onSelect !== undefined;
                return (
                  <tr
                    key={key}
                    aria-selected={selected}
                    tabIndex={interactive ? 0 : undefined}
                    onClick={interactive ? () => onSelect(row) : undefined}
                    onKeyDown={
                      interactive
                        ? (e) => {
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault();
                              onSelect(row);
                            }
                          }
                        : undefined
                    }
                    className={clsx(
                      // Row-wide hover tint: the eye has to cross several
                      // columns to read one cluster, so the band is the cue.
                      'border-line border-b hover:bg-card-hover focus-visible:bg-card-hover focus-visible:outline-none',
                      interactive && 'cursor-pointer',
                      selected && 'text-orange',
                      rowClassName?.(row),
                    )}
                  >
                    {columns.map((column) => (
                      <td
                        key={column.key}
                        className={clsx(
                          'px-3 py-1.5',
                          column.align === 'right' ? 'text-right' : 'text-left',
                        )}
                      >
                        {column.render?.(row) ??
                          column.text?.(row) ??
                          String(column.value(row))}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <div className="mt-3 flex items-center justify-between text-dim text-xs">
        {/* Counting a query's hits only says something once it excludes rows. */}
        <span>
          {view.matched === view.total ? (
            <>
              TOTAL{' '}
              <span className="text-ink">
                {NumberFormat.grouped(view.total)}
              </span>
            </>
          ) : (
            <>
              <span className="text-ink">
                {NumberFormat.grouped(view.matched)}
              </span>{' '}
              OF{' '}
              <span className="text-ink">
                {NumberFormat.grouped(view.total)}
              </span>
            </>
          )}
        </span>
        {view.pageCount > 1 && (
          <TablePagination
            page={view.page}
            pageCount={view.pageCount}
            onPageChange={onPageChange}
          />
        )}
      </div>
    </div>
  );
}
