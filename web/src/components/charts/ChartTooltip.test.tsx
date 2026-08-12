import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { PlotRect } from '../../lib/chartPlot';
import { ChartTooltip, type ChartTooltipProps } from './ChartTooltip';

const PLOT: PlotRect = { left: 48, top: 16, width: 896, height: 276 };

/** Renders into a host svg and reads back the box's real geometry. */
function box(over: Partial<ChartTooltipProps> = {}) {
  const { container } = render(
    <svg role="img" aria-label="host">
      <ChartTooltip
        lines={['abc', 'de']}
        plot={PLOT}
        gap={12}
        rightOf={400}
        leftOf={400}
        anchorY={{ at: 100, align: 'center' }}
        {...over}
      />
    </svg>,
  );

  const rect = container.querySelector('rect') as SVGRectElement;
  const translate = container
    .querySelector('g')
    ?.getAttribute('transform')
    ?.match(/translate\(([-\d.]+), ([-\d.]+)\)/);

  return {
    width: Number(rect.getAttribute('width')),
    height: Number(rect.getAttribute('height')),
    x: Number(translate?.[1]),
    y: Number(translate?.[2]),
    texts: Array.from(container.querySelectorAll('text')),
  };
}

describe('ChartTooltip sizing', () => {
  it('sizes to the longest line, wherever it sits', () => {
    const base = box({ lines: ['ab', 'cd'] }).width;
    const long = box({ lines: ['ab', 'cdefgh'] }).width;

    expect(long).toBeGreaterThan(base);
    expect(box({ lines: ['cdefgh', 'ab'] }).width).toBe(long);
  });

  it('grows by one glyph advance per extra character', () => {
    const nine = box({ lines: ['aaaaaaaaa'] }).width;
    const ten = box({ lines: ['aaaaaaaaaa'] }).width;

    expect(ten - nine).toBeCloseTo(7.2);
  });

  it('grows by one row per extra line', () => {
    const one = box({ lines: ['a'] }).height;
    const two = box({ lines: ['a', 'b'] }).height;
    const three = box({ lines: ['a', 'b', 'c'] }).height;

    expect(two - one).toBe(three - two);
  });

  it('renders every line in order', () => {
    const lines = ['2026-08-11 14:30:00', '1,234'];
    expect(box({ lines }).texts.map((t) => t.textContent)).toEqual(lines);
  });

  it('keeps every line monospace, so the width formula holds', () => {
    for (const text of box({ lines: ['a', 'b', 'c'] }).texts) {
      expect(text.getAttribute('class')).toContain('font-mono');
    }
  });
});

describe('ChartTooltip placement', () => {
  it('sits `gap` to the right of the anchor when it fits', () => {
    expect(box({ rightOf: 400, leftOf: 400 }).x).toBe(412);
  });

  it('flips to the left rather than overflowing the plot', () => {
    const { x, width } = box({ rightOf: 940, leftOf: 940 });

    expect(x).toBeLessThan(940);
    expect(x + width).toBeLessThanOrEqual(PLOT.left + PLOT.width);
  });

  it('centres on the anchor for `center`', () => {
    const { y, height } = box({ anchorY: { at: 100, align: 'center' } });
    expect(y + height / 2).toBe(100);
  });

  it('clears the anchor for `above`, rather than straddling it', () => {
    expect(box({ anchorY: { at: 100, align: 'above' } }).y).toBeLessThan(100);
  });

  it('sizes before placing, so a taller box still centres correctly', () => {
    const short = box({ lines: ['a'], anchorY: { at: 150, align: 'center' } });
    const tall = box({
      lines: ['a', 'b', 'c', 'd'],
      anchorY: { at: 150, align: 'center' },
    });

    expect(short.y + short.height / 2).toBe(150);
    expect(tall.y + tall.height / 2).toBe(150);
    expect(tall.y).toBeLessThan(short.y);
  });
});
