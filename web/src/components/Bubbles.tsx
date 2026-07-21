import type { ClustersResult, Simulation } from '../lib/sim';

export interface BubblesProps {
  sim: Simulation;
  clusters: ClustersResult;
  interactive?: boolean;
  showCounts?: boolean;
  selIdx?: number;
  onSelect?: (i: number) => void;
}

export function Bubbles({
  sim,
  clusters,
  interactive = false,
  showCounts = false,
  selIdx = 0,
  onSelect,
}: BubblesProps) {
  const { W, H, circles } = clusters;
  const sel = Math.min(selIdx, circles.length - 1);
  return (
    <svg
      width="100%"
      height="100%"
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMid meet"
    >
      <title>Cluster graph packed by vsize</title>
      {circles.map((c, i) => (
        // biome-ignore lint/a11y/noStaticElementInteractions: bubble is a selectable data point in the cluster explorer
        <circle
          key={c.seed}
          cx={c.x}
          cy={c.y}
          r={c.r}
          fill={sim.feeColor(c.fee)}
          fillOpacity={0.88}
          stroke={interactive && i === sel ? '#e8eef4' : 'none'}
          strokeWidth={1.5}
          style={interactive ? { cursor: 'pointer' } : undefined}
          onClick={interactive && onSelect ? () => onSelect(i) : undefined}
        />
      ))}
      {interactive &&
        showCounts &&
        circles.map((c) =>
          c.sz >= 14 ? (
            <text
              key={`t${c.seed}`}
              x={c.x}
              y={c.y + 3.5}
              textAnchor="middle"
              fontSize={10.5}
              pointerEvents="none"
              fontFamily="'JetBrains Mono', monospace"
              fill="#0a0c0f"
              fontWeight={700}
            >
              {c.sz}
            </text>
          ) : null,
        )}
    </svg>
  );
}
