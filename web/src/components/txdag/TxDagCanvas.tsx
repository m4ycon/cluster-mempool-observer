import clsx from 'clsx';
import {
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import {
  clamp,
  fitToView,
  MAX_K,
  MIN_K,
  screenToLayout,
  type Viewport,
  viewScale,
  zoomAbout,
} from '../../lib/svgViewport';
import {
  type TxDagInput,
  type TxDagNode,
  type TxDagOrientation,
  txDagLayout,
} from '../../lib/txDagLayout';
import {
  FEE_UNKNOWN_COLOR,
  INPUTS_UNKNOWN_COLOR,
  txColor,
  txRadius,
  txScaleFor,
  txValue,
  UNIFORM_R,
  VSIZE_UNKNOWN_COLOR,
} from '../../lib/txMetrics';
import type { TransactionRef } from '../../types/events';
import { TooltipBubble } from '../TooltipBubble';
import { VizButton } from '../VizButton';

export interface TxDagCanvasProps {
  txids: string[];
  txs: Map<string, TransactionRef>;
  missing: Set<string>;
  loading: boolean;
  error: Error | null;
  selectedTxid: string | null;
  onSelectTxid: (txid: string) => void;
  /** Shown when `txids` is empty; the caller knows why it is empty. */
  emptyLabel?: string;
}

const VIEW_W = 720;
const VIEW_H = 480;
const VIEW = { w: VIEW_W, h: VIEW_H };
const FIT_PADDING = 0;
const IDENTITY_VP: Viewport = { tx: 0, ty: 0, k: 1 };

// Below this on-screen radius a 6-char label just clutters a dense
// (up to 64-node) graph -- checked against r * k, the actual screen size,
// not the layout-space r, since labels must stay legible at any zoom.
const LABEL_MIN_R = 9;
const BASE_FONT_SIZE = 12;

const ZOOM_SENSITIVITY = 0.002;

// Missing-field badges: a stack of dots off the node's upper-right edge,
// one colour per unresolved field, closest-to-farthest in a fixed order so
// the same field always lands in the same slot. Layout-space units, so
// they scale with the graph as the user zooms.
const BADGE_R = 3;
const BADGE_GAP = 7;
const BADGE_ANGLE = -Math.PI / 4;

const BADGE_FIELD_LABEL: Record<string, string> = {
  inputs: 'unknown inputs',
  vsize: 'unknown vsize',
  fee: 'unknown fee',
};

interface BadgeTip {
  field: string;
  x: number;
  y: number;
}

// Fixed encoding, independent of the cluster page's SIZE/COLOR BY controls.
const SIZE_METRIC: ClusterMetric = 'vsize';
const COLOR_METRIC: ClusterMetric = 'feerate';

interface PanState {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startTx: number;
  startTy: number;
}

/** Zoom ceiling for the initial fit. */
function maxFitK(nodeCount: number): number {
  return nodeCount === 1 ? 1 : MAX_K;
}

/** Interactive SVG render of a transaction DAG: pan and zoom. */
export function TxDagCanvas({
  txids,
  txs,
  missing,
  loading,
  error,
  selectedTxid,
  onSelectTxid,
  emptyLabel = 'nothing to graph',
}: TxDagCanvasProps) {
  const arrowId = useId();
  const [svgEl, setSvgEl] = useState<SVGSVGElement | null>(null);
  const panStateRef = useRef<PanState | null>(null);
  // True once the user has panned/zoomed; gates the fit-on-layout-change
  // effect below so a live websocket tick can't yank the view back.
  const touchedRef = useRef(false);
  const [orientation, setOrientation] =
    useState<TxDagOrientation>('horizontal');
  // Badge hover tooltip; position read from getBoundingClientRect, which
  // already accounts for the SVG's pan/zoom transform.
  const [badgeTip, setBadgeTip] = useState<BadgeTip | null>(null);

  const sizeValues = txids.map((txid) => {
    const tx = txs.get(txid);
    return tx ? txValue(tx, SIZE_METRIC) : null;
  });
  const finiteSize = sizeValues.filter((v): v is number => v !== null);
  const sizeDomain = {
    min: finiteSize.length ? Math.min(...finiteSize) : 0,
    max: finiteSize.length ? Math.max(...finiteSize) : 0,
  };

  // One shared scale for every node, built once, not recomputed per mark.
  const colorValues = txids.map((txid) => {
    const tx = txs.get(txid);
    return tx ? txValue(tx, COLOR_METRIC) : null;
  });
  const colorScale = txScaleFor(COLOR_METRIC, colorValues);

  const inputs: TxDagInput[] = txids.map((txid) => {
    const tx = txs.get(txid);
    // Still loading or reported missing look identical here: neither is a
    // reason to blank the node, only to leave its parents unresolved.
    if (!tx) return { txid, parents: null, r: UNIFORM_R };
    return {
      txid,
      parents: tx.input_txids,
      r: txRadius(txValue(tx, SIZE_METRIC), sizeDomain),
    };
  });

  const layout = txDagLayout(inputs, orientation);
  const stubCount = layout.nodes.filter((n) => n.kind === 'stub').length;
  const elidedTotal = layout.nodes
    .filter((n) => n.kind === 'aggregate')
    .reduce((sum, n) => sum + (n.elidedCount ?? 0), 0);
  const missingCount = txids.filter((t) => missing.has(t)).length;

  const [viewport, setViewport] = useState<Viewport>(() =>
    fitToView(
      layout.width,
      layout.height,
      VIEW,
      FIT_PADDING,
      maxFitK(layout.nodes.length),
    ),
  );

  // Layout dims changed (new/departed transactions): snap to fit unless the
  // user has already taken the wheel, so live data can't fight their pan.
  useEffect(() => {
    if (!touchedRef.current) {
      setViewport(
        fitToView(
          layout.width,
          layout.height,
          VIEW,
          FIT_PADDING,
          maxFitK(layout.nodes.length),
        ),
      );
    }
  }, [layout.width, layout.height, layout.nodes.length]);

  useEffect(() => {
    const svg = svgEl;
    if (!svg) return;

    // React may bind `wheel` passively at the root; only an imperative
    // listener can reliably preventDefault to stop the page scrolling.
    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      touchedRef.current = true;
      const rect = svg.getBoundingClientRect();
      setViewport((prev) => {
        const nextK = clamp(
          prev.k * Math.exp(-e.deltaY * ZOOM_SENSITIVITY),
          MIN_K,
          MAX_K,
        );
        const layoutPt = screenToLayout(e.clientX, e.clientY, rect, VIEW, prev);
        const viewPt = screenToLayout(
          e.clientX,
          e.clientY,
          rect,
          VIEW,
          IDENTITY_VP,
        );
        return zoomAbout(layoutPt, viewPt, nextK);
      });
    };

    svg.addEventListener('wheel', handleWheel, { passive: false });
    return () => svg.removeEventListener('wheel', handleWheel);
  }, [svgEl]);

  const displayNodes = layout.nodes;
  const nodeByTxid = new Map(displayNodes.map((n) => [n.txid, n]));

  // New-transaction flash cue: compared and committed synchronously during
  // render (React's "adjust state during render" pattern), not in an effect.
  // An effect fires only after commit, and the sibling fit-on-layout-change
  // effect above can trigger a second render for the same update before
  // paint; that second render would then read an already-advanced "previous"
  // set from the first effect's write and miss the flash. Comparing here
  // avoids that ordering entirely.
  const currentTxidKey = displayNodes
    .map((n) => n.txid)
    .sort()
    .join(',');
  const [flashState, setFlashState] = useState<{
    seenTxidKey: string | null;
    flashedTxids: ReadonlySet<string>;
  }>({ seenTxidKey: null, flashedTxids: new Set() });
  let { flashedTxids } = flashState;
  if (currentTxidKey !== flashState.seenTxidKey) {
    // null seenTxidKey means first paint, which never flashes.
    const seen =
      flashState.seenTxidKey === null
        ? null
        : new Set(flashState.seenTxidKey.split(','));
    flashedTxids = seen
      ? new Set(
          displayNodes.filter((n) => !seen.has(n.txid)).map((n) => n.txid),
        )
      : new Set();
    setFlashState({ seenTxidKey: currentTxidKey, flashedTxids });
  }
  const isNewNode = (txid: string) => flashedTxids.has(txid);

  const ariaLabel =
    `Transaction graph: ${txids.length} transaction${txids.length === 1 ? '' : 's'}, ${stubCount} ` +
    `external parent${stubCount === 1 ? '' : 's'} shown` +
    (elidedTotal > 0 ? `, ${elidedTotal} more collapsed` : '') +
    (missingCount > 0 ? `, ${missingCount} missing` : '');

  // Aggregate nodes stand for many parents, not one transaction -- clicking
  // or keying one selects nothing.
  const handleClick = (node: TxDagNode) => () => {
    if (node.kind === 'aggregate') return;
    onSelectTxid(node.txid);
  };

  const handleKeyDown =
    (node: TxDagNode) => (e: KeyboardEvent<SVGGElement>) => {
      if (node.kind === 'aggregate') return;
      if (e.key !== 'Enter' && e.key !== ' ') return;
      e.preventDefault();
      onSelectTxid(node.txid);
    };

  const handleReset = () => {
    touchedRef.current = false;
    setViewport(
      fitToView(
        layout.width,
        layout.height,
        VIEW,
        FIT_PADDING,
        maxFitK(layout.nodes.length),
      ),
    );
  };

  // Reset the touched flag too: the layout's shape just changed entirely,
  // so a pan/zoom framed for the old orientation is meaningless here -- the
  // width/height-change effect above then refits for the new one.
  const handleOrientationChange = (next: TxDagOrientation) => {
    if (next === orientation) return;
    touchedRef.current = false;
    setOrientation(next);
  };

  const handleBackgroundPointerDown = (
    e: ReactPointerEvent<SVGRectElement>,
  ) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    touchedRef.current = true;
    panStateRef.current = {
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startTx: viewport.tx,
      startTy: viewport.ty,
    };
  };

  const handleBackgroundPointerMove = (
    e: ReactPointerEvent<SVGRectElement>,
  ) => {
    const pan = panStateRef.current;
    const rect = svgEl?.getBoundingClientRect();
    if (!pan || pan.pointerId !== e.pointerId || !rect) return;
    // One shared scale for both axes -- see viewScale's doc for why using
    // rect.width/height independently per axis breaks under letterboxing.
    const scale = viewScale(rect, VIEW);
    const dTx = (e.clientX - pan.startClientX) / scale;
    const dTy = (e.clientY - pan.startClientY) / scale;
    setViewport((prev) => ({
      ...prev,
      tx: pan.startTx + dTx,
      ty: pan.startTy + dTy,
    }));
  };

  const endBackgroundPan = (e: ReactPointerEvent<SVGRectElement>) => {
    if (panStateRef.current?.pointerId === e.pointerId) {
      panStateRef.current = null;
    }
  };

  if (txids.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        {emptyLabel}
      </div>
    );
  }

  // No node has data yet, so the layout below has no edges and degenerates
  // into a loose column -- show loading instead. Checked against cached
  // data, not the raw `loading` flag: that flag also flips true for
  // background retries of stale rows, which must not blank a working graph.
  if (loading && !txids.some((txid) => txs.has(txid))) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        loading transactions...
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col gap-1">
      <div className="flex items-center justify-between gap-2">
        <div>
          {error ? (
            <span className="text-alert text-xs">
              some transactions failed to load; showing what is cached
            </span>
          ) : (
            loading && (
              <span className="text-faint text-xs">
                loading transactions...
              </span>
            )
          )}
        </div>
        <div className="flex items-center gap-1 p-1">
          <VizButton
            active={orientation === 'horizontal'}
            onClick={() => handleOrientationChange('horizontal')}
            ariaLabel="horizontal layout"
          >
            →
          </VizButton>
          <VizButton
            active={orientation === 'vertical'}
            onClick={() => handleOrientationChange('vertical')}
            ariaLabel="vertical layout"
          >
            ↓
          </VizButton>
          <VizButton
            active={false}
            onClick={handleReset}
            ariaLabel="reset view"
          >
            RESET
          </VizButton>
        </div>
      </div>
      <svg
        ref={setSvgEl}
        width="100%"
        height="100%"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="xMidYMid meet"
        className="flex-1 touch-none"
        role="img"
        aria-label={ariaLabel}
        data-testid="tx-dag"
        data-orientation={orientation}
        data-node-count={layout.nodes.length}
        data-stub-count={stubCount}
        data-edge-count={layout.edges.length}
      >
        <defs>
          <marker
            id={arrowId}
            markerWidth={6}
            markerHeight={6}
            refX={5}
            refY={3}
            orient="auto-start-reverse"
          >
            <path d="M0,0 L6,3 L0,6 Z" className="fill-faint" />
          </marker>
        </defs>
        <rect
          x={0}
          y={0}
          width={VIEW_W}
          height={VIEW_H}
          fill="transparent"
          onPointerDown={handleBackgroundPointerDown}
          onPointerMove={handleBackgroundPointerMove}
          onPointerUp={endBackgroundPan}
          onPointerCancel={endBackgroundPan}
        />
        <g
          transform={
            `translate(${viewport.tx} ${viewport.ty}) ` + `scale(${viewport.k})`
          }
          data-testid="tx-dag-viewport"
          data-zoom={viewport.k}
          data-pan-x={viewport.tx}
          data-pan-y={viewport.ty}
        >
          {layout.edges.map((edge) => {
            const from = nodeByTxid.get(edge.from);
            const to = nodeByTxid.get(edge.to);
            if (!from || !to) return null;
            // Trim to each circle's boundary so the arrowhead lands beside
            // the child mark rather than being buried under its fill.
            const dx = to.x - from.x;
            const dy = to.y - from.y;
            const dist = Math.hypot(dx, dy) || 1;
            const ux = dx / dist;
            const uy = dy / dist;
            return (
              <line
                key={`${edge.from}->${edge.to}`}
                x1={from.x + ux * from.r}
                y1={from.y + uy * from.r}
                x2={to.x - ux * to.r}
                y2={to.y - uy * to.r}
                className="stroke-faint"
                strokeWidth={1}
                strokeDasharray={edge.back ? '4 3' : undefined}
                vectorEffect="non-scaling-stroke"
                markerEnd={`url(#${arrowId})`}
              />
            );
          })}

          {displayNodes.map((node) => {
            const isStub = node.kind === 'stub';
            const isAggregate = node.kind === 'aggregate';
            const tx = isStub || isAggregate ? undefined : txs.get(node.txid);
            const colorVal = tx ? txValue(tx, COLOR_METRIC) : null;
            // Stubs/aggregates carry no fields to begin with (see their
            // fill/stroke below), so badges are only ever for a real tx row.
            const badges =
              isStub || isAggregate
                ? []
                : [
                    node.inputsUnknown && {
                      field: 'inputs' as const,
                      color: INPUTS_UNKNOWN_COLOR,
                    },
                    (!tx || tx.vsize === 0) && {
                      field: 'vsize' as const,
                      color: VSIZE_UNKNOWN_COLOR,
                    },
                    (!tx || tx.fee === null) && {
                      field: 'fee' as const,
                      color: FEE_UNKNOWN_COLOR,
                    },
                  ].filter(
                    (
                      b,
                    ): b is {
                      field: 'inputs' | 'vsize' | 'fee';
                      color: string;
                    } => !!b,
                  );
            const unresolved = !isStub && !isAggregate && badges.length > 0;
            const unknown = isStub || isAggregate || unresolved;
            // Aggregate stands for the same thing a stub does -- an input
            // outside the fetched set -- so it wears the same fill.
            const fill =
              isStub || isAggregate
                ? 'none'
                : txColor(colorVal, colorScale, COLOR_METRIC);
            const selected = node.txid === selectedTxid;
            // Stands for many parents, not one txid -- give focus/hover a
            // label people can actually read.
            const nodeLabel = isAggregate
              ? `${node.elidedCount} more external parent${node.elidedCount === 1 ? '' : 's'}`
              : badges.length > 0
                ? `${node.txid} (missing ${badges.map((b) => b.field).join(', ')})`
                : node.txid;

            return (
              // biome-ignore lint/a11y/noStaticElementInteractions: selectable data point; Enter/Space already covered by onKeyDown
              <g
                // Keyed on txid alone: identity never changes, only appears
                // or disappears, so any other key would remount and
                // re-flash every node on each re-layout.
                key={node.txid}
                tabIndex={isAggregate ? -1 : 0}
                aria-label={nodeLabel}
                className={clsx(
                  'cursor-pointer outline-none',
                  isNewNode(node.txid) && 'animate-mark-new',
                )}
                onClick={handleClick(node)}
                onKeyDown={handleKeyDown(node)}
                data-testid="tx-dag-node"
                data-txid={node.txid}
                data-kind={node.kind}
                data-unknown={unknown}
              >
                <title>{nodeLabel}</title>
                <circle
                  cx={node.x}
                  cy={node.y}
                  r={node.r}
                  fill={fill}
                  strokeWidth={
                    selected ? 2 : isStub || isAggregate || unresolved ? 1 : 0
                  }
                  strokeDasharray={unresolved ? '3 2' : undefined}
                  vectorEffect="non-scaling-stroke"
                  className={clsx(
                    (isStub || isAggregate || unresolved) && 'stroke-faint',
                    selected && 'stroke-ink',
                  )}
                />
                {selected && (
                  <circle
                    cx={node.x}
                    cy={node.y}
                    r={node.r + 3}
                    fill="none"
                    strokeWidth={2}
                    vectorEffect="non-scaling-stroke"
                    className="stroke-orange"
                  />
                )}
                {badges.map((badge, i) => {
                  const dist = node.r + BADGE_R + 2 + i * BADGE_GAP;
                  return (
                    <circle
                      key={badge.field}
                      data-testid="tx-dag-badge"
                      data-field={badge.field}
                      cx={node.x + Math.cos(BADGE_ANGLE) * dist}
                      cy={node.y + Math.sin(BADGE_ANGLE) * dist}
                      r={BADGE_R}
                      fill={badge.color}
                      stroke="var(--color-bg)"
                      strokeWidth={1}
                      vectorEffect="non-scaling-stroke"
                      onPointerEnter={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        setBadgeTip({
                          field: badge.field,
                          x: r.left + r.width / 2,
                          y: r.top,
                        });
                      }}
                      onPointerLeave={() => setBadgeTip(null)}
                    />
                  );
                })}
              </g>
            );
          })}

          {displayNodes
            // A count with no label is meaningless, so aggregate nodes
            // always get one; other kinds only above the legibility floor.
            .filter(
              (node) =>
                node.kind === 'aggregate' || node.r * viewport.k > LABEL_MIN_R,
            )
            .map((node) => (
              <text
                key={node.txid}
                x={node.x}
                y={node.y + 3}
                textAnchor="middle"
                style={{ fontSize: BASE_FONT_SIZE / viewport.k }}
                className="pointer-events-none select-none fill-ink font-mono"
              >
                {node.kind === 'aggregate'
                  ? `+${node.elidedCount}`
                  : node.txid.slice(0, 6)}
              </text>
            ))}
        </g>
      </svg>
      {badgeTip && (
        <TooltipBubble
          style={{
            position: 'fixed',
            left: badgeTip.x,
            top: badgeTip.y,
            transform: 'translate(-50%, calc(-100% - 6px))',
          }}
        >
          {BADGE_FIELD_LABEL[badgeTip.field]}
        </TooltipBubble>
      )}
    </div>
  );
}
