import type { PlotRect } from './chartPlot';

export interface TooltipPlacement {
  /** x the tooltip sits to the right of, when there is room. */
  rightOf: number;
  /** x the tooltip sits to the left of, once flipped. Same as `rightOf` for a
   * point anchor; the bar's opposite edge for a bar anchor. */
  leftOf: number;
  /** Desired top edge; clamped into the plot, never used as-is. */
  preferredTop: number;
  gap: number;
  width: number;
  height: number;
  plot: PlotRect;
}

/** Top-left corner for a tooltip box, flipped and clamped to stay in `plot`. */
export function tooltipPlacement({
  rightOf,
  leftOf,
  preferredTop,
  gap,
  width,
  height,
  plot,
}: TooltipPlacement): { x: number; y: number } {
  const preferredX = rightOf + gap;
  const overflowsRight = preferredX + width > plot.left + plot.width;

  return {
    x: overflowsRight ? Math.max(plot.left, leftOf - gap - width) : preferredX,
    y: Math.min(
      Math.max(preferredTop, plot.top),
      plot.top + plot.height - height,
    ),
  };
}
