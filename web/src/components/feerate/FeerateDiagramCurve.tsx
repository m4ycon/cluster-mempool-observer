import { type MouseEvent, useRef, useState } from 'react';
import { nearestPointIndex } from '../../lib/chartPlot';
import {
  CHUNK_DIFF_COLOR,
  type FeerateChunkMark,
  type FeerateDiagramCurves,
  type FeerateDiagramWindow,
  feerateDiagramLayout,
  NODE_COLOR,
  OURS_COLOR,
} from '../../lib/feerateDiagramChart';
import { NumberFormat } from '../../lib/format';
import { AxisX } from '../charts/AxisX';
import { AxisY } from '../charts/AxisY';
import { ChartTooltip } from '../charts/ChartTooltip';
import { openTermDialog } from '../glossary/Term';
import { ChunkMarkShape } from './ChunkMarkShape';

export interface FeerateDiagramCurveProps {
  curves: FeerateDiagramCurves;
  blockWindow: FeerateDiagramWindow;
}

const VIEW_W = 960;
const VIEW_H = 320;

const TOOLTIP_GAP = 12; // offset from the hovered point

const MARK_HIT_RADIUS = 6; // a 7px mark with a 1px stroke is too thin to hover

const WHO_HAS_IT = {
  missing: 'node has it, ours does not',
  extra: 'ours has it, the node does not',
} as const;

function markTooltipLines(mark: FeerateChunkMark): string[] {
  return [
    `${mark.diff} chunk`,
    `${NumberFormat.grouped(mark.feeSats)} sats · ${NumberFormat.grouped(mark.weight)} WU`,
    `${mark.satPerVb.toFixed(2)} sat/vB`,
    WHO_HAS_IT[mark.diff],
  ];
}

/**
 * The node's cumulative feerate diagram as a growth curve, ours dashed over it,
 * each divergent chunk marked on its side's curve, and dashed block boundaries.
 */
export function FeerateDiagramCurve({
  curves,
  blockWindow,
}: FeerateDiagramCurveProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [hoveredMark, setHoveredMark] = useState<number | null>(null);

  if (curves.node.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        no sample yet
      </div>
    );
  }

  const layout = feerateDiagramLayout(curves, blockWindow, VIEW_W, VIEW_H);
  const { plot } = layout;
  const last = layout.node.points[layout.node.points.length - 1];
  const hovered =
    hoveredIndex !== null ? layout.node.points[hoveredIndex] : null;
  const mark = hoveredMark !== null ? layout.marks[hoveredMark] : null;

  const handleMouseMove = (e: MouseEvent<SVGRectElement>) => {
    const svg = svgRef.current;
    if (!svg) return;

    const rect = svg.getBoundingClientRect();
    if (rect.width === 0) return;

    const localX = ((e.clientX - rect.left) / rect.width) * VIEW_W;
    setHoveredIndex(nearestPointIndex(layout.node.points, localX));
  };

  return (
    <div
      className="flex h-full w-full flex-col gap-1"
      data-testid="feerate-diagram-chart"
      data-point-count={layout.node.points.length}
    >
      <svg
        ref={svgRef}
        width="100%"
        height="100%"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="xMidYMid meet"
        className="flex-1"
        role="img"
        aria-label="Mempool cumulative feerate diagram, the node's and ours"
      >
        <AxisY
          plot={plot}
          ticks={layout.yTicks}
          format={NumberFormat.compact}
          label="cumulative fee (sat)"
          gridlines
        />

        {/* Block boundaries: a faint dashed grid, not per-line annotations --
            drawn under the curve so the curve stays the only prominent mark. */}
        {layout.blockBoundaries.map((b) => (
          <line
            key={b.weight}
            x1={b.px}
            y1={plot.top}
            x2={b.px}
            y2={plot.top + plot.height}
            className="stroke-line"
            strokeWidth={1}
            strokeDasharray="4 4"
          />
        ))}

        <AxisX
          plot={plot}
          ticks={layout.xTicks}
          format={NumberFormat.compact}
          label="cumulative sigops-adjusted weight (WU)"
          onLabelClick={() => openTermDialog('sigops-adjusted-weight')}
        />

        {/* Area + line */}
        <path
          d={layout.node.areaPath}
          fill="rgba(247,147,26,0.09)"
          stroke="none"
        />
        <path
          d={layout.node.linePath}
          fill="none"
          stroke={NODE_COLOR}
          strokeWidth={2}
        />
        {/* Dashed on top: the two curves nearly coincide, and the gaps in the
            dashes let the node's line show through. */}
        <path
          data-testid="ours-curve"
          d={layout.ours.linePath}
          fill="none"
          stroke={OURS_COLOR}
          strokeWidth={2}
          strokeDasharray="6 5"
        />

        {/* Direct label on the final point only -- not every point */}
        <circle cx={last.px} cy={last.py} r={3} fill={NODE_COLOR} />
        <text
          x={last.px}
          y={last.py - 8}
          textAnchor="end"
          className="fill-ink font-mono text-xs"
        >
          {NumberFormat.grouped(last.point.fee_sats)} sats
        </text>

        {/* Hover overlay: one large hit target, nearest point found by x */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: hover-only hit area for the crosshair tooltip */}
        <rect
          x={plot.left}
          y={plot.top}
          width={plot.width}
          height={plot.height}
          fill="transparent"
          onMouseMove={handleMouseMove}
          onMouseLeave={() => setHoveredIndex(null)}
        />

        {/* Divergent chunks: a background-colored halo keeps each mark legible on the lines */}
        <g className="pointer-events-none">
          {layout.marks.map((m, i) => (
            <ChunkMarkShape
              // biome-ignore lint/suspicious/noArrayIndexKey: marks are a stable, non-reordered layout output
              key={i}
              diff={m.diff}
              cx={m.px}
              cy={m.py}
              className="stroke-bg"
              strokeWidth={3}
            />
          ))}
          {layout.marks.map((m, i) => (
            <g
              // biome-ignore lint/suspicious/noArrayIndexKey: marks are a stable, non-reordered layout output
              key={i}
              data-testid="chunk-mark"
              data-diff={m.diff}
            >
              <ChunkMarkShape
                diff={m.diff}
                cx={m.px}
                cy={m.py}
                stroke={CHUNK_DIFF_COLOR[m.diff]}
                strokeWidth={1}
              />
            </g>
          ))}
        </g>

        {/* Mark hit targets, above the hover overlay so a mark wins over the crosshair */}
        {layout.marks.map((m, i) => (
          // biome-ignore lint/a11y/noStaticElementInteractions: hover-only hit area for the mark tooltip
          <circle
            // biome-ignore lint/suspicious/noArrayIndexKey: marks are a stable, non-reordered layout output
            key={i}
            data-testid="chunk-mark-hit"
            cx={m.px}
            cy={m.py}
            r={MARK_HIT_RADIUS}
            fill="transparent"
            onMouseEnter={() => setHoveredMark(i)}
            onMouseLeave={() => setHoveredMark(null)}
          />
        ))}

        {/* Crosshair + tooltip */}
        {hovered && (
          <g className="pointer-events-none">
            <line
              x1={hovered.px}
              y1={plot.top}
              x2={hovered.px}
              y2={plot.top + plot.height}
              className="stroke-dim"
              strokeWidth={1}
            />
            <circle cx={hovered.px} cy={hovered.py} r={3} fill={NODE_COLOR} />
            <ChartTooltip
              lines={[
                `${NumberFormat.grouped(hovered.point.weight)} WU`,
                `${NumberFormat.grouped(hovered.point.fee_sats)} sats`,
                // null (origin, or a repeated weight) has no rate to show --
                // a dash, never a fake 0.0, since 0 is a real tail reading.
                hovered.marginalSatPerVb === null
                  ? '-- sat/vB (marginal)'
                  : `${hovered.marginalSatPerVb.toFixed(2)} sat/vB (marginal)`,
              ]}
              plot={plot}
              gap={TOOLTIP_GAP}
              rightOf={hovered.px}
              leftOf={hovered.px}
              anchorY={{ at: hovered.py, align: 'center' }}
            />
          </g>
        )}

        {mark && (
          <g className="pointer-events-none">
            <ChartTooltip
              lines={markTooltipLines(mark)}
              lineColors={[CHUNK_DIFF_COLOR[mark.diff]]}
              plot={plot}
              gap={TOOLTIP_GAP}
              rightOf={mark.px}
              leftOf={mark.px}
              anchorY={{ at: mark.py, align: 'center' }}
            />
          </g>
        )}
      </svg>
    </div>
  );
}
