export type ColorTierMethod =
  | 'quantile' // equal count per tier (data-relative, always fills tiers)
  | 'jenks'; // Fisher-Jenks natural breaks (minimise within-tier variance)

const DEFAULT_METHOD: ColorTierMethod = 'jenks';

const TIER_COLORS = [
  '#4d5a6b',
  '#79604a',
  '#a05e17',
  '#cc7a1c',
  '#f7931a',
  '#ffc46b',
] as const;

/** Linear-interpolated quantile over sorted-asc data. */
function quantile(sorted: number[], p: number): number {
  const n = sorted.length;
  if (n === 0) return 0;
  if (n === 1) return sorted[0];
  const pos = p * (n - 1);
  const base = Math.floor(pos);
  const rest = pos - base;
  const lo = sorted[base];
  const hi = sorted[base + 1] ?? lo;
  return lo + (hi - lo) * rest;
}

/** `nBreaks` equal-count breaks (interpolated quantiles at i/(nBreaks+1)). */
function quantileBreaks(sorted: number[], nBreaks: number): number[] {
  const out: number[] = [];
  for (let i = 1; i <= nBreaks; i++) {
    out.push(quantile(sorted, i / (nBreaks + 1)));
  }
  return out;
}

/** Number of distinct values in a sorted-asc array. */
function distinctCount(sorted: number[]): number {
  let d = sorted.length === 0 ? 0 : 1;
  for (let i = 1; i < sorted.length; i++) if (sorted[i] !== sorted[i - 1]) d++;
  return d;
}

/** Fisher-Jenks natural breaks; returns the `nClasses - 1` inner boundaries. */
function jenksBreaks(sorted: number[], nClasses: number): number[] {
  // Jenks needs at least one distinct value per class: with fewer, every
  // partition ties at zero variance and the backtrack below walks off the
  // start of the DP tables. Quantiles degrade gracefully instead.
  const n = sorted.length;
  if (n <= nClasses || distinctCount(sorted) < nClasses) {
    return quantileBreaks(sorted, nClasses - 1);
  }

  const lower: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(nClasses + 1).fill(0),
  );
  const variance: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(nClasses + 1).fill(Number.POSITIVE_INFINITY),
  );
  for (let i = 1; i <= nClasses; i++) {
    lower[1][i] = 1;
    variance[1][i] = 0;
  }
  for (let l = 2; l <= n; l++) {
    let s1 = 0;
    let s2 = 0;
    let w = 0;
    for (let m = 1; m <= l; m++) {
      const i3 = l - m + 1;
      const val = sorted[i3 - 1];
      s2 += val * val;
      s1 += val;
      w++;
      const v = s2 - (s1 * s1) / w;
      const i4 = i3 - 1;
      if (i4 !== 0) {
        for (let j = 2; j <= nClasses; j++) {
          if (variance[l][j] >= v + variance[i4][j - 1]) {
            lower[l][j] = i3;
            variance[l][j] = v + variance[i4][j - 1];
          }
        }
      }
    }
    lower[l][1] = 1;
    variance[l][1] = s2 - (s1 * s1) / w;
  }
  const bounds: number[] = new Array(nClasses + 1).fill(0);
  bounds[nClasses] = sorted[n - 1];
  let k = n;
  for (let count = nClasses; count >= 2; count--) {
    const start = lower[k]?.[count] ?? 1;
    const id = Math.max(0, start - 2);
    bounds[count - 1] = sorted[id];
    k = Math.max(1, start - 1);
  }
  return bounds.slice(1, nClasses); // inner boundaries only
}

/**
 * `n` colours sub-sampled from `TIER_COLORS`, keeping both ends so a short
 * ramp still spans the full perceptual range. A lone tier is not "low", so it
 * gets the brand orange rather than the dark bottom of the ramp.
 */
function ramp(n: number): string[] {
  const last = TIER_COLORS.length - 1;
  if (n <= 1) return [TIER_COLORS[4]];
  if (n >= TIER_COLORS.length) return [...TIER_COLORS];
  return Array.from(
    { length: n },
    (_, i) => TIER_COLORS[Math.round((i * last) / (n - 1))],
  );
}

/** A colour ramp paired with its breaks: `colors.length === breaks.length + 1`. */
export interface ColorScale {
  colors: string[];
  /** Ascending upper bounds; tier `i` holds `v <= breaks[i]`, last tier the rest. */
  breaks: number[];
}

/**
 * Colour scale for `vals`, using at most one tier per distinct value. Data with
 * fewer distinct values than colours gets a shorter ramp, so no tier is ever
 * empty and the legend never advertises a range nothing falls into.
 */
function scale(
  vals: number[],
  method: ColorTierMethod = DEFAULT_METHOD,
): ColorScale {
  const sorted = vals.filter(Number.isFinite).sort((a, b) => a - b);
  const nClasses = Math.min(
    TIER_COLORS.length,
    Math.max(1, distinctCount(sorted)),
  );
  const colors = ramp(nClasses);
  if (nClasses === 1) return { colors, breaks: [] };

  const inner =
    method === 'jenks'
      ? jenksBreaks(sorted, nClasses)
      : quantileBreaks(sorted, nClasses - 1);
  // Enforce ascending order defensively (breaks can tie on clustered data).
  return { colors, breaks: [...inner].sort((a, b) => a - b) };
}

/**
 * Re-tiers a scale onto `breaks`, which must be strictly ascending. Used when a
 * caller drops breaks its labels cannot tell apart: the ramp is rebuilt for the
 * surviving tier count rather than punched full of holes, so both ends survive.
 */
function reTier(breaks: number[]): ColorScale {
  return { colors: ramp(breaks.length + 1), breaks };
}

/** Theme-grouped choropleth colour-tier helpers. */
export const ColorTiers = {
  COLORS: TIER_COLORS,
  DEFAULT_METHOD,
  scale,
  reTier,
};
