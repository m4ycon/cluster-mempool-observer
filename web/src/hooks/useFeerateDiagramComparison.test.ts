import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FeerateDiagramComparison } from '../types/generated/FeerateDiagramComparison';
import { useFeerateDiagramComparison } from './useFeerateDiagramComparison';

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

describe('useFeerateDiagramComparison', () => {
  it('requests the comparison path', () => {
    const fetchMock = vi.fn((_url: string) => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchMock);

    renderHook(() => useFeerateDiagramComparison());

    expect(fetchMock.mock.calls[0][0]).toContain(
      '/mempool/feerate-diagram/comparison',
    );
  });

  it('surfaces a successful response as loaded with the comparison', async () => {
    const comparison: FeerateDiagramComparison = {
      sampled_at: '2026-08-12T00:00:00Z',
      node: [
        { weight: 0, fee_sats: 0 },
        { weight: 4000, fee_sats: 1500 },
      ],
      ours: [{ weight: 0, fee_sats: 0 }],
      missing: 1,
      extra: 0,
      node_total: 1,
      unit: 'chunk_count',
      missing_chunks: [1],
      extra_chunks: [],
    };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(comparison)));

    const { result } = renderHook(() => useFeerateDiagramComparison());

    await waitFor(() =>
      expect(result.current).toEqual({ status: 'loaded', comparison }),
    );
  });

  it('surfaces the cold-start null as loaded, not error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(null)));

    const { result } = renderHook(() => useFeerateDiagramComparison());

    await waitFor(() =>
      expect(result.current).toEqual({ status: 'loaded', comparison: null }),
    );
  });

  it('surfaces a failed request as error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    const { result } = renderHook(() => useFeerateDiagramComparison());

    await waitFor(() => expect(result.current.status).toBe('error'));
  });
});
