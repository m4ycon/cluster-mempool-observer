import dayjs from './dayjs';

const DISPLAY_LOCALE: string | undefined = 'en-US';

const NO_TIME = '--';

/** Locale-grouped integer, e.g. `1,234`. */
function grouped(n: number): string {
  return n.toLocaleString(DISPLAY_LOCALE);
}

/** Compact number formatting for tight labels, e.g. `1.2K`, `3.4M`. */
function compact(n: number): string {
  if (n >= 1000) {
    return new Intl.NumberFormat(DISPLAY_LOCALE, {
      notation: 'compact',
      maximumFractionDigits: 1,
    }).format(n);
  }
  return String(Math.round(n));
}

/** Wall-clock timestamp, e.g. `2026-08-25 14:00:03`. */
function at(iso: string | null | undefined): string {
  return iso ? dayjs(iso).format('YYYY-MM-DD HH:mm:ss') : NO_TIME;
}

/** How long ago, unsuffixed, e.g. `12 min`. Pair it with an AGO label. */
function ago(iso: string | null | undefined): string {
  return iso ? dayjs(iso).fromNow(true) : NO_TIME;
}

/** Milliseconds since the epoch, for sorting. A missing time sorts oldest. */
function epoch(iso: string | null | undefined): number {
  return iso ? dayjs(iso).valueOf() : 0;
}

/** Theme-grouped number-format helpers. */
export const NumberFormat = {
  grouped,
  compact,
};

/** Theme-grouped time-format helpers; every one tolerates a missing time. */
export const TimeFormat = {
  at,
  ago,
  epoch,
};
