/**
 * Layered layout of a cluster's transaction graph with each chunk drawn as a
 * box around its members, built on elkjs. Places the same graph as
 * txDagLayout.ts, so stubs, aggregates and back edges behave alike.
 */

import type { ELK, ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk-api';
import {
  buildTxDagGraph,
  layoutWithD3Dag,
  place,
  type TxDagInput,
  type TxDagLayout,
  type TxDagNode,
  type TxDagOrientation,
} from './txDagLayout';

/** A chunk's box, in the same coordinates as the nodes. */
export interface ChunkBox {
  chunk: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TxDagElkLayout extends TxDagLayout {
  boxes: ChunkBox[];
}

const NODE_GAP = 16;
const LAYER_GAP = 32;
const PADDING = 24;
// Room above a box's members for its fee-rate label.
const BOX_LABEL_ROOM = 16;
const BOX_PADDING = 8;
/** Distance from a box's top edge to the centre of its label. */
export const CHUNK_LABEL_OFFSET = (BOX_LABEL_ROOM + BOX_PADDING) / 2;

const chunkId = (chunk: number) => `chunk:${chunk}`;

// elkjs is large; load it the first time a chunk graph is laid out rather
// than with the page.
let elkPromise: Promise<ELK> | null = null;
function loadElk(): Promise<ELK> {
  elkPromise ??= import('elkjs/lib/elk.bundled.js').then(
    ({ default: ElkConstructor }) => new ElkConstructor(),
  );
  return elkPromise;
}

/**
 * Lays the graph out with every chunk as a compound node. Members outside
 * `chunkOf`, and the stubs and aggregates, sit at the top level. Falls back
 * to the boxless d3-dag layout if elkjs throws, so a layout bug never blanks
 * the panel.
 */
export async function txDagElkLayout(
  txs: TxDagInput[],
  chunkOf: Map<string, number>,
  orientation: TxDagOrientation = 'horizontal',
): Promise<TxDagElkLayout> {
  if (txs.length === 0) {
    return { nodes: [], edges: [], width: 0, height: 0, boxes: [] };
  }
  const { graphNodes, edges } = buildTxDagGraph(txs);
  const byId = new Map(graphNodes.map((d) => [d.node.txid, d.node]));

  const chunks = new Map<number, ElkNode[]>();
  const topLevel: ElkNode[] = [];
  for (const { node } of graphNodes) {
    const shape: ElkNode = {
      id: node.txid,
      width: 2 * node.halfW,
      height: 2 * node.halfH,
    };
    const chunk = node.kind === 'tx' ? chunkOf.get(node.txid) : undefined;
    if (chunk === undefined) topLevel.push(shape);
    else chunks.set(chunk, [...(chunks.get(chunk) ?? []), shape]);
  }
  const boxPadding = `[top=${BOX_LABEL_ROOM + BOX_PADDING},left=${BOX_PADDING},bottom=${BOX_PADDING},right=${BOX_PADDING}]`;
  const elkEdges: ElkExtendedEdge[] = graphNodes.flatMap(
    ({ node, parentIds }) =>
      parentIds.map((parent) => ({
        id: `${parent}->${node.txid}`,
        sources: [parent],
        targets: [node.txid],
      })),
  );
  const graph: ElkNode = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': orientation === 'horizontal' ? 'RIGHT' : 'DOWN',
      // One layout run across the boxes, so an edge between two chunks is
      // layered like any other instead of each box being laid out alone.
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.spacing.nodeNode': String(NODE_GAP),
      'elk.layered.spacing.nodeNodeBetweenLayers': String(LAYER_GAP),
      'elk.padding': `[top=${PADDING},left=${PADDING},bottom=${PADDING},right=${PADDING}]`,
    },
    children: [
      ...[...chunks.entries()]
        .sort(([a], [b]) => a - b)
        .map(([chunk, children]) => ({
          id: chunkId(chunk),
          layoutOptions: { 'elk.padding': boxPadding },
          children,
        })),
      ...topLevel,
    ],
    edges: elkEdges,
  };

  let out: ElkNode;
  try {
    out = await (await loadElk()).layout(graph);
  } catch {
    const fallback = layoutWithD3Dag(graphNodes, orientation);
    return { ...fallback, edges, boxes: [] };
  }

  const nodes: TxDagNode[] = [];
  const boxes: ChunkBox[] = [];
  const placeAt = (shape: ElkNode, offsetX: number, offsetY: number) => {
    const node = byId.get(shape.id);
    if (!node) return;
    nodes.push(
      place(
        node,
        0,
        offsetX + (shape.x ?? 0) + (shape.width ?? 0) / 2,
        offsetY + (shape.y ?? 0) + (shape.height ?? 0) / 2,
      ),
    );
  };
  for (const child of out.children ?? []) {
    if (!child.id.startsWith('chunk:')) {
      placeAt(child, 0, 0);
      continue;
    }
    const x = child.x ?? 0;
    const y = child.y ?? 0;
    boxes.push({
      chunk: Number(child.id.slice('chunk:'.length)),
      x,
      y,
      width: child.width ?? 0,
      height: child.height ?? 0,
    });
    for (const member of child.children ?? []) placeAt(member, x, y);
  }

  // Layer is the rank of a node along the flow axis, as in txDagLayout.
  const along = (n: TxDagNode) => (orientation === 'horizontal' ? n.x : n.y);
  const ranks = [...new Set(nodes.map(along))].sort((a, b) => a - b);
  const layerOf = new Map(ranks.map((v, i) => [v, i]));
  for (const n of nodes) n.layer = layerOf.get(along(n)) ?? 0;
  nodes.sort((a, b) => (a.txid < b.txid ? -1 : a.txid > b.txid ? 1 : 0));

  return {
    nodes,
    edges,
    width: out.width ?? 0,
    height: out.height ?? 0,
    boxes,
  };
}
