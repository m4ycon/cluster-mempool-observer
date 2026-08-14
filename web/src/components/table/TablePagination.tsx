import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useEffect, useState } from 'react';
import { TextInput } from '../TextInput';

export interface TablePaginationProps {
  page: number; // 1-based
  pageCount: number;
  onPageChange: (page: number) => void;
}

/** Prev/next arrows plus a jump-to-page input; page numbers are clamped and 1-based. */
export function TablePagination({
  page,
  pageCount,
  onPageChange,
}: TablePaginationProps) {
  const [draft, setDraft] = useState(String(page));

  // Follow the committed page (arrows, or a healed page from the table).
  useEffect(() => {
    setDraft(String(page));
  }, [page]);

  const commit = () => {
    const n = Number(draft);
    if (Number.isInteger(n) && n >= 1 && n <= pageCount) {
      if (n !== page) onPageChange(n);
    } else {
      setDraft(String(page));
    }
  };

  return (
    <div className="flex items-center gap-2 text-dim text-xs tracking-[0.12em]">
      <button
        type="button"
        aria-label="Previous page"
        disabled={page <= 1}
        onClick={() => onPageChange(page - 1)}
        className="mco-reset text-dim disabled:opacity-30 enabled:hover:text-orange"
      >
        <ChevronLeft size={14} />
      </button>
      <span>
        PAGE{' '}
        <TextInput
          value={draft}
          onChange={setDraft}
          ariaLabel="Jump to page"
          inputMode="numeric"
          align="center"
          size="sm"
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
          }}
          className="inline-flex w-10 align-middle"
        />{' '}
        OF {pageCount}
      </span>
      <button
        type="button"
        aria-label="Next page"
        disabled={page >= pageCount}
        onClick={() => onPageChange(page + 1)}
        className="mco-reset text-dim disabled:opacity-30 enabled:hover:text-orange"
      >
        <ChevronRight size={14} />
      </button>
    </div>
  );
}
