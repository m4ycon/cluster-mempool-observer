import clsx from 'clsx';
import type { PlotRect } from '../../lib/chartPlot';
import { tooltipPlacement } from '../../lib/chartTooltip';

const PAD = 8;
const LINE_HEIGHT = 14;
const BASELINE = 9; // first line's text baseline, below the top pad

// JetBrains Mono advances 0.6em per glyph, and `text-xs` is 12px. Every line
// is mono, so the box can be sized by character count with no measuring. If the
// webfont fails and a fallback kicks in the box only gets roomy, never tight --
// no common monospace is wider than 0.6em.
const CHAR_W = 7.2;

const boxWidth = (lines: string[]) =>
  lines.reduce((max, line) => Math.max(max, line.length), 0) * CHAR_W + PAD * 2;

const boxHeight = (lines: string[]) => PAD * 1.2 + lines.length * LINE_HEIGHT;

export interface ChartTooltipProps {
  /** Drives the box geometry; first line is the heading, the rest are dimmed. */
  lines: string[];
  plot: PlotRect;
  gap: number;
  /** x the box sits to the right of, when there is room. */
  rightOf: number;
  /** x the box sits to the left of, once flipped. */
  leftOf: number;
  /** `center` rides level with the value; `above` clears a bar's top edge. */
  anchorY: { at: number; align: 'center' | 'above' };
}

// TODO: maybe use some lib to avoid this complexity?
/** Hover box for a chart. Sizes itself to `lines`, then places itself in `plot`. */
export function ChartTooltip({
  lines,
  plot,
  gap,
  rightOf,
  leftOf,
  anchorY,
}: ChartTooltipProps) {
  const width = boxWidth(lines);
  const height = boxHeight(lines);

  const { x, y } = tooltipPlacement({
    rightOf,
    leftOf,
    preferredTop:
      anchorY.align === 'center' ? anchorY.at - height / 2 : anchorY.at - PAD,
    gap,
    width,
    height,
    plot,
  });

  return (
    <g transform={`translate(${x}, ${y})`} className="pointer-events-none">
      <rect
        width={width}
        height={height}
        className="fill-bg stroke-line"
        strokeWidth={1}
      />
      {lines.map((line, i) => (
        <text
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed-order rows, never reordered or keyed by identity
          key={i}
          x={PAD}
          y={PAD + BASELINE + i * LINE_HEIGHT}
          className={clsx(
            'font-mono text-xs',
            i === 0 ? 'fill-dim' : 'fill-ink',
          )}
        >
          {line}
        </text>
      ))}
    </g>
  );
}
