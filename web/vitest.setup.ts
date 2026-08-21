import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// jsdom has no layout engine, so `scrollTo` is unimplemented and every render
// of a scroll-restoring route logs "Not implemented: window.scrollTo". Stub it
// rather than silencing console.error, so React's act/key warnings and any
// genuinely unexpected error still stand out in the output.
window.scrollTo = vi.fn();

// jsdom has no PointerEvent constructor at all, so fireEvent.pointerDown/Move
// built from a PointerEventInit (clientX/clientY/pointerId) silently produces
// an event with none of those fields set. MouseEvent already supports them.
if (typeof window.PointerEvent === 'undefined') {
  class PointerEventPolyfill extends MouseEvent {
    pointerId: number;
    constructor(type: string, params: PointerEventInit = {}) {
      super(type, params);
      this.pointerId = params.pointerId ?? 0;
    }
  }
  // @ts-expect-error jsdom lacks a real PointerEvent to type against
  window.PointerEvent = PointerEventPolyfill;
}
