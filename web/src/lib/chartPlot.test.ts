import { describe, expect, it } from 'vitest';
import { viewBoxX } from './chartPlot';

const VIEW_W = 960;
const VIEW_H = 320;

describe('viewBoxX', () => {
  it('scales by width when the box keeps the viewBox aspect', () => {
    const rect = { left: 100, width: 1920, height: 640 };

    expect(viewBoxX(rect, 100, VIEW_W, VIEW_H)).toBe(0);
    expect(viewBoxX(rect, 1060, VIEW_W, VIEW_H)).toBe(480);
    expect(viewBoxX(rect, 2020, VIEW_W, VIEW_H)).toBe(960);
  });

  it('undoes the side bands of a box wider than the viewBox aspect', () => {
    // Height-bound: drawn at scale 1, centered with 120px empty on each side.
    const rect = { left: 0, width: 1200, height: 320 };

    expect(viewBoxX(rect, 120, VIEW_W, VIEW_H)).toBe(0);
    expect(viewBoxX(rect, 600, VIEW_W, VIEW_H)).toBe(480);
    expect(viewBoxX(rect, 1080, VIEW_W, VIEW_H)).toBe(960);
  });

  it('has no answer before the svg is laid out', () => {
    expect(
      viewBoxX({ left: 0, width: 0, height: 0 }, 10, VIEW_W, VIEW_H),
    ).toBeNull();
  });
});
