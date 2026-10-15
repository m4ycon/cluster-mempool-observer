import { describe, expect, it } from 'vitest';
import { type ChunkBox, txDagElkLayout } from './txDagElkLayout';
import { type TxDagInput, type TxDagNode, txDagLayout } from './txDagLayout';

// a fans out to b, c, d; e spends b and c. Chunks interleave across the fan:
// {a, c}, {b, e}, {d}.
const TXS: TxDagInput[] = [
  { txid: 'a', parents: ['ext1'], r: 10 },
  { txid: 'b', parents: ['a'], r: 8 },
  { txid: 'c', parents: ['a'], r: 12 },
  { txid: 'd', parents: ['a'], r: 6 },
  { txid: 'e', parents: ['b', 'c'], r: 9 },
];
const CHUNK_OF = new Map([
  ['a', 0],
  ['c', 0],
  ['b', 1],
  ['e', 1],
  ['d', 2],
]);

function inside(n: TxDagNode, box: ChunkBox): boolean {
  return (
    n.x - n.halfW >= box.x &&
    n.x + n.halfW <= box.x + box.width &&
    n.y - n.halfH >= box.y &&
    n.y + n.halfH <= box.y + box.height
  );
}

describe('txDagElkLayout', () => {
  it('boxes every chunk around its own members only', async () => {
    const layout = await txDagElkLayout(TXS, CHUNK_OF);
    expect(layout.boxes.map((b) => b.chunk).sort()).toEqual([0, 1, 2]);
    for (const box of layout.boxes) {
      for (const n of layout.nodes) {
        const own = n.kind === 'tx' && CHUNK_OF.get(n.txid) === box.chunk;
        expect(inside(n, box), `${n.txid} vs chunk ${box.chunk}`).toBe(own);
      }
    }
  });

  it('places the same nodes and edges as the boxless layout', async () => {
    const elk = await txDagElkLayout(TXS, CHUNK_OF);
    const plain = txDagLayout(TXS);
    expect(elk.nodes.map((n) => [n.txid, n.kind])).toEqual(
      plain.nodes.map((n) => [n.txid, n.kind]),
    );
    expect(elk.edges).toEqual(plain.edges);
  });

  it('keeps external parents out of every box', async () => {
    const layout = await txDagElkLayout(TXS, CHUNK_OF);
    const stub = layout.nodes.find((n) => n.kind === 'stub');
    if (!stub) throw new Error('expected a stub for ext1');
    for (const box of layout.boxes) expect(inside(stub, box)).toBe(false);
  });

  it('runs the flow left to right, or top down when vertical', async () => {
    const at = (layout: { nodes: TxDagNode[] }, txid: string) => {
      const n = layout.nodes.find((x) => x.txid === txid);
      if (!n) throw new Error(`no node ${txid}`);
      return n;
    };
    const horizontal = await txDagElkLayout(TXS, CHUNK_OF, 'horizontal');
    expect(at(horizontal, 'e').x).toBeGreaterThan(at(horizontal, 'a').x);
    const vertical = await txDagElkLayout(TXS, CHUNK_OF, 'vertical');
    expect(at(vertical, 'e').y).toBeGreaterThan(at(vertical, 'a').y);
  });

  it('lays out nothing for no transactions', async () => {
    expect(await txDagElkLayout([], new Map())).toEqual({
      nodes: [],
      edges: [],
      width: 0,
      height: 0,
      boxes: [],
    });
  });
});
