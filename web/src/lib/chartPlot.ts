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

/**
 * `clientX` in viewBox units, for an svg drawn with `preserveAspectRatio="xMidYMid meet"`:
 * when the box is wider than the viewBox's aspect, the drawing is scaled by
 * height and centered, so a width-only ratio would drift toward the edges.
 * Null while the svg has no layout box.
 */
export function viewBoxX(
  rect: Pick<DOMRect, 'left' | 'width' | 'height'>,
  clientX: number,
  viewW: number,
  viewH: number,
): number | null {
  if (rect.width === 0 || rect.height === 0) return null;
  const scale = Math.min(rect.width / viewW, rect.height / viewH);
  const offsetX = (rect.width - viewW * scale) / 2;
  return (clientX - rect.left - offsetX) / scale;
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
