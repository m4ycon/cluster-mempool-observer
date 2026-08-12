import type { ChartTick, PlotRect } from '../../lib/chartPlot';

const TICK_LENGTH = 4;
const TICK_TEXT_BASELINE = 16;
const LABEL_BASELINE = 34; // clears the tick labels

export interface AxisXProps {
  plot: PlotRect;
  ticks: ChartTick[];
  /** Tick values are axis-specific, so the caller renders them. */
  format: (value: number) => string;
  /** Axis title under the ticks; omit where the unit is already obvious. */
  label?: string;
}

/** Bottom axis: baseline, tick marks, tick labels, optional title. */
export function AxisX({ plot, ticks, format, label }: AxisXProps) {
  const baseline = plot.top + plot.height;

  return (
    <>
      <line
        x1={plot.left}
        y1={baseline}
        x2={plot.left + plot.width}
        y2={baseline}
        className="stroke-line"
        strokeWidth={1}
      />
      {ticks.map((t) => (
        <g key={t.value}>
          <line
            x1={t.px}
            y1={baseline}
            x2={t.px}
            y2={baseline + TICK_LENGTH}
            className="stroke-line"
            strokeWidth={1}
          />
          <text
            x={t.px}
            y={baseline + TICK_TEXT_BASELINE}
            textAnchor="middle"
            className="fill-dim font-mono text-xs"
          >
            {format(t.value)}
          </text>
        </g>
      ))}
      {label && (
        <text
          x={plot.left + plot.width / 2}
          y={baseline + LABEL_BASELINE}
          textAnchor="middle"
          className="fill-faint text-xs"
        >
          {label}
        </text>
      )}
    </>
  );
}
