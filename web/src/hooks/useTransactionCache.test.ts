import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { TransactionLookup, TransactionRef } from '../types/events';
import { STALE_RETRY_MS, useTransactionCache } from './useTransactionCache';

function tx(
  txid: string,
  overrides: Partial<TransactionRef> = {},
): TransactionRef {
  return {
    txid,
    fee: 100,
    vsize: 200,
    first_seen_at: '2026-01-01T00:00:00.000Z',
    cluster_id: null,
    hollow: false,
    input_txids: [],
    ...overrides,
  };
}

function lookupResponse(lookup: TransactionLookup): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(lookup),
  } as Response;
}

function errorResponse(status = 500): Response {
  return {
    ok: false,
    status,
    statusText: 'Error',
    json: () => Promise.resolve(null),
  } as Response;
}

function requestedTxids(url: string): string[] {
  const query = new URL(url, 'http://test').searchParams.get('txids') ?? '';
  return query.split(',').filter(Boolean);
}

afterEach(() => {
  vi.unstubAllGlobals();
  // A failed fake-timer test must not leave them installed for the next one.
  vi.useRealTimers();
});

describe('useTransactionCache', () => {
  it('fetches all txids on first render', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        lookupResponse({ found: [tx('a'), tx('b')], missing: [] }),
      );
    vi.stubGlobal('fetch', fetchMock);

    const { result } = renderHook(() => useTransactionCache(['a', 'b']));

    expect(result.current.loading).toBe(true);

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(result.current.txs.get('a')).toEqual(tx('a'));
    expect(result.current.txs.get('b')).toEqual(tx('b'));
    expect(result.current.missing.size).toBe(0);
    expect(result.current.error).toBeNull();
  });

  it('issues no second fetch when rerendered with a new array of the same txids', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        lookupResponse({ found: [tx('a'), tx('b')], missing: [] }),
      );
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ txids }) => useTransactionCache(txids),
      { initialProps: { txids: ['a', 'b'] } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // A brand-new array identity with the same membership, as a websocket tick produces.
    rerender({ txids: ['b', 'a'] });

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('fetches only the newly added txid', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(lookupResponse({ found: [tx('a')], missing: [] }))
      .mockResolvedValueOnce(lookupResponse({ found: [tx('c')], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ txids }) => useTransactionCache(txids),
      { initialProps: { txids: ['a'] } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    rerender({ txids: ['a', 'c'] });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));

    const secondUrl = fetchMock.mock.calls[1][0] as string;
    expect(requestedTxids(secondUrl)).toEqual(['c']);

    await waitFor(() => expect(result.current.txs.has('c')).toBe(true));
    expect(result.current.txs.get('a')).toEqual(tx('a'));
  });

  it('issues no fetch and leaves the cache intact when a txid is removed', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        lookupResponse({ found: [tx('a'), tx('b')], missing: [] }),
      );
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ txids }) => useTransactionCache(txids),
      { initialProps: { txids: ['a', 'b'] } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    rerender({ txids: ['a'] });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(result.current.txs.get('a')).toEqual(tx('a'));
    expect(result.current.txs.get('b')).toEqual(tx('b'));
  });

  it('does not refetch a txid the server already reported as missing', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        lookupResponse({ found: [tx('a')], missing: ['b'] }),
      )
      .mockResolvedValueOnce(lookupResponse({ found: [tx('c')], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ txids }) => useTransactionCache(txids),
      { initialProps: { txids: ['a', 'b'] } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.missing.has('b')).toBe(true);

    rerender({ txids: ['a', 'b', 'c'] });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));

    const secondUrl = fetchMock.mock.calls[1][0] as string;
    expect(requestedTxids(secondUrl)).toEqual(['c']);
  });

  it('sets error and leaves previously cached txs intact on a rejected fetch', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(lookupResponse({ found: [tx('a')], missing: [] }))
      .mockResolvedValueOnce(errorResponse(500));
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(
      ({ txids }) => useTransactionCache(txids),
      { initialProps: { txids: ['a'] } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.txs.get('a')).toEqual(tx('a'));

    rerender({ txids: ['a', 'b'] });

    await waitFor(() => expect(result.current.error).not.toBeNull());
    expect(result.current.txs.get('a')).toEqual(tx('a'));
    expect(result.current.txs.has('b')).toBe(false);
  });

  it('aborts the in-flight request on unmount without setting state', () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init?: RequestInit) => {
        signal = init?.signal ?? undefined;
        return new Promise(() => {});
      }),
    );

    const { unmount } = renderHook(() => useTransactionCache(['a']));

    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });

  it('refetches a hollow row after STALE_RETRY_MS but not before', async () => {
    const hollow = tx('a', { hollow: true, vsize: 0, fee: null });
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(lookupResponse({ found: [hollow], missing: [] }))
      .mockResolvedValue(lookupResponse({ found: [tx('a')], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers();

    const { result } = renderHook(() => useTransactionCache(['a']));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(result.current.txs.get('a')?.hollow).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALE_RETRY_MS - 1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.txs.get('a')?.hollow).toBe(false);
  });

  it('refetches a row with input_txids: null after STALE_RETRY_MS but not before', async () => {
    const parentless = tx('a', { input_txids: null });
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        lookupResponse({ found: [parentless], missing: [] }),
      )
      .mockResolvedValue(
        lookupResponse({
          found: [tx('a', { input_txids: ['p'] })],
          missing: [],
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers();

    const { result } = renderHook(() => useTransactionCache(['a']));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(result.current.txs.get('a')?.input_txids).toBeNull();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALE_RETRY_MS - 1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.txs.get('a')?.input_txids).toEqual(['p']);
  });

  it('never refetches a complete row, however much time passes', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(lookupResponse({ found: [tx('a')], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers();

    renderHook(() => useTransactionCache(['a']));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALE_RETRY_MS * 3);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('retries a txid the server reported missing after STALE_RETRY_MS', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(lookupResponse({ found: [], missing: ['a'] }))
      .mockResolvedValue(lookupResponse({ found: [tx('a')], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers();

    const { result } = renderHook(() => useTransactionCache(['a']));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(result.current.missing.has('a')).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALE_RETRY_MS - 1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.missing.has('a')).toBe(false);
    expect(result.current.txs.get('a')).toEqual(tx('a'));
  });

  it('does not retry on every rerender, only once per STALE_RETRY_MS interval', async () => {
    const hollow = tx('a', { hollow: true, vsize: 0, fee: null });
    const fetchMock = vi
      .fn()
      .mockResolvedValue(lookupResponse({ found: [hollow], missing: [] }));
    vi.stubGlobal('fetch', fetchMock);
    vi.useFakeTimers();

    const { rerender } = renderHook(({ txids }) => useTransactionCache(txids), {
      initialProps: { txids: ['a'] },
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Five websocket-style ticks, each a brand-new array of the same txid.
    for (let i = 0; i < 5; i++) {
      rerender({ txids: ['a'] });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1000);
      });
    }
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STALE_RETRY_MS - 5000);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
