import {
  CHUNK_DIFF_COLOR,
  type FeerateChunkDiff,
  NODE_COLOR,
  OURS_COLOR,
} from '../../lib/feerateDiagramChart';
import { NumberFormat, TimeFormat } from '../../lib/format';
import type { FeerateDiagramComparison } from '../../types/generated/FeerateDiagramComparison';
import type { FeerateDiagramDiffUnit } from '../../types/generated/FeerateDiagramDiffUnit';
import { ChunkMarkShape } from './ChunkMarkShape';

const NODE_TOTAL_NOUN: Record<FeerateDiagramDiffUnit, string> = {
  chunk_count: 'node chunks',
  weight: 'node WU',
};

function LegendLine({ color, dashed }: { color: string; dashed: boolean }) {
  return (
    <svg width="24" height="8" aria-hidden="true">
      <line
        x1="0"
        y1="4"
        x2="24"
        y2="4"
        stroke={color}
        strokeWidth={2}
        strokeDasharray={dashed ? '6 4' : undefined}
      />
    </svg>
  );
}

function DiffCount({ diff, value }: { diff: FeerateChunkDiff; value: number }) {
  const color = CHUNK_DIFF_COLOR[diff];
  return (
    <span className="flex items-center gap-1.5 text-muted">
      <svg width="8" height="8" aria-hidden="true">
        <ChunkMarkShape
          diff={diff}
          cx={4}
          cy={4}
          stroke={color}
          strokeWidth={1}
        />
      </svg>
      {diff}
      <span className="text-base" style={{ color }}>
        {NumberFormat.grouped(value)}
      </span>
    </span>
  );
}

export interface FeerateComparisonBarProps {
  comparison: FeerateDiagramComparison;
}

/** Legend and the current pair's readings: how far ours is from the node's, in absolute terms. */
export function FeerateComparisonBar({
  comparison,
}: FeerateComparisonBarProps) {
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border border-line px-4 py-2.5 text-xs">
      <span className="flex items-center gap-2 text-muted">
        <LegendLine color={NODE_COLOR} dashed={false} />
        node
      </span>
      <span className="flex items-center gap-2 text-muted">
        <LegendLine color={OURS_COLOR} dashed />
        ours
      </span>
      <span className="h-4 w-px bg-line" />
      <DiffCount diff="missing" value={comparison.missing} />
      <DiffCount diff="extra" value={comparison.extra} />
      <span className="text-muted">
        of{' '}
        <span className="text-base text-ink">
          {NumberFormat.grouped(comparison.node_total)}
        </span>{' '}
        {NODE_TOTAL_NOUN[comparison.unit]}
      </span>
      <span className="grow" />
      <span className="text-slate">
        sampled {TimeFormat.at(comparison.sampled_at)} ·{' '}
        {TimeFormat.ago(comparison.sampled_at)} ago
      </span>
    </div>
  );
}
