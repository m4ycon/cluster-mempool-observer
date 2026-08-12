import { describe, expect, it } from 'vitest';
import type { PlotRect } from './chartPlot';
import { tooltipPlacement } from './chartTooltip';

const plot: PlotRect = { left: 48, top: 16, width: 896, height: 276 };

const place = (over: Partial<Parameters<typeof tooltipPlacement>[0]> = {}) =>
  tooltipPlacement({
    rightOf: 400,
    leftOf: 400,
    preferredTop: 100,
    gap: 12,
    width: 160,
    height: 44,
    plot,
    ...over,
  });

describe('tooltipPlacement', () => {
  it('sits `gap` to the right of the anchor when it fits', () => {
    expect(place().x).toBe(412);
  });

  it('flips to the left of `leftOf` when the right side would overflow', () => {
    // rightOf 900 + gap 12 + width 160 = 1072 > plot right edge (944)
    expect(place({ rightOf: 900, leftOf: 900 }).x).toBe(728);
  });

  it('flips around the far edge for a bar anchor, not the near one', () => {
    // a bar spanning 880..900: flip must clear its left edge, not its right
    expect(place({ rightOf: 900, leftOf: 880 }).x).toBe(708);
  });

  it('clamps a flipped tooltip to the plot rather than escaping left', () => {
    // no room on either side: left of the anchor would land at -124
    expect(place({ rightOf: 940, leftOf: 48 }).x).toBe(plot.left);
  });

  it('leaves a vertically fitting top untouched', () => {
    expect(place({ preferredTop: 100 }).y).toBe(100);
  });

  it('clamps a top above the plot down to the plot top', () => {
    expect(place({ preferredTop: -50 }).y).toBe(plot.top);
  });

  it('clamps a top below the plot up so the whole box stays visible', () => {
    // bottom edge is 16 + 276 = 292, so the box must start at 292 - 44
    expect(place({ preferredTop: 400 }).y).toBe(248);
  });

  it('keeps the box inside the plot for every anchor along the x axis', () => {
    for (let anchor = plot.left; anchor <= plot.left + plot.width; anchor++) {
      const { x } = place({ rightOf: anchor, leftOf: anchor });
      expect(x).toBeGreaterThanOrEqual(plot.left);
      expect(x + 160).toBeLessThanOrEqual(plot.left + plot.width);
    }
  });
});
