import {
  CHUNK_DIFF_COLOR,
  NODE_COLOR,
  OURS_COLOR,
} from '../lib/feerateDiagramChart';
import { sim } from '../lib/sim';
import { ChunkMarkShape } from './feerate/ChunkMarkShape';

export interface FeerateDiagramPreviewProps {
  width: number;
  height: number;
}

/** Decorative feerate-diagram shape for the home preview card: the node's curve, ours dashed over it. */
export function FeerateDiagramPreview({
  width,
  height,
}: FeerateDiagramPreviewProps) {
  const { node, area, ours, missing, extra } = sim.feerateDiagramAt(
    width,
    height,
  );
  const marks = [
    ...missing.map((p) => ['missing', p] as const),
    ...extra.map((p) => ['extra', p] as const),
  ];

  return (
    <svg width="100%" height={height} className="block">
      <title>Mempool feerate diagram preview</title>
      <svg
        aria-hidden="true"
        width="100%"
        height="100%"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
      >
        <path d={area} fill="rgba(247,147,26,0.09)" stroke="none" />
        <path d={node} fill="none" stroke={NODE_COLOR} strokeWidth={1.5} />
        <path
          d={ours}
          fill="none"
          stroke={OURS_COLOR}
          strokeWidth={1.5}
          strokeDasharray="5 4"
        />
      </svg>
      {/* Placed by percentage outside the stretched viewBox, so the ring stays round. */}
      {marks.map(([diff, p]) => (
        <svg
          key={`${diff}-${p.x}`}
          aria-hidden="true"
          x={`${p.x * 100}%`}
          y={`${p.y * 100}%`}
          overflow="visible"
        >
          <ChunkMarkShape
            diff={diff}
            cx={0}
            cy={0}
            className="stroke-bg"
            strokeWidth={3}
          />
          <ChunkMarkShape
            diff={diff}
            cx={0}
            cy={0}
            stroke={CHUNK_DIFF_COLOR[diff]}
            strokeWidth={1.5}
          />
        </svg>
      ))}
    </svg>
  );
}
