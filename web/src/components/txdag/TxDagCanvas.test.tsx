import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MAX_STUBS_PER_TX } from '../../lib/txDagLayout';
import { UNIFORM_R } from '../../lib/txMetrics';
import type { TransactionRef } from '../../types/events';
import { TxDagCanvas, type TxDagCanvasProps } from './TxDagCanvas';

// jsdom has no Pointer Capture support at all; stub it so the component's
// setPointerCapture calls don't throw when a test fires pointer events.
if (!Element.prototype.setPointerCapture) {
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
}

function tx(overrides: Partial<TransactionRef> = {}): TransactionRef {
  return {
    txid: 'a',
    fee: 500,
    vsize: 200,
    first_seen_at: '2026-01-01T00:00:00Z',
    cluster_id: 1,
    hollow: false,
    input_txids: [],
    ...overrides,
  };
}

const BASE_PROPS: TxDagCanvasProps = {
  txids: [],
  txs: new Map(),
  missing: new Set(),
  loading: false,
  error: null,
  selectedTxid: null,
  onSelectTxid: () => {},
};

function renderDag(overrides: Partial<TxDagCanvasProps> = {}) {
  return render(<TxDagCanvas {...BASE_PROPS} {...overrides} />);
}

function nodes(container: HTMLElement) {
  return Array.from(container.querySelectorAll('[data-testid="tx-dag-node"]'));
}

describe('TxDagCanvas node rendering', () => {
  it('renders one node per txid, with no external parents in this chain', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
      ['b', tx({ txid: 'b', input_txids: ['a'] })],
      ['c', tx({ txid: 'c', input_txids: ['b'] })],
    ]);

    const { container } = renderDag({ txids: ['a', 'b', 'c'], txs });

    expect(nodes(container)).toHaveLength(3);
    const wrapper = screen.getByTestId('tx-dag');
    expect(wrapper).toHaveAttribute('data-node-count', '3');
    expect(wrapper).toHaveAttribute('data-stub-count', '0');
  });

  it('renders a parent outside the requested set as a stub node', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: ['outside'] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    const stub = nodes(container).find(
      (n) => n.getAttribute('data-kind') === 'stub',
    );
    expect(stub).toBeDefined();
    expect(stub).toHaveAttribute('data-txid', 'outside');
  });

  it('marks a row with an unknown feerate as unresolved', () => {
    // fee null makes feerate (the fixed color metric) unresolvable too.
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', fee: null, input_txids: [] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    const [node] = nodes(container);
    expect(node).toHaveAttribute('data-unknown', 'true');
  });

  it('gives an unknown-vsize node the uniform default radius', () => {
    // vsize 0 is a hollow row's unknown-value placeholder (see txValue).
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', vsize: 0, input_txids: [] })],
      ['b', tx({ txid: 'b', vsize: 500, input_txids: ['a'] })],
    ]);

    const { container } = renderDag({ txids: ['a', 'b'], txs });

    const radiusOf = (txid: string) =>
      nodes(container)
        .find((n) => n.getAttribute('data-txid') === txid)
        ?.querySelector('circle')
        ?.getAttribute('r');

    expect(radiusOf('a')).toBe(String(UNIFORM_R));
    expect(radiusOf('b')).not.toBe(String(UNIFORM_R));
  });

  it('still renders a node for a txid missing from the tx cache', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);
    const missing = new Set(['b']);

    const { container } = renderDag({ txids: ['a', 'b'], txs, missing });

    expect(nodes(container)).toHaveLength(2);
    const bNode = nodes(container).find(
      (n) => n.getAttribute('data-txid') === 'b',
    );
    expect(bNode).toHaveAttribute('data-kind', 'tx');
    expect(bNode).toHaveAttribute('data-unknown', 'true');
  });

  it('shows the default placeholder instead of an empty graph', () => {
    renderDag({ txids: [] });

    expect(screen.getByText('nothing to graph')).toBeInTheDocument();
    expect(screen.queryByTestId('tx-dag')).not.toBeInTheDocument();
  });

  it('shows the caller-supplied placeholder when given one', () => {
    renderDag({ txids: [], emptyLabel: 'select a cluster' });

    expect(screen.getByText('select a cluster')).toBeInTheDocument();
  });

  it('shows a loading placeholder instead of the graph before anything is cached', () => {
    renderDag({ txids: ['a', 'b'], txs: new Map(), loading: true });

    expect(screen.getByText('loading transactions...')).toBeInTheDocument();
    expect(screen.queryByTestId('tx-dag')).not.toBeInTheDocument();
  });

  it('keeps rendering the graph while loading once some transactions are cached', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    renderDag({ txids: ['a', 'b'], txs, loading: true });

    expect(screen.getByTestId('tx-dag')).toBeInTheDocument();
  });

  it('collapses external parents beyond the cap into one labelled aggregate node', () => {
    const parents = Array.from({ length: MAX_STUBS_PER_TX + 7 }, (_, i) =>
      String(i).padStart(2, '0'),
    );
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: parents })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    const aggregate = nodes(container).find(
      (n) => n.getAttribute('data-kind') === 'aggregate',
    );
    expect(aggregate).toBeDefined();

    const elidedCount = parents.length - MAX_STUBS_PER_TX;
    const labels = Array.from(container.querySelectorAll('text')).map(
      (t) => t.textContent,
    );
    expect(labels).toContain(`+${elidedCount}`);
  });

  it('fires onSelectTxid on click', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);
    const onSelectTxid = vi.fn();

    const { container } = renderDag({ txids: ['a'], txs, onSelectTxid });

    const [node] = nodes(container);
    fireEvent.click(node);

    expect(onSelectTxid).toHaveBeenCalledWith('a');
  });
});

describe('TxDagCanvas missing-field badges', () => {
  function badgeFieldsFor(container: HTMLElement, txid: string) {
    return Array.from(
      container.querySelectorAll(
        `[data-txid="${txid}"] [data-testid="tx-dag-badge"]`,
      ),
    ).map((b) => b.getAttribute('data-field'));
  }

  it('shows no badges when fee, vsize, and inputs are all known', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    expect(badgeFieldsFor(container, 'a')).toEqual([]);
  });

  it('shows only a red inputs badge when just inputs are unknown', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: null })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    expect(badgeFieldsFor(container, 'a')).toEqual(['inputs']);
  });

  it('shows vsize and fee badges together when both are unknown but inputs are known', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', vsize: 0, fee: null, input_txids: [] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    expect(badgeFieldsFor(container, 'a')).toEqual(['vsize', 'fee']);
  });

  it('shows all three badges for a txid not yet in the cache', () => {
    const { container } = renderDag({ txids: ['a'], txs: new Map() });

    expect(badgeFieldsFor(container, 'a').sort()).toEqual([
      'fee',
      'inputs',
      'vsize',
    ]);
  });

  it('never badges a stub node, even though it carries no data', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: ['outside'] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    expect(badgeFieldsFor(container, 'outside')).toEqual([]);
  });
});

describe('TxDagCanvas badge tooltip', () => {
  it('shows nothing before a badge is hovered', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    renderDag({ txids: ['a'], txs });

    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('shows the field label on hover and hides it on leave', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: null })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    const badge = container.querySelector('[data-testid="tx-dag-badge"]');
    if (!badge) throw new Error('expected an inputs badge');
    badge.getBoundingClientRect = () =>
      ({ left: 100, top: 50, width: 6, height: 6 }) as DOMRect;

    fireEvent.pointerEnter(badge);
    expect(screen.getByRole('tooltip')).toHaveTextContent('unknown inputs');

    fireEvent.pointerLeave(badge);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });
});

describe('TxDagCanvas new-transaction cue', () => {
  it('flashes only a txid that was not in the previous render', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
      ['b', tx({ txid: 'b', input_txids: ['a'] })],
    ]);

    const { container, rerender } = renderDag({ txids: ['a'], txs });

    // First paint never flashes, even though the ref starts out empty.
    const [firstNode] = nodes(container);
    expect(firstNode).not.toHaveClass('animate-mark-new');

    rerender(<TxDagCanvas {...BASE_PROPS} txids={['a', 'b']} txs={txs} />);

    const byTxid = (txid: string) =>
      nodes(container).find((n) => n.getAttribute('data-txid') === txid);

    expect(byTxid('a')).not.toHaveClass('animate-mark-new');
    expect(byTxid('b')).toHaveClass('animate-mark-new');
  });
});

describe('TxDagCanvas pan/zoom viewport', () => {
  it('RESET restores the fitted zoom', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    renderDag({ txids: ['a'], txs });

    const viewportG = screen.getByTestId('tx-dag-viewport');
    const svg = screen.getByTestId('tx-dag');
    const initialZoom = viewportG.getAttribute('data-zoom');

    // Wheel zoom: no assertion on the resulting number (that lives in
    // svgViewport.test.ts) -- this only needs `k` to move off its fit value.
    fireEvent.wheel(svg, { deltaY: -500 });
    expect(viewportG.getAttribute('data-zoom')).not.toBe(initialZoom);

    fireEvent.click(screen.getByRole('button', { name: 'reset view' }));

    expect(viewportG.getAttribute('data-zoom')).toBe(initialZoom);
  });

  it('drags equally in x and y under a container wider than the viewBox aspect', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    const { container } = renderDag({ txids: ['a'], txs });

    const svg = screen.getByTestId('tx-dag');
    // VIEW is 720x480 (aspect 1.5); this rect is height-bound, so horizontal
    // is the letterboxed axis -- the case naive per-axis rectW/rectH ratios
    // got wrong (see viewScale in svgViewport.ts).
    svg.getBoundingClientRect = () =>
      ({ left: 0, top: 0, width: 900, height: 480 }) as DOMRect;

    const background = container.querySelector('rect');
    if (!background) throw new Error('expected the pan background rect');
    const viewportG = screen.getByTestId('tx-dag-viewport');
    const startTx = Number(viewportG.getAttribute('data-pan-x'));
    const startTy = Number(viewportG.getAttribute('data-pan-y'));

    fireEvent.pointerDown(background, {
      pointerId: 1,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(background, {
      pointerId: 1,
      clientX: 110,
      clientY: 110,
    });

    const dTx = Number(viewportG.getAttribute('data-pan-x')) - startTx;
    const dTy = Number(viewportG.getAttribute('data-pan-y')) - startTy;
    expect(dTx).toBeCloseTo(dTy);
  });
});

describe('TxDagCanvas graph identity', () => {
  // Two chains of identical shape, so the layout dimensions are the same
  // either way: nothing but resetKey can tell the canvas it is now showing
  // a different graph.
  const FIRST = new Map<string, TransactionRef>([
    ['a', tx({ txid: 'a' })],
    ['b', tx({ txid: 'b', input_txids: ['a'] })],
  ]);
  const SECOND = new Map<string, TransactionRef>([
    ['c', tx({ txid: 'c' })],
    ['d', tx({ txid: 'd', input_txids: ['c'] })],
  ]);

  function zoom() {
    // Outwards: the initial fit of a small graph can already sit at MAX_K,
    // where zooming in is clamped to a no-op.
    fireEvent.wheel(screen.getByTestId('tx-dag'), { deltaY: 500 });
  }

  function currentZoom() {
    return screen.getByTestId('tx-dag-viewport').getAttribute('data-zoom');
  }

  it('refits when resetKey changes, discarding the pan/zoom of the old graph', () => {
    const { rerender } = render(
      <TxDagCanvas
        {...BASE_PROPS}
        txids={['a', 'b']}
        txs={FIRST}
        resetKey={1}
      />,
    );
    const fitted = currentZoom();

    zoom();
    expect(currentZoom()).not.toBe(fitted);

    rerender(
      <TxDagCanvas
        {...BASE_PROPS}
        txids={['c', 'd']}
        txs={SECOND}
        resetKey={2}
      />,
    );

    expect(currentZoom()).toBe(fitted);
  });

  it('keeps the pan/zoom when the same graph gains a transaction', () => {
    const txs = new Map(FIRST);
    txs.set('e', tx({ txid: 'e', input_txids: ['b'] }));

    const { rerender } = render(
      <TxDagCanvas {...BASE_PROPS} txids={['a', 'b']} txs={txs} resetKey={1} />,
    );

    zoom();
    const zoomed = currentZoom();

    rerender(
      <TxDagCanvas
        {...BASE_PROPS}
        txids={['a', 'b', 'e']}
        txs={txs}
        resetKey={1}
      />,
    );

    expect(currentZoom()).toBe(zoomed);
  });
});

describe('TxDagCanvas orientation', () => {
  it('defaults to horizontal', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
    ]);

    renderDag({ txids: ['a'], txs });

    expect(screen.getByTestId('tx-dag')).toHaveAttribute(
      'data-orientation',
      'horizontal',
    );
  });

  it('switches to vertical when its toggle is clicked, and back', () => {
    const txs = new Map<string, TransactionRef>([
      ['a', tx({ txid: 'a', input_txids: [] })],
      ['b', tx({ txid: 'b', input_txids: ['a'] })],
    ]);

    renderDag({ txids: ['a', 'b'], txs });
    const svg = screen.getByTestId('tx-dag');

    fireEvent.click(screen.getByRole('button', { name: 'vertical layout' }));
    expect(svg).toHaveAttribute('data-orientation', 'vertical');

    fireEvent.click(screen.getByRole('button', { name: 'horizontal layout' }));
    expect(svg).toHaveAttribute('data-orientation', 'horizontal');
  });
});
