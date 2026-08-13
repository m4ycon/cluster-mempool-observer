/** Drawing area inside a chart's viewBox, once margins are subtracted. */
export interface PlotRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** One axis tick. `value` is axis-specific (epoch ms, raw metric value, ...). */
export interface ChartTick {
  value: number;
  px: number;
}

/** Index of the point nearest `px` in pixel-x, for hover hit-testing. */
export function nearestPointIndex(
  points: readonly { px: number }[],
  px: number,
): number {
  let nearest = 0;
  let nearestDist = Number.POSITIVE_INFINITY;
  for (let i = 0; i < points.length; i++) {
    const dist = Math.abs(points[i].px - px);
    if (dist < nearestDist) {
      nearestDist = dist;
      nearest = i;
    }
  }
  return nearest;
}
