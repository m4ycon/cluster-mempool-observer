import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// jsdom has no layout engine, so `scrollTo` is unimplemented and every render
// of a scroll-restoring route logs "Not implemented: window.scrollTo". Stub it
// rather than silencing console.error, so React's act/key warnings and any
// genuinely unexpected error still stand out in the output.
window.scrollTo = vi.fn();
