import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useHttpGet } from './useHttpGet';

interface Thing {
  hello: string;
}

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
  } as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('useHttpGet', () => {
  it('starts in the loading state', () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );

    const { result } = renderHook(() => useHttpGet<Thing>('/things'));

    expect(result.current).toEqual({ status: 'loading' });
  });

  it('transitions to loaded with the parsed response on success', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ hello: 'world' })),
    );

    const { result } = renderHook(() => useHttpGet<Thing>('/things'));

    await waitFor(() =>
      expect(result.current).toEqual({
        status: 'loaded',
        data: { hello: 'world' },
      }),
    );
  });

  it('transitions to error on a non-OK response, never loaded', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    const { result } = renderHook(() => useHttpGet<Thing>('/things'));

    await waitFor(() => expect(result.current.status).toBe('error'));
    expect(result.current.status).not.toBe('loaded');
  });

  it('aborts the in-flight request on unmount', () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init?: RequestInit) => {
        signal = init?.signal ?? undefined;
        return new Promise(() => {});
      }),
    );

    const { unmount } = renderHook(() => useHttpGet<Thing>('/things'));

    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });

  // A real DOM unmount can't be probed afterwards: React drops updates on an
  // unmounted fiber regardless of the aborted-check, so the only place the
  // abort-vs-error distinction is actually observable is a path change, where
  // the same cleanup/abort runs but the hook stays mounted.
  it('does not surface a stale request aborted by a path change as an error', async () => {
    const pending: Array<{
      resolve: (res: Response) => void;
      reject: (err: unknown) => void;
    }> = [];

    vi.stubGlobal(
      'fetch',
      vi.fn(
        () =>
          new Promise<Response>((resolve, reject) => {
            pending.push({ resolve, reject });
          }),
      ),
    );

    const { result, rerender } = renderHook(
      ({ path }) => useHttpGet<Thing>(path),
      { initialProps: { path: '/a' } },
    );

    expect(pending).toHaveLength(1);

    rerender({ path: '/b' });
    expect(pending).toHaveLength(2);

    // The stale request settles late (as a real aborted fetch would reject),
    // after the cleanup already aborted its controller.
    pending[0].reject(new DOMException('aborted', 'AbortError'));
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current).toEqual({ status: 'loading' });

    pending[1].resolve(jsonResponse({ hello: 'b' }));
    await waitFor(() =>
      expect(result.current).toEqual({
        status: 'loaded',
        data: { hello: 'b' },
      }),
    );
  });

  it('re-fetches when path changes', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ hello: 'a' }))
      .mockResolvedValueOnce(jsonResponse({ hello: 'b' }));
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ path }) => useHttpGet<Thing>(path),
      { initialProps: { path: '/a' } },
    );

    await waitFor(() =>
      expect(result.current).toEqual({
        status: 'loaded',
        data: { hello: 'a' },
      }),
    );

    rerender({ path: '/b' });

    await waitFor(() =>
      expect(result.current).toEqual({
        status: 'loaded',
        data: { hello: 'b' },
      }),
    );
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1][0]).toContain('/b');
  });
});
