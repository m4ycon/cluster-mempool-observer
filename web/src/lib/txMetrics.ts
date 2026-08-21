/**
 * Per-transaction reading of a `ClusterMetric`, plus its radius/colour
 * encoding for the transaction DAG. Sibling to clusterMetrics.ts, not an
 * extension of it: `ClusterMetrics.value` is typed on `ClusterRef` and
 * returns a plain `number`, but a per-tx reading can genuinely be unknown.
 */

import type { TransactionRef } from '../types/events';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';
import type { ColorScale } from './colorTiers';

/**
 * Reads a metric off one transaction; null where it has none.
 *
 * `txs` always reads null -- a single transaction is not a count of
 * transactions -- which routes "no per-tx meaning" through the same null
 * channel as "value unknown", so callers need only one branch for both.
 */
export function txValue(
  tx: TransactionRef,
  metric: ClusterMetric,
): number | null {
  switch (metric) {
    case 'vsize':
      // A hollow row's vsize is 0 as a placeholder, never a real reading.
      return tx.vsize > 0 ? tx.vsize : null;
    case 'fee':
      return tx.fee;
    case 'feerate':
      return tx.fee !== null && tx.vsize > 0 ? tx.fee / tx.vsize : null;
    case 'txs':
      return null;
  }
}

const MIN_R = 6;
const MAX_R = 16;

/** Radius when a metric has no per-tx meaning (txs) or its value is unknown. */
export const UNIFORM_R = 10;

/**
 * Sqrt-interpolated radius between `MIN_R`/`MAX_R`, so *area* (not radius) is
 * proportional to `v` -- matching `packLayout` on the main cluster canvas.
 */
export function txRadius(
  v: number | null,
  domain: { min: number; max: number },
): number {
  if (v === null) return UNIFORM_R;
  const { min, max } = domain;
  // Every visible tx sharing one value is normal, not an error: nothing to
  // compare against, so there's no "small"/"large" to show either way.
  if (max <= min) return (MIN_R + MAX_R) / 2;
  const t = Math.sqrt(Math.max(0, Math.min(1, (v - min) / (max - min))));
  return MIN_R + t * (MAX_R - MIN_R);
}

/** Fill when a value is unknown -- unresolved, not a deliberate choice. */
export const UNKNOWN_COLOR = 'var(--color-idle)';

/** Fill when uniform by choice (`txs` has no per-tx meaning). */
export const UNIFORM_COLOR = 'var(--color-tier-5)';

/**
 * Badge colours for which specific field a DAG node is still missing --
 * distinct from `UNKNOWN_COLOR`, which just says "something's unresolved"
 * without saying what. A node can wear more than one at once.
 */
export const INPUTS_UNKNOWN_COLOR = 'var(--color-alert)';
export const VSIZE_UNKNOWN_COLOR = 'var(--color-orange)';
export const FEE_UNKNOWN_COLOR = 'var(--color-warn)';

/**
 * Node fill for a transaction. A real value maps through the shared colour
 * scale; a null needs the metric to tell apart *why* -- `txs` is uniform by
 * choice, any other metric reading null means the value is genuinely
 * unresolved, and those two must not look the same on screen.
 */
export function txColor(
  v: number | null,
  scale: ColorScale,
  metric: ClusterMetric,
): string {
  if (v === null) return metric === 'txs' ? UNIFORM_COLOR : UNKNOWN_COLOR;
  return ClusterMetrics.colorAt(scale, v, metric);
}

/** Colour scale over a visible set of per-tx values, nulls excluded. */
export function txScaleFor(
  colorMetric: ClusterMetric,
  values: (number | null)[],
): ColorScale {
  return ClusterMetrics.scaleFor(
    colorMetric,
    values.filter((v): v is number => v !== null),
  );
}

/** Caption text for one DAG encoding; `txs` has no per-transaction meaning. */
export function encodingCaption(metric: ClusterMetric): string {
  return metric === 'txs'
    ? 'uniform (transaction count has no per-transaction meaning)'
    : ClusterMetrics.LABEL[metric];
}
