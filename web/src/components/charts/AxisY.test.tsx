import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ChartTick, PlotRect } from '../../lib/chartPlot';
import { AxisY, type AxisYProps } from './AxisY';

const PLOT: PlotRect = { left: 64, top: 16, width: 880, height: 264 };

const TICKS: ChartTick[] = [
  { value: 0, px: 280 },
  { value: 500, px: 148 },
  { value: 1000, px: 16 },
];

function axis(over: Partial<AxisYProps> = {}) {
  const { container } = render(
    <svg role="img" aria-label="host">
      <AxisY plot={PLOT} ticks={TICKS} format={(v) => String(v)} {...over} />
    </svg>,
  );
  return container;
}

/** `x1 -> x2` of every line, which is what separates the two tick styles. */
function spans(container: Element) {
  return Array.from(container.querySelectorAll('line')).map(
    (l) => `${l.getAttribute('x1')}->${l.getAttribute('x2')}`,
  );
}

function titleTransform(over: Partial<AxisYProps> = {}) {
  return Array.from(
    axis({ label: 'clusters', ...over }).querySelectorAll('text'),
  )
    .find((t) => t.textContent === 'clusters')
    ?.getAttribute('transform');
}

describe('AxisY', () => {
  it('draws an axis line with outward tick marks by default', () => {
    expect(spans(axis())).toEqual(['64->64', '60->64', '60->64', '60->64']);
  });

  it('draws gridlines across the plot instead, with no axis line', () => {
    expect(spans(axis({ gridlines: true }))).toEqual([
      '64->944',
      '64->944',
      '64->944',
    ]);
  });

  it('labels every tick through the caller-supplied format', () => {
    const texts = Array.from(axis().querySelectorAll('text')).map(
      (t) => t.textContent,
    );
    expect(texts).toEqual(['0', '500', '1000']);
  });

  it('rotates the title upright and keeps it clear of the tick labels', () => {
    expect(titleTransform()).toBe('translate(19, 148) rotate(-90)');
  });

  it('tucks the title closer when the tick labels are narrower', () => {
    expect(titleTransform({ format: (v) => String(v).slice(0, 1) })).toBe(
      'translate(41, 148) rotate(-90)',
    );
  });

  it('keeps the title inside the viewBox when the tick labels are wide', () => {
    expect(titleTransform({ format: (v) => `${v} transactions` })).toBe(
      'translate(10, 148) rotate(-90)',
    );
  });
});
