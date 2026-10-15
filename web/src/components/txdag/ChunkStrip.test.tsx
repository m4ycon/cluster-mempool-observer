import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ClusterChunkRef } from '../../types/events';
import { ChunkStrip, type ChunkStripProps } from './ChunkStrip';

const CHUNKS: ClusterChunkRef[] = [
  { txids: ['a', 'b'], fee: 3000, vsize: 300 }, // 10 s/vB
  { txids: ['c'], fee: 500, vsize: 100 }, // 5 s/vB
];

function renderStrip(overrides: Partial<ChunkStripProps> = {}) {
  return render(
    <ChunkStrip
      chunks={CHUNKS}
      colors={['#111111', '#222222']}
      highlighted={null}
      selected={null}
      onHoverChunk={() => {}}
      {...overrides}
    />,
  );
}

function bar(chunk: number) {
  const g = screen
    .getAllByTestId('chunk-bar')
    .find((el) => el.getAttribute('data-chunk') === String(chunk));
  if (!g) throw new Error(`no bar for chunk ${chunk}`);
  // The second rect is the visible bar; the first is the hit area.
  return { g, rect: g.querySelectorAll('rect')[1] };
}

describe('ChunkStrip', () => {
  it('draws one bar per chunk, as wide as its share of the vsize', () => {
    renderStrip();
    expect(screen.getAllByTestId('chunk-bar')).toHaveLength(2);
    const w0 = Number(bar(0).rect.getAttribute('width'));
    const w1 = Number(bar(1).rect.getAttribute('width'));
    expect(w0 / w1).toBeCloseTo(3);
  });

  it('makes the bar as tall as its fee-rate against the highest one', () => {
    renderStrip();
    const h0 = Number(bar(0).rect.getAttribute('height'));
    const h1 = Number(bar(1).rect.getAttribute('height'));
    expect(h1 / h0).toBeCloseTo(0.5);
  });

  it("fills each bar with its chunk's colour", () => {
    renderStrip();
    expect(bar(0).rect).toHaveAttribute('fill', '#111111');
    expect(bar(1).rect).toHaveAttribute('fill', '#222222');
  });

  it('reports the chunk under the pointer, and null when it leaves', () => {
    const onHoverChunk = vi.fn();
    renderStrip({ onHoverChunk });
    fireEvent.pointerEnter(bar(1).g);
    expect(onHoverChunk).toHaveBeenLastCalledWith(1);
    fireEvent.pointerLeave(bar(1).g);
    expect(onHoverChunk).toHaveBeenLastCalledWith(null);
  });

  it('dims every chunk but the highlighted one', () => {
    renderStrip({ highlighted: 0 });
    expect(bar(0).g).not.toHaveClass('opacity-25');
    expect(bar(1).g).toHaveClass('opacity-25');
  });

  it("outlines the selected transaction's chunk", () => {
    renderStrip({ selected: 1 });
    expect(screen.getAllByTestId('chunk-bar-selected')).toHaveLength(1);
    expect(
      bar(1).g.querySelector('[data-testid="chunk-bar-selected"]'),
    ).not.toBeNull();
  });

  it('labels a wide enough bar with its fee-rate', () => {
    renderStrip();
    expect(screen.getByText('10.0')).toBeInTheDocument();
    expect(screen.getByText('5.0')).toBeInTheDocument();
  });

  it('renders nothing for a cluster without chunks', () => {
    const { container } = renderStrip({ chunks: [], colors: [] });
    expect(container).toBeEmptyDOMElement();
  });
});
