import clsx from 'clsx';
import { NumberFormat } from '../../lib/format';
import type { ClusterChunkRef } from '../../types/events';

export interface ChunkStripProps {
  chunks: ClusterChunkRef[];
  /** One per chunk, same order. */
  colors: string[];
  /** Chunk under the pointer here or in the graph; the rest dim. */
  highlighted: number | null;
  /** Chunk of the selected transaction, outlined. */
  selected: number | null;
  onHoverChunk: (chunk: number | null) => void;
}

// viewBox units; the strip stretches to its box, so only ratios matter.
const W = 1000;
const H = 40;
const MIN_BAR_H = 2;
// A label narrower than this share of the strip would overlap its neighbours.
const LABEL_MIN_SHARE = 0.06;

function feerate(chunk: ClusterChunkRef): number {
  return chunk.vsize > 0 ? chunk.fee / chunk.vsize : 0;
}

/**
 * The cluster's chunks in mining order: width is a chunk's share of the
 * cluster's vsize, height its fee-rate, so a bar's area is its fee.
 */
export function ChunkStrip({
  chunks,
  colors,
  highlighted,
  selected,
  onHoverChunk,
}: ChunkStripProps) {
  const total = chunks.reduce((sum, c) => sum + c.vsize, 0);
  const maxRate = Math.max(0, ...chunks.map(feerate));
  if (chunks.length === 0 || total <= 0) return null;

  let x = 0;
  const bars = chunks.map((chunk, i) => {
    const share = chunk.vsize / total;
    const rate = feerate(chunk);
    const bar = {
      i,
      chunk,
      rate,
      share,
      x,
      w: share * W,
      h: maxRate > 0 ? Math.max(MIN_BAR_H, (rate / maxRate) * H) : MIN_BAR_H,
    };
    x += bar.w;
    return bar;
  });

  return (
    <div className="relative" data-testid="chunk-strip">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="block h-10 w-full"
        role="img"
        aria-label={`${chunks.length} chunk${chunks.length === 1 ? '' : 's'} in mining order`}
      >
        {bars.map((b) => (
          <g
            key={b.i}
            data-testid="chunk-bar"
            data-chunk={b.i}
            onPointerEnter={() => onHoverChunk(b.i)}
            onPointerLeave={() => onHoverChunk(null)}
            className={clsx(
              highlighted !== null && highlighted !== b.i && 'opacity-25',
            )}
          >
            <title>
              {`chunk ${b.i + 1}/${chunks.length} · ${b.chunk.txids.length} tx${b.chunk.txids.length === 1 ? '' : 's'} · ` +
                `${NumberFormat.grouped(b.chunk.fee)} sats · ${NumberFormat.grouped(b.chunk.vsize)} vB · ${b.rate.toFixed(1)} s/vB`}
            </title>
            {/* Full-height hit area, so a short bar is as easy to hover as a tall one. */}
            <rect x={b.x} y={0} width={b.w} height={H} fill="transparent" />
            <rect
              x={b.x}
              y={H - b.h}
              width={b.w}
              height={b.h}
              fill={colors[b.i]}
              stroke="var(--color-bg)"
              strokeWidth={1}
              vectorEffect="non-scaling-stroke"
            />
            {selected === b.i && (
              <rect
                x={b.x}
                y={0}
                width={b.w}
                height={H}
                fill="none"
                strokeWidth={2}
                vectorEffect="non-scaling-stroke"
                className="stroke-ink"
                data-testid="chunk-bar-selected"
              />
            )}
          </g>
        ))}
      </svg>
      <div className="relative h-4 text-dim text-xs">
        {bars
          .filter((b) => b.share >= LABEL_MIN_SHARE)
          .map((b) => (
            <span
              key={b.i}
              className="absolute -translate-x-1/2 font-mono"
              style={{ left: `${((b.x + b.w / 2) / W) * 100}%` }}
            >
              {b.rate.toFixed(1)}
            </span>
          ))}
      </div>
    </div>
  );
}
