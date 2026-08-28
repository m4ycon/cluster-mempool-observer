import type { ReactNode } from 'react';

export interface Column<T, K extends string = string> {
  key: K;
  label: string;
  /** Raw value, used for sorting and for matching an unformatted query. */
  value: (row: T) => string | number;
  /** Display text, e.g. `1,234`. Defaults to String(value(row)). Also searched. */
  text?: (row: T) => string;
  /** Custom cell node. Falls back to `text`. */
  render?: (row: T) => ReactNode;
  align?: 'left' | 'right';
  sortable?: boolean;
  /** Opt a column into the search. Off by default. */
  searchable?: boolean;
}

export interface SortState<K extends string = string> {
  key: K;
  dir: 'asc' | 'desc';
}

export interface TableViewInput<T, K extends string = string> {
  rows: readonly T[];
  columns: readonly Column<T, K>[];
  sort: SortState<K>;
  page: number; // 1-based
  pageSize: number;
  query?: string;
  /** Extra hidden strings each row can be found by, e.g. a cluster's txids. */
  searchExtra?: (row: T) => readonly string[];
  /** Stable identity, used to break ties so a live feed cannot reshuffle rows. */
  tiebreak?: (row: T) => string | number;
}

export interface TableView<T> {
  rows: T[]; // the current page's rows, sorted+filtered
  page: number; // clamped, 1-based
  pageCount: number;
  matched: number; // rows surviving the query
  total: number; // rows before the query
}

/** Rows whose searchable columns, or whose searchExtra strings, contain `query` (case-insensitive). */
export function filterRows<T, K extends string>(
  rows: readonly T[],
  columns: readonly Column<T, K>[],
  query?: string,
  searchExtra?: (row: T) => readonly string[],
): T[] {
  const q = (query ?? '').trim().toLowerCase();
  if (q === '') return [...rows];

  return rows.filter((row) => {
    for (const column of columns) {
      if (!column.searchable) continue;

      const raw = column.value(row);
      if (String(raw).toLowerCase().includes(q)) return true;

      const text = column.text ? column.text(row) : String(raw);
      if (text.toLowerCase().includes(q)) return true;
    }

    if (searchExtra) {
      for (const extra of searchExtra(row)) {
        if (extra.toLowerCase().includes(q)) return true;
      }
    }

    return false;
  });
}

function compareValues(a: string | number, b: string | number): number {
  return typeof a === 'number' && typeof b === 'number'
    ? a - b
    : String(a).localeCompare(String(b));
}

/**
 * Sorts by `sort.key`'s column value; a key not found in `columns` leaves order
 * untouched. Ties fall back to `tiebreak`, always ascending, so the sort is a
 * total order.
 */
export function sortRows<T, K extends string>(
  rows: readonly T[],
  columns: readonly Column<T, K>[],
  sort: SortState<K>,
  tiebreak?: (row: T) => string | number,
): T[] {
  const column = columns.find((c) => c.key === sort.key);
  if (!column) return [...rows];

  const dir = sort.dir === 'desc' ? -1 : 1;
  return [...rows].sort((a, b) => {
    const cmp = compareValues(column.value(a), column.value(b));
    if (cmp !== 0) return cmp * dir;
    return tiebreak ? compareValues(tiebreak(a), tiebreak(b)) : 0;
  });
}

/** Page count for `matched` rows; always >= 1, and 1 for a non-positive/non-finite pageSize. */
export function pageCount(matched: number, pageSize: number): number {
  if (!Number.isFinite(pageSize) || pageSize <= 0) return 1;
  return Math.max(1, Math.ceil(matched / pageSize));
}

/** Heals `page` into [1, pageCount] -- the row set can shrink under a live query or feed. */
export function clampPage(
  page: number,
  matched: number,
  pageSize: number,
): number {
  const count = pageCount(matched, pageSize);
  const floored = Number.isFinite(page) ? Math.floor(page) : 1;
  return Math.min(Math.max(floored, 1), count);
}

/** Pipeline: filter -> sort -> clamp page -> slice. Returns the clamped page for the caller to persist. */
export function tableView<T, K extends string>(
  input: TableViewInput<T, K>,
): TableView<T> {
  const { rows, columns, sort, page, pageSize, query, searchExtra, tiebreak } =
    input;
  const total = rows.length;
  const filtered = filterRows(rows, columns, query, searchExtra);
  const sorted = sortRows(filtered, columns, sort, tiebreak);
  const matched = sorted.length;
  const clampedPage = clampPage(page, matched, pageSize);
  const count = pageCount(matched, pageSize);

  // Non-positive/non-finite pageSize means "one page containing everything".
  const size = Number.isFinite(pageSize) && pageSize > 0 ? pageSize : matched;
  const start = (clampedPage - 1) * size;
  const pageRows = sorted.slice(start, start + size);

  return {
    rows: pageRows,
    page: clampedPage,
    pageCount: count,
    matched,
    total,
  };
}
