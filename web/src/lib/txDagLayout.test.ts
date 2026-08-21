import { describe, expect, it } from 'vitest';
import {
  MAX_STUBS_PER_TX,
  type TxDagInput,
  type TxDagNode,
  txDagLayout,
} from './txDagLayout';

const tx = (txid: string, parents: string[] | null, r = 10): TxDagInput => ({
  txid,
  parents,
  r,
});

const byTxid = (nodes: TxDagNode[]) =>
  Object.fromEntries(nodes.map((n): [string, TxDagNode] => [n.txid, n]));

/**
 * No two nodes sharing a layer overlap: centre distance covers both radii +
 * NODE_GAP. Layers run left-to-right, so siblings within one spread
 * vertically -- checked on y, not x.
 */
function assertNoOverlap(nodes: TxDagNode[]) {
  const byLayer = new Map<number, TxDagNode[]>();
  for (const n of nodes) {
    const list = byLayer.get(n.layer) ?? [];
    list.push(n);
    byLayer.set(n.layer, list);
  }
  for (const list of byLayer.values()) {
    const sortedByY = [...list].sort((a, b) => a.y - b.y);
    for (let i = 1; i < sortedByY.length; i++) {
      const prev = sortedByY[i - 1];
      const cur = sortedByY[i];
      const gap = cur.y - prev.y - prev.r - cur.r;
      expect(gap).toBeGreaterThan(0);
    }
  }
}

describe('txDagLayout', () => {
  it('lays a linear chain a -> b -> c onto three ordered layers', () => {
    const { nodes } = txDagLayout([
      tx('a', []),
      tx('b', ['a']),
      tx('c', ['b']),
    ]);
    const n = byTxid(nodes);
    expect(n.a.layer).toBe(0);
    expect(n.b.layer).toBe(1);
    expect(n.c.layer).toBe(2);
  });

  it('puts the diamond join strictly downstream of both of its parents', () => {
    const { nodes, edges } = txDagLayout([
      tx('a', []),
      tx('b', ['a']),
      tx('c', ['a']),
      tx('d', ['b', 'c']),
    ]);
    const n = byTxid(nodes);
    expect(n.b.layer).toBeGreaterThan(n.a.layer);
    expect(n.c.layer).toBeGreaterThan(n.a.layer);
    expect(n.d.layer).toBeGreaterThan(n.b.layer);
    expect(n.d.layer).toBeGreaterThan(n.c.layer);
    expect(edges.every((e) => !e.back)).toBe(true);
  });

  it('turns an external parent into exactly one stub node, one layer upstream of its shallowest child', () => {
    const { nodes, edges } = txDagLayout([tx('a', ['p'])]);
    const stubs = nodes.filter((x) => x.kind === 'stub');
    expect(stubs).toHaveLength(1);
    expect(stubs[0].txid).toBe('p');
    const n = byTxid(nodes);
    expect(stubs[0].layer).toBe(n.a.layer - 1);
    expect(edges).toEqual([{ from: 'p', to: 'a', back: false }]);
  });

  it('shares a single stub node between two children spending the same external parent', () => {
    const { nodes, edges } = txDagLayout([tx('b', ['p']), tx('c', ['p'])]);
    const stubs = nodes.filter((x) => x.kind === 'stub');
    expect(stubs).toHaveLength(1);
    const stubEdges = edges.filter((e) => e.from === 'p');
    expect(stubEdges.map((e) => e.to).sort()).toEqual(['b', 'c']);
  });

  it('treats null parents as unknown inputs, distinct from an empty (coinbase) parent list', () => {
    const { nodes, edges } = txDagLayout([
      tx('unknown', null),
      tx('coinbase', []),
    ]);
    const n = byTxid(nodes);
    expect(n.unknown.inputsUnknown).toBe(true);
    expect(n.coinbase.inputsUnknown).toBe(false);
    expect(edges).toEqual([]);
  });

  it('produces identical output regardless of input array order', () => {
    const inputA = [
      tx('a', []),
      tx('b', ['a']),
      tx('c', ['a']),
      tx('d', ['b', 'c']),
      tx('e', ['x']),
    ];
    const inputB = [inputA[3], inputA[0], inputA[4], inputA[2], inputA[1]];

    expect(txDagLayout(inputB)).toEqual(txDagLayout(inputA));
  });

  it('terminates on a synthetic cycle, marks the closing edges back, and yields finite coordinates', () => {
    const { nodes, edges, width, height } = txDagLayout([
      tx('a', ['b']),
      tx('b', ['a']),
    ]);
    expect(nodes).toHaveLength(2);
    expect(edges.every((e) => e.back)).toBe(true);
    for (const n of nodes) {
      expect(Number.isFinite(n.x)).toBe(true);
      expect(Number.isFinite(n.y)).toBe(true);
    }
    expect(Number.isFinite(width)).toBe(true);
    expect(Number.isFinite(height)).toBe(true);
  });

  it('never overlaps nodes within a layer, given mixed radii', () => {
    const { nodes } = txDagLayout([
      tx('a', []),
      tx('b', ['a'], 4),
      tx('c', ['a'], 30),
      tx('d', ['a'], 12),
      tx('e', ['a'], 1),
    ]);
    assertNoOverlap(nodes);
  });

  it('returns empty nodes/edges and zero width/height for empty input', () => {
    expect(txDagLayout([])).toEqual({
      nodes: [],
      edges: [],
      width: 0,
      height: 0,
    });
  });

  it('caps external parents at MAX_STUBS_PER_TX and collapses the rest into one aggregate node', () => {
    const parents = Array.from({ length: MAX_STUBS_PER_TX + 5 }, (_, i) =>
      String(i).padStart(2, '0'),
    );
    const { nodes, edges } = txDagLayout([tx('a', parents)]);

    const stubs = nodes.filter((n) => n.kind === 'stub');
    const aggregates = nodes.filter((n) => n.kind === 'aggregate');
    expect(stubs).toHaveLength(MAX_STUBS_PER_TX);
    expect(stubs.map((n) => n.txid).sort()).toEqual(
      parents.slice(0, MAX_STUBS_PER_TX).sort(),
    );
    expect(aggregates).toHaveLength(1);
    expect(aggregates[0].elidedCount).toBe(parents.length - MAX_STUBS_PER_TX);

    const aggEdges = edges.filter(
      (e) => e.to === 'a' && e.from === aggregates[0].txid,
    );
    expect(aggEdges).toEqual([
      { from: aggregates[0].txid, to: 'a', back: false },
    ]);
  });

  it('creates no aggregate node when external parents are at or under the cap', () => {
    const parents = Array.from({ length: MAX_STUBS_PER_TX }, (_, i) =>
      String(i).padStart(2, '0'),
    );
    const { nodes } = txDagLayout([tx('a', parents)]);

    expect(nodes.filter((n) => n.kind === 'aggregate')).toHaveLength(0);
    expect(nodes.filter((n) => n.kind === 'stub')).toHaveLength(
      MAX_STUBS_PER_TX,
    );
  });

  it('never caps internal parents, even well beyond MAX_STUBS_PER_TX', () => {
    const parentTxids = Array.from({ length: MAX_STUBS_PER_TX + 5 }, (_, i) =>
      String(i).padStart(2, '0'),
    );
    const { nodes, edges } = txDagLayout([
      ...parentTxids.map((id) => tx(id, [])),
      tx('a', parentTxids),
    ]);

    expect(nodes.filter((n) => n.kind === 'aggregate')).toHaveLength(0);
    expect(nodes.filter((n) => n.kind === 'stub')).toHaveLength(0);
    expect(edges.filter((e) => e.to === 'a')).toHaveLength(parentTxids.length);
  });

  it('produces identical output regardless of input order when a transaction elides parents', () => {
    const parents = Array.from({ length: MAX_STUBS_PER_TX + 3 }, (_, i) =>
      String(i).padStart(2, '0'),
    );
    const inputA = [tx('a', parents), tx('b', ['a'])];
    const inputB = [inputA[1], inputA[0]];

    expect(txDagLayout(inputB)).toEqual(txDagLayout(inputA));
  });
});

describe('txDagLayout orientation', () => {
  const chain = [tx('a', []), tx('b', ['a'])];

  it('defaults to horizontal: later layers sit further right, not lower', () => {
    const { nodes } = txDagLayout(chain);
    const n = byTxid(nodes);
    expect(n.b.x).toBeGreaterThan(n.a.x);
    expect(n.b.y).toBeCloseTo(n.a.y);
  });

  it('vertical: later layers sit further down, not right', () => {
    const { nodes } = txDagLayout(chain, 'vertical');
    const n = byTxid(nodes);
    expect(n.b.y).toBeGreaterThan(n.a.y);
    expect(n.b.x).toBeCloseTo(n.a.x);
  });

  it('reports width/height swapped between orientations for the same input', () => {
    const inputs = [tx('a', []), tx('b', ['a']), tx('c', ['a'])];
    const h = txDagLayout(inputs, 'horizontal');
    const v = txDagLayout(inputs, 'vertical');
    expect(v.width).toBeCloseTo(h.height);
    expect(v.height).toBeCloseTo(h.width);
  });

  it('layer numbering is unaffected by orientation', () => {
    const inputs = [tx('a', []), tx('b', ['a']), tx('c', ['b'])];
    const h = txDagLayout(inputs, 'horizontal');
    const v = txDagLayout(inputs, 'vertical');
    expect(byTxid(v.nodes).a.layer).toBe(byTxid(h.nodes).a.layer);
    expect(byTxid(v.nodes).c.layer).toBe(byTxid(h.nodes).c.layer);
  });
});
