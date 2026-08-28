/**
 * Pure pan/zoom maths for an SVG that keeps a fixed viewBox and transforms a
 * `<g>` inside it. No DOM access, no React -- see TxDagCanvas.tsx for the
 * event handlers that call these.
 */

import type { Point } from './txDagLayout';

export interface Viewport {
  tx: number;
  ty: number;
  k: number;
}

export const MIN_K = 0.25;
export const MAX_K = 8;

/** Clamps `v` to `[min, max]`. */
export function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

/**
 * Px-per-viewBox-unit scale under `preserveAspectRatio="xMidYMid meet"`: one
 * uniform factor (the tighter-fitting axis), not independent ratios per
 * axis -- the SVG never stretches, it letterboxes whichever axis has slack.
 */
export function viewScale(
  rect: { width: number; height: number },
  view: { w: number; h: number },
): number {
  // A hidden panel or jsdom reports a zero-size rect; fall back to 1 so the
  // division below yields a finite (if meaningless) scale, never NaN/0.
  const rectW = rect.width || 1;
  const rectH = rect.height || 1;
  return Math.min(rectW / view.w, rectH / view.h);
}

/** Client coords -> layout coords, via the element rect and the transform. */
export function screenToLayout(
  clientX: number,
  clientY: number,
  rect: DOMRect,
  view: { w: number; h: number },
  vp: Viewport,
): Point {
  const scale = viewScale(rect, view);
  // "meet" centres the content on the letterboxed axis, so the client rect
  // has blank margin there that must be subtracted before scaling back down.
  const offsetX = (rect.width - view.w * scale) / 2;
  const offsetY = (rect.height - view.h * scale) / 2;
  const vx = (clientX - rect.left - offsetX) / scale;
  const vy = (clientY - rect.top - offsetY) / scale;
  return { x: (vx - vp.tx) / vp.k, y: (vy - vp.ty) / vp.k };
}

/** Zoom to `nextK` while pinning `layoutPt` under `viewPt`. */
export function zoomAbout(
  layoutPt: Point,
  viewPt: Point,
  nextK: number,
): Viewport {
  return {
    k: nextK,
    tx: viewPt.x - layoutPt.x * nextK,
    ty: viewPt.y - layoutPt.y * nextK,
  };
}

/** Transform that fits a `width` x `height` drawing into the viewBox. */
export function fitToView(
  width: number,
  height: number,
  view: { w: number; h: number },
  padding: number,
  maxK: number = MAX_K,
): Viewport {
  if (width <= 0 || height <= 0) return { tx: 0, ty: 0, k: 1 };

  const availW = Math.max(view.w - 2 * padding, 1);
  const availH = Math.max(view.h - 2 * padding, 1);
  const k = clamp(Math.min(availW / width, availH / height), MIN_K, maxK);
  return {
    k,
    tx: (view.w - width * k) / 2,
    ty: (view.h - height * k) / 2,
  };
}
