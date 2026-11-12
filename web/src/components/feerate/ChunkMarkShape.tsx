import type { FeerateChunkDiff } from '../../lib/feerateDiagramChart';

const MARK_SIZE = 7;

export interface ChunkMarkShapeProps {
  diff: FeerateChunkDiff;
  cx: number;
  cy: number;
  stroke?: string;
  className?: string;
  strokeWidth: number;
}

/** A missing chunk draws as an "x", an extra one as a ring, so the two read apart without color. */
export function ChunkMarkShape({
  diff,
  cx,
  cy,
  stroke,
  className,
  strokeWidth,
}: ChunkMarkShapeProps) {
  const r = MARK_SIZE / 2;
  return diff === 'missing' ? (
    <path
      d={`M${cx - r} ${cy - r}L${cx + r} ${cy + r}M${cx - r} ${cy + r}L${cx + r} ${cy - r}`}
      fill="none"
      stroke={stroke}
      className={className}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
    />
  ) : (
    <circle
      cx={cx}
      cy={cy}
      r={r}
      fill="none"
      stroke={stroke}
      className={className}
      strokeWidth={strokeWidth}
    />
  );
}
