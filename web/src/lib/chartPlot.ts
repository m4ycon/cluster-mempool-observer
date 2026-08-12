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
