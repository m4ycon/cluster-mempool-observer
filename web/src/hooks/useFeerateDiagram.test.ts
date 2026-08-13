import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MempoolFeerateDiagram } from '../types/generated/MempoolFeerateDiagram';
import { useFeerateDiagram } from './useFeerateDiagram';

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

describe('useFeerateDiagram', () => {
  it('requests the feerate-diagram path', () => {
    const fetchMock = vi.fn((_url: string) => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchMock);

    renderHook(() => useFeerateDiagram());

    expect(fetchMock.mock.calls[0][0]).toContain('/mempool/feerate-diagram');
  });

  it('surfaces a successful response as loaded with the diagram', async () => {
    const diagram: MempoolFeerateDiagram = {
      sampled_at: '2026-08-12T00:00:00Z',
      points: [
        { weight: 0, fee_sats: 0 },
        { weight: 4000, fee_sats: 1500 },
      ],
    };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(diagram)));

    const { result } = renderHook(() => useFeerateDiagram());

    await waitFor(() =>
      expect(result.current).toEqual({ status: 'loaded', diagram }),
    );
  });

  it('surfaces the cold-start empty diagram as loaded, not error', async () => {
    const coldStart: MempoolFeerateDiagram = { sampled_at: null, points: [] };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(coldStart)));

    const { result } = renderHook(() => useFeerateDiagram());

    await waitFor(() =>
      expect(result.current).toEqual({
        status: 'loaded',
        diagram: coldStart,
      }),
    );
  });

  it('surfaces a failed request as error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    const { result } = renderHook(() => useFeerateDiagram());

    await waitFor(() => expect(result.current.status).toBe('error'));
  });
});
