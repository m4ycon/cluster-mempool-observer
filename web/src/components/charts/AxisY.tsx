import type { ChartTick, PlotRect } from '../../lib/chartPlot';

const TICK_LENGTH = 4;
const TICK_TEXT_GAP = 8;
const TICK_TEXT_CENTRE = 3;
const TICK_CHAR_W = 7.2; // font-mono at text-xs advances 0.6em per character
const LABEL_GAP = 8;
const LABEL_MIN_X = 10;

export interface AxisYProps {
  plot: PlotRect;
  ticks: ChartTick[];
  format: (value: number) => string;
  /** Axis title, rotated alongside the ticks. */
  label?: string;
  gridlines?: boolean;
}

/** Left axis: ticks (as marks or gridlines), tick labels, optional title. */
export function AxisY({ plot, ticks, format, label, gridlines }: AxisYProps) {
  const widestTick = ticks.reduce(
    (chars, t) => Math.max(chars, format(t.value).length),
    0,
  );
  const labelX = Math.max(
    Math.round(
      plot.left - TICK_TEXT_GAP - widestTick * TICK_CHAR_W - LABEL_GAP,
    ),
    LABEL_MIN_X,
  );

  return (
    <>
      {!gridlines && (
        <line
          x1={plot.left}
          y1={plot.top}
          x2={plot.left}
          y2={plot.top + plot.height}
          className="stroke-line"
          strokeWidth={1}
        />
      )}
      {ticks.map((t) => (
        <g key={t.value}>
          <line
            x1={gridlines ? plot.left : plot.left - TICK_LENGTH}
            y1={t.px}
            x2={gridlines ? plot.left + plot.width : plot.left}
            y2={t.px}
            className="stroke-line"
            strokeWidth={1}
          />
          <text
            x={plot.left - TICK_TEXT_GAP}
            y={t.px + TICK_TEXT_CENTRE}
            textAnchor="end"
            className="fill-dim font-mono text-xs"
          >
            {format(t.value)}
          </text>
        </g>
      ))}
      {label && (
        <text
          transform={`translate(${labelX}, ${plot.top + plot.height / 2}) rotate(-90)`}
          textAnchor="middle"
          className="fill-faint text-xs"
        >
          {label}
        </text>
      )}
    </>
  );
}
