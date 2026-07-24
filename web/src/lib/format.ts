const DISPLAY_LOCALE: string | undefined = 'en-US';

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

/** Theme-grouped number-format helpers. */
export const NumberFormat = {
  grouped,
  compact,
};
