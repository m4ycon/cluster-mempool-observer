import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ChartRange } from '../lib/routes';
import type { SystemEvent } from '../types/generated/SystemEvent';
import { useSystemEvents } from './useSystemEvents';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
  } as Response;
}

const range: ChartRange = { from: 0, to: 86_400_000 };

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('useSystemEvents', () => {
  it('requests the system-events path for the given range', () => {
    const fetchMock = vi.fn((_url: string) => new Promise<Response>(() => {}));
    vi.stubGlobal('fetch', fetchMock);

    renderHook(() => useSystemEvents(range));

    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toContain('/system-events');
    expect(url).toContain('from=');
    expect(url).toContain('to=');
  });

  it('transitions to loaded with the fetched events', async () => {
    const events: SystemEvent[] = [
      {
        id: 1,
        kind: 'server_started',
        details: {},
        created_at: '2026-01-01T00:00:00Z',
      },
    ];
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(events)));

    const { result } = renderHook(() => useSystemEvents(range));

    await waitFor(() =>
      expect(result.current).toEqual({ status: 'loaded', events }),
    );
  });

  it('surfaces a failed request as error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );

    const { result } = renderHook(() => useSystemEvents(range));

    await waitFor(() => expect(result.current.status).toBe('error'));
  });
});
