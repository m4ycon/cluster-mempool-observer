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

interface TxDagNodeBase {
  txid: string;
  layer: number;
  x: number;
  y: number;
  /**
   * Half the footprint in output coordinates: `r` for a circle, the text
   * box for a label. Text never rotates with the orientation.
   */
  halfW: number;
  halfH: number;
}

/** A cluster member, drawn as a circle. */
export interface TxDagTxNode extends TxDagNodeBase {
  kind: 'tx';
  r: number;
  /** Row exists but its parents are unknown -- not the same as having none. */
  inputsUnknown: boolean;
}

/** An input from outside the cluster, drawn as bare text. */
export interface TxDagStubNode extends TxDagNodeBase {
  kind: 'stub';
  label: string;
}

/** External inputs past the per-tx cap, drawn as one bare-text count. */
export interface TxDagAggregateNode extends TxDagNodeBase {
  kind: 'aggregate';
  label: string;
  elidedCount: number;
}

export type TxDagNode = TxDagTxNode | TxDagStubNode | TxDagAggregateNode;

export type TxDagNodeKind = TxDagNode['kind'];

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

/** Font size, in layout units, of the bare-text stub/aggregate labels. */
export const EXTERNAL_LABEL_FONT_SIZE = 9;

// JetBrains Mono's advance width is 600/1000 em; the box is sized from it
// because d3-dag must reserve the room before anything is measured.
const MONO_ADVANCE_EM = 0.6;

const STUB_LABEL_CHARS = 6;

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

// Distributes over the union, so each variant keeps its own fields and its
// `kind`; a plain Omit<TxDagNode, ...> would collapse them into one shape.
type Unplaced<N> = N extends TxDagNode ? Omit<N, 'layer' | 'x' | 'y'> : never;
type UnplacedNode = Unplaced<TxDagNode>;

interface GraphNodeDatum {
  node: UnplacedNode;
  parentIds: string[];
}

/** Prefixed so it can never collide with a real (64 lowercase hex char) txid. */
function aggregateId(txid: string): string {
  return `agg:${txid}`;
}

function aggregateLabel(elidedCount: number): string {
  return `+${elidedCount} others`;
}

function labelBox(label: string): { halfW: number; halfH: number } {
  return {
    halfW: (label.length * MONO_ADVANCE_EM * EXTERNAL_LABEL_FONT_SIZE) / 2,
    halfH: EXTERNAL_LABEL_FONT_SIZE / 2,
  };
}

export interface Line {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/**
 * An edge clipped so it touches both marks instead of running under them. It
 * leaves a circle from its rim, and a label from the end of its text facing
 * downstream: the right end when horizontal, the bottom when vertical. Only a
 * cluster member is ever a child, so `to` is always a circle.
 */
export function edgeLine(
  from: TxDagNode,
  to: TxDagTxNode,
  orientation: TxDagOrientation,
): Line {
  const start =
    from.kind === 'tx'
      ? { x: from.x, y: from.y }
      : orientation === 'horizontal'
        ? { x: from.x + from.halfW, y: from.y }
        : { x: from.x, y: from.y + from.halfH };
  const dx = to.x - start.x;
  const dy = to.y - start.y;
  const dist = Math.hypot(dx, dy) || 1;
  const ux = dx / dist;
  const uy = dy / dist;
  const fromR = from.kind === 'tx' ? from.r : 0;
  return {
    x1: start.x + ux * fromR,
    y1: start.y + uy * fromR,
    x2: to.x - ux * to.r,
    y2: to.y - uy * to.r,
  };
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

function place(
  node: UnplacedNode,
  layer: number,
  x: number,
  y: number,
): TxDagNode {
  return { ...node, layer, x, y };
}

/** Single-row (or column) fallback so an unanticipated d3-dag throw never escapes. */
function fallbackStack(
  graphNodes: GraphNodeDatum[],
  orientation: TxDagOrientation,
): Positioned {
  const halfAlong = ({ node }: GraphNodeDatum) =>
    orientation === 'horizontal' ? node.halfW : node.halfH;
  const halfAcross = ({ node }: GraphNodeDatum) =>
    orientation === 'horizontal' ? node.halfH : node.halfW;
  const nodes: TxDagNode[] = [];
  let cursor = PADDING;
  let prevHalf = 0;
  graphNodes.forEach((d, i) => {
    if (i > 0) cursor += 2 * prevHalf + LAYER_GAP;
    prevHalf = halfAlong(d);
    const along = cursor + halfAlong(d);
    const across = PADDING + halfAcross(d);
    nodes.push(
      place(
        d.node,
        i,
        orientation === 'horizontal' ? along : across,
        orientation === 'horizontal' ? across : along,
      ),
    );
  });
  const alongTotal =
    graphNodes.length === 0 ? 0 : cursor + 2 * prevHalf + PADDING;
  const maxHalfAcross = Math.max(0, ...graphNodes.map(halfAcross));
  const acrossTotal = 2 * PADDING + 2 * maxHalfAcross;
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
      .id((d: GraphNodeDatum) => d.node.txid)
      .parentIds((d: GraphNodeDatum) => d.parentIds);
    const graph = stratify(graphNodes);

    // topDown(false): sources (stubs, and coinbase/unknown-input txs)
    // sit as close as possible to what they feed, instead of all pinning
    // to layer 0 and stranding a wall of grey stubs on the graph's left edge.
    const layout = sugiyama()
      .layering(layeringLongestPath().topDown(false))
      // d3-dag's own frame: horizontal swaps its axes on the way out (see
      // below), so a text box must be swapped on the way in.
      .nodeSize((node: GraphNode<GraphNodeDatum, undefined>) => {
        const w = 2 * node.data.node.halfW;
        const h = 2 * node.data.node.halfH;
        return orientation === 'horizontal'
          ? ([h, w] as const)
          : ([w, h] as const);
      })
      .gap([NODE_GAP, LAYER_GAP]);
    const { width, height } = layout(graph);

    const laidOut = [...graph.nodes()];
    const layerYs = [...new Set(laidOut.map((n) => n.y))].sort((a, b) => a - b);
    const layerOf = new Map(layerYs.map((y, i) => [y, i]));

    // Horizontal swaps d3-dag's (x, y) on the way out: its layer axis (y)
    // becomes our horizontal axis, its within-layer spread (x) becomes our
    // vertical one. Vertical keeps d3-dag's own frame as-is.
    const nodes: TxDagNode[] = laidOut.map((n) =>
      place(
        n.data.node,
        layerOf.get(n.y) ?? 0,
        (orientation === 'horizontal' ? n.y : n.x) + PADDING,
        (orientation === 'horizontal' ? n.x : n.y) + PADDING,
      ),
    );

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
    // One parent past the cap is shown rather than collapsed: an aggregate
    // standing for a single txid hides it while saving no space.
    const keptExternal =
      external.length > MAX_STUBS_PER_TX + 1
        ? external.slice(0, MAX_STUBS_PER_TX)
        : external;
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
      node: {
        txid: t.txid,
        kind: 'tx' as const,
        r: t.r,
        halfW: t.r,
        halfH: t.r,
        inputsUnknown: t.parents === null,
      },
      parentIds: parentsOf.get(t.txid) ?? [],
    })),
    ...stubIds.map((id) => {
      const label = id.slice(0, STUB_LABEL_CHARS);
      return {
        node: { txid: id, kind: 'stub' as const, label, ...labelBox(label) },
        parentIds: [],
      };
    }),
    // Source nodes with no parents of their own, one per transaction with
    // elided parents -- built in the already-deterministic `sorted` order.
    ...aggregates.map((a) => {
      const label = aggregateLabel(a.elidedCount);
      return {
        node: {
          txid: a.id,
          kind: 'aggregate' as const,
          label,
          elidedCount: a.elidedCount,
          ...labelBox(label),
        },
        parentIds: [],
      };
    }),
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
