import { describe, expect, it } from 'vitest';
import {
  fitToView,
  MAX_K,
  MIN_K,
  screenToLayout,
  type Viewport,
  viewScale,
  zoomAbout,
} from './svgViewport';

const VIEW = { w: 720, h: 480 };

function rectOf(left: number, top: number, width: number, height: number) {
  return { left, top, width, height } as DOMRect;
}

// Places a layout point through `vp` and `rect` to get the client coords
// `screenToLayout` should invert back to that same layout point.
function clientCoordsFor(
  layoutPt: { x: number; y: number },
  rect: DOMRect,
  vp: Viewport,
) {
  const viewX = layoutPt.x * vp.k + vp.tx;
  const viewY = layoutPt.y * vp.k + vp.ty;
  return {
    clientX: rect.left + (viewX / VIEW.w) * rect.width,
    clientY: rect.top + (viewY / VIEW.h) * rect.height,
  };
}

describe('screenToLayout', () => {
  it('round-trips against a synthetic transform at k = 1', () => {
    const vp: Viewport = { tx: 10, ty: 20, k: 1 };
    const rect = rectOf(0, 0, 720, 480);
    const layoutPt = { x: 100, y: 50 };
    const { clientX, clientY } = clientCoordsFor(layoutPt, rect, vp);

    const result = screenToLayout(clientX, clientY, rect, VIEW, vp);

    expect(result.x).toBeCloseTo(layoutPt.x);
    expect(result.y).toBeCloseTo(layoutPt.y);
  });

  it('round-trips at k = 3 with an offset, scaled client rect', () => {
    const vp: Viewport = { tx: -50, ty: 30, k: 3 };
    const rect = rectOf(20, 10, 360, 240); // rendered at half the viewBox size
    const layoutPt = { x: 40, y: 60 };
    const { clientX, clientY } = clientCoordsFor(layoutPt, rect, vp);

    const result = screenToLayout(clientX, clientY, rect, VIEW, vp);

    expect(result.x).toBeCloseTo(layoutPt.x);
    expect(result.y).toBeCloseTo(layoutPt.y);
  });

  it('returns finite numbers for a zero-size rect instead of NaN', () => {
    const vp: Viewport = { tx: 0, ty: 0, k: 1 };
    const rect = rectOf(0, 0, 0, 0);

    const result = screenToLayout(5, 5, rect, VIEW, vp);

    expect(Number.isFinite(result.x)).toBe(true);
    expect(Number.isFinite(result.y)).toBe(true);
  });

  // Regression for a rect whose aspect ratio doesn't match VIEW's (1.5):
  // preserveAspectRatio="xMidYMid meet" picks one scale for both axes and
  // letterboxes the other, so a same-pixel move in x vs y must land the same
  // distance in layout space -- naive independent rectW/rectH ratios don't.
  it('tracks x and y equally under a wider-than-view (horizontally letterboxed) rect', () => {
    const vp: Viewport = { tx: 0, ty: 0, k: 1 };
    const rect = rectOf(0, 0, 900, 480); // aspect 1.875 > VIEW's 1.5

    const origin = screenToLayout(450, 240, rect, VIEW, vp);
    const movedX = screenToLayout(460, 240, rect, VIEW, vp);
    const movedY = screenToLayout(450, 250, rect, VIEW, vp);

    expect(movedX.x - origin.x).toBeCloseTo(movedY.y - origin.y);
  });

  it('tracks x and y equally under a taller-than-view (vertically letterboxed) rect', () => {
    const vp: Viewport = { tx: 0, ty: 0, k: 1 };
    const rect = rectOf(0, 0, 360, 480); // aspect 0.75 < VIEW's 1.5

    const origin = screenToLayout(180, 240, rect, VIEW, vp);
    const movedX = screenToLayout(190, 240, rect, VIEW, vp);
    const movedY = screenToLayout(180, 250, rect, VIEW, vp);

    expect(movedX.x - origin.x).toBeCloseTo(movedY.y - origin.y);
  });
});

describe('viewScale', () => {
  it('picks the tighter-fitting axis, not independent per-axis ratios', () => {
    expect(viewScale(rectOf(0, 0, 900, 480), VIEW)).toBeCloseTo(1); // height-bound
    expect(viewScale(rectOf(0, 0, 360, 480), VIEW)).toBeCloseTo(0.5); // width-bound
  });

  it('falls back to a finite scale for a zero-size rect', () => {
    expect(Number.isFinite(viewScale(rectOf(0, 0, 0, 0), VIEW))).toBe(true);
  });
});

describe('zoomAbout', () => {
  it('leaves the anchor point fixed on screen after zooming in', () => {
    const vp: Viewport = { tx: 5, ty: -10, k: 1 };
    const layoutPt = { x: 30, y: 40 };
    const viewPt = {
      x: layoutPt.x * vp.k + vp.tx,
      y: layoutPt.y * vp.k + vp.ty,
    };

    const next = zoomAbout(layoutPt, viewPt, 2.5);

    expect(next.k).toBe(2.5);
    const reprojected = {
      x: layoutPt.x * next.k + next.tx,
      y: layoutPt.y * next.k + next.ty,
    };
    expect(reprojected.x).toBeCloseTo(viewPt.x);
    expect(reprojected.y).toBeCloseTo(viewPt.y);
  });

  it('leaves the anchor point fixed on screen after zooming out', () => {
    const vp: Viewport = { tx: 0, ty: 0, k: 4 };
    const layoutPt = { x: 12, y: 8 };
    const viewPt = {
      x: layoutPt.x * vp.k + vp.tx,
      y: layoutPt.y * vp.k + vp.ty,
    };

    const next = zoomAbout(layoutPt, viewPt, 0.5);

    const reprojected = {
      x: layoutPt.x * next.k + next.tx,
      y: layoutPt.y * next.k + next.ty,
    };
    expect(reprojected.x).toBeCloseTo(viewPt.x);
    expect(reprojected.y).toBeCloseTo(viewPt.y);
  });
});

describe('fitToView', () => {
  it('centres a drawing smaller than the viewBox on both axes', () => {
    const vp = fitToView(100, 50, VIEW, 24);

    expect(vp.tx).toBeCloseTo((VIEW.w - 100 * vp.k) / 2);
    expect(vp.ty).toBeCloseTo((VIEW.h - 50 * vp.k) / 2);
  });

  it('clamps the scale to MAX_K for a tiny drawing', () => {
    const vp = fitToView(1, 1, VIEW, 24);
    expect(vp.k).toBe(MAX_K);
  });

  it('clamps the scale to MIN_K for a huge drawing', () => {
    const vp = fitToView(100_000, 100_000, VIEW, 24);
    expect(vp.k).toBe(MIN_K);
  });

  it('falls back to a finite identity transform for a zero-size input', () => {
    const vp = fitToView(0, 0, VIEW, 24);
    expect(vp).toEqual({ tx: 0, ty: 0, k: 1 });
  });
});
