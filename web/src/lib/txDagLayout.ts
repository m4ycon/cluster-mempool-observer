/**
 * Layered DAG layout for a cluster's transaction graph, built on d3-dag.
 * No dependency on network state or React.
 */

import {
  type GraphNode,
  graphStratify,
  layeringLongestPath,
  sugiyama,
} from 'd3-dag';

/**
 * `horizontal` swaps d3-dag's native top-down axes so layers run left to
 * right; `vertical` is d3-dag's own frame, unswapped.
 */
export type TxDagOrientation = 'horizontal' | 'vertical';

export interface TxDagInput {
  txid: string;
  /** Parent txids, or null when this row's inputs were never learned. */
  parents: string[] | null;
  /** Node radius in layout units; the caller resolves the metric. */
  r: number;
}

export type TxDagNodeKind = 'tx' | 'stub' | 'aggregate';

export interface TxDagNode {
  txid: string;
  kind: TxDagNodeKind;
  layer: number;
  x: number;
  y: number;
  r: number;
  /** Row exists but its parents are unknown -- not the same as having none. */
  inputsUnknown: boolean;
  /** Aggregate nodes only: how many external parents it stands in for. */
  elidedCount?: number;
}

export interface TxDagEdge {
  from: string; // parent
  to: string; // child
  /** Closes a cycle; drawn, but ignored when assigning layers. */
  back: boolean;
}

export interface TxDagLayout {
  nodes: TxDagNode[];
  edges: TxDagEdge[];
  width: number;
  height: number;
}

/** Fixed radius for stub nodes -- they carry no metric to size by. */
export const STUB_R = 8;

/** Fixed radius for aggregate nodes -- larger than a stub since it stands for several. */
export const AGGREGATE_R = 11;

/** External (non-cluster) parents kept per transaction before the rest collapse into one aggregate node. */
export const MAX_STUBS_PER_TX = 5;

// d3-dag lays out top-down (x = spread within a layer, y = layer depth);
// `horizontal` orientation swaps those two axes on the way out so layers run
// left-to-right instead, `vertical` leaves them as d3-dag produced them. Gap
// names stay in d3-dag's own frame regardless: NODE_GAP is always the
// within-layer gap, LAYER_GAP always the between-layers gap.
const NODE_GAP = 16;
const LAYER_GAP = 48;
const PADDING = 24;

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

interface GraphNodeDatum {
  txid: string;
  kind: TxDagNodeKind;
  r: number;
  inputsUnknown: boolean;
  parentIds: string[];
  elidedCount?: number;
}

/** Prefixed so it can never collide with a real (64 lowercase hex char) txid. */
function aggregateId(txid: string): string {
  return `agg:${txid}`;
}

interface RawEdge {
  from: string;
  to: string;
}

/**
 * Kahn's algorithm over internal edges: a node never dequeued is stuck in a
 * cycle. Real tx data can't cycle, but a torn incremental fetch could
 * synthesize one, so this drops the closing edges before d3-dag ever sees
 * them -- d3-dag throws on a cycle, and a throw in a render path blanks
 * the panel.
 */
function findCyclicTargets(txids: string[], edges: RawEdge[]): Set<string> {
  const indeg = new Map(txids.map((id) => [id, 0]));
  const childrenOf = new Map<string, string[]>(txids.map((id) => [id, []]));
  for (const e of edges) {
    indeg.set(e.to, (indeg.get(e.to) ?? 0) + 1);
    childrenOf.get(e.from)?.push(e.to);
  }

  const queue = txids.filter((id) => indeg.get(id) === 0);
  const finalized = new Set<string>();
  let qi = 0;
  while (qi < queue.length) {
    const u = queue[qi++];
    finalized.add(u);
    for (const c of childrenOf.get(u) ?? []) {
      const d = (indeg.get(c) ?? 0) - 1;
      indeg.set(c, d);
      if (d === 0) queue.push(c);
    }
  }

  return new Set(txids.filter((id) => !finalized.has(id)));
}

interface Positioned {
  nodes: TxDagNode[];
  width: number;
  height: number;
}

/** Single-row (or column) fallback so an unanticipated d3-dag throw never escapes. */
function fallbackStack(
  graphNodes: GraphNodeDatum[],
  orientation: TxDagOrientation,
): Positioned {
  const nodes: TxDagNode[] = [];
  let cursor = PADDING;
  let maxR = 0;
  graphNodes.forEach((d, i) => {
    if (i > 0) cursor += 2 * maxR + LAYER_GAP;
    maxR = d.r;
    const along = cursor + d.r;
    const across = PADDING + d.r;
    nodes.push({
      txid: d.txid,
      kind: d.kind,
      layer: i,
      x: orientation === 'horizontal' ? along : across,
      y: orientation === 'horizontal' ? across : along,
      r: d.r,
      inputsUnknown: d.inputsUnknown,
      elidedCount: d.elidedCount,
    });
  });
  const alongTotal = graphNodes.length === 0 ? 0 : cursor + 2 * maxR + PADDING;
  const maxNodeR = Math.max(0, ...graphNodes.map((d) => d.r));
  const acrossTotal = 2 * PADDING + 2 * maxNodeR;
  return orientation === 'horizontal'
    ? { nodes, width: alongTotal, height: acrossTotal }
    : { nodes, width: acrossTotal, height: alongTotal };
}

/**
 * Runs the d3-dag layering/decrossing/coordinate pipeline. Layer is not a
 * d3-dag concept -- it's derived by ranking distinct y values ascending, so
 * we don't need a second hand-rolled layering pass just for that field.
 */
function layoutWithD3Dag(
  graphNodes: GraphNodeDatum[],
  orientation: TxDagOrientation,
): Positioned {
  try {
    // graphStratify/sugiyama aren't generic-callable (see the d3-dag
    // typings) -- accessors must annotate their own parameter type or N/L
    // infer as `unknown`/`never` and later field accesses stop typechecking.
    const stratify = graphStratify()
      .id((d: GraphNodeDatum) => d.txid)
      .parentIds((d: GraphNodeDatum) => d.parentIds);
    const graph = stratify(graphNodes);

    // topDown(false): sources (stubs, and coinbase/unknown-input txs)
    // sit as close as possible to what they feed, instead of all pinning
    // to layer 0 and stranding a wall of grey stubs on the graph's left edge.
    const layout = sugiyama()
      .layering(layeringLongestPath().topDown(false))
      .nodeSize((node: GraphNode<GraphNodeDatum, undefined>) => {
        const d = 2 * node.data.r;
        return [d, d] as const;
      })
      .gap([NODE_GAP, LAYER_GAP]);
    const { width, height } = layout(graph);

    const laidOut = [...graph.nodes()];
    const layerYs = [...new Set(laidOut.map((n) => n.y))].sort((a, b) => a - b);
    const layerOf = new Map(layerYs.map((y, i) => [y, i]));

    // Horizontal swaps d3-dag's (x, y) on the way out: its layer axis (y)
    // becomes our horizontal axis, its within-layer spread (x) becomes our
    // vertical one. Vertical keeps d3-dag's own frame as-is.
    const nodes: TxDagNode[] = laidOut.map((n) => ({
      txid: n.data.txid,
      kind: n.data.kind,
      layer: layerOf.get(n.y) ?? 0,
      x: (orientation === 'horizontal' ? n.y : n.x) + PADDING,
      y: (orientation === 'horizontal' ? n.x : n.y) + PADDING,
      r: n.data.r,
      inputsUnknown: n.data.inputsUnknown,
      elidedCount: n.data.elidedCount,
    }));

    return orientation === 'horizontal'
      ? { nodes, width: height + 2 * PADDING, height: width + 2 * PADDING }
      : { nodes, width: width + 2 * PADDING, height: height + 2 * PADDING };
  } catch {
    return fallbackStack(graphNodes, orientation);
  }
}

/** Turns cluster transactions with parent txids into positioned nodes/edges. */
export function txDagLayout(
  txs: TxDagInput[],
  orientation: TxDagOrientation = 'horizontal',
): TxDagLayout {
  if (txs.length === 0) {
    return { nodes: [], edges: [], width: 0, height: 0 };
  }

  // Recomputed every websocket tick from a Map built out of network
  // responses -- sort so the same logical input always produces identical
  // output, or nodes would jump between ticks for no reason.
  const sorted = [...txs].sort((a, b) => cmp(a.txid, b.txid));
  const known = new Set(sorted.map((t) => t.txid));

  const internalEdges: RawEdge[] = [];
  const stubEdges: RawEdge[] = [];
  const aggregateEdges: RawEdge[] = [];
  const stubChildren = new Map<string, string[]>();
  const aggregates: { id: string; elidedCount: number }[] = [];
  const edgeSeen = new Set<string>();

  for (const t of sorted) {
    if (t.parents === null) continue;
    // Already sorted, so a plain slice keeps the deterministic order below.
    const uniqueParents = [...new Set(t.parents)].sort(cmp);
    const internal = uniqueParents.filter((p) => known.has(p));
    const external = uniqueParents.filter((p) => !known.has(p));

    for (const p of internal) {
      const key = `${p}->${t.txid}`;
      if (edgeSeen.has(key)) continue;
      edgeSeen.add(key);
      internalEdges.push({ from: p, to: t.txid });
    }

    // Cap external parents only -- internal edges are real cluster
    // relationships and are never elided. Capping is decided per
    // transaction; a parent kept for one child but beyond another child's
    // cap simply gets no edge from that other child (folded into its count).
    const keptExternal = external.slice(0, MAX_STUBS_PER_TX);
    const elidedCount = external.length - keptExternal.length;

    for (const p of keptExternal) {
      const key = `${p}->${t.txid}`;
      if (edgeSeen.has(key)) continue;
      edgeSeen.add(key);
      stubEdges.push({ from: p, to: t.txid });
      const list = stubChildren.get(p) ?? [];
      list.push(t.txid);
      stubChildren.set(p, list);
    }

    if (elidedCount > 0) {
      const id = aggregateId(t.txid);
      aggregates.push({ id, elidedCount });
      aggregateEdges.push({ from: id, to: t.txid });
    }
  }

  const txids = sorted.map((t) => t.txid);
  const cyclic = findCyclicTargets(txids, internalEdges);

  const parentsOf = new Map<string, string[]>(txids.map((id) => [id, []]));
  for (const e of internalEdges) {
    if (cyclic.has(e.to)) continue;
    parentsOf.get(e.to)?.push(e.from);
  }
  for (const e of stubEdges) {
    parentsOf.get(e.to)?.push(e.from);
  }
  for (const e of aggregateEdges) {
    parentsOf.get(e.to)?.push(e.from);
  }

  const stubIds = [...stubChildren.keys()].sort(cmp);
  const graphNodes: GraphNodeDatum[] = [
    ...sorted.map((t) => ({
      txid: t.txid,
      kind: 'tx' as const,
      r: t.r,
      inputsUnknown: t.parents === null,
      parentIds: parentsOf.get(t.txid) ?? [],
    })),
    ...stubIds.map((id) => ({
      txid: id,
      kind: 'stub' as const,
      r: STUB_R,
      inputsUnknown: false,
      parentIds: [],
    })),
    // Source nodes with no parents of their own, one per transaction with
    // elided parents -- built in the already-deterministic `sorted` order.
    ...aggregates.map((a) => ({
      txid: a.id,
      kind: 'aggregate' as const,
      r: AGGREGATE_R,
      inputsUnknown: false,
      parentIds: [],
      elidedCount: a.elidedCount,
    })),
  ];

  const edges: TxDagEdge[] = [
    ...internalEdges.map((e) => ({ ...e, back: cyclic.has(e.to) })),
    ...stubEdges.map((e) => ({ ...e, back: false })),
    ...aggregateEdges.map((e) => ({ ...e, back: false })),
  ].sort((a, b) => cmp(a.from, b.from) || cmp(a.to, b.to));

  const { nodes, width, height } = layoutWithD3Dag(graphNodes, orientation);
  nodes.sort((a, b) => cmp(a.txid, b.txid));

  return { nodes, edges, width, height };
}

export interface Point {
  x: number;
  y: number;
}
