import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ClusterUpdate } from '../../hooks/useClusterDeltaSocket';
import type { PackedCluster, TreemapCell } from '../../lib/clusterLayout';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { ClusterRef } from '../../types/events';
import { ClusterCanvas } from './ClusterCanvas';

const CLUSTERS: ClusterRef[] = [
  { id: 1, txids: ['a1', 'a2'], total_vsize: 100, total_fee: 500 },
  { id: 2, txids: ['b1', 'b2'], total_vsize: 300, total_fee: 3000 },
];

const PACKED: PackedCluster[] = [
  { c: CLUSTERS[0], x: 100, y: 100, r: 40 },
  { c: CLUSTERS[1], x: 220, y: 100, r: 60 },
];

const CELLS: TreemapCell[] = [
  { c: CLUSTERS[0], x0: 0, y0: 0, x1: 200, y1: 280 },
  { c: CLUSTERS[1], x0: 200, y0: 0, x1: 720, y1: 280 },
];

const EMPTY_HISTOGRAM = {
  bars: [],
  xTicks: [],
  yTicks: [],
  maxCount: 0,
  plot: { left: 56, top: 16, width: 648, height: 504 },
};

const SCALE = ClusterMetrics.scaleFor('feerate', [5, 10]);

function canvas(
  lastUpdates: Map<number, ClusterUpdate>,
  vizType: 'circles' | 'treemap' = 'circles',
) {
  return (
    <ClusterCanvas
      vizType={vizType}
      packed={PACKED}
      cells={CELLS}
      histogramLayout={EMPTY_HISTOGRAM}
      sizeMetric="feerate"
      colorMetric="feerate"
      colorScale={SCALE}
      lastUpdates={lastUpdates}
      selectedId={null}
      onSelect={() => {}}
    />
  );
}

/** The mark drawn for a cluster, in `PACKED`/`CELLS` order. */
function marks(container: HTMLElement, tag: 'circle' | 'rect'): Element[] {
  return Array.from(container.querySelectorAll(tag));
}

const update = (
  revision: number,
  kind: ClusterUpdate['kind'],
): ClusterUpdate => ({ revision, kind });

describe('ClusterCanvas update cues', () => {
  it('cues a newly seen cluster differently from a changed one', () => {
    const { container } = render(
      canvas(
        new Map([
          [1, update(1, 'new')],
          [2, update(4, 'changed')],
        ]),
      ),
    );

    const [first, second] = marks(container, 'circle');
    expect(first.getAttribute('class')).toContain('animate-mark-new');
    expect(second.getAttribute('class')).toContain('animate-mark-changed');
  });

  it('leaves clusters with no recorded update uncued', () => {
    const { container } = render(canvas(new Map()));

    for (const mark of marks(container, 'circle')) {
      expect(mark.getAttribute('class')).not.toContain('animate-mark');
    }
  });

  it('cues treemap cells too', () => {
    const { container } = render(
      canvas(new Map([[2, update(1, 'changed')]]), 'treemap'),
    );

    const [first, second] = marks(container, 'rect');
    expect(first.getAttribute('class')).not.toContain('animate-mark');
    expect(second.getAttribute('class')).toContain('animate-mark-changed');
  });

  it('remounts the mark on a new revision so the animation replays', () => {
    const { container, rerender } = render(
      canvas(new Map([[1, update(1, 'changed')]])),
    );
    const before = marks(container, 'circle')[0];

    rerender(canvas(new Map([[1, update(2, 'changed')]])));

    expect(marks(container, 'circle')[0]).not.toBe(before);
  });

  it('keeps the mark mounted while the revision holds', () => {
    const { container, rerender } = render(
      canvas(new Map([[1, update(1, 'changed')]])),
    );
    const before = marks(container, 'circle')[0];

    // Same update, unrelated re-render: replaying here would make an old
    // change look like a fresh one.
    rerender(canvas(new Map([[1, update(1, 'changed')]])));

    expect(marks(container, 'circle')[0]).toBe(before);
  });
});
